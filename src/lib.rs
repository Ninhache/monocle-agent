//! # monocle-agent
//!
//! Plug-and-play [OpenTelemetry] export to [Monocle] over OTLP/HTTP for Rust
//! services — **traces, metrics and logs** in one call.
//!
//! ```no_run
//! // 1. Init as early as possible. Pass your crate's name/version so `env!`
//! //    resolves in *your* crate, not this library.
//! let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
//!     env!("CARGO_PKG_NAME"),
//!     env!("CARGO_PKG_VERSION"),
//! ));
//!
//! // 2. ... build and serve your app (e.g. with axum) ...
//!
//! // 3. Flush buffered telemetry before exit.
//! telemetry.shutdown();
//! ```
//!
//! Export is **off by default**: nothing is sent until `MONOCLE_API_KEY` is set
//! (or [`MonocleConfig::with_api_key`] is called). When disabled the crate still
//! installs a stdout `fmt` subscriber and makes no network calls.
//!
//! ## Configuration
//!
//! See [`MonocleConfig::from_env`] for the environment variables. The transport
//! is OTLP/HTTP protobuf; the per-signal paths (`/v1/traces`, `/v1/metrics`,
//! `/v1/logs`) are appended to the base endpoint internally.
//!
//! ## Axum helpers (feature `axum`, on by default)
//!
//! - [`MonocleMakeSpan`] — names request spans `"<METHOD> <route>"`.
//! - [`track_http_metrics`] — records `http.server.request.duration`.
//! - [`spawn_blocking_in_span`] — keeps the trace waterfall intact across
//!   `spawn_blocking` boundaries.
//!
//! ## Runtime requirements
//!
//! The OTLP exporter uses native-tls (OpenSSL on Linux). Ensure `libssl` and
//! `ca-certificates` are available in your runtime image.
//!
//! [OpenTelemetry]: https://opentelemetry.io/
//! [Monocle]: https://monocle.sh/

#![warn(missing_docs)]

mod blocking;
mod config;
mod providers;

#[cfg(feature = "axum")]
mod http;

pub use blocking::spawn_blocking_in_span;
pub use config::MonocleConfig;

#[cfg(feature = "axum")]
pub use http::{record_http, track_http_metrics, MonocleMakeSpan};

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use providers::{
    build_headers, build_resource, build_tls_http_client, is_self_referential, EXPORT_TIMEOUT,
};

/// Holds the OTel providers so they can be flushed on shutdown.
///
/// Keep it alive for the whole process lifetime and call
/// [`TelemetryGuard::shutdown`] before exit. When telemetry is disabled all
/// fields are `None` and `shutdown` is a no-op.
#[must_use = "hold the guard for the process lifetime and call shutdown() before exit to flush telemetry"]
pub struct TelemetryGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

impl TelemetryGuard {
    /// Flush and shut down all providers, ensuring buffered spans/metrics/logs
    /// are exported before the process exits. Safe to call when disabled.
    pub fn shutdown(&self) {
        if let Some(p) = &self.tracer_provider {
            if let Err(e) = p.shutdown() {
                tracing::warn!("otel tracer shutdown: {e}");
            }
        }
        if let Some(p) = &self.meter_provider {
            if let Err(e) = p.shutdown() {
                tracing::warn!("otel meter shutdown: {e}");
            }
        }
        if let Some(p) = &self.logger_provider {
            if let Err(e) = p.shutdown() {
                tracing::warn!("otel logger shutdown: {e}");
            }
        }
    }
}

/// Initialise tracing and (when enabled) OpenTelemetry export to Monocle.
///
/// Installs the global `tracing` subscriber. Always installs a stdout `fmt`
/// layer; when `cfg.api_key` is present it additionally exports traces, metrics
/// and logs over OTLP/HTTP. Returns a [`TelemetryGuard`] that must be kept alive
/// for the process lifetime and flushed via [`TelemetryGuard::shutdown`].
///
/// # Panics
///
/// Panics if the global subscriber is already set, or if an OTLP exporter fails
/// to build (misconfigured endpoint/TLS). Call this once, early in `main`.
pub fn init(cfg: MonocleConfig) -> TelemetryGuard {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cfg.log_filter));
    let fmt_layer = tracing_subscriber::fmt::layer();

    let Some(api_key) = cfg.api_key.clone() else {
        // Telemetry disabled: stdout-only, no network traffic.
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
        tracing::info!("monocle-agent disabled (no MONOCLE_API_KEY) — telemetry export off");
        return TelemetryGuard {
            tracer_provider: None,
            meter_provider: None,
            logger_provider: None,
        };
    };

    let resource = build_resource(&cfg);
    let headers = build_headers(&cfg, api_key);
    let base = cfg.endpoint.trim_end_matches('/');
    // Shared blocking client with an explicit TLS backend — reused by all three
    // exporters (reqwest::blocking::Client is cheap to clone and Send+Sync).
    let http_client = build_tls_http_client();

    // ── Traces ──────────────────────────────────────────────────────────────
    let span_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_http_client(http_client.clone())
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(format!("{base}/v1/traces"))
        .with_headers(headers.clone())
        .with_timeout(EXPORT_TIMEOUT)
        .build()
        .expect("build OTLP span exporter");
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource.clone())
        .build();

    // ── Metrics ─────────────────────────────────────────────────────────────
    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_http_client(http_client.clone())
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(format!("{base}/v1/metrics"))
        .with_headers(headers.clone())
        .with_timeout(EXPORT_TIMEOUT)
        .build()
        .expect("build OTLP metric exporter");
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource.clone())
        .build();

    // ── Logs ────────────────────────────────────────────────────────────────
    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .with_http_client(http_client)
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(format!("{base}/v1/logs"))
        .with_headers(headers.clone())
        .with_timeout(EXPORT_TIMEOUT)
        .build()
        .expect("build OTLP log exporter");
    let logger_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(log_exporter)
        .with_resource(resource)
        .build();

    // Register globals so `global::tracer` / `global::meter` resolve to these.
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    let tracer = tracer_provider.tracer(cfg.service_name.clone());
    let otel_trace_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(filter_fn(|m| !is_self_referential(m.target())));
    let otel_logs_layer = OpenTelemetryTracingBridge::new(&logger_provider)
        .with_filter(filter_fn(|m| !is_self_referential(m.target())));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(otel_trace_layer)
        .with(otel_logs_layer)
        .init();

    tracing::info!(
        endpoint = %base,
        environment = %cfg.environment,
        service.name = %cfg.service_name,
        service.version = %cfg.service_version,
        "monocle-agent enabled → exporting traces/metrics/logs to Monocle"
    );

    TelemetryGuard {
        tracer_provider: Some(tracer_provider),
        meter_provider: Some(meter_provider),
        logger_provider: Some(logger_provider),
    }
}
