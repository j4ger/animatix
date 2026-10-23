use super::*;

/// Human-readable, platform-aware shortcut label for an action (e.g. `"Ctrl+S"`),
/// or `None` if the action has no binding.
pub fn shortcut_hint(
    registry: &ShortcutRegistry,
    action: &KeyboardAction,
    ctx: &Context,
) -> Option<String> {
    registry.shortcut_for(action).map(|sc| eparts::widget::format_shortcut(sc, ctx))
}

/// Human-readable, platform-aware labels for every shortcut sharing `name`.
pub fn shortcut_hints_for_name(
    registry: &ShortcutRegistry,
    name: &str,
    ctx: &Context,
) -> Vec<String> {
    registry
        .shortcuts_for_name(name)
        .into_iter()
        .map(|sc| eparts::widget::format_shortcut(sc, ctx))
        .collect()
}

/// A tooltip string with the action's shortcut appended in parentheses when one
/// exists, e.g. `tooltip_with_shortcut("Save", &KeyboardAction::Save, ctx)` ->
/// `"Save (Ctrl+S)"`. Falls back to the bare label when unbound.
pub fn tooltip_with_shortcut(
    registry: &ShortcutRegistry,
    label: &str,
    action: &KeyboardAction,
    ctx: &Context,
) -> String {
    match shortcut_hint(registry, action, ctx) {
        Some(hint) => format!("{label} ({hint})"),
        None => label.to_string(),
    }
}
