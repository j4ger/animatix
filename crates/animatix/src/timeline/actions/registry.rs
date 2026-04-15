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
