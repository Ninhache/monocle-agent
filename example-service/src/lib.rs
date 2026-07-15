//! A small axum service wired with `monocle-agent`.
//!
//! The router is exposed as [`build_app`] so both the binary (`src/main.rs`) and
//! the integration test (`tests/integration.rs`) share the exact same wiring.

use std::sync::LazyLock;

use axum::{extract::Path, routing::get, Router};
use monocle_agent::opentelemetry::metrics::Counter;
use tower_http::trace::TraceLayer;

/// A custom metric, built once and reused (see the `monocle_agent::counter` helper).
static GREETINGS: LazyLock<Counter<u64>> =
    LazyLock::new(|| monocle_agent::counter("greetings.served", "Greetings served"));

/// Build the router with monocle-agent's request-span namer and HTTP metrics.
///
/// - [`monocle_agent::request_span`] names each request span `"<METHOD> <route>"`.
/// - [`monocle_agent::track_http_metrics`] records `http.server.request.duration`.
pub fn build_app() -> Router {
    Router::new()
        // axum 0.7 path-param syntax (`:name`); the crate pins axum 0.7.
        .route("/hello/:name", get(hello))
        .route("/health", get(|| async { "ok" }))
        .layer(TraceLayer::new_for_http().make_span_with(monocle_agent::request_span))
        .layer(axum::middleware::from_fn(monocle_agent::track_http_metrics))
}

/// Greet `name`. Demonstrates a custom metric and keeping the trace waterfall
/// intact across a `spawn_blocking` boundary.
async fn hello(Path(name): Path<String>) -> String {
    GREETINGS.add(1, &[]);
    monocle_agent::spawn_blocking_in_span(move || {
        // A child span of the request span — nests in the trace waterfall.
        monocle_agent::tracing::info_span!("render_greeting").in_scope(|| format!("Hello, {name}!"))
    })
    .await
    .expect("render_greeting task panicked")
}
