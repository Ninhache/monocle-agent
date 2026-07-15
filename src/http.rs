//! Axum HTTP integration: an inbound-request metrics middleware and
//! [`request_span`], which names request spans `"<METHOD> <route>"`
//! (e.g. `GET /render`).
//!
//! Only compiled with the `axum` feature (on by default).

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::metrics::Histogram;
use opentelemetry::KeyValue;
use tracing::Span;

/// Meter name for the instruments this crate owns.
const METER_NAME: &str = "monocle-agent";

/// `http.server.request.duration` — inbound HTTP request latency, in seconds.
static HTTP_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    opentelemetry::global::meter(METER_NAME)
        .f64_histogram("http.server.request.duration")
        .with_unit("s")
        .with_description("Duration of inbound HTTP requests")
        .build()
});

/// Record one HTTP request against `http.server.request.duration`.
///
/// `route` should be the low-cardinality matched path (e.g. `/items/{id}`),
/// never the raw URI, to keep metric label cardinality bounded.
pub fn record_http(route: &str, status: u16, dur: Duration) {
    HTTP_DURATION.record(
        dur.as_secs_f64(),
        &[
            KeyValue::new("http.route", route.to_string()),
            KeyValue::new("http.response.status_code", status as i64),
        ],
    );
}

/// Axum middleware recording `http.server.request.duration` for every request.
///
/// Wire it after your routes so [`MatchedPath`] is populated:
/// ```no_run
/// # use axum::Router;
/// # let router: Router = Router::new();
/// let app = router.layer(axum::middleware::from_fn(monocle_agent::track_http_metrics));
/// ```
pub async fn track_http_metrics(req: Request, next: Next) -> Response {
    // Prefer the matched route template; fall back to a constant so 404s / unmatched
    // requests don't blow up label cardinality with arbitrary paths.
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "unmatched".to_string());
    let start = Instant::now();
    let resp = next.run(req).await;
    record_http(&route, resp.status().as_u16(), start.elapsed());
    resp
}

/// Build the per-request span, named `"<METHOD> <route>"` (e.g. `GET /render`)
/// instead of tower-http's static `"request"`, so traces read clearly in Monocle.
///
/// Pass it straight to tower-http's `make_span_with`:
///
/// ```no_run
/// # use tower_http::trace::TraceLayer;
/// let layer = TraceLayer::new_for_http().make_span_with(monocle_agent::request_span);
/// ```
///
/// It relies on tower-http's blanket `MakeSpan` implementation for
/// `Fn(&Request) -> Span`, so this crate depends on **no** tower-http version —
/// it works with whatever tower-http (0.5, 0.6, 0.7, …) your service already uses.
///
/// The span is created at INFO level (required so it passes the default filter
/// and reaches the OTLP exporter) with `otel.kind = "server"` and the
/// `http.request.method` / `http.route` semantic-convention fields. The dynamic
/// name is carried via the special `otel.name` field understood by
/// `tracing-opentelemetry`.
pub fn request_span(request: &Request) -> Span {
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());
    let name = format!("{} {}", method.as_str(), route);
    tracing::info_span!(
        "http.request",
        otel.name = %name,
        otel.kind = "server",
        http.request.method = %method,
        http.route = %route,
    )
}
