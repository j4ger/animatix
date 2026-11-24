//!
//! Top-level parsers for the Animatix DSL.
//!
//! These parsers handle constructs that appear at the top level of a `.amx` file:
//! scene declarations, keyframes, config blocks, and `play` transitions.

use chumsky::input::MapExtra;
use chumsky::prelude::*;

use super::common::{self, ModifiersParser, PropertyParser, TimeParser};
use super::token_parser::*;
use crate::ast::*;
use crate::occurrence::OccurrenceKind;
use crate::token::TokenKind;

/// Build the top-level parser combining all top-level constructs.
pub(crate) fn parser<'src>(
    stmt: common::StmtParser<'src>,
    property: PropertyParser<'src>,
    modifiers: ModifiersParser<'src>,
    time: TimeParser<'src>,
    source: common::SourceIndex<'src>,
) -> Boxed<'src, 'src, common::StrInput<'src>, Vec<Stmt>, common::ParserExtra<'src>> {
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
        .separated_by(colon_colon())
        .collect::<Vec<_>>()
        .map(|parts: Vec<String>| parts.join("::"));

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

    let scene_source = source.clone();
    let scene_decl = hash()
        .ignore_then(common::ident_decl_occ(OccurrenceKind::Scene).clone())
        .map_with(move |name, extra| Stmt::Scene {
            name,
            config: vec![],
            body: vec![],
            span: common::source_span(&scene_source, extra.span()),
        })
        .labelled("scene declaration")
        .as_context();

    let keyframe_source = source.clone();
    let cue_name = hash()
        .ignore_then(select! { TokenKind::Ident(name) if name == "cue" => () })
        .ignore_then(colon())
        .ignore_then(ident().labelled("cue name"));
    let keyframe = hash()
        .ignore_then(plus().or_not())
        .then(time)
        .then_ignore(colon().or_not())
        .then(cue_name.repeated().collect::<Vec<_>>())
        .then(common::scoped(stmt.clone().repeated().collect::<Vec<_>>()))
        .map_with(
            move |(((is_relative, t), names), body),
                  extra: &mut MapExtra<
                'src,
                '_,
                common::StrInput<'src>,
                common::ParserExtra<'src>,
            >| {
                let span = common::source_span(&keyframe_source, extra.span());
                let cue_group = (!names.is_empty()).then(|| {
                    Box::new(CueGroup {
                        preceding: Vec::new(),
                        names,
                    })
                });
                if is_relative.is_some() {
                    Stmt::RelativeKeyframe {
                        offset: t,
                        cue_group,
                        body,
                        span,
                    }
                } else {
                    Stmt::Keyframe {
                        time: t,
                        cue_group,
                        body,
                        span,
                    }
                }
            },
        )
        .labelled("keyframe");

    let narration = select! { TokenKind::Comment(marker) => marker }
        .map_with(move |marker, extra| (marker, common::source_span(&source, extra.span())))
        .then(common::scoped(stmt.clone().repeated().collect::<Vec<_>>()))
        .map(|((marker, span), body)| Stmt::Keyframe {
            time: Time::SpeechMarker {
                track: crate::narration::TRACK.into(),
                marker,
                offset_ms: 0,
            },
            cue_group: None,
            body,
            span,
        });

    let step_marker = hash()
        .ignore_then(select! {
            TokenKind::Keyword(k) if k == "step" || k == "pause" => (),
            TokenKind::Ident(i) if i == "step" || i == "pause" => (),
        })
        .then(number().or_not())
        .map(|(_, num)| Stmt::Step {
            index: num.map(|n| n as u64),
            span: None,
        })
        .labelled("step marker");

    // Recovery synchronization point: a token that can begin a fresh top-level
    // statement. When a statement fails to parse, the recovery strategy skips
    // forward to the next such token instead of abandoning the whole file, so
    // one malformed statement does not blind the analyzer to everything after
    // it.
    let statement_start = select! {
        TokenKind::Hash | TokenKind::Comment(_) => (),
        TokenKind::Keyword(k) if k == "config" || k == "play" => (),
        TokenKind::Ident(_) => (),
        TokenKind::LBrace => (),
        TokenKind::RBrace => (),
    };

    choice((
        step_marker,
        keyframe,
        narration,
        scene_decl,
        play_stmt,
        config_stmt,
        stmt.map(|s| match s {
            Stmt::Expr(Expr::Call(..), _) | Stmt::Sequence { .. } | Stmt::Stagger { .. } => {
                Stmt::Keyframe {
                    time: Time::Seconds(0.0),
                    cue_group: None,
                    body: vec![s],
                    span: None,
                }
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
    .map(|flat| group_scenes(group_cue_headers(prefer_explicit_keyframes(flat))))
    .boxed()
}

fn group_cue_headers(flat: Vec<Stmt>) -> Vec<Stmt> {
    let mut result = Vec::with_capacity(flat.len());
    for mut stmt in flat {
        if let Stmt::Keyframe {
            cue_group: Some(group),
            ..
        }
        | Stmt::RelativeKeyframe {
            cue_group: Some(group),
            ..
        } = &mut stmt
        {
            while let Some((relative, time)) = result.last().and_then(empty_time_header) {
                group.preceding.push((relative, time.clone()));
                result.pop();
            }
            group.preceding.reverse();
        }
        result.push(stmt);
    }
    result
}

fn empty_time_header(stmt: &Stmt) -> Option<(bool, &Time)> {
    match stmt {
        Stmt::Keyframe {
            time,
            body,
            cue_group: None,
            ..
        } if body.is_empty()
            && !matches!(time, Time::SpeechMarker { marker, .. } if crate::narration::is_implicit_anchor(marker)) =>
        {
            Some((false, time))
        },
        Stmt::RelativeKeyframe {
            offset,
            body,
            cue_group: None,
            ..
        } if body.is_empty() => Some((true, offset)),
        _ => None,
    }
}

fn prefer_explicit_keyframes(flat: Vec<Stmt>) -> Vec<Stmt> {
    let mut result = Vec::with_capacity(flat.len());
    for mut statement in flat {
        if let Stmt::Keyframe {
            time: Time::SpeechMarker { marker, .. },
            body,
            ..
        } = &mut statement
            && crate::narration::is_implicit_anchor(marker)
            && let Some(previous) = result.last_mut()
        {
            let previous_body = match previous {
                Stmt::Keyframe { time, body, .. } if !matches!(&*time, Time::SpeechMarker { marker, .. } if crate::narration::is_implicit_anchor(marker)) => {
                    Some(body)
                },
                Stmt::RelativeKeyframe { body, .. } => Some(body),
                _ => None,
            };
            if let Some(previous_body) = previous_body.filter(|body| body.is_empty()) {
                std::mem::swap(previous_body, body);
            }
        }
        result.push(statement);
    }
    result
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
                // configuration, whatever precedes it. Requiring the scene to
                // still be empty lets one leading statement — an `import`,
                // which belongs to no keyframe — demote the block into the
                // scene body, leaving `Scene.config` empty for the composition
                // to fall back on keyframe-span duration.
                if let Some(Stmt::Scene { ref mut config, .. }) = current_scene
                    && let Stmt::Config { settings, .. } = stmt
                {
                    config.extend(settings);
                    continue;
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
            (None, Expr::Duration(seconds)) if duration_ms == 0 => {
                duration_ms = (seconds * 1000.0).round() as u64;
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
