//! Shared hierarchical stage tracing (PF-8).
//!
//! One lightweight, always-available instrumentation layer so the Criterion
//! benches, the GUI HUD, and future perf reporting measure the *same* stages
//! instead of re-deriving timings. See `docs/performance_evaluation.md`
//! §3.5 (design) and §7 (cost-of-instrumentation constraints).
//!
//! Semantics:
//! - [`ScopedStage::new`] pushes `(name, Instant)` onto a thread-local stack;
//!   the [`Drop`] impl pops it and accumulates the elapsed nanoseconds into a
//!   thread-local ledger keyed by stage name (nested instances of the same
//!   stage sum into one entry).
//! - [`take_measurements`] drains the ledger. Callers (GUI HUD frame tick,
//!   tests, future perf sinks) own the drained values.
//! - The hot path pays only a thread-local push/pop plus a bounded linear scan
//!   over at most [`MAX_LEDGER_ENTRIES`] entries — no allocation on push, pop,
//!   or drop. The ledger is only allowed to grow to its fixed cap; stage kinds
//!   beyond the cap are intentionally dropped (bounded memory by design; the
//!   canonical set in [`stage`] is well under the cap).
//!
//! Compile-time gating: the `perf-tracing` feature (default-on) provides the
//! real implementation. Without it, [`ScopedStage`] is a zero-sized no-op and
//! [`take_measurements`] returns an empty vec, so call sites stay identical
//! across both configurations (this is what the CI
//! `--no-default-features --features render,text,svg` build compiles).

// Only the no-op stub below needs `Duration` when tracing is compiled out;
// the real implementation imports it inside `imp`.
#[cfg(not(feature = "perf-tracing"))]
use std::time::Duration;

/// Maximum nested [`ScopedStage`] depth tracked per thread. Pushes beyond this
/// depth are not tracked (the tracer never panics or grows unboundedly).
pub const MAX_STACK_DEPTH: usize = 64;

/// Maximum distinct stage names retained in the per-thread ledger between
/// drains. The canonical set in [`stage`] is well under this cap.
pub const MAX_LEDGER_ENTRIES: usize = 64;

/// Canonical stage names, shared verbatim by benches, the GUI HUD, and perf
/// reporting so reports read as the same measured pipeline.
pub mod stage {
    /// `Timeline::build` seam (parse→expand happen upstream; this covers the
    /// build layer itself).
    pub const REBUILD: &str = "rebuild";
    /// Per-frame evaluation environment construction.
    pub const BUILD_FRAME_ENV: &str = "build_frame_env";
    /// Per-frame property sampling + node evaluation into the scene.
    pub const SAMPLE: &str = "sample";
    /// Taffy-based linear layout resolution.
    pub const LAYOUT: &str = "layout";
    /// Modifier IR execution per frame.
    pub const MODIFIER_EXEC: &str = "modifier_exec";
    /// Reserved for the scene-encode seam (currently interleaved with
    /// sampling inside node evaluation; split out when it shows up as a cost).
    pub const ENCODE_SCENE: &str = "encode_scene";
    /// GPU rasterization of an encoded scene (`Renderer::render_vello_scene*`).
    pub const RASTERIZE: &str = "rasterize";
    /// Reserved for the export-pipeline seam (PF-7 Layer-3 work).
    pub const EXPORT: &str = "export";
}

/// Whether the real tracer is compiled in (`perf-tracing` feature, default-on).
pub fn is_enabled() -> bool {
    cfg!(feature = "perf-tracing")
}

/// RAII stage timer: measures from construction to drop on the current thread.
///
/// Stage names must be `&'static str` — the canonical [`stage`] constants —
/// so recording never allocates.
#[cfg(feature = "perf-tracing")]
#[must_use = "a ScopedStage dropped immediately measures nothing useful"]
pub struct ScopedStage {
    name: &'static str,
    start: std::time::Instant,
}

#[cfg(feature = "perf-tracing")]
mod imp {
    use std::cell::RefCell;
    use std::time::{Duration, Instant};

    thread_local! {
        /// Open-stage stack: `(name, start)` pairs, pre-reserved to the cap so
        /// push/pop on the hot path never allocate.
        static STACK: RefCell<Vec<(&'static str, Instant)>> =
            RefCell::new(Vec::with_capacity(super::MAX_STACK_DEPTH));
        /// Accumulated nanoseconds per stage name since the last drain.
        static LEDGER: RefCell<Vec<(&'static str, u128)>> =
            RefCell::new(Vec::with_capacity(super::MAX_LEDGER_ENTRIES));
    }

    impl super::ScopedStage {
        /// Start measuring `name` on the current thread until this guard drops.
        #[inline]
        pub fn new(name: &'static str) -> Self {
            STACK.with(|stack| {
                let mut stack = stack.borrow_mut();
                if stack.len() < super::MAX_STACK_DEPTH {
                    stack.push((name, Instant::now()));
                }
                // Over-depth stages are intentionally untracked: the tracer
                // must never panic or grow unboundedly (see MAX_STACK_DEPTH).
            });
            Self {
                name,
                start: Instant::now(),
            }
        }
    }

    impl Drop for super::ScopedStage {
        #[inline]
        fn drop(&mut self) {
            let elapsed = self.start.elapsed().as_nanos();
            let name = self.name;
            STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
            LEDGER.with(|ledger| {
                let mut ledger = ledger.borrow_mut();
                for entry in ledger.iter_mut() {
                    if entry.0 == name {
                        entry.1 += elapsed;
                        return;
                    }
                }
                if ledger.len() < super::MAX_LEDGER_ENTRIES {
                    ledger.push((name, elapsed));
                }
                // Ledger full: extra stage kinds are dropped on purpose —
                // bounded memory; the canonical `stage` set fits comfortably.
            });
        }
    }

    /// Drain the current thread's ledger, resetting it to empty.
    pub fn take_measurements() -> Vec<(String, Duration)> {
        LEDGER.with(|ledger| {
            std::mem::take(&mut *ledger.borrow_mut())
                .into_iter()
                // u128 nanos → u64: truncation needs ~584 years of accumulation
                // on a single stage between drains.
                .map(|(name, nanos)| (name.to_string(), Duration::from_nanos(nanos as u64)))
                .collect()
        })
    }
}

#[cfg(feature = "perf-tracing")]
pub use imp::take_measurements;

/// No-op variant used when the `perf-tracing` feature is compiled out; keeps
/// call sites identical across feature configurations.
#[cfg(not(feature = "perf-tracing"))]
#[derive(Debug)]
pub struct ScopedStage(());

#[cfg(not(feature = "perf-tracing"))]
impl ScopedStage {
    /// No-op: measurements are unavailable without the `perf-tracing` feature.
    #[inline]
    pub fn new(_name: &'static str) -> Self {
        Self(())
    }
}

/// No-op drain: always empty without the `perf-tracing` feature.
#[cfg(not(feature = "perf-tracing"))]
pub fn take_measurements() -> Vec<(String, Duration)> {
    Vec::new()
}

#[cfg(all(test, feature = "perf-tracing"))]
mod tests {
    use super::stage;
    use std::time::Duration;

    use super::{MAX_STACK_DEPTH, ScopedStage, take_measurements};

    #[test]
    fn scopes_accumulate_and_drain() {
        // Reset any prior accumulation on this thread.
        let _ = take_measurements();

        {
            let _outer = ScopedStage::new(stage::BUILD_FRAME_ENV);
            {
                let _inner = ScopedStage::new(stage::MODIFIER_EXEC);
            }
            let _second = ScopedStage::new(stage::MODIFIER_EXEC);
        }

        let measurements = take_measurements();
        let find = |name: &str| measurements.iter().find(|(n, _)| n == name).map(|(_, d)| *d);
        let outer = find(stage::BUILD_FRAME_ENV).expect("outer stage recorded");
        let inner = find(stage::MODIFIER_EXEC).expect("inner stage recorded");
        assert!(outer > Duration::ZERO, "outer stage must accumulate");
        assert!(inner > Duration::ZERO, "inner stage must accumulate");
        // Draining resets the ledger.
        assert!(take_measurements().is_empty());
    }

    #[test]
    fn same_stage_nests_into_one_entry() {
        let _ = take_measurements();
        {
            let _a = ScopedStage::new(stage::SAMPLE);
            let _b = ScopedStage::new(stage::SAMPLE);
        }
        let measurements = take_measurements();
        let sample_entries = measurements.iter().filter(|(n, _)| n == stage::SAMPLE).count();
        assert_eq!(sample_entries, 1, "nested same-name stages merge into one entry");
    }

    #[test]
    fn over_depth_stages_do_not_panic() {
        let _ = take_measurements();
        // Exceed MAX_STACK_DEPTH; the tracer must neither panic nor misreport
        // the stages it did track.
        let _guards: Vec<_> =
            (0..MAX_STACK_DEPTH + 8).map(|_| ScopedStage::new(stage::REBUILD)).collect();
        drop(_guards);
        assert!(take_measurements().iter().all(|(n, _)| n == stage::REBUILD));
    }
}
