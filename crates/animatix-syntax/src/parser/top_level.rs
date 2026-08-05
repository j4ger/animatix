//!
//! Top-level parsers for the Animatix DSL.
//!
//! These parsers handle constructs that appear at the top level of a `.amx` file:
//! scene declarations, keyframes, config blocks, and `play` transitions.

use chumsky::prelude::*;

use super::common::{self, ModifiersParser, PropertyParser};
use crate::ast::*;

/// Build the top-level parser combining all top-level constructs.
///
/// Takes the recursive statement parser, property parser, and modifiers parser
/// as arguments to avoid circular dependencies. Returns a parser that produces
/// a flat `Vec<Stmt>` which is then grouped into scenes by [`group_scenes`].
pub(crate) fn parser<'src>(
    stmt: common::StmtParser<'src>,
    property: PropertyParser<'src>,
    modifiers: ModifiersParser<'src>,
) -> Boxed<'src, 'src, common::StrInput<'src>, Vec<Stmt>, common::ParserExtra<'src>> {
    let ident = common::ident();
    let dotted_ident = common::dotted_ident();
    let time = common::time();

    // --- Config statement ---
    let config_props = property
        .clone()
        .separated_by(just(',').padded())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just('{').padded(), just('}').padded());

    let config_stmt = text::keyword("config")
        .ignore_then(config_props)
        .map(|settings| Stmt::Config {
            settings,
            span: None,
        })
        .labelled("config")
        .as_context()
        .padded();

    // --- Scene reference ---
    let scene_ref = dotted_ident.clone().map(|parts: Vec<String>| parts.join("."));

    // `play SceneName [modifier, ...]` — scene-level transition statement
    let play_stmt = text::keyword("play")
        .padded()
        .ignore_then(scene_ref)
        .then(modifiers.clone())
        .map(|(scene_name, mods)| {
            let transition = parse_transition_from_modifiers(&mods);
            Stmt::Play {
                scene_name,
                transition,
                span: None,
            }
        })
        .labelled("play statement")
        .padded();

    // `# SceneName` — scene declaration
    let scene_decl = just('#')
        .ignore_then(ident.clone().padded())
        .map(|name| Stmt::Scene {
            name,
            config: vec![],
            body: vec![],
            span: None,
        })
        .labelled("reactive binding")
        .as_context()
        .padded();

    // --- Keyframe parser ---
    let keyframe = just('#')
        .ignore_then(just('+').or_not())
        .then(time)
        .then(stmt.clone().repeated().collect::<Vec<_>>())
        .map(|((is_relative, t), body)| {
            if is_relative.is_some() {
                Stmt::RelativeKeyframe {
                    offset: t,
                    body,
                    span: None,
                }
            } else {
                Stmt::Keyframe {
                    time: t,
                    body,
                    span: None,
                }
            }
        })
        .labelled("keyframe")
        .padded();

    // Top-level: scenes, keyframes, play, config, or standalone statements.
    // Actions, sequences, and staggers are wrapped in a default #0s keyframe
    // because they need a timeline context. Actor declarations and other
    // statements remain top-level so the compiler can distinguish pre-keyframe
    // actors (hidden by default) from in-keyframe actors (visible by default).
    choice((
        keyframe,
        scene_decl,
        play_stmt,
        config_stmt,
        stmt.map(|s| match s {
            Stmt::Action(..) | Stmt::Sequence { .. } | Stmt::Stagger { .. } => Stmt::Keyframe {
                time: Time::Seconds(0.0),
                body: vec![s],
                span: None,
            },
            other => other,
        }),
    ))
    .repeated()
    .collect::<Vec<_>>()
    .map(group_scenes)
    .boxed()
}

/// After parsing, group flat statements into scenes.
///
/// If any `Stmt::Scene` markers exist in the parsed output:
///   - Everything before the first scene is the shared prelude.
///   - Each scene marker starts a new scene; its body accumulates all subsequent statements until
///     the next scene marker or EOF.
///   - A `config { ... }` immediately after a scene marker is absorbed as that scene's config.
///   - `play` statements belong to the current scene's body.
///
/// If no scene markers exist, the output is returned unmodified
/// (single-scene file, backward compatible).
pub fn group_scenes(flat: Vec<Stmt>) -> Vec<Stmt> {
    let has_scenes = flat.iter().any(|s| matches!(s, Stmt::Scene { .. }));
    if !has_scenes {
        return flat;
    }

    let mut result: Vec<Stmt> = Vec::new();
    let mut current_scene: Option<Stmt> = None;

    for stmt in flat {
        match stmt {
            Stmt::Scene {
                name,
                config: _,
                body: _,
                span,
            } => {
                // Finish previous scene if any
                if let Some(scene) = current_scene.take() {
                    result.push(scene);
                }
                current_scene = Some(Stmt::Scene {
                    name,
                    config: vec![],
                    body: vec![],
                    span,
                });
            },
            Stmt::Config { .. } => {
                if let Some(Stmt::Scene {
                    ref mut config,
                    ref body,
                    ..
                }) = current_scene
                {
                    // Absorb config into the scene only if both config and body
                    // are still empty (config must be the first thing after the scene name).
                    if config.is_empty() && body.is_empty() {
                        if let Stmt::Config { settings, .. } = stmt {
                            *config = settings;
                            continue;
                        }
                    }
                }
                // Otherwise treat as part of the body
                if let Some(Stmt::Scene { ref mut body, .. }) = current_scene {
                    body.push(stmt);
                } else {
                    // Prelude config — keep in result
                    result.push(stmt);
                }
            },
            other => {
                if let Some(Stmt::Scene { ref mut body, .. }) = current_scene {
                    body.push(other);
                } else {
                    // Prelude statements (imports, pub lets, etc.)
                    result.push(other);
                }
            },
        }
    }

    // Push the last scene
    if let Some(scene) = current_scene {
        result.push(scene);
    }

    result
}

/// Convert play statement modifiers into a `Transition` descriptor.
///
/// Modifiers format: `[fade, 300ms]` or `[wipe-left, 200ms]`.
/// The first bare identifier (not a time) is the transition type.
/// The first time literal is the duration.
pub(crate) fn parse_transition_from_modifiers(
    modifiers: &[Modifier],
) -> Option<crate::ast::Transition> {
    let mut transition_id: Option<String> = None;
    let mut duration_ms: u64 = 0;

    for m in modifiers {
        match (&m.name, &m.value) {
            (None, Expr::Ident(name)) if transition_id.is_none() => {
                if crate::transition_registry::find(name).is_some() {
                    transition_id = Some(name.clone());
                }
            },
            (None, Expr::Ident(name)) if name.ends_with("ms") => {
                if let Ok(ms) = name.trim_end_matches("ms").parse::<u64>() {
                    if duration_ms == 0 {
                        duration_ms = ms;
                    }
                }
            },
            (None, Expr::Ident(name))
                if name.ends_with('s') && !name.starts_with(|c: char| c.is_alphabetic()) =>
            {
                if let Ok(s) = name.trim_end_matches('s').parse::<f64>() {
                    if duration_ms == 0 {
                        duration_ms = (s * 1000.0) as u64;
                    }
                }
            },
            _ => {},
        }
    }

    transition_id.map(|id| crate::ast::Transition {
        id,
        duration_ms,
        easing: crate::easing::Easing::Linear,
    })
}
