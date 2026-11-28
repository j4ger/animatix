//! Compile a component's appearance plan once and sample its isolated local clock.
use crate::ast::{Expr, Stmt};
use crate::scene::VectorArtwork;
use crate::timeline::plan_value::PlanOp;
use crate::timeline::{Environment, SceneDimensions, Timeline, Value, evaluate_expr};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub(super) struct ParticleClip {
    duration_ms: f64,
    sampler: Arc<Mutex<Sampler>>,
}

struct Sampler {
    local: Timeline,
    dims: SceneDimensions,
}

// Reuse exact poses even for non-adjacent particles while preserving paint order.
// The frame owns these poses already. Drop the lookup when the frame is complete.
pub(super) struct ClipFrame<'a> {
    sampler: MutexGuard<'a, Sampler>,
    duration_ms: f64,
    poses: HashMap<u64, Option<Arc<VectorArtwork>>>,
}

impl std::fmt::Debug for ParticleClip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParticleClip").field("duration_ms", &self.duration_ms).finish()
    }
}

impl ParticleClip {
    pub(super) fn compile(
        mut local: Timeline,
        expr: &Expr,
        author_env: &Environment,
        dims: SceneDimensions,
    ) -> Result<Self, String> {
        let Value::Closure(params, body, captures) =
            evaluate_expr(expr, author_env).map_err(|err| format!("particle_animation: {err}"))?
        else {
            return Err("particle_animation expects a closure taking the component instance".into());
        };
        let env = local.build_eval_env(0);
        let actor = env
            .get(animatix_syntax::module::PARTICLE_TEMPLATE_ROOT)
            .ok_or("missing particle_shape instance")?;
        let value = crate::timeline::utils::invoke_closure_value(
            &params,
            &body,
            &captures,
            vec![actor],
            "particle_animation",
            &env,
            None,
        )
        .map_err(|err| format!("particle_animation: {err}"))?;
        let Value::TimelinePlan(plan) = value else {
            return Err("particle_animation must return a Timeline".into());
        };
        local.compiling_particle_clip = true;
        let mut diagnostics = Vec::new();
        let extent = local.mount_plan(&plan, 0., None, 1., None, &mut diagnostics);
        if let Some(d) = diagnostics.iter().find(|d| d.is_error()) {
            return Err(format!("particle_animation: {}", d.message));
        }
        // Build-time lookups can cache transform state before all keys are mounted.
        local.invalidate_frame_cache();
        let duration_ms = extent
            .filter(|ms| ms.is_finite() && *ms > 0.)
            .ok_or("particle_animation requires a positive finite duration")?;
        Ok(Self {
            duration_ms,
            sampler: Arc::new(Mutex::new(Sampler { local, dims })),
        })
    }

    pub(super) fn frame(&self) -> Result<ClipFrame<'_>, String> {
        let sampler = self.sampler.lock().map_err(|_| "particle clip sampler lock poisoned")?;
        Ok(ClipFrame {
            sampler,
            duration_ms: self.duration_ms,
            poses: HashMap::new(),
        })
    }
}

impl ClipFrame<'_> {
    pub(super) fn sample(&mut self, progress: f64) -> Result<Option<Arc<VectorArtwork>>, String> {
        let ms = crate::time::sample_tick_millis(self.duration_ms * progress.clamp(0., 1.) / 1000.);
        let entry = self.poses.entry(ms);
        if let std::collections::hash_map::Entry::Occupied(pose) = entry {
            return Ok(pose.get().clone());
        }
        let sampler = &self.sampler;
        let frame = sampler.local.evaluate(ms as f64 / 1000., sampler.dims);
        if let Some(d) = sampler.local.runtime_diagnostics().into_iter().find(|d| d.is_error()) {
            return Err(d.message);
        }
        let ink = VectorArtwork::from_draws(frame.into_draws().into_iter().skip(1))?.map(Arc::new);
        Ok(entry.or_insert(ink).clone())
    }
}

pub(in crate::timeline) fn local_property(property: &str) -> bool {
    matches!(
        property,
        "at" | "position" | "shift" | "rotation" | "scale" | "color" | "opacity"
    )
}

fn validate_statements(body: &[Stmt]) -> Result<(), String> {
    for stmt in body {
        match stmt {
            Stmt::Assignment { .. } | Stmt::Expr(..) | Stmt::LetDecl { .. } => {},
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Block { body, .. } => validate_statements(body)?,
            _ => {
                return Err(
                    "particle_animation expects local property tracks or nested appearance plans"
                        .into(),
                );
            },
        }
    }
    Ok(())
}

pub(in crate::timeline) fn validate_op(op: &PlanOp, local: &Timeline) -> Result<(), String> {
    match op {
        PlanOp::Statements(body) => validate_statements(body),
        PlanOp::Property {
            actor, property, ..
        } if !local_property(property) || local.world.get_id(actor.label()).is_none() => {
            Err("particle_animation can only write component-local appearance properties".into())
        },
        PlanOp::Builtin(call)
            if !matches!(
                call.name.as_str(),
                "fade_in" | "fade_out" | "move" | "shift" | "rotate" | "scale"
            ) =>
        {
            Err(format!("particle_animation does not support the {} action", call.name))
        },
        _ => Ok(()), // Child plans are checked as they are mounted.
    }
}
