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
//! | [`animating_flag`] | `label.__anim__prop` | Internal per-frame animation-state flags, consumed by `is_animating(&label.prop)`. Internal shape — user code spells `&label.prop` |
//! | *(syntax crate)* | `base__index` | Array actor tracks use `animatix_syntax::ast::array_actor_label` — that constructor is canonical, do not re-spell the shape |
//!
//! **Rule:** every site that constructs or looks up one of these keys goes
//! through a constructor above. Known past violations (each fixed by routing
//! through the constructor): graph.map registration/lookup drift (2026-08-26),
//! plot-param injection in scene_eval (2026-09-08). If you are about to write
//! `format!("{label}.{...}")` for an env key, add or use a constructor here.

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

/// Env key for the per-frame animation-state flag of a property. Written by
/// the frame-env property injection, read by `is_animating(&label.prop)`.
/// The `__anim__` shape is internal — user code goes through the `&`
/// reference operator, which is checked against the property registry.
pub(crate) fn animating_flag(label: &str, prop: &str) -> String {
    format!("{label}.__anim__{prop}")
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
    fn property_into_matches_dotted_injection_shape() {
        // Frame-time injections (`ring.radius_x`) and plot-parameter
        // injections (`curve.freq`) both use this shape; IR lookups and the
        // expression `label.prop` path must match it.
        let mut out = String::new();
        property_into("ring", "radius_x", &mut out);
        assert_eq!(out, "ring.radius_x");
        property_into("curve", "freq", &mut out);
        assert_eq!(out, "curve.freq");
    }

    #[test]
    fn array_member_delegates_to_syntax_constructor() {
        assert_eq!(animatix_syntax::ast::array_actor_label("bar", 0), "bar__0");
    }
}
