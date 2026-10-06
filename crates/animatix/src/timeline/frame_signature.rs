//! The inputs a presented frame depends on, and the guard that skips a raster
//! when they have not moved.
//!
//! Evaluation is not where a frame costs: `sample` is ~47 µs and
//! `build_frame_env` ~7.4 µs against a 3.4–4.1 ms web raster
//! (`docs/performance_evaluation.md`), so the engine's existing scene cache
//! [`crate::timeline::EvalCaches`] memoizes about one percent of the work. The
//! raster is the other ninety-nine, and it is the part that repeats: a looping
//! embed renders its finished frame for the whole `hold` window — measured at
//! 13% of all ticks on the tour — and a filtered figure re-rasterizes tens of
//! milliseconds each time to produce pixels it already has.
//!
//! This module is what makes that skip legal: [`FrameSignature`] names every
//! input the presented pixels depend on, and [`FrameDedup`] remembers the last
//! one that actually reached the screen.

use std::cell::RefCell;

/// Everything a presented frame's pixels are a function of.
///
/// Two signatures that compare equal must render the same image, so a field
/// belongs here if changing it can change a pixel — and if it can change a
/// pixel *without* being in this key, the dedup is wrong and will show a stale
/// frame. `every_field_of_the_signature_can_be_detected` exists to keep that
/// honest as the type grows; the completeness argument for each field is in the
/// doc comment on that field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameSignature {
    /// Frame time, quantised to milliseconds exactly the way the scene cache
    /// quantises it (`scene_eval.rs`): the engine already treats sub-millisecond
    /// differences as the same frame, and the dedup must not be finer than the
    /// thing it is skipping.
    pub time_ms: u64,
    /// Scene dimensions the document was built for. Layout follows these, so
    /// they are *not* implied by `raster_*`: a 640×360 scene at scale 1 and a
    /// 1280×720 scene at scale 0.5 rasterize the same number of pixels and draw
    /// different pictures.
    pub scene_width: u32,
    /// See [`FrameSignature::scene_width`].
    pub scene_height: u32,
    /// The extent vello is about to rasterize (scene size × the achieved raster
    /// scale). Fill rate is this product, so it is the field that carries the
    /// embed's adaptive quality step.
    pub raster_width: u32,
    /// See [`FrameSignature::raster_width`].
    pub raster_height: u32,
    /// The surface the result is blitted onto. A canvas resize both changes the
    /// output and clears the browser's copy of it, so it must never dedup.
    pub surface_width: u32,
    /// See [`FrameSignature::surface_width`].
    pub surface_height: u32,
    /// Which document this frame belongs to. A driver bumps this whenever it
    /// builds a new one, because a freshly built [`crate::timeline::Timeline`]
    /// starts at content epoch 0 — the same value its predecessor started at,
    /// while holding different content. Without this field, swapping a scene at
    /// the same time would look like no change at all.
    pub document_generation: u64,
    /// The timeline's mutation epoch, from
    /// [`crate::timeline::Timeline::content_epoch`]. Every in-place edit of the
    /// document funnels through `Timeline::invalidate_frame_cache`, which bumps
    /// it, so this field is what keeps an edit from being deduped away.
    pub content_epoch: u64,
    /// Debug overlay flags (bounds, layout, spacing) packed into bits by the
    /// driver. They draw pixels, so a driver that can toggle them has to feed
    /// them in; a driver that never shows them (the web player) keeps this at 0.
    ///
    /// Hit-region computation is deliberately *not* here: it produces metadata,
    /// not pixels, and for a frame that dedups to the previous one the regions
    /// are by definition the ones already collected.
    pub debug_bits: u8,
}

/// Remembers the signature of the last frame that reached the screen.
///
/// Deliberately one slot, not a cache: the repetition this exists for is the
/// same frame over and over (a loop's hold window, a scene resting between
/// keyframes), and a single compared value costs no GPU memory. A multi-frame
/// LRU of *pixels* would cost a 720p RGBA frame per slot and only pay off for
/// scrubbing back and forth, which is a different problem with a different
/// answer.
#[derive(Default)]
pub struct FrameDedup {
    presented: RefCell<Option<FrameSignature>>,
}

impl FrameDedup {
    /// A dedup with nothing presented yet, so the first frame always renders.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a frame with this signature is already on screen.
    pub fn is_presented(&self, signature: &FrameSignature) -> bool {
        self.presented.borrow().as_ref() == Some(signature)
    }

    /// Record that a frame reached the screen. Call this only after a
    /// successful present: a frame that failed and was left unpresented keeps
    /// the *previous* image on screen, so recording it would claim pixels that
    /// are not there.
    pub fn record(&self, signature: FrameSignature) {
        *self.presented.borrow_mut() = Some(signature);
    }

    /// Forget what is on screen. Needed wherever the displayed content can be
    /// lost without any of the signature's inputs changing — a surface
    /// reconfigure being the one that actually happens.
    pub fn invalidate(&self) {
        *self.presented.borrow_mut() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature() -> FrameSignature {
        FrameSignature {
            time_ms: 1_500,
            scene_width: 1_280,
            scene_height: 720,
            raster_width: 1_280,
            raster_height: 720,
            surface_width: 1_100,
            surface_height: 619,
            document_generation: 3,
            content_epoch: 17,
            debug_bits: 0,
        }
    }

    /// Every field participates in the comparison, each for its own reason:
    /// mutate one in isolation and the signature must differ.
    ///
    /// Be clear about what this does and does not guard. It catches a field that
    /// stopped mattering (a type change that makes two distinct inputs compare
    /// equal, or a fixture whose base value cannot move). It cannot catch an
    /// input that was never added to the key at all — no test can enumerate what
    /// a struct does not have. That half of completeness is held by two
    /// arguments instead: every in-place document mutation funnels through
    /// `Timeline::invalidate_frame_cache`, which bumps `content_epoch` (pinned by
    /// `timeline::tests::frame_dedup`), and every other pixel-affecting setter a
    /// driver owns must either name itself in this key or call
    /// `FrameDedup::invalidate`.
    #[test]
    fn every_field_of_the_signature_can_be_detected() {
        // One assertion per field, spelled out: this is the guard a whole
        // optimization rests on, so it should read as a list of inputs, not as
        // machinery. A field added without a line here is a field the key can
        // ignore — which is exactly the failure this test exists to catch.
        let mut changed = signature();
        changed.time_ms += 1;
        assert_ne!(changed, signature(), "time_ms must change the key");

        let mut changed = signature();
        changed.scene_width += 1;
        assert_ne!(changed, signature(), "scene_width must change the key");

        let mut changed = signature();
        changed.scene_height += 1;
        assert_ne!(changed, signature(), "scene_height must change the key");

        let mut changed = signature();
        changed.raster_width += 1;
        assert_ne!(changed, signature(), "raster_width must change the key");

        let mut changed = signature();
        changed.raster_height += 1;
        assert_ne!(changed, signature(), "raster_height must change the key");

        let mut changed = signature();
        changed.surface_width += 1;
        assert_ne!(changed, signature(), "surface_width must change the key");

        let mut changed = signature();
        changed.surface_height += 1;
        assert_ne!(changed, signature(), "surface_height must change the key");

        let mut changed = signature();
        changed.document_generation += 1;
        assert_ne!(changed, signature(), "document_generation must change the key");

        let mut changed = signature();
        changed.content_epoch += 1;
        assert_ne!(changed, signature(), "content_epoch must change the key");

        let mut changed = signature();
        changed.debug_bits |= 1;
        assert_ne!(changed, signature(), "debug_bits must change the key");
    }

    #[test]
    fn an_unseen_frame_is_not_presented() {
        let dedup = FrameDedup::new();
        assert!(!dedup.is_presented(&signature()));
    }

    #[test]
    fn only_an_exact_repeat_dedups() {
        let dedup = FrameDedup::new();
        let first = signature();
        dedup.record(first.clone());
        assert!(dedup.is_presented(&first));
        let mut later = signature();
        later.time_ms += 16;
        assert!(!dedup.is_presented(&later));
    }

    #[test]
    fn invalidating_forgets_the_screen() {
        let dedup = FrameDedup::new();
        dedup.record(signature());
        dedup.invalidate();
        assert!(!dedup.is_presented(&signature()));
    }
}
