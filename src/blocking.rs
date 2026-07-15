//! Span-preserving `spawn_blocking` helper.

use tracing::Span;

/// Spawn a blocking task that inherits the caller's current tracing span.
///
/// [`tokio::task::spawn_blocking`] takes a plain `FnOnce` closure, so tracing's
/// thread-local span context is **not** propagated into it — any span or event
/// created inside the closure would be detached from the caller's span in the
/// exported trace (breaking the waterfall).
///
/// This helper captures [`Span::current`] before spawning and re-enters it as
/// the first thing inside the closure, so child spans created in the blocking
/// work nest correctly under the caller's span (e.g. an HTTP request span).
///
/// ```no_run
/// # async fn demo() {
/// let out = monocle_agent::spawn_blocking_in_span(|| {
///     tracing::info_span!("encode").in_scope(|| heavy_encode())
/// })
/// .await
/// .unwrap();
/// # let _ = out;
/// # }
/// # fn heavy_encode() -> Vec<u8> { Vec::new() }
/// ```
pub fn spawn_blocking_in_span<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    tokio::task::spawn_blocking(move || {
        let _guard = span.enter();
        f()
    })
}
