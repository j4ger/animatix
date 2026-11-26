pub mod diagnostics;
mod parse_diagnostic;

pub use eparts::widget::{
    Alert, AlertLevel, Badge, TabBar, Tag, anim, button, context_menu, dialog, row, text_tooltip,
    toast,
};

mod icon_menu;
pub(crate) use icon_menu::icon_menu;
