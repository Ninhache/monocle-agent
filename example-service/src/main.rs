//! Runnable entrypoint: initialise telemetry, serve the router, flush on exit.
//!
//! ```sh
//! # stdout-only (export off):
//! cargo run
//! # export to Monocle:
//! MONOCLE_API_KEY=your-key cargo run
//! # then: curl localhost:3000/hello/world
//! ```

use monocle_agent_example_service::build_app;

#[tokio::main]
async fn main() {
    // Telemetry first — off unless MONOCLE_API_KEY is set. env! resolves here.
    let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    ));

    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("bind listener");
    monocle_agent::tracing::info!("listening on {bind}");

    axum::serve(listener, build_app())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    // Flush buffered telemetry before exit.
    telemetry.shutdown();
}

/// Resolve on Ctrl-C or SIGTERM so buffered telemetry is flushed on shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
