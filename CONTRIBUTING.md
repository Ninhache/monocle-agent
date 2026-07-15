# Contributing

Thanks for helping improve `monocle-agent`.

## Development

```sh
cargo build --all-features
cargo test  --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
```

You need `libssl-dev` + `pkg-config` for the native-tls build (already present in
the CI image; install locally on Debian/Ubuntu with
`sudo apt-get install -y libssl-dev pkg-config`).

## Branching & merging

`main` is protected. All changes land through a pull request:

1. Branch off `main` (`feat/…`, `fix/…`, `docs/…`).
2. Open a PR. CI (fmt, clippy, build, test) must be green before merge.
3. Squash-merge into `main`.

Direct pushes to `main` are rejected — this keeps the release automation
(below) honest.

## Releasing

Releases are **automatic**. To cut one:

1. Bump `version` in `Cargo.toml` (follow [SemVer](https://semver.org/)).
2. Add a section to [`CHANGELOG.md`](./CHANGELOG.md).
3. Merge to `main`.

On merge, the `release` workflow reads the version, and if no `vX.Y.Z` tag
exists yet it creates the tag and a GitHub release with generated notes. The
`docs` workflow rebuilds and publishes the API docs to GitHub Pages.

Nothing to tag by hand — just bump the version.

## Style

- Public items are documented (`#![warn(missing_docs)]`-friendly).
- Comments explain *why*, not *what*.
- Keep the vendor-specific bits (Monocle endpoint/headers) in `config.rs` /
  `providers.rs`; keep the generic OTLP wiring reusable.
