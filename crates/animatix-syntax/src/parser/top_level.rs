//!
//! Top-level parsers for the Animatix DSL.
//!
//! These parsers handle constructs that appear at the top level of a `.amx` file:
//! scene declarations, keyframes, config blocks, and `play` transitions.

use chumsky::prelude::*;

use super::common::{self, ModifiersParser, PropertyParser};
use super::token_parser::*;
use crate::ast::*;
use crate::occurrence::OccurrenceKind;

/// Build the top-level parser combining all top-level constructs.
pub(crate) fn parser<'src>(
    stmt: common::StmtParser<'src>,
    property: PropertyParser<'src>,
    modifiers: ModifiersParser<'src>,
) -> Boxed<'src, 'src, common::StrInput<'src>, Vec<Stmt>, common::ParserExtra<'src>> {
    let time = common::time();

    let config_props = property
        .clone()
        .separated_by(comma())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(lbrace(), rbrace());

    let config_stmt = keyword("config")
        .ignore_then(config_props)
        .map(|settings| Stmt::Config {
            settings,
            span: None,
        })
        .labelled("config")
        .as_context();

    let scene_ref = common::ident_occ(OccurrenceKind::Scene)
        .clone()
        .separated_by(dot())
        .collect::<Vec<_>>()
        .map(|parts: Vec<String>| parts.join("."));

    let play_stmt = keyword("play")
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
        .labelled("play statement");

    let scene_decl = hash()
        .ignore_then(common::ident_decl_occ(OccurrenceKind::Scene).clone())
        .map(|name| Stmt::Scene {
            name,
            config: vec![],
            body: vec![],
            span: None,
        })
        .labelled("reactive binding")
        .as_context();

    let keyframe = hash()
        .ignore_then(plus().or_not())
        .then(time)
        .then(common::scoped(stmt.clone().repeated().collect::<Vec<_>>()))
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
        .labelled("keyframe");

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
                    if config.is_empty() && body.is_empty() {
                        if let Stmt::Config { settings, .. } = stmt {
                            *config = settings;
                            continue;
                        }
                    }
                }
                if let Some(Stmt::Scene { ref mut body, .. }) = current_scene {
                    body.push(stmt);
                } else {
                    result.push(stmt);
                }
            },
            other => {
                if let Some(Stmt::Scene { ref mut body, .. }) = current_scene {
                    body.push(other);
                } else {
                    result.push(other);
                }
            },
        }
    }

    if let Some(scene) = current_scene {
        result.push(scene);
    }

    result
}

/// Convert play statement modifiers into a `Transition` descriptor.
pub(crate) fn parse_transition_from_modifiers(
    modifiers: &[Modifier],
) -> Option<crate::ast::Transition> {
    let mut transition_id: Option<String> = None;
    let mut duration_ms: u64 = 0;

    for m in modifiers {
        match (&m.name, &m.value) {
            (None, Expr::Ident(name))
                if transition_id.is_none() && crate::transition_registry::find(name).is_some() =>
            {
                transition_id = Some(name.clone());
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
