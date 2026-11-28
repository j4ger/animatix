use super::{Emitter, Motion, Schedule, Style, hash, schedule::Train};
use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::actions::BuiltinInvocation;
use crate::timeline::{ActorPoint, EntityId, Environment, Timeline, Value, evaluate_expr};

fn issue(subject: &str, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::InvalidPropertyValue, DiagnosticPhase::Build, msg.into())
        .with_subject(subject)
}
fn number(value: Value, lo: f64, hi: f64) -> Result<f64, String> {
    match value {
        Value::Num(n) if n.is_finite() && (lo..=hi).contains(&n) => Ok(n),
        _ => Err(format!("expected a finite number between {lo} and {hi}")),
    }
}
fn range(value: Value, min: f64) -> Result<[f64; 2], String> {
    match value {
        Value::Vec2([lo, hi]) if lo.is_finite() && hi.is_finite() && lo >= min && lo <= hi => {
            Ok([lo, hi])
        },
        _ => Err("expected a finite (minimum, maximum) pair in ascending order".into()),
    }
}
fn seconds(value: Value, lo: f64, hi: f64) -> Result<f64, String> {
    let value = match value {
        Value::Duration(v) => Value::Num(v),
        other => other,
    };
    number(value, lo, hi)
}
fn integer(value: Value, lo: u32, hi: u32) -> Result<u32, String> {
    let n = number(value, lo.into(), hi.into())?;
    if n.fract() != 0.0 {
        return Err("expected a whole number".into());
    }
    Ok(n as u32)
}
fn eval(expr: &Expr, env: &Environment) -> Result<Value, String> {
    evaluate_expr(expr, env).map_err(|e| format!("{e:?}"))
}

pub(crate) fn build(
    timeline: &mut Timeline,
    label: &str,
    ty: &'static str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    if !children.is_empty() || !modifiers.is_empty() || timeline.world.get_id(label).is_some() {
        return Err(vec![issue(
            label,
            "Declare an Emitter once as a leaf; schedule changes on its timeline",
        )]);
    }
    let env = timeline.build_eval_env(time_ms as u64);
    let mut e = Emitter {
        schedule: Schedule::default(),
        origin: None,
        // Plan mounts prefix private names with a process-local mount id.
        // Derive the default seed from the authored name across rebuilds.
        seed: hash(if label.starts_with("@animatix:plan:") {
            label.rsplit(':').next().unwrap_or(label)
        } else {
            label
        }),
        life: 2.0,
        speed: 75.0,
        radius: 4.0,
        spread: 24.0,
        heading: -90.0,
        jitter: 0.0,
        style: Style::Bubble,
        template: None,
        shrink: true,
        clock_scale: timeline.plan_clock_rate,
        motion: None,
        scale: None,
        angle: [0.; 2],
        spin: [0.; 2],
    };
    if props.iter().any(|p| p.name == "particle_shape")
        && props
            .iter()
            .any(|p| matches!(p.name.as_str(), "particle_style" | "particle_size"))
    {
        return Err(vec![issue(
            label,
            "A particle_shape uses its component colors and dimensions; choose particle_scale for size variations and omit particle_style and particle_size",
        )]);
    }
    let animation = props.iter().find(|p| p.name == "particle_animation");
    if animation.is_some() && !props.iter().any(|p| p.name == "particle_shape") {
        return Err(vec![issue(label, "particle_animation requires particle_shape")]);
    }
    let mut visual = Vec::new();
    for p in props {
        let parse = || eval(&p.value, &env);
        let mut apply = || -> Result<(), String> {
            match p.name.as_str() {
                "origin" => {
                    if let Some((name, point)) =
                        crate::timeline::lookup::actor_point_reference(&p.value, &env)
                    {
                        e.origin = Some((
                            timeline
                                .world
                                .resolve_path(&name.split('.').collect::<Vec<_>>())
                                .and_then(|path| timeline.world.get_id(&path))
                                .ok_or_else(|| {
                                    format!("declare origin actor {name} before {label}")
                                })?,
                            point,
                        ));
                    } else if let Ok(Value::ActorRef(a)) = parse() {
                        e.origin = Some((
                            timeline.world.get_id(a.label()).ok_or("unknown emitter origin")?,
                            ActorPoint::Origin,
                        ));
                    } else if let Ok(Value::Vec2(_)) = parse() {
                        visual.push(Property::new("at", p.value.clone()));
                    } else {
                        return Err("origin expects a point, a declared actor, or (x,y)".into());
                    }
                },
                "seed" => e.seed = integer(parse()?, 0, u32::MAX)?.into(),
                "rate" => e.schedule.rate(
                    time_ms / 1000.,
                    time_ms / 1000.,
                    number(parse()?, 0., 10000.)? / e.clock_scale,
                ),
                "lifetime" => e.life = number(parse()?, 0.001, 600.)?,
                "speed" => e.speed = number(parse()?, 0., 1e6)?,
                "spread" => e.spread = number(parse()?, 0., 360.)?,
                "heading" => e.heading = number(parse()?, -360., 360.)?,
                "particle_size" => e.radius = number(parse()?, 0.01, 10000.)?,
                "jitter" => e.jitter = number(parse()?, 0., 60.)?,
                "particle_shape" => {
                    e.template = Some(super::template::compile(
                        &p.value,
                        animation.map(|p| &p.value),
                        timeline,
                        &env,
                    )?);
                },
                "particle_animation" => {}, // compiled with the shape regardless of declaration order
                "particle_exit" => match parse()? {
                    Value::Str(s) if s == "none" => e.shrink = false,
                    Value::Str(s) if s == "shrink" => e.shrink = true,
                    _ => return Err("particle_exit expects shrink or none".into()),
                },
                "particle_angle" => e.angle = range(parse()?, f64::NEG_INFINITY)?,
                "particle_spin" => e.spin = range(parse()?, f64::NEG_INFINITY)?,
                "particle_scale" => e.scale = Some(range(parse()?, 0.)?),
                "motion" => {
                    e.motion = Some(match parse()? {
                        Value::Str(s) => match s.as_str() {
                            "linear" => Motion::Linear,
                            "drift" => Motion::Drift,
                            "ballistic" => Motion::Ballistic,
                            _ => return Err("motion expects linear, drift or ballistic".into()),
                        },
                        _ => return Err("motion expects a string".into()),
                    });
                },
                "particle_style" => {
                    e.style = match parse()? {
                        Value::Str(s) => match s.as_str() {
                            "bubble" => Style::Bubble,
                            "dust" => Style::Dust,
                            "spark" => Style::Spark,
                            _ => return Err("expected bubble, dust, or spark".into()),
                        },
                        _ => return Err("expected a particle style string".into()),
                    }
                },
                _ => visual.push(p.clone()),
            }
            Ok(())
        };
        apply().map_err(|m| vec![issue(&format!("{label}.{}", p.name), m)])?;
    }
    timeline.process_media_actor_decl(ty, label, &visual, &[], time_ms, parent, diagnostics);
    let id = timeline
        .world
        .get_id(label)
        .ok_or_else(|| vec![issue(label, "could not create emitter")])?;
    timeline.world.emitters.insert(id, e);
    Ok(())
}

pub(crate) fn set_rate(
    timeline: &mut Timeline,
    id: EntityId,
    property: &str,
    value: &Expr,
    env: &Environment,
    start: u64,
    end: u64,
    easing: crate::easing::Easing,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if !matches!(
        property,
        "rate"
            | "particle_animation"
            | "particle_exit"
            | "particle_shape"
            | "motion"
            | "particle_angle"
            | "particle_spin"
            | "particle_scale"
            | "origin"
            | "seed"
            | "lifetime"
            | "speed"
            | "spread"
            | "heading"
            | "particle_size"
            | "particle_style"
            | "jitter"
    ) {
        return false;
    }
    let result = if property != "rate" {
        Err(format!("Emitter.{property} is set on the declaration"))
    } else if start != end && easing != crate::easing::Easing::Linear {
        Err("Emitter rate ramps support linear easing".into())
    } else {
        eval(value, env).and_then(|v| number(v, 0., 10000.)).and_then(|r| {
            let e = timeline.world.emitters.get_mut(id).ok_or("missing emitter")?;
            e.schedule.rate(start as f64 / 1000., end as f64 / 1000., r / e.clock_scale);
            Ok(())
        })
    };
    if let Err(msg) = result {
        diagnostics.push(issue(property, msg))
    }
    true
}

pub(crate) fn emit(
    action: &BuiltinInvocation,
    ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let env = timeline.build_eval_env(ms as u64);
    let arg = |name: &str| {
        action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .map(|m| eval(&m.value, &env))
            .transpose()
    };
    let train = (|| -> Result<Train, String> {
        let count = integer(arg("count")?.ok_or("expected count")?, 1, 10000)?;
        let groups = integer(arg("groups")?.unwrap_or(Value::Num(1.)), 1, 1_000_000)?;
        let period = seconds(arg("period")?.unwrap_or(Value::Num(1.)), 0.001, 1e9)?;
        let jitter = seconds(arg("jitter")?.unwrap_or(Value::Num(0.)), 0., period / 2.)?;
        let name = match arg("track")? {
            Some(Value::Str(s)) => s,
            None => format!("{}@{}", action.verb, ms),
            _ => return Err("track expects a name".into()),
        };
        Ok(Train {
            start: ms / 1000.,
            period,
            groups,
            count,
            jitter,
            track: hash(&name),
        })
    })();
    match train {
        Err(m) => diagnostics.push(issue(&action.verb, m)),
        Ok(train) => {
            for label in &action.targets {
                let result = timeline
                    .world
                    .get_id(label)
                    .and_then(|id| timeline.world.emitters.get_mut(id))
                    .ok_or("particle actions require an Emitter target")
                    .and_then(|e| e.schedule.train(train));
                if let Err(m) = result {
                    diagnostics.push(issue(label, m))
                }
            }
        },
    }
}
