//! OTLP exporter plumbing: resource attributes, the blocking TLS HTTP client,
//! request headers and the self-referential noise filter.

use std::collections::HashMap;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

use crate::config::MonocleConfig;

/// Per-exporter network timeout for OTLP export requests.
pub(crate) const EXPORT_TIMEOUT: Duration = Duration::from_secs(10);

/// Build an OTLP resource describing this service.
///
/// Emits the standard `service.*` / `deployment.environment*` semantic-convention
/// attributes, the Monocle-specific `monocle.language.name`, then merges any
/// caller-supplied [`MonocleConfig::extra_resource_attributes`].
pub(crate) fn build_resource(cfg: &MonocleConfig) -> Resource {
    let mut attrs = vec![
        KeyValue::new("service.version", cfg.service_version.clone()),
        KeyValue::new("deployment.environment", cfg.environment.clone()),
        // Newer semconv key; emit both so either dashboard convention works.
        KeyValue::new("deployment.environment.name", cfg.environment.clone()),
        KeyValue::new("monocle.language.name", "rust"),
    ];
    attrs.extend(cfg.extra_resource_attributes.iter().cloned());
    Resource::builder()
        .with_service_name(cfg.service_name.clone())
        .with_attributes(attrs)
        .build()
}

/// Build a blocking reqwest client with an explicit native-tls (OpenSSL) backend
/// for the OTLP exporters.
///
/// Two constraints force this over the exporter's built-in client:
/// 1. The batch processors run on a dedicated thread with no Tokio reactor, so
///    the export must use the *blocking* client (the async one panics there).
/// 2. `opentelemetry-otlp` builds its internal blocking client with no TLS
///    connector configured, and reqwest only auto-selects native-tls via
///    `default-tls` (which is rustls/aws-lc-rs in reqwest 0.13). Without an
///    explicit connector, every HTTPS request fails with "network error".
///    Calling `.use_native_tls()` here wires OpenSSL as the connector.
///
/// Built on a separate std thread + join: `reqwest::blocking::Client::build`
/// must not run inside the Tokio runtime that drives `main`.
pub(crate) fn build_tls_http_client() -> reqwest::blocking::Client {
    std::thread::spawn(|| {
        reqwest::blocking::Client::builder()
            .use_native_tls()
            .timeout(EXPORT_TIMEOUT)
            .build()
            .expect("build blocking reqwest client (native-tls) for OTLP export")
    })
    .join()
    .expect("reqwest client builder thread panicked")
}

/// Common OTLP headers required by Monocle: the API key and the environment tag.
pub(crate) fn build_headers(cfg: &MonocleConfig, api_key: String) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("x-api-key".to_string(), api_key);
    headers.insert("x-monocle-env".to_string(), cfg.environment.clone());
    headers
}

/// Events from these targets are excluded from the OTLP layers to avoid a
/// telemetry-generating-telemetry feedback loop (an export failure logs an
/// error → the appender ships it → another export → …).
pub(crate) fn is_self_referential(target: &str) -> bool {
    const NOISY_PREFIXES: [&str; 6] = [
        "opentelemetry",
        "hyper",
        "h2",
        "reqwest",
        "tonic",
        "tower::",
    ];
    NOISY_PREFIXES.iter().any(|p| target.starts_with(p))
}
