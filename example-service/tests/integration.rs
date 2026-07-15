//! End-to-end integration test: point `monocle-agent` at a local mock OTLP
//! collector, drive one request through the example router, flush, and assert
//! traces and metrics were actually exported (with the `x-api-key` header).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use monocle_agent_example_service::build_app;

/// Captured `(request_path, had_x_api_key)` for each OTLP POST the mock received.
type Captured = Arc<Mutex<Vec<(String, bool)>>>;

#[test]
fn exports_traces_and_metrics_over_otlp() {
    let (addr, captured) = start_mock_collector();

    // Point the exporter at the mock and enable export.
    std::env::set_var("MONOCLE_API_KEY", "test-key");
    std::env::set_var("MONOCLE_ENDPOINT", format!("http://{addr}"));
    std::env::set_var("MONOCLE_ENV", "test");
    std::env::remove_var("RUST_LOG"); // don't let a stray filter drop INFO spans

    let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
        "example-integration-test",
        "0.0.0",
    ));

    // Drive one real request through the example router — produces a request
    // span and HTTP + custom metrics — without binding a socket.
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let response = build_app()
            .oneshot(
                Request::builder()
                    .uri("/hello/world")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    });

    // Flush synchronously: the blocking exporter POSTs to the mock and waits for
    // the response, so every export has landed by the time this returns.
    telemetry.shutdown();

    let got = captured.lock().unwrap();
    let paths: Vec<&str> = got.iter().map(|(p, _)| p.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("/v1/traces")),
        "no trace export received; paths = {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("/v1/metrics")),
        "no metric export received; paths = {paths:?}"
    );
    assert!(
        !got.is_empty() && got.iter().all(|(_, key)| *key),
        "x-api-key header missing on an export; captured = {:?}",
        *got
    );
}

/// Start a minimal HTTP server that records OTLP POSTs and replies `200`.
fn start_mock_collector() -> (SocketAddr, Captured) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock collector");
    let addr = listener.local_addr().unwrap();
    let captured: Captured = Arc::new(Mutex::new(Vec::new()));

    let cap = captured.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let cap = cap.clone();
            thread::spawn(move || handle_conn(stream, cap));
        }
    });

    (addr, captured)
}

/// Read every HTTP request on one connection, record it, and reply `200` with an
/// empty (valid) OTLP protobuf response.
fn handle_conn(stream: TcpStream, captured: Captured) {
    // Time-box reads so the handler thread ends once the exporter is done.
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok();
    let mut writer = stream.try_clone().expect("clone stream");
    let mut reader = BufReader::new(stream);

    loop {
        let mut request_line = String::new();
        match reader.read_line(&mut request_line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if request_line.trim().is_empty() {
            continue;
        }
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_string();

        let mut content_length = 0usize;
        let mut has_api_key = false;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).unwrap_or(0) == 0 {
                break;
            }
            let line = header.trim_end();
            if line.is_empty() {
                break; // end of headers
            }
            let lower = line.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                content_length = v.trim().parse().unwrap_or(0);
            }
            if lower.starts_with("x-api-key:") {
                has_api_key = true;
            }
        }

        if content_length > 0 {
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).ok();
        }

        captured.lock().unwrap().push((path, has_api_key));

        let _ = writer.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n",
        );
        let _ = writer.flush();
    }
}
