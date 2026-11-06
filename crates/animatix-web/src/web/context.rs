//! Access to the process-wide WebGPU context shared by every player.

use std::cell::RefCell;

use super::EngineContext;

thread_local! {
    /// wasm runs single-threaded, so a `thread_local` slot is the safe home
    /// for the non-`Sync` vello renderer; every player method reaches the
    /// context through [`with_engine`].
    pub(super) static CONTEXT: RefCell<Option<EngineContext>> = const { RefCell::new(None) };
}

/// Run `f` with the shared context (or the reason it is missing).
pub(super) fn with_engine<R>(f: impl FnOnce(Result<&EngineContext, String>) -> R) -> R {
    CONTEXT.with(|cell| {
        let ctx = cell.borrow();
        f(ctx.as_ref().ok_or_else(|| "engine not initialized".to_string()))
    })
}
