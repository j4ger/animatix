//! Registry-driven transition definitions.
//!
//! New transitions are added here; parser, renderer, and GUI auto-generate from
//! this single source of truth.

/// Metadata for a single transition type.
pub struct TransitionDef {
    /// Machine-readable identifier used in source text (e.g. `"fade"`).
    pub id: &'static str,
    /// Human-readable name shown in the GUI inspector.
    pub display_name: &'static str,
    /// Default duration in milliseconds when no explicit duration is given.
    pub default_duration_ms: u64,
    /// Shader dispatch case index (must match the switch arms in transition.wgsl).
    pub shader_case: u32,
}

/// Global registry of all supported transitions.
pub static REGISTRY: &[TransitionDef] = &[
    TransitionDef {
        id: "cut",
        display_name: "Cut",
        default_duration_ms: 0,
        shader_case: 0,
    },
    TransitionDef {
        id: "fade",
        display_name: "Fade",
        default_duration_ms: 300,
        shader_case: 1,
    },
    TransitionDef {
        id: "wipe-left",
        display_name: "Wipe Left",
        default_duration_ms: 300,
        shader_case: 2,
    },
    TransitionDef {
        id: "wipe-right",
        display_name: "Wipe Right",
        default_duration_ms: 300,
        shader_case: 3,
    },
    TransitionDef {
        id: "wipe-up",
        display_name: "Wipe Up",
        default_duration_ms: 300,
        shader_case: 4,
    },
    TransitionDef {
        id: "wipe-down",
        display_name: "Wipe Down",
        default_duration_ms: 300,
        shader_case: 5,
    },
    TransitionDef {
        id: "push-left",
        display_name: "Push Left",
        default_duration_ms: 300,
        shader_case: 6,
    },
    TransitionDef {
        id: "push-right",
        display_name: "Push Right",
        default_duration_ms: 300,
        shader_case: 7,
    },
    TransitionDef {
        id: "push-up",
        display_name: "Push Up",
        default_duration_ms: 300,
        shader_case: 8,
    },
    TransitionDef {
        id: "push-down",
        display_name: "Push Down",
        default_duration_ms: 300,
        shader_case: 9,
    },
    TransitionDef {
        id: "iris",
        display_name: "Iris",
        default_duration_ms: 600,
        shader_case: 10,
    },
    TransitionDef {
        id: "clock-wipe",
        display_name: "Clock Wipe",
        default_duration_ms: 600,
        shader_case: 11,
    },
    TransitionDef {
        id: "zoom-through",
        display_name: "Zoom Through",
        default_duration_ms: 700,
        shader_case: 12,
    },
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

/// All registered transition IDs, derived from [`REGISTRY`].
pub fn all_ids() -> &'static [&'static str] {
    static IDS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| REGISTRY.iter().map(|def| def.id).collect())
}
