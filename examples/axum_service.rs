//! Minimal axum service wired with monocle-agent.
//!
//! Doubles as a compile check that `request_span` satisfies tower-http's
//! `MakeSpan` through its blanket `Fn(&Request) -> Span` impl when actually
//! layered onto a `Router` (the doctests only build the layer, not the router).
//!
//! Run with: `cargo run --example axum_service`

use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() {
    let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    ));

    let app: Router = Router::new()
        .route("/hello", get(|| async { "hello" }))
        .layer(TraceLayer::new_for_http().make_span_with(monocle_agent::request_span))
        .layer(axum::middleware::from_fn(monocle_agent::track_http_metrics));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    telemetry.shutdown();
}
