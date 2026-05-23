use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::timeline::Timeline;

/// Description of a single action parameter or modifier.
#[derive(Debug, Clone)]
pub struct ActionParam {
    /// Parameter name as used in source code (e.g. `"ease"`, `"to"`).
    pub name: String,
    /// Human-readable description of the parameter's purpose.
    pub description: String,
    /// Expected type for documentation and UI hints (e.g. `"vec2"`, `"string"`).
    pub type_info: String,
}

/// Metadata describing a built-in action for LSP completions and UI tooltips.
#[derive(Debug, Clone)]
pub struct ActionSignature {
    /// Action verb as written in source (e.g. `"fade-in"`, `"move"`).
    pub name: String,
    /// High-level grouping for UI organization (e.g. `"Entrance"`, `"Motion"`).
    pub category: String,
    /// One-line explanation of what the action does.
    pub description: String,
    /// Positional arguments accepted by the action.
    pub params: Vec<ActionParam>,
    /// Named modifiers accepted by the action.
    pub modifiers: Vec<ActionParam>,
}

/// Trait implemented by every built-in timeline action.
pub trait BuiltinAction: Send + Sync {
    /// Returns the action's metadata signature.
    fn signature(&self) -> ActionSignature;
    /// Executes the action against the timeline at the given time.
    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    );
}

/// Common timing modifier parameters shared by all actions.
pub fn base_timing_params() -> Vec<ActionParam> {
    vec![
        ActionParam {
            name: "ease".to_string(),
            description: "Easing function for the animation".to_string(),
            type_info: "string".to_string(),
        },
        ActionParam {
            name: "duration-shorthand".to_string(),
            description: "Bare positional duration shorthand in brackets (e.g. [1s], [500ms])"
                .to_string(),
            type_info: "positional time literal".to_string(),
        },
        ActionParam {
            name: "delay".to_string(),
            description: "Delay before the action starts (e.g. [delay: 250ms])".to_string(),
            type_info: "time literal".to_string(),
        },
    ]
}
