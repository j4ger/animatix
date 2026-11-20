//! Source paragraphs and transcript selectors for narrated scenes.

mod selector;
mod source;

pub use selector::{
    AnchorEdge, AnchorKey, NarrationAnchor, TRACK, decode_anchor, encode_anchor,
    is_implicit_anchor, normalize, phrase_ranges,
};
pub(crate) use selector::{implicit_anchor, implicit_anchor_text, strip_implicit_anchor_text};
pub(crate) use source::paragraphs_with_tokens;
pub use source::{NarrationParagraph, paragraphs};
