//! Generic, action-type-agnostic keyboard map.
//!
//! `Keymap<A>` is the reusable framework core of a scoped shortcut system: it
//! stores `(KeyboardShortcut, action, scope)` bindings where the action type
//! `A` and the scope type `S` are supplied by the consuming app. The app
//! defines its own action enum (e.g. `Save`, `Undo`) and its own scope gating;
//! eparts only provides the storage, matching, and reverse-lookup machinery.
//!
//! This mirrors the app-side `ShortcutRegistry` pattern but carries no
//! domain-specific actions, so any app can reuse it.
//!
//! # Example
//! ```ignore
//! #[derive(Clone, Copy, PartialEq)]
//! enum Action { Save, Undo }
//! #[derive(Clone, Copy, PartialEq)]
//! enum Scope { Global, TextSafe }
//!
//! let mut km = Keymap::new();
//! km.bind(KeyboardShortcut::new(Modifiers::COMMAND, Key::S), Action::Save, Scope::Global);
//!
//! // Each frame, with a predicate that decides whether a scope is currently active:
//! if let Some(action) = km.check(ctx, |scope| matches!(scope, Scope::Global) || !text_focused) {
//!     // dispatch `action`
//! }
//! ```

use egui::{Context, KeyboardShortcut};

/// One registered binding: a shortcut, the action it triggers, and its scope.
#[derive(Clone, Debug)]
pub struct Binding<A, S> {
    pub shortcut: KeyboardShortcut,
    pub action: A,
    pub scope: S,
}

/// A generic keymap: bindings of shortcut → action, gated by an app-defined scope.
///
/// `A` is the app's action type, `S` is the app's scope type. Neither needs any
/// trait bound for storage; matching/lookup use the shortcut and a caller-supplied
/// scope predicate, so `A`/`S` only need the bounds the caller's own logic requires.
#[derive(Clone, Debug, Default)]
pub struct Keymap<A, S> {
    bindings: Vec<Binding<A, S>>,
}

impl<A, S> Keymap<A, S> {
    /// Create an empty keymap.
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Register a binding.
    pub fn bind(&mut self, shortcut: KeyboardShortcut, action: A, scope: S) {
        self.bindings.push(Binding {
            shortcut,
            action,
            scope,
        });
    }

    /// All registered bindings (e.g. to render a cheat sheet).
    pub fn bindings(&self) -> &[Binding<A, S>] {
        &self.bindings
    }

    /// Number of bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether the keymap has no bindings.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

impl<A: Clone, S: Clone> Keymap<A, S> {
    /// Check for a pressed-and-consumed shortcut this frame, returning its action.
    ///
    /// `scope_allows` decides whether a binding's scope is currently active (the
    /// app supplies its focus/context logic here). The shortcut is consumed via
    /// `egui`'s input only when the scope allows, so a gated-out binding does not
    /// swallow the key.
    pub fn check(&self, ctx: &Context, mut scope_allows: impl FnMut(&S) -> bool) -> Option<A> {
        for b in &self.bindings {
            if scope_allows(&b.scope) && ctx.input_mut(|i| i.consume_shortcut(&b.shortcut)) {
                return Some(b.action.clone());
            }
        }
        None
    }

    /// Reverse lookup: the first shortcut bound to an action matching `pred`.
    ///
    /// The caller supplies the equality predicate so this works for action types
    /// that cannot derive `Eq`/`Hash` (e.g. those carrying `f32` payloads) —
    /// match by discriminant or any custom rule.
    pub fn shortcut_for(&self, mut pred: impl FnMut(&A) -> bool) -> Option<&KeyboardShortcut> {
        self.bindings.iter().find(|b| pred(&b.action)).map(|b| &b.shortcut)
    }
}

#[cfg(test)]
mod tests {
    use egui::{Context, Key, Modifiers};

    use super::*;

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Action {
        Save,
        Undo,
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Scope {
        Global,
        TextSafe,
    }

    fn save_shortcut() -> KeyboardShortcut {
        KeyboardShortcut::new(Modifiers::COMMAND, Key::S)
    }

    #[test]
    fn bind_and_len() {
        let mut km: Keymap<Action, Scope> = Keymap::new();
        assert!(km.is_empty());
        km.bind(save_shortcut(), Action::Save, Scope::Global);
        km.bind(KeyboardShortcut::new(Modifiers::COMMAND, Key::Z), Action::Undo, Scope::TextSafe);
        assert_eq!(km.len(), 2);
        assert!(!km.is_empty());
    }

    #[test]
    fn shortcut_for_reverse_lookup() {
        let mut km: Keymap<Action, Scope> = Keymap::new();
        km.bind(save_shortcut(), Action::Save, Scope::Global);
        let found = km.shortcut_for(|a| *a == Action::Save);
        assert_eq!(found, Some(&save_shortcut()));
        assert!(km.shortcut_for(|a| *a == Action::Undo).is_none());
    }

    #[test]
    fn check_respects_scope_gate() {
        let ctx = Context::default();
        let mut km: Keymap<Action, Scope> = Keymap::new();
        km.bind(save_shortcut(), Action::Save, Scope::Global);

        // Inject the Save shortcut press.
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: Key::S,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            });
        });

        // Scope gated OUT -> no action, shortcut not consumed.
        let blocked = km.check(&ctx, |_| false);
        assert_eq!(blocked, None);

        // Scope allowed -> action returned.
        let allowed = km.check(&ctx, |_| true);
        assert_eq!(allowed, Some(Action::Save));
    }
}
