# monocle-agent

[![CI](https://github.com/Ninhache/monocle-agent/actions/workflows/ci.yml/badge.svg)](https://github.com/Ninhache/monocle-agent/actions/workflows/ci.yml)
[![docs](https://img.shields.io/badge/docs-github--pages-blue)](https://ninhache.github.io/monocle-agent)
[![license](https://img.shields.io/badge/license-MIT-green)](./LICENSE)

Plug-and-play [OpenTelemetry](https://opentelemetry.io/) export to
[Monocle](https://monocle.sh/) over **OTLP/HTTP** for Rust services — traces,
metrics and logs in a single `init()` call.

There is no official Monocle agent for Rust; this crate fills that gap with the
same ingestion contract as the JS agents (`x-api-key` / `x-monocle-env`
headers, per-signal `/v1/*` paths).

## Features

- **Works anywhere** — a web server, a worker, a CLI, a batch job. The default
  install pulls **no web framework**, and you never have to write async code.
- **One call to wire everything** — `init(config)` installs the `tracing`
  subscriber and the OTLP trace/metric/log exporters.
- **Off by default** — nothing is exported until `MONOCLE_API_KEY` is set; with
  no key you keep a plain stdout `fmt` subscriber and zero network calls.
- **Graceful shutdown** — a returned guard flushes buffered telemetry on exit.
- **Simple custom metrics** — `counter` / `histogram` / `gauge` helpers, plus
  re-exported `opentelemetry` / `tracing`, so you instrument without adding a
  separately-versioned dependency of your own.
- **Axum helpers** (feature `axum`): `request_span` (names request spans
  `GET /render`, works with **any** tower-http version), `track_http_metrics`,
  and `spawn_blocking_in_span` (keeps the trace waterfall across `spawn_blocking`).

## Feature matrix

| Feature | Adds | Pulls |
|---------|------|-------|
| _(default)_ | `init`, `MonocleConfig`, `TelemetryGuard`, `counter`/`histogram`/`gauge`, `spawn_blocking_in_span`, re-exports | — |
| `axum` | `request_span`, `track_http_metrics` | `axum` |

## Getting Started

### Any application (worker / CLI / batch)

```toml
[dependencies]
monocle-agent = "0.3"
```

```rust
use std::sync::LazyLock;
use monocle_agent::opentelemetry::metrics::Counter;

// Build custom instruments once, reuse them.
static JOBS: LazyLock<Counter<u64>> =
    LazyLock::new(|| monocle_agent::counter("jobs.processed", "Jobs processed"));

fn main() {
    // env! resolves in YOUR crate. Off unless MONOCLE_API_KEY is set.
    let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    ));

    monocle_agent::tracing::info_span!("process_batch").in_scope(|| {
        // ... do work ...
        JOBS.add(1, &[]);
    });

    telemetry.shutdown(); // flush before exit
}
```

Instrument with the re-exported `monocle_agent::tracing` (spans/events) and the
`counter`/`histogram`/`gauge` helpers — no `opentelemetry`/`tracing` dependency of
your own to keep version-matched. See `examples/worker.rs`.

### Web service (feature `axum`)

```toml
[dependencies]
monocle-agent = { version = "0.3", features = ["axum"] }
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.7", features = ["trace"] }   # any version works
```

```rust
use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() {
    let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    ));

    let app = Router::new()
        .route("/hello", get(|| async { "hello" }))
        // Names request spans "GET /hello" instead of "request".
        .layer(TraceLayer::new_for_http().make_span_with(monocle_agent::request_span))
        // Records http.server.request.duration for every request.
        .layer(axum::middleware::from_fn(monocle_agent::track_http_metrics));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    telemetry.shutdown();
}
```

Set `MONOCLE_API_KEY` (and optionally `MONOCLE_ENV`) to export; without it the app
runs the same, logging to stdout with no network calls.

**Keeping the waterfall across `spawn_blocking`** — offloaded work otherwise
loses the tracing context:

```rust
let bytes = monocle_agent::spawn_blocking_in_span(move || {
    monocle_agent::tracing::info_span!("encode").in_scope(|| encode(&frame))
})
.await
.unwrap();
```

### Configuration via builder

`from_env` is enough for most services, but everything is overridable:

```rust
let cfg = monocle_agent::MonocleConfig::from_env("my-service", "1.2.3")
    .with_environment("staging")
    .with_endpoint("http://localhost:4318")     // e.g. a local OTLP collector
    .with_resource_attribute("team", "platform");
let telemetry = monocle_agent::init(cfg);
```

## Configuration

All optional except the key that turns export on:

| Env var | Role | Default |
|---------|------|---------|
| `MONOCLE_API_KEY` | API key → `x-api-key`. Absent = export disabled. | _(none)_ |
| `MONOCLE_ENDPOINT` | OTLP/HTTP base URL | `https://ingest.monocle.sh` |
| `MONOCLE_ENV` | `deployment.environment` + `x-monocle-env` | `production` |
| `OTEL_SERVICE_NAME` | `service.name` | the name you pass to `from_env` |
| `MONOCLE_SERVICE_VERSION` | `service.version` | the version you pass to `from_env` |
| `RUST_LOG` | tracing filter | crate's `log_filter` (default `info`) |

Builder overrides are available too — see [`MonocleConfig`](https://ninhache.github.io/monocle-agent/monocle_agent/struct.MonocleConfig.html).

## Runtime requirements

The OTLP exporter uses **native-tls** (OpenSSL on Linux). Make sure `libssl`
and `ca-certificates` are present in your runtime image. On Debian/Ubuntu:

```sh
apt-get install -y libssl3 ca-certificates
```

## Documentation

Full API docs are published to GitHub Pages:
<https://ninhache.github.io/monocle-agent>.

## License

MIT — see [LICENSE](./LICENSE). Attribution appreciated: if you use
`monocle-agent`, a mention is enough.
