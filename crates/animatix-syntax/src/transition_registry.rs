//! Registry-driven transition definitions.
//!
//! New transitions are added here; parser, renderer, and GUI auto-generate from
//! this single source of truth.

/// Metadata for a single transition type.
pub struct TransitionDef {
    pub id: &'static str,
    pub display_name: &'static str,
    pub default_duration_ms: u64,
    /// Shader dispatch case index (must match the switch arms in transition.wgsl).
    pub shader_case: u32,
}

/// Global registry of all supported transitions.
pub static REGISTRY: &[TransitionDef] = &[
    TransitionDef { id: "cut",        display_name: "Cut",        default_duration_ms: 0,   shader_case: 0 },
    TransitionDef { id: "fade",       display_name: "Fade",       default_duration_ms: 300, shader_case: 1 },
    TransitionDef { id: "wipe-left",  display_name: "Wipe Left",  default_duration_ms: 300, shader_case: 2 },
    TransitionDef { id: "wipe-right", display_name: "Wipe Right", default_duration_ms: 300, shader_case: 3 },
    TransitionDef { id: "wipe-up",    display_name: "Wipe Up",    default_duration_ms: 300, shader_case: 4 },
    TransitionDef { id: "wipe-down",  display_name: "Wipe Down",  default_duration_ms: 300, shader_case: 5 },
];

/// Look up a transition definition by its ID.
pub fn find(id: &str) -> Option<&'static TransitionDef> {
    REGISTRY.iter().find(|def| def.id == id)
}

/// Get the shader case for a transition ID, or 0 (cut) if unknown.
pub fn shader_case(id: &str) -> u32 {
    find(id).map(|d| d.shader_case).unwrap_or(0)
}

/// Get the display name for a transition ID.
pub fn display_name(id: &str) -> &'static str {
    find(id).map(|d| d.display_name).unwrap_or("unknown")
}

/// All registered transition IDs.
pub fn all_ids() -> &'static [&'static str] {
    &[
        "cut", "fade", "wipe-left", "wipe-right", "wipe-up", "wipe-down",
    ]
}
