//! Generic egui widgets — the domain-free half of the Animatix component library.
//!
//! These widgets carry no animatix-domain semantics (no scene graph, no timeline,
//! no diagnostic phases). They depend only on `egui`, `egui-phosphor`, and the
//! generic `eparts::tokens` design-token system.

pub mod anim;
pub mod overlay;
pub mod button;
pub mod context_menu;
pub mod diagnostics;
pub mod dialog;
pub mod easing_curve_editor;
pub mod kbd;
pub mod layout;
pub mod row;
pub mod spinner;
pub mod text;
pub mod timeline;
pub mod toast;
pub mod tooltip;
pub mod popover;
pub mod traits;

pub use traits::{Collapsible, Disableable, Selectable, Size, Sizable};
pub use spinner::Spinner;
pub use tooltip::{Tooltip, text_tooltip};
pub use popover::Popover;
pub use kbd::{Kbd, format_shortcut};
