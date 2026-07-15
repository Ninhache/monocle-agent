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

- **One call to wire everything** — `init(config)` installs the `tracing`
  subscriber and the OTLP trace/metric/log exporters.
- **Off by default** — nothing is exported until `MONOCLE_API_KEY` is set; with
  no key you keep a plain stdout `fmt` subscriber and zero network calls.
- **Graceful shutdown** — a returned guard flushes buffered telemetry on exit.
- **Axum helpers** (feature `axum`, on by default):
  - `MonocleMakeSpan` — names request spans `GET /render` instead of `request`.
  - `track_http_metrics` — records `http.server.request.duration`.
  - `spawn_blocking_in_span` — keeps the trace waterfall intact across
    `spawn_blocking` boundaries (so rasterize/encode/DB steps stay nested).

## Usage

Add the dependency (git until published to crates.io):

```toml
[dependencies]
monocle-agent = { git = "https://github.com/Ninhache/monocle-agent" }
```

Initialise early in `main`, hold the guard, flush on exit:

```rust
#[tokio::main]
async fn main() {
    let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
        env!("CARGO_PKG_NAME"),    // resolves in YOUR crate, not the library
        env!("CARGO_PKG_VERSION"),
    ));

    // ... build and serve your app ...

    telemetry.shutdown();          // flush buffered telemetry before exit
}
```

With axum, name request spans and record HTTP metrics:

```rust
use tower_http::trace::TraceLayer;

let app = router
    .layer(TraceLayer::new_for_http().make_span_with(monocle_agent::MonocleMakeSpan::new()))
    .layer(axum::middleware::from_fn(monocle_agent::track_http_metrics));
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
