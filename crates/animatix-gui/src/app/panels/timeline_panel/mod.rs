//! Global Timeline Panel
//!
//! A vertical-scrolling timeline with a scene track (for compositions),
//! per-actor tracks, a ruler, draggable playhead, loop-region overlay,
//! and a work/export range slider.
//!
//! Design:
//! ┌────────────────────────────────────────────────────┐
//! │  Ruler: 0s  1s  2s  3s  4s  5s                    │
//! ├────────┬──────────────────────────────────────────-┤
//! │ Scenes │ [──Intro───][─Diagram──][───Outro──]      │
//! ├────────┼──────────────────────────────────────────-┤
//! │ rect1  │ ◆         ◆        ◆                      │
//! │ rect2  │    ◆           ◆                          │
//! │ circle │ ◆  ◆     ◆                                │
//! ├────────┼──────────────────────────────────────────-┤
//! │ Region │ [◀────────────▶]                          │
//! └────────┴──────────────────────────────────────────-┘
//!
//! The implementation lives in [`content`]; `track_lanes`, `track_tree`,
//! `keyframes`, and `ruler` are the per-section drawing modules.

mod audio;
mod content;
pub mod keyframes;
mod marker_source;
mod markers;
mod review;
pub mod ruler;
pub mod track_lanes;
pub mod track_tree;

pub(crate) use content::{
    TimelineContext, TimelineCoords, draw_loop_region, render_timeline_content,
};
