//! Generic egui widgets — the domain-free half of the Animatix component library.
//!
//! These widgets carry no animatix-domain semantics (no scene graph, no timeline,
//! no diagnostic phases). They depend only on `egui`, `egui-phosphor`, and the
//! generic `eparts::tokens` design-token system.

pub mod feedback;
pub mod slider;
pub mod select;
pub mod anim;
pub mod overlay;
pub mod button;
pub mod context_menu;
pub mod diagnostics;
pub mod dialog;
pub mod easing_curve_editor;
pub mod kbd;
pub mod label;
pub mod layout;
pub mod collapsible;
pub mod resize;
pub mod form;
pub mod input;
pub mod row;
pub mod spinner;
pub mod tabs;
pub mod text;
pub mod timeline;
pub mod toast;
pub mod tooltip;
pub mod popover;
pub mod traits;
pub mod link;

pub use traits::{Collapsible, Disableable, Selectable, Size, Sizable};
pub use spinner::Spinner;
pub use tooltip::{Tooltip, text_tooltip};
pub use popover::Popover;
pub use kbd::{Kbd, format_shortcut};
pub use label::Label;
pub use form::{Field, Form};
pub use input::{NumberField, TextField};
pub use slider::Slider;
pub use select::Select;
pub use resize::ResizeHandle;
pub use collapsible::Accordion;
pub use tabs::TabBar;
pub use link::Link;
pub use feedback::{Skeleton, ProgressBar, Badge, Tag, Alert, AlertLevel};
