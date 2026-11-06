//! `EvalCaches`: per-frame evaluation caches and their plot/primitive bookkeeping.

use super::{BoundsRegistry, EvalCaches};

impl EvalCaches {
    /// Build the label registry and stamp each track's bounds slot. No-op while
    /// valid; invalidated by `invalidate_frame_cache` and `Clone`.
    pub(super) fn ensure_bounds_registry(&self, world: &crate::timeline::World) {
        if self.bounds_registry.borrow().is_some() {
            return;
        }
        let labels: Vec<String> = world.tracks.keys().cloned().collect();
        {
            let mut table = self.precise_bounds.borrow_mut();
            table.clear_frame();
            table.slots.clear();
            table.slots.resize(labels.len(), None);
        }
        for (slot, (_, id)) in world.tracks.iter().enumerate() {
            world.set_bounds_slot(id, slot as u32);
        }
        *self.bounds_registry.borrow_mut() = Some(BoundsRegistry { labels });
    }

    /// Clear the fast plot programs cache if the timeline mutation epoch has advanced.
    pub(crate) fn clear_fast_plot_programs_if_stale(&self) {
        if self.fast_plot_epoch.get() != self.content_epoch.get() {
            self.fast_plot_programs.borrow_mut().clear();
            self.fast_plot_epoch.set(self.content_epoch.get());
        }
    }

    /// Retrieve or compile a fast plot program for `key = (body_pointer, kind_or_arity)`.
    pub(crate) fn get_or_compile_fast_plot<F>(
        &self,
        key: (usize, usize),
        compile: F,
    ) -> Option<std::sync::Arc<crate::timeline::plot::FastPlotProgram>>
    where
        F: FnOnce() -> Option<crate::timeline::plot::FastPlotProgram>,
    {
        self.clear_fast_plot_programs_if_stale();
        if let Some(entry) = self.fast_plot_programs.borrow().get(&key) {
            return entry.clone();
        }
        let compiled = compile().map(std::sync::Arc::new);
        self.fast_plot_programs.borrow_mut().insert(key, compiled.clone());
        compiled
    }
}

impl Clone for EvalCaches {
    fn clone(&self) -> Self {
        Self::default()
    }
}
