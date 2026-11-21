//! Modifier statement execution (IR interpreter).
//!
//! Modifier bodies (`always`, `drive`, reactive bindings) are lowered to IR at
//! build time and interpreted per frame here. This module is a thin wrapper
//! around [`modifier_runtime::ir::execute_modifier_ir`].

use super::modifier_runtime::ir::{ModifierGeometry, ModifierIrProgram, execute_with_geometry};
use super::{EvalError, SceneDimensions, Timeline, Value};

impl Timeline {
    /// Execute a lowered modifier IR program against the current frame environment.
    pub fn apply_modifier_program(
        &self,
        program: &ModifierIrProgram,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut super::Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        let _stage = crate::perf::ScopedStage::new(crate::perf::stage::MODIFIER_EXEC);
        let geometry = ModifierGeometry {
            timeline: self,
            time_ms,
            dimensions: scene_dimensions,
        };
        if let Some(prepared) = &self.prepared_modifiers
            && prepared.matches(program, frame_env)
        {
            return prepared.execute(frame_env, overrides, geometry);
        }
        execute_with_geometry(program, frame_env, overrides, Some(geometry))
    }
}
