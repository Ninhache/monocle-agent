# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Releases are cut automatically: bumping `version` in `Cargo.toml` on `main`
creates the matching `vX.Y.Z` tag and GitHub release (see
[`CONTRIBUTING.md`](./CONTRIBUTING.md)).

## [Unreleased]

## [0.1.0]

### Added

- `init(MonocleConfig)` — one-call setup of the `tracing` subscriber plus OTLP/HTTP
  export of traces, metrics and logs to Monocle. Off unless `MONOCLE_API_KEY` is set.
- `MonocleConfig` with `from_env` + `with_*` builders.
- `TelemetryGuard::shutdown` to flush buffered telemetry on exit.
- Axum feature: `MonocleMakeSpan` (named request spans), `track_http_metrics`
  (`http.server.request.duration`), and `spawn_blocking_in_span`.

[Unreleased]: https://github.com/Ninhache/monocle-agent/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Ninhache/monocle-agent/releases/tag/v0.1.0
