# monocle-agent example service

A small, runnable [axum](https://github.com/tokio-rs/axum) service wired with
[`monocle-agent`](../) — use it as a starting point, and as this repo's
end-to-end integration test.

## Use it as a base

1. Copy this directory into your project.
2. In `Cargo.toml`, swap the path dependency for a released version:
   ```toml
   monocle-agent = { version = "0.3", features = ["axum"] }
   ```
3. Build on `src/lib.rs` (`build_app`) and `src/main.rs`.

## Run

```sh
# stdout-only, export off:
cargo run
# export to Monocle:
MONOCLE_API_KEY=your-key cargo run
# then, in another shell:
curl localhost:3000/hello/world
curl localhost:3000/health
```

What it demonstrates:

- `monocle_agent::init` + graceful `shutdown` (flush on exit).
- `request_span` — request spans named `GET /hello/:name`.
- `track_http_metrics` — `http.server.request.duration`.
- `monocle_agent::counter` — a custom `greetings.served` metric.
- `spawn_blocking_in_span` — a child span nested under the request span.

## Integration test

`tests/integration.rs` starts a mock OTLP collector, points `MONOCLE_ENDPOINT`
at it, drives one request through the router, flushes, and asserts that traces
and metrics were POSTed to `/v1/traces` and `/v1/metrics` with the `x-api-key`
header:

```sh
cargo test
```
