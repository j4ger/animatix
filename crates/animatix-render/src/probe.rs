//! Opt-in timing probes for the filter-scope render path.
//!
//! The clock is gated exactly like the output: `std::time::Instant::now()`
//! traps on `wasm32-unknown-unknown`, and a probe is additive or it does not
//! land (see `docs/ecs.md` and the history archive).

/// Whether this process asked for the probe. Read once: these calls sit on the
/// per-scope render path, and `std::env::var_os` locks the environment on every
/// lookup, so deciding per frame would put a mutex on the hot path for a knob
/// that cannot change after start-up.
static TIMING_ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    !cfg!(target_arch = "wasm32") && std::env::var_os("ANIMATIX_FILTER_TIMING").is_some()
});

/// Start a probe timer, or return `None` when the probe cannot run.
///
/// The printing at every call site was env-gated, but the *clock* was not:
/// `std::time::Instant::now()` traps on wasm32-unknown-unknown with "time not
/// implemented on this platform", and the scopes these probes sit on —
/// `Filter`, `Mask`, `Glass` — are exactly the ones the web player renders, so
/// a browser frame with a backdrop scope panicked inside the engine and left
/// the player unable to render for the rest of the page's life. A probe is
/// additive or it does not land: the clock is now as conditional as the output.
pub(crate) fn timing_probe() -> Option<std::time::Instant> {
    if *TIMING_ON {
        Some(std::time::Instant::now())
    } else {
        None
    }
}

/// Milliseconds since a [`timing_probe`] start; `0.0` for a probe that was off,
/// which is never printed.
pub(crate) fn probe_ms(start: Option<std::time::Instant>) -> f64 {
    start.map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0)
}
