//! # monocle-agent
//!
//! Plug-and-play [OpenTelemetry] export to [Monocle] over OTLP/HTTP for Rust
//! services — **traces, metrics and logs** in one call.
//!
//! Works in **any** program — a web server, a worker, a CLI, a batch job. The
//! default install pulls no web framework; you never have to write async code.
//!
//! ## Getting started (any application)
//!
//! ```no_run
//! use std::sync::LazyLock;
//! use monocle_agent::opentelemetry::metrics::Counter;
//!
//! // Build custom instruments once and reuse them.
//! static JOBS: LazyLock<Counter<u64>> =
//!     LazyLock::new(|| monocle_agent::counter("jobs.processed", "Jobs processed"));
//!
//! fn main() {
//!     // Telemetry first. Pass your crate's name/version so `env!` resolves in
//!     // *your* crate, not this library. Off unless MONOCLE_API_KEY is set.
//!     let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
//!         env!("CARGO_PKG_NAME"),
//!         env!("CARGO_PKG_VERSION"),
//!     ));
//!
//!     // Spans nest into a trace; metrics record against the global meter.
//!     monocle_agent::tracing::info_span!("process_batch").in_scope(|| {
//!         // ... do work ...
//!         JOBS.add(1, &[]);
//!     });
//!
//!     telemetry.shutdown(); // flush buffered telemetry before exit
//! }
//! ```
//!
//! Export is **off by default**: nothing is sent until `MONOCLE_API_KEY` is set
//! (or [`MonocleConfig::with_api_key`] is called); when disabled the crate still
//! installs a stdout `fmt` subscriber and makes no network calls. Instrument with
//! the re-exported [`tracing`] (spans/events) and [`counter`]/[`histogram`]/
//! [`gauge`] (or [`opentelemetry`] directly) — no separately-versioned dependency.
//!
//! ## Web services (feature `axum`)
//!
//! Enable `features = ["axum"]` for HTTP helpers:
//!
//! ```no_run
//! # #[cfg(feature = "axum")] {
//! use axum::{routing::get, Router};
//! use tower_http::trace::TraceLayer;
//!
//! # async fn build() {
//! let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
//!     env!("CARGO_PKG_NAME"),
//!     env!("CARGO_PKG_VERSION"),
//! ));
//!
//! let app = Router::new()
//!     .route("/hello", get(|| async { "hello" }))
//!     // Names request spans "GET /hello" instead of "request".
//!     .layer(TraceLayer::new_for_http().make_span_with(monocle_agent::request_span))
//!     // Records http.server.request.duration for every request.
//!     .layer(axum::middleware::from_fn(monocle_agent::track_http_metrics));
//!
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
//! axum::serve(listener, app).await.unwrap();
//! telemetry.shutdown();
//! # }
//! # }
//! ```
//!
//! [`request_span`] is a plain function passed to tower-http's `make_span_with`,
//! so this crate depends on no tower-http version — it works with whatever
//! tower-http (0.5, 0.6, 0.7, …) your service already uses.
//!
//! ## Features
//!
//! | Feature | Adds | Pulls |
//! |---------|------|-------|
//! | _(default)_ | `init`, [`MonocleConfig`], [`TelemetryGuard`], [`counter`]/[`histogram`]/[`gauge`], [`spawn_blocking_in_span`], re-exports | — |
//! | `axum` | [`request_span`], [`track_http_metrics`] | `axum` |
//!
//! ## Configuration
//!
//! See [`MonocleConfig::from_env`] for the environment variables. The transport
//! is OTLP/HTTP protobuf; the per-signal paths (`/v1/traces`, `/v1/metrics`,
//! `/v1/logs`) are appended to the base endpoint internally.
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
mod metrics;
mod providers;

#[cfg(feature = "axum")]
mod http;

pub use blocking::spawn_blocking_in_span;
pub use config::MonocleConfig;
pub use metrics::{counter, gauge, histogram};

#[cfg(feature = "axum")]
pub use http::{record_http, request_span, track_http_metrics};

/// Re-export of the exact `opentelemetry` version this crate builds against.
///
/// Use it to record custom metrics or build `KeyValue` attributes without adding
/// a separately-versioned `opentelemetry` dependency of your own (which would
/// have to be kept in lockstep with this crate).
pub use opentelemetry;

/// Re-export of `tracing` — create spans/events with `tracing::info_span!` /
/// `#[tracing::instrument]` and they flow to the exporter installed by [`init`].
pub use tracing;

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
