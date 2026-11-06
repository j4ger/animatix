//! Dense per-frame precise-bounds storage (`BoundsTable`).

use super::BoundsTable;

impl BoundsTable {
    /// Record bounds for `slot`, appending to [`Self::written`] on first write.
    pub(super) fn write(&mut self, slot: u32, rect: kurbo::Rect) {
        let index = slot as usize;
        if index >= self.slots.len() {
            // Only reachable if tracks changed without `invalidate_frame_cache`
            // (every public mutator funnels through it). Log rather than panic.
            tracing::warn!("bounds slot {slot} outside registry; bounds dropped for this frame");
            return;
        }
        if self.slots[index].is_none() {
            self.written.push(slot);
        }
        self.slots[index] = Some(rect);
    }

    /// Clear the current frame's entries. Sparse: touches only written slots.
    pub(super) fn clear_frame(&mut self) {
        for &slot in &self.written {
            self.slots[slot as usize] = None;
        }
        self.written.clear();
    }

    /// `(slot, bounds)` pairs for recorded entries from `written[from..]`. Pre-sized
    /// so the frame-end stash is one allocation.
    pub(super) fn pairs_from(&self, from: usize) -> Vec<(u32, kurbo::Rect)> {
        let mut pairs = Vec::with_capacity(self.written.len() - from);
        pairs.extend(self.written[from..].iter().filter_map(|&slot| {
            let rect = self.slots[slot as usize]?;
            Some((slot, rect))
        }));
        pairs
    }
}
