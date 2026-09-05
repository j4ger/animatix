//! Central definitions for the environment key conventions.
//!
//! The build/eval env is a flat `String → Value` map, but several distinct
//! key conventions coexist inside it. Every site that constructs or looks up
//! one of these keys MUST go through the constructors below — the
//! `graph.map`-in-`always` bug (fixed 2026-08-26) happened because the
//! registration site and the lookup site each spelled the key their own way.
//!
//! | Constructor | Shape | Used for |
//! |---|---|---|
//! | [`native_fn`] | `label.method` | Graph NativeFns (`g.map`, `g.map_inverse`) registered at build and resolved by modifier-IR method calls |
//! | [`property_into`] | `label.property` | Frame-time property injections (`ring.radius_x`, ...) |
//! | [`side_channel`] | `label_prop` | Build-time side-channel values (`g_size`) read back by hosted-plot children |
//! | *(syntax crate)* | `base__index` | Array actor tracks use `animatix_syntax::ast::array_actor_label` — that constructor is canonical, do not re-spell the shape |

/// Env key for a NativeFn registered on a label (`g.map`, `g.map_inverse`).
/// Modifier-IR method calls with a plain path/ident receiver join the
/// receiver parts with [`native_fn`]'s shape — keep both on this function.
pub(crate) fn native_fn(label: &str, method: &str) -> String {
    format!("{label}.{method}")
}

/// Env key for an injected frame-time property (`ring.radius_x`), written
/// into a reusable buffer (PF-6: the frame-env injection path builds every
/// key through this so the steady-state frame performs zero key allocations
/// — `out` is cleared, never read before write). Callers append sub-key
/// suffixes (`.x`, `.y`, …) to the same buffer. The key shape is
/// `{label}.{prop}` — modifier-IR and expression lookups must match it.
pub(crate) fn property_into(label: &str, prop: &str, out: &mut String) {
    out.clear();
    out.push_str(label);
    out.push('.');
    out.push_str(prop);
}

/// Env key for a build-time side-channel value (`g_size`).
pub(crate) fn side_channel(label: &str, prop: &str) -> String {
    format!("{label}_{prop}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_fn_matches_ir_lowering_join() {
        // The modifier-IR lowering of `g.map(a, b)` joins the receiver parts
        // with '.' — the result must equal the registration key.
        let mut parts = vec!["g".to_string()];
        parts.push("map".to_string());
        assert_eq!(parts.join("."), native_fn("g", "map"));
        assert_eq!(native_fn("descent_graph", "map"), "descent_graph.map");
    }

    #[test]
    fn side_channel_matches_plot_registration() {
        assert_eq!(side_channel("g", "size"), "g_size");
    }

    #[test]
    fn array_member_delegates_to_syntax_constructor() {
        assert_eq!(animatix_syntax::ast::array_actor_label("bar", 0), "bar__0");
    }
}
