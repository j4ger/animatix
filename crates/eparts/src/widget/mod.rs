//! Generic egui widgets — the domain-free half of the Animatix component library.
//!
//! These widgets carry no animatix-domain semantics (no scene graph, no timeline,
//! no diagnostic phases). They depend only on `egui`, `egui-phosphor`, and the
//! generic `eparts::tokens` design-token system.

pub mod anim;
pub mod button;
pub mod collapsible;
pub mod color_picker;
pub mod context_menu;
pub mod diagnostics;
pub mod dialog;
pub mod easing_curve_editor;
pub mod feedback;
pub mod form;
pub mod input;
pub mod kbd;
pub mod keymap;
pub mod label;
pub mod layout;
pub mod link;
pub mod list;
pub mod overlay;
pub mod popover;
pub mod resize;
pub mod row;
pub mod select;
pub mod slider;
pub mod spinner;
pub mod tabs;
pub mod text;
pub mod timeline;
pub mod toast;
pub mod toggle;
pub mod tooltip;
pub mod traits;
pub mod tree;

pub use collapsible::Accordion;
pub use color_picker::{
    ColorPicker, ColorPickerResponse, color_to_hex_rgb, color_to_hex_rgba, parse_hex_color,
};
pub use feedback::{Alert, AlertLevel, Badge, ProgressBar, Skeleton, Tag};
pub use form::{Field, Form};
pub use input::{NumberField, TextField};
pub use kbd::{Kbd, format_shortcut};
pub use keymap::{Binding, Keymap};
pub use label::Label;
pub use link::Link;
pub use list::{List, ListAction, ListResponse, SearchableList, SearchableListResponse};
pub use popover::Popover;
pub use resize::ResizeHandle;
pub use row::Row;
pub use select::Select;
pub use slider::Slider;
pub use spinner::Spinner;
pub use tabs::TabBar;
pub use toggle::{Checkbox, Radio, Side, Switch};
pub use tooltip::{Tooltip, text_tooltip};
pub use traits::{Collapsible, Disableable, Selectable, Sizable, Size};
pub use tree::{Tree, TreeAction, TreeId, TreeItem, TreeResponse};
