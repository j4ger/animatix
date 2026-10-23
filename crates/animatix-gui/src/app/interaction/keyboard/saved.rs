use super::*;

/// A serializable shortcut override used by persisted settings.
///
/// `key` uses stable names such as `"S"`, `"Space"`, `"ArrowLeft"`, or `"/"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedShortcut {
    pub command: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: String,
}

impl SavedShortcut {
    /// Convert an egui shortcut into the persisted representation.
    pub fn from_shortcut(shortcut: &KeyboardShortcut) -> Self {
        let modifiers = shortcut.modifiers;
        Self {
            command: modifiers.command,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            key: saved_key_name(&shortcut.logical_key).unwrap_or_default().to_string(),
        }
    }

    /// Convert back into an egui shortcut, or `None` when the key is unknown.
    pub fn to_shortcut(&self) -> Option<KeyboardShortcut> {
        Some(KeyboardShortcut::new(
            Modifiers {
                command: self.command,
                ctrl: self.ctrl,
                shift: self.shift,
                alt: self.alt,
                mac_cmd: false,
            },
            parse_saved_key(&self.key)?,
        ))
    }

    /// Platform-neutral display, e.g. `Cmd+Shift+Z`.
    ///
    /// The `command` flag is shown literally as `Cmd`; use [`display_with`] for
    /// UI text so it renders as `Ctrl` on Linux/Windows.
    ///
    /// [`display_with`]: SavedShortcut::display_with
    pub fn display(&self) -> String {
        let mut out = String::new();
        if self.command {
            out.push_str("Cmd+");
        }
        if self.ctrl {
            out.push_str("Ctrl+");
        }
        if self.shift {
            out.push_str("Shift+");
        }
        if self.alt {
            out.push_str("Alt+");
        }
        out.push_str(&self.key);
        out
    }

    /// Platform-aware display for UI text: `Ctrl+S` on Linux/Windows,
    /// `⌘S` on macOS. Falls back to the neutral form for unrepresentable keys.
    pub fn display_with(&self, ctx: &Context) -> String {
        match self.to_shortcut() {
            Some(shortcut) => ctx.format_shortcut(&shortcut),
            None => self.display(),
        }
    }
}

fn saved_key_name(key: &egui::Key) -> Option<&'static str> {
    use egui::Key::*;
    Some(match key {
        A => "A",
        C => "C",
        D => "D",
        E => "E",
        F => "F",
        G => "G",
        I => "I",
        M => "M",
        P => "P",
        R => "R",
        S => "S",
        V => "V",
        Y => "Y",
        Z => "Z",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Space => "Space",
        Comma => ",",
        Period => ".",
        Slash => "/",
        Delete => "Delete",
        Backspace => "Backspace",
        Escape => "Escape",
        ArrowLeft => "ArrowLeft",
        ArrowRight => "ArrowRight",
        ArrowUp => "ArrowUp",
        ArrowDown => "ArrowDown",
        _ => return None,
    })
}

fn parse_saved_key(key: &str) -> Option<egui::Key> {
    use egui::Key::*;
    Some(match key {
        "A" => A,
        "C" => C,
        "D" => D,
        "E" => E,
        "F" => F,
        "G" => G,
        "I" => I,
        "M" => M,
        "P" => P,
        "R" => R,
        "S" => S,
        "V" => V,
        "Y" => Y,
        "Z" => Z,
        "1" => Num1,
        "2" => Num2,
        "3" => Num3,
        "Space" => Space,
        "," => Comma,
        "." => Period,
        "/" => Slash,
        "Delete" => Delete,
        "Backspace" => Backspace,
        "Escape" => Escape,
        "ArrowLeft" => ArrowLeft,
        "ArrowRight" => ArrowRight,
        "ArrowUp" => ArrowUp,
        "ArrowDown" => ArrowDown,
        _ => return None,
    })
}

pub(super) fn shortcut_hint_text(shortcut: &KeyboardShortcut) -> String {
    SavedShortcut::from_shortcut(shortcut).display()
}

/// Errors produced when applying persisted shortcut overrides.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyBindingError {
    #[error("unknown shortcut binding '{0}'")]
    UnknownBinding(String),
    #[error("invalid key '{0}' in shortcut binding")]
    InvalidKey(String),
    #[error("shortcut {0} conflicts between '{1}' and '{2}'")]
    Conflict(String, String, String),
}

/// Build a persisted shortcut from a raw key press.
///
/// Returns `None` when the key has no stable serialized name.
pub(crate) fn saved_shortcut_from_key(
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<SavedShortcut> {
    Some(SavedShortcut {
        command: modifiers.command,
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        alt: modifiers.alt,
        key: saved_key_name(&key)?.to_string(),
    })
}
