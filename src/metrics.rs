//! Ergonomic helpers to record custom application metrics without naming
//! `opentelemetry` directly.
//!
//! Each helper builds an instrument against the global meter installed by
//! [`crate::init`]. When telemetry is disabled (no `MONOCLE_API_KEY`) the global
//! meter is a no-op, so recording is a cheap no-op too — you never branch on
//! whether export is on.
//!
//! Build an instrument **once** and reuse it (instruments are cheap to record,
//! not to create):
//!
//! ```
//! use std::sync::LazyLock;
//! use monocle_agent::opentelemetry::metrics::Counter;
//!
//! static JOBS: LazyLock<Counter<u64>> =
//!     LazyLock::new(|| monocle_agent::counter("jobs.processed", "Jobs processed"));
//!
//! JOBS.add(1, &[]);
//! ```
//!
//! Need a custom instrumentation scope or an instrument type not covered here?
//! Drop down to [`crate::opentelemetry`]`::global::meter(...)`.

use opentelemetry::metrics::{Counter, Gauge, Histogram};

/// Instrumentation scope used for helper-created instruments.
const SCOPE: &str = "monocle-agent";

/// Create a monotonic `u64` counter (e.g. requests, jobs, errors).
pub fn counter(
    name: impl Into<std::borrow::Cow<'static, str>>,
    description: impl Into<std::borrow::Cow<'static, str>>,
) -> Counter<u64> {
    opentelemetry::global::meter(SCOPE)
        .u64_counter(name)
        .with_description(description)
        .build()
}

/// Create an `f64` histogram (e.g. durations, sizes). `unit` follows UCUM
/// (`"s"` for seconds, `"By"` for bytes, `""` for dimensionless).
pub fn histogram(
    name: impl Into<std::borrow::Cow<'static, str>>,
    unit: impl Into<std::borrow::Cow<'static, str>>,
    description: impl Into<std::borrow::Cow<'static, str>>,
) -> Histogram<f64> {
    opentelemetry::global::meter(SCOPE)
        .f64_histogram(name)
        .with_unit(unit)
        .with_description(description)
        .build()
}

/// Create an `f64` gauge for a last-value measurement (e.g. queue depth,
/// temperature, in-flight requests).
pub fn gauge(
    name: impl Into<std::borrow::Cow<'static, str>>,
    description: impl Into<std::borrow::Cow<'static, str>>,
) -> Gauge<f64> {
    opentelemetry::global::meter(SCOPE)
        .f64_gauge(name)
        .with_description(description)
        .build()
}
