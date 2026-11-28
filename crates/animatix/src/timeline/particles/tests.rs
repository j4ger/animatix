use super::{
    hash,
    schedule::{Schedule, Train, unit},
};
use crate::scene::Draw;
use crate::timeline::{SceneDimensions, Timeline};
use kurbo::Shape;

fn build(source: &str) -> Timeline {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
    assert!(!report.diagnostics.iter().any(|d| d.is_error()), "{:?}", report.diagnostics);
    report.output
}
fn births(s: &Schedule, t: f64, life: f64, seed: u64, jitter: f64) -> Vec<(f64, u64, u64, u32)> {
    let mut v: Vec<_> = s
        .batches(t, life, seed, jitter)
        .unwrap()
        .into_iter()
        .map(|b| (b.time, b.track, b.group, b.count))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}
#[test]
fn cold_stream_ranges_and_fractional_rate_changes() {
    let mut s = Schedule::default();
    s.rate(0., 0., 20.);
    let v = births(&s, 598., 2., 2, 0.);
    assert_eq!(v.len(), 40);
    assert_eq!((v[0].2, v[39].2), (11921, 11960));
    let mut s = Schedule::default();
    s.rate(0., 0., 1.);
    s.rate(0.5, 0.5, 2.);
    s.rate(1., 1., 0.);
    let v = births(&s, 2., 4., 0, 0.);
    assert_eq!(v.iter().map(|x| x.0).collect::<Vec<_>>(), [0., 0.75]);
    assert!(births(&s, 50., 2., 0, 0.).is_empty());
}
#[test]
fn linear_ramp_solves_the_integral() {
    let mut s = Schedule::default();
    s.rate(0., 2., 4.);
    let v = births(&s, 2., 3., 1, 0.);
    assert_eq!(v.len(), 5);
    for (t, _, id, _) in v {
        assert!((t * t - id as f64).abs() < 1e-9)
    }
}
#[test]
fn equivalent_rate_keys_preserve_jittered_birth_times() {
    for ramp in [false, true] {
        let build_schedule = |with_split: bool| {
            let mut s = Schedule::default();
            s.rate(0.25, 0.25, 8.);
            if ramp {
                if with_split {
                    s.rate(0.25, 0.75, 12.);
                    s.rate(0.75, 1.25, 16.);
                } else {
                    s.rate(0.25, 1.25, 16.);
                }
            } else if with_split {
                s.rate(1.0, 1.0, 8.);
            }
            s.rate(1.4, 1.4, 0.);
            s.rate(1.8, 1.8, 8.);
            if with_split {
                s.rate(2.5, 2.5, 8.); // Redundant trailing key must not clamp jitter either.
            }
            s
        };
        let control = build_schedule(false);
        let candidate = build_schedule(true);
        for seed in [0, 4, 42] {
            for t in [2.9, 0.24, 0.25, 0.77, 0.95, 1.0, 1.4, 1.7, 1.8, 2.5, 2.9] {
                let original = births(&control, t, 0.63, seed, 0.115);
                assert_eq!(
                    births(&candidate, t, 0.63, seed, 0.115),
                    original,
                    "ramp={ramp}, t={t}, seed={seed}"
                );
                assert!(
                    original
                        .iter()
                        .all(|(birth, _, _, _)| { (0.25..1.4).contains(birth) || *birth >= 1.8 })
                );
            }
        }
    }
}

#[test]
fn warmed_jitter_schedule_is_invalidated_by_a_rate_edit() {
    let mut s = Schedule::default();
    s.rate(0., 0., 8.);
    let before = births(&s, 1., 1., 4, 0.115);
    assert!(!before.is_empty());
    s.rate(0.5, 0.5, 0.);
    let after = births(&s, 1., 1., 4, 0.115);
    assert_ne!(before, after);
    assert!(after.iter().all(|b| b.0 < 0.5));
}

#[test]
fn jitter_is_per_batch_and_channels_do_not_depend_on_batch_size() {
    let train = Train {
        start: 1.,
        period: 0.5,
        groups: 4000,
        count: 3,
        jitter: 0.18,
        track: hash("valve"),
    };
    let mut a = Schedule::default();
    a.train(train).unwrap();
    let mut b = Schedule::default();
    b.train(Train { count: 4, ..train }).unwrap();
    let read = |s: &Schedule, seed| births(s, 598., 2., seed, 0.);
    let first = read(&a, 42);
    assert!(first.len() >= 3);
    assert_ne!(first, read(&a, 43));
    for ((t, track, g, n), (other, _, h, m)) in first.iter().zip(read(&b, 42)) {
        assert_eq!((*t, *g, *n), (other, h, 3));
        assert_eq!(m, 4);
        assert!((*t - (1. + *g as f64 * 0.5)).abs() <= 0.18);
        assert_ne!(unit(42, *track, *g, 0, 1), unit(42, *track, *g, 0, 2));
    }
    assert_ne!(unit(42, train.track, 0, 0, 1), unit(42, train.track, 1, 0, 2));
    assert_eq!(read(&a, 42), first);
}

const DIMS: SceneDimensions = SceneDimensions {
    width: 640,
    height: 360,
};
fn particle_geometry(tl: &Timeline, t: f64) -> Vec<kurbo::BezPath> {
    tl.evaluate(t, DIMS)
        .draws()
        .iter()
        .filter_map(|draw| match draw {
            Draw::Fill {
                path, transform, ..
            } if path.elements().iter().any(|el| matches!(el, kurbo::PathEl::CurveTo(..))) => {
                Some(*transform * path.clone())
            },
            _ => None,
        })
        .collect()
}
#[test]
fn idle_emitter_has_a_pick_region_but_no_ink_bounds() {
    let tl = build(
        r#"#0s
e: Emitter, at: (100,100), lifetime: 1, speed: 0, opacity: 1
#1s
burst(target:e, count:1)
#2s
"#,
    );
    let options = crate::timeline::DebugRenderOptions {
        compute_hit_regions: true,
        ..Default::default()
    };
    for (t, active) in [(0.5, false), (1.25, true), (2.0, false), (0.5, false)] {
        let program = tl.evaluate_program_with_debug(t, DIMS, options, &mut None);
        assert_eq!(program.items.iter().any(|item| item.label == "e"), active);
        assert_eq!(program.precise_bounds.contains_key("e"), active);
        assert!(tl.hit_regions().iter().any(|(label, _)| label == "e"));
    }
}

#[test]
fn direct_reverse_and_forward_preserve_birth_world_positions() {
    let src = r#"config { duration: 6, canvas_size: (640,360) }
#0s
  rig: Group, at: (100,200) {
    port: Rect, at: (20,0), size: (2,2)
  }
  e: Emitter, origin: rig.port.origin, particle_style: "dust", speed: 0, lifetime: 5, seed: 42, opacity: 1
#1s
  burst(target:e,count:3,track:"first")
  rig.at = (400,200) [2s,ease:linear]
#2s
  burst(target:e,count:3,track:"second")
#6s
"#;
    let tl = build(src);
    let direct = particle_geometry(&tl, 3.);
    let bounds = direct.iter().map(Shape::bounding_box).reduce(|a, b| a.union(b)).unwrap();
    assert!(
        bounds.x0 > 110. && bounds.x0 < 122. && bounds.x1 > 265. && bounds.x1 < 280.,
        "{bounds:?}"
    );
    particle_geometry(&tl, 4.);
    assert_eq!(particle_geometry(&tl, 3.), direct);
    for i in 0..90 {
        particle_geometry(&tl, i as f64 / 30.);
    }
    assert_eq!(particle_geometry(&tl, 3.), direct);
    assert_eq!(build(src).world.len(), 3);
}

#[test]
#[ignore = "manual random-scrub latency sample, run with --ignored --nocapture"]
fn random_scrub_benchmark() {
    for duration in [10, 600] {
        for (moving, emit) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut src = format!(
                "config {{ duration: {duration}, canvas_size: (640,360) }}\n#0s\n  rig: Group, at: (100,200) {{ port: Rect, size: (2,2), at: (20,0) }}\n"
            );
            if emit {
                src.push_str("  e: Emitter, origin: rig.port.origin, rate:20, lifetime:2, seed:42, opacity:1\n");
            }
            if moving {
                src.push_str(&format!("  rig.at = (500,200) [{duration}s,ease:linear]\n"));
            }
            src.push_str(&format!("#{duration}s\n"));
            let tl = build(&src);
            println!(
                "duration={duration} moving={moving} emit={emit} CPU_p95_us={:.1}",
                scrub_p95_us(&tl, duration)
            );
        }
    }
}

#[test]
fn rate_step_at_ramp_end_preserves_earlier_births() {
    let mut s = Schedule::default();
    s.rate(0., 2., 4.);
    let before = births(&s, 1.95, 2., 1, 0.);
    s.rate(2., 2., 0.);
    assert_eq!(before, births(&s, 1.95, 2., 1, 0.));
    assert_eq!(before, births(&s, 3., 4., 1, 0.));
}

#[test]
fn changing_seed_changes_particle_geometry_and_invalid_inputs_are_diagnosed() {
    let source =
        |seed| format!("#0s\n  e: Emitter, at:(120,200), rate:7, seed:{seed}, opacity:1\n#4s\n");
    assert_ne!(
        particle_geometry(&build(&source(5)), 1.),
        particle_geometry(&build(&source(6)), 1.)
    );
    for bad in [
        "e: Emitter, seed:-1",
        "e: Emitter, rate:-1",
        "e: Emitter, particle_style:\"unknown\"",
        "r: Rect\nburst(target:r,count:2)",
        "e: Emitter\nemit_pulse(target:e,count:1,groups:3,period:0)",
    ] {
        let src = format!("#0s\n{bad}\n#4s\n");
        let (ast, errors) = animatix_syntax::parser::parse_source(&src);
        assert!(errors.is_empty(), "{src}: {errors:?}");
        assert!(
            Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default())
                .diagnostics
                .iter()
                .any(|d| d.is_error()),
            "{bad}"
        );
    }
}

#[test]
fn modifier_driven_origin_uses_birth_time() {
    let src = r#"config {duration:4}
#0s
  port: Rect, size:(2,2), opacity:1
  e: Emitter, origin:port.origin, particle_style:"dust", speed:0, lifetime:4, seed:7, opacity:1
always { port.at=(120+60*t, 160) }
#1s
  burst(target:e,count:1)
#2s
  burst(target:e,count:1)
#4s
"#;
    let tl = build(src);
    let paths = particle_geometry(&tl, 2.5);
    let centers: Vec<_> = paths.iter().map(|p| p.bounding_box().center().x).collect();
    assert!(centers.iter().any(|x| (x - 180.).abs() < 0.01), "{centers:?}");
    assert!(centers.iter().any(|x| (x - 240.).abs() < 0.01), "{centers:?}");
}

#[test]
fn pulse_accepts_durations_and_infers_the_final_tail() {
    let source = "#0s\n  e: Emitter, lifetime:2, opacity:1\n#1s\n  emit_pulse(target:e,count:2,groups:3,period:500ms,jitter:0ms)\n";
    let timeline = build(source);
    assert_eq!(timeline.playback_duration_seconds(), 4.);
    assert!(!particle_geometry(&timeline, 2.25).is_empty());
    assert!(particle_geometry(&timeline, 4.).is_empty());
}

#[test]
fn dense_pulse_fails_before_allocating_paths_and_recovers_after_the_window() {
    let mut schedule = Schedule::default();
    schedule
        .train(Train {
            start: 0.,
            period: 0.001,
            groups: 1_000_000,
            count: 10_000,
            jitter: 0.,
            track: 7,
        })
        .unwrap();
    assert!(schedule.batches(2., 2., 7, 0.).is_err());
    assert!(schedule.batches(1003., 2., 7, 0.).unwrap().is_empty());
}

fn loaded_particle_test(source: &str) -> crate::diagnostics::BuildReport<Timeline> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/particles/autumn.amx");
    load_particle_source(&path, source)
}

fn loaded_lifecycle_test(source: &str) -> crate::diagnostics::BuildReport<Timeline> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../web/content/scenes/particle-test-caller.amx");
    load_particle_source(&path, source)
}

fn load_particle_source(
    path: &std::path::Path,
    source: &str,
) -> crate::diagnostics::BuildReport<Timeline> {
    let mut graph = crate::module::ModuleGraph::new();
    let program = graph.load_program_with_source(path, Some(source)).unwrap();
    let expanded = program.expand_components().unwrap();
    Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    )
}

#[test]
fn authored_leaf_shares_compiled_ink_preserves_pivot_and_replays_spin_on_seek() {
    let report = loaded_particle_test(
        r#"import "leaf.amx" as art
config { duration:4, canvas_size:(640,360) }
#0s
  e: Emitter, at:(100,160), particle_shape:art::Leaf, particle_angle:(10,10), particle_spin:(90,90), particle_scale:(2,2), motion:"linear", lifetime:4, speed:0, seed:42
  burst(target:e,count:3)
"#,
    );
    assert!(!report.diagnostics.iter().any(|d| d.is_error()), "{:?}", report.diagnostics);
    let timeline = report.output;
    assert_eq!(timeline.world.len(), 1);
    let sample = |time| {
        let p = timeline.evaluate_program_with_debug(time, DIMS, Default::default(), &mut None);
        let item = p.items.iter().find(|i| i.label == "e").unwrap();
        let crate::primitives::RenderCommand::VectorInstances {
            artwork,
            transforms,
        } = &item.commands[0]
        else {
            panic!("expected instanced vectors")
        };
        (
            artwork.clone(),
            transforms.iter().map(|t| item.transform * *t).collect::<Vec<_>>(),
        )
    };
    let (ink, first) = sample(0.);
    assert!(
        ink.bounds.width() > 30. && ink.bounds.width() < 45. && ink.bounds.contains((0., 0.)),
        "{:?}",
        ink.bounds
    );
    assert_eq!(first.len(), 3);
    let (same, after) = sample(1.);
    assert!(std::sync::Arc::ptr_eq(&ink, &same));
    for (t, a) in [(&first[0], 10_f64), (&after[0], 100_f64)] {
        assert_eq!(*t * kurbo::Point::ORIGIN, kurbo::Point::new(100., 160.));
        let v = (*t * kurbo::Point::new(1., 0.)) - (*t * kurbo::Point::ORIGIN);
        assert!(
            (v.x - 2. * a.to_radians().cos()).abs() < 1e-8
                && (v.y - 2. * a.to_radians().sin()).abs() < 1e-8
        );
    }
    let expired = timeline.evaluate_program_with_debug(4.0, DIMS, Default::default(), &mut None);
    assert!(!expired.items.iter().any(|item| item.label == "e"));
    assert_eq!(sample(1.).1, after);
    for i in 0..30 {
        sample(i as f64 / 30.);
    }
    assert_eq!(sample(1.).1, after);
}

#[test]
fn custom_particles_report_invalid_art_and_ranges() {
    for (component, props) in [
        ("", "particle_shape: Missing"),
        ("component Glyph() { a: Text, text:\"x\" }", "particle_shape: Glyph"),
        (
            "component Glyph() { a: Rect }",
            "particle_shape: Glyph, particle_style:\"spark\"",
        ),
        ("", "particle_scale:(-1,1)"),
        ("", "particle_spin:(10,-10)"),
        ("", "motion:\"undefined\""),
    ] {
        let report = loaded_particle_test(&format!("{component}\n#0s\n  e: Emitter, {props}\n"));
        assert!(report.diagnostics.iter().any(|d| d.is_error()), "{props}");
    }
}

fn scrub_p95_us(timeline: &Timeline, duration: i32) -> f64 {
    let mut samples = Vec::new();
    for i in 0..257 {
        let t = 2.1 + ((i * 137) % 257) as f64 / 257. * (f64::from(duration) - 2.2);
        let start = std::time::Instant::now();
        std::hint::black_box(timeline.evaluate(t, DIMS));
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    samples[244] as f64 / 1000.
}

#[test]
#[ignore = "manual CPU seek timing for multi-path authored particle geometry"]
fn custom_shape_scrub_benchmark() {
    for duration in [10, 600] {
        let report = loaded_particle_test(&format!(
            "import \"leaf.amx\"\nconfig {{duration:{duration},canvas_size:(640,360)}}\n#0s\n  rig: Rect, at:(80,100)\n  e: Emitter, origin:rig.origin, particle_shape:Leaf, motion:\"drift\", particle_spin:(-100,100), lifetime:2, rate:20, seed:42\n  rig.at = (500,100) [{duration}s,ease:linear]\n#{duration}s\n"
        ));
        assert!(!report.diagnostics.iter().any(|d| d.is_error()), "{:?}", report.diagnostics);
        println!(
            "duration={duration} leaf_cpu_p95_us={:.1}",
            scrub_p95_us(&report.output, duration)
        );
    }
}

fn appearance(
    timeline: &Timeline,
    time: f64,
) -> Vec<(kurbo::BezPath, kurbo::Affine, peniko::Brush)> {
    timeline
        .evaluate(time, DIMS)
        .into_draws()
        .into_iter()
        .skip(1)
        .filter_map(|draw| match draw {
            Draw::Fill {
                path,
                transform,
                brush,
                ..
            }
            | Draw::Stroke {
                path,
                transform,
                brush,
                ..
            } => Some((path, transform, brush)),
            _ => None,
        })
        .collect()
}

fn checked_scene(source: &str) -> Timeline {
    let result = loaded_lifecycle_test(source);
    assert!(!result.diagnostics.iter().any(|d| d.is_error()), "{:?}", result.diagnostics);
    result.output
}

#[test]
fn lifecycle_matches_actor_tracks_on_cold_reverse_and_sequential_seeks() {
    let header = "import \"particle-lifecycle.amx\" as art\nlet red = hex_color(\"ab3434\")\n#0s\n";
    let actor = checked_scene(&format!("{header}l:art::Leaf\nl.autumn(end_color:red)\n#4s"));
    let source = format!(
        "{header}e:Emitter, speed:0, lifetime:4, seed:42, particle_exit:\"none\", particle_animation:(l) => stretch(l.autumn(end_color:red), 2), particle_shape:art::Leaf\nburst(target:e,count:1,track:\"one\")\n#4s"
    );
    let flight = checked_scene(&source);
    assert_eq!(flight.world.len(), 1);
    let life = 4. * (0.72 + 0.28 * unit(42, hash("one"), 0, 0, 1));
    let check = |flight: &Timeline, u: f64| {
        // Both clocks sample the same position inside an authored millisecond.
        let u = u + 0.0001;
        let actual = appearance(flight, u * life);
        let expected = appearance(&actor, u);
        assert!(!expected.is_empty());
        assert_eq!(actual, expected, "normalized age {u}");
    };
    for u in [0.95, 0.25, 0.7] {
        check(&checked_scene(&source), u);
        check(&flight, u);
    }
    for i in 0..96 {
        appearance(&flight, f64::from(i) / 96. * life);
    }
    check(&flight, 0.25);
    assert_ne!(appearance(&actor, 0.25), appearance(&actor, 0.95));
    assert!(appearance(&flight, life + 0.001).is_empty());
}

#[test]
fn invalid_lifecycles_fail_during_build() {
    let base = "import \"particle-lifecycle.amx\" as art\n#0s\nparticle:Rect\n";
    for (props, reason) in [
        ("particle_animation:(p) => 1", "requires particle_shape"),
        ("particle_shape:art::Leaf, particle_animation:\"autumn\"", "expects a closure"),
        ("particle_shape:art::Leaf, particle_animation:(p) => 1", "return a Timeline"),
        (
            "particle_shape:art::Leaf, particle_animation:(p) => #{ p.opacity = 0 }",
            "positive finite",
        ),
        (
            "particle_shape:art::Leaf, particle_animation:(p) => #{ particle.opacity = 0 [1s] }",
            "component-local",
        ),
        (
            "particle_shape:art::Leaf, particle_animation:(p) => #{ scene.background_color = hex_color(\"ffffff\") [1s] }",
            "component-local",
        ),
        (
            "particle_shape:art::Leaf, particle_animation:(p) => stretch(#{ extra:Rect }, 2)",
            "local property tracks",
        ),
        (
            "particle_shape:art::Leaf, particle_animation:(p) => fade_out(target:particle,duration:1s)",
            "component-local",
        ),
        (
            "particle_shape:art::Leaf, particle_animation:(p) => persist(target:p)",
            "does not support",
        ),
        ("particle_exit:\"fade\"", "expects shrink or none"),
    ] {
        let report = loaded_lifecycle_test(&format!("{base}e:Emitter, {props}\n"));
        assert!(
            report.diagnostics.iter().any(|d| d.is_error() && d.message.contains(reason)),
            "{props}: {:?}",
            report.diagnostics
        );
    }
}

#[test]
fn retiming_only_changes_the_flight_clock_of_an_owned_emitter() {
    let scene = |factor: u32, owned| {
        let decl = "e:Emitter, particle_shape:art::Leaf, particle_animation:(p) => p.autumn(end_color:hex_color(\"901212\")), speed:50, lifetime:1, particle_spin:(40,40), particle_exit:\"none\"";
        let (outer, inner) = if owned { ("", decl) } else { (decl, "") };
        checked_scene(&format!(
            "import \"particle-lifecycle.amx\" as art\n#0s\n{outer}\nlet clip = #{{\n{inner}\n#1s\nburst(target:e,count:1,track:\"one\")\n#2s\n}}\nstretch(clip,{factor})\n#6s"
        ))
    };
    for owned in [true, false] {
        let plain = scene(1, owned);
        let stretched = scene(2, owned);
        let expected = appearance(&plain, 1.25);
        assert!(!expected.is_empty());
        assert!(appearance(&stretched, 1.99).is_empty());
        assert_eq!(expected, appearance(&stretched, if owned { 2.5 } else { 2.25 }));
    }
}

#[test]
#[ignore = "manual CPU random-seek timing of the same leaf with and without a lifecycle"]
fn lifecycle_scrub_benchmark() {
    for (duration, rate) in [(10, 20), (600, 20), (600, 200)] {
        for (kind, animation) in [
            ("static", ""),
            (
                "animated",
                ", particle_animation:(leaf) => leaf.autumn(end_color:hex_color(\"a33832\"))",
            ),
        ] {
            let timeline = checked_scene(&format!(
                "import \"particle-lifecycle.amx\" as art\nconfig {{duration:{duration},canvas_size:(640,360)}}\n#0s\ne:Emitter, particle_shape:art::Leaf, lifetime:2, rate:{rate}, seed:42{animation}\n#{duration}s"
            ));
            println!(
                "{duration}s rate={rate} {kind} CPU seek p95 {:.0} us",
                scrub_p95_us(&timeline, duration)
            );
        }
    }
}

#[test]
fn a_local_fade_action_and_a_literal_plan_can_use_the_closure_receiver() {
    for (animation, reference) in [
        ("(p) => fade_out(target:p,duration:1s)", "fade_out(target:l,duration:1s)"),
        ("(p) => #{ p.opacity = 0 [1s] }", "l.opacity = 0 [1s]"),
    ] {
        let scene = checked_scene(&format!(
            "import \"particle-lifecycle.amx\" as art\n#0s\ne:Emitter, particle_shape:art::Leaf, particle_animation:{animation}, speed:0, seed:42, particle_exit:\"none\", lifetime:2\nburst(target:e,count:1,track:\"one\")"
        ));
        let actor = checked_scene(&format!(
            "import \"particle-lifecycle.amx\" as art\n#0s\nl:art::Leaf\n{reference}"
        ));
        let life = 2. * (0.72 + 0.28 * unit(42, hash("one"), 0, 0, 1));
        let expected = appearance(&actor, 0.7001);
        assert_ne!(expected, appearance(&actor, 0.));
        assert_eq!(expected, appearance(&scene, life * 0.7001));
    }
}
