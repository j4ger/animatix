use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::timeline::Timeline;

#[derive(Debug, Clone)]
pub struct ActionParam {
    pub name: String,
    pub description: String,
    pub type_info: String,
}

#[derive(Debug, Clone)]
pub struct ActionSignature {
    pub name: String,
    pub category: String,
    pub description: String,
    pub params: Vec<ActionParam>,
    pub modifiers: Vec<ActionParam>,
}

pub trait BuiltinAction: Send + Sync {
    fn signature(&self) -> ActionSignature;
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
