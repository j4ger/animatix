//! Build an authored vector component once, in an isolated local coordinate system.
use crate::ast::{Expr, InlineItem, Stmt};
use crate::scene::VectorArtwork;
use crate::timeline::{Environment, Timeline};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) enum Template {
    Static(Arc<VectorArtwork>),
    Animated(super::clip::ParticleClip),
}

fn validate_actor(ty: &str, children: &[InlineItem]) -> Result<(), String> {
    if !matches!(ty, "Group" | "Rect" | "Ellipse" | "Line" | "Arrow" | "Polygon" | "Path") {
        return Err(format!("particle_shape requires static vector geometry; found {ty}"));
    }
    for child in children {
        match child {
            InlineItem::Labeled {
                ty,
                children,
                modifiers,
                ..
            }
            | InlineItem::Anonymous {
                ty,
                children,
                modifiers,
                ..
            } if modifiers.is_empty() => validate_actor(ty, children)?,
            _ => return Err("particle_shape expects static vector children without actions".into()),
        }
    }
    Ok(())
}

pub(super) fn compile(
    source: &Expr,
    animation: Option<&Expr>,
    scene: &Timeline,
    env: &Environment,
) -> Result<Template, String> {
    let Expr::Timeline(plan) = source else {
        return Err("particle_shape expects the name of a visible static vector component".into());
    };
    for stmt in &plan.statements {
        match stmt {
            Stmt::ActorDecl {
                ty,
                children,
                modifiers,
                ..
            } if modifiers.is_empty() => validate_actor(ty, children)?,
            Stmt::LetDecl { .. } => {},
            _ => return Err("particle_shape supports static component constructors only".into()),
        }
    }
    let mut local = Timeline::new_with_font_context(scene.font_context.clone());
    local.env = env.clone();
    crate::timeline::build::extend_input_methods(
        &mut local.env,
        &plan.instance_methods,
        env.function_context.as_deref(),
    );
    local.colorscheme = scene.colorscheme.clone();
    local.style_preset = scene.style_preset;
    let mut diagnostics = Vec::new();
    local.process_body(0., &plan.statements, None, &mut diagnostics);
    if let Some(d) = diagnostics.iter().find(|d| d.is_error()) {
        return Err(d.message.clone());
    }
    if let Some(expr) = animation {
        return super::clip::ParticleClip::compile(local, expr, env, scene.canvas_dimensions())
            .map(Template::Animated);
    }
    // A component instance is drawn in its authored local units around (0,0).
    let program = local.evaluate_program_with_debug(
        0.,
        scene.canvas_dimensions(),
        Default::default(),
        &mut None,
    );
    if let Some(d) = program.diagnostics.iter().find(|d| d.is_error()) {
        return Err(d.message.clone());
    }
    // render_frame_roots emits the canvas fill first. Only foreground ink belongs
    // to the particle, including the component's own clip and opacity scopes.
    VectorArtwork::from_draws(program.scene.into_draws().into_iter().skip(1))?
        .map(|ink| Template::Static(Arc::new(ink)))
        .ok_or_else(|| "particle_shape produced no vector geometry".into())
}
