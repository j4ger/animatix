//! Track-writing backend for standard timeline-producing functions.

pub use animatix_core::callable::CallableDescriptor;

use super::BuiltinInvocation;
use crate::diagnostics::Diagnostic;
use crate::timeline::Timeline;

/// Executes one already validated, scheduled operation against ordinary tracks.
pub trait BuiltinAction: Send + Sync {
    /// The authoritative callable contract.
    fn signature(&self) -> CallableDescriptor;
    /// Write this operation at its resolved scene time.
    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    );
}
