//! A non-web worker wired with monocle-agent — no axum, no async code.
//!
//! Doubles as a compile check that the default (feature-less) core is usable on
//! its own: `init`, spans via the re-exported `tracing`, custom metrics via the
//! `counter`/`histogram` helpers, and a graceful `shutdown`.
//!
//! Run with: `cargo run --example worker`

use std::sync::LazyLock;
use std::time::Instant;

use monocle_agent::opentelemetry::metrics::{Counter, Histogram};

// Build instruments once, reuse them for every measurement.
static JOBS: LazyLock<Counter<u64>> =
    LazyLock::new(|| monocle_agent::counter("jobs.processed", "Jobs processed"));
static JOB_DURATION: LazyLock<Histogram<f64>> =
    LazyLock::new(|| monocle_agent::histogram("job.duration", "s", "Per-job processing time"));

fn main() {
    // Off unless MONOCLE_API_KEY is set. env! resolves in THIS crate.
    let telemetry = monocle_agent::init(monocle_agent::MonocleConfig::from_env(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    ));

    for id in 0..5 {
        // Each job is a span; child work would nest underneath it in the trace.
        monocle_agent::tracing::info_span!("process_job", job.id = id).in_scope(|| {
            let started = Instant::now();
            do_work(id);
            JOB_DURATION.record(started.elapsed().as_secs_f64(), &[]);
            JOBS.add(1, &[]);
        });
    }

    // Flush buffered spans/metrics/logs before exit.
    telemetry.shutdown();
}

fn do_work(id: u64) {
    monocle_agent::tracing::debug!(job.id = id, "working");
}
