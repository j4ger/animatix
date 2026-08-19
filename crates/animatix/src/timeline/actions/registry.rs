use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::timeline::Timeline;

/// Description of a single action parameter or modifier.
pub use animatix_syntax::schema::{ActionParam, ActionSignature};

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
