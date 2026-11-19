//! Access to the process-wide WebGPU context shared by every player.

use std::cell::{Cell, RefCell};

use super::EngineContext;

thread_local! {
    /// wasm runs single-threaded, so a `thread_local` slot is the safe home
    /// for the non-`Sync` vello renderer; every player method reaches the
    /// context through [`with_engine`].
    pub(super) static CONTEXT: RefCell<Option<EngineContext>> = const { RefCell::new(None) };
    static NEXT_GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Allocate the generation for a newly initialized WebGPU device.
pub(super) fn next_generation() -> u64 {
    NEXT_GENERATION.with(|generation| {
        let next = generation.get().wrapping_add(1).max(1);
        generation.set(next);
        next
    })
}

/// Drop only the context whose loss callback fired. A late callback from an
/// older device must never clear a replacement context.
pub(super) fn clear_if_generation(generation: u64) -> bool {
    CONTEXT.with(|cell| {
        let mut context = cell.borrow_mut();
        if context.as_ref().is_some_and(|current| current.generation == generation) {
            *context = None;
            true
        } else {
            false
        }
    })
}

/// Run `f` with the shared context (or the reason it is missing).
pub(super) fn with_engine<R>(f: impl FnOnce(Result<&EngineContext, String>) -> R) -> R {
    CONTEXT.with(|cell| {
        let ctx = cell.borrow();
        f(ctx.as_ref().ok_or_else(|| "engine not initialized".to_string()))
    })
}
