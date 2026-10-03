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
use crate::token::TokenKind;

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
        .map(|(scene_name, mut mods)| {
            let transition = parse_transition_from_modifiers(&mut mods);
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

    // Recovery synchronization point: a token that can begin a fresh top-level
    // statement. When a statement fails to parse, the recovery strategy skips
    // forward to the next such token instead of abandoning the whole file, so
    // one malformed statement no longer blinds the analyzer to everything after
    // it.
    let statement_start = select! {
        TokenKind::Hash => (),
        TokenKind::Keyword(k) if k == "config" || k == "play" => (),
        TokenKind::Ident(_) => (),
        TokenKind::LBrace => (),
        TokenKind::RBrace => (),
    };

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
    // Skip the offending run of tokens and resume at the next statement start.
    // `any()` consumes at least one token, so the repeat always advances and
    // can never spin on the same position.
    .recover_with(skip_then_retry_until(any().ignored(), statement_start))
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
                // A `config` block anywhere inside a scene is that scene's
                // configuration, whatever precedes it. The gate used to require
                // the scene still be empty, so one leading statement — an
                // `import`, which belongs to no keyframe — demoted the block
                // into the scene body, leaving `Scene.config` empty for the
                // composition to fall back on keyframe-span duration.
                if let Some(Stmt::Scene { ref mut config, .. }) = current_scene {
                    if let Stmt::Config { settings, .. } = stmt {
                        config.extend(settings);
                        continue;
                    }
                }
                result.push(stmt);
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
    modifiers: &mut Vec<Modifier>,
) -> Option<crate::ast::Transition> {
    let mut transition_id: Option<String> = None;
    let mut duration_ms: u64 = 0;

    // `ease:` is a named modifier, so the ident loop below would skip it and
    // every transition would run Linear no matter what the author wrote —
    // while the compositor downstream does apply the easing. Lift it first
    // through the shared extractor, which also handles the parameterized
    // `ease: cubic-bezier(…)` form and leaves unresolvable values in place for
    // the build layer to report.
    let easing = common::extract_easing(modifiers).unwrap_or(crate::easing::Easing::Linear);

    for m in modifiers.iter() {
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
        easing,
    })
}
