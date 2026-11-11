use super::*;
use crate::frame_source::Box2;
use crate::review::facts::{
    ActorFact, ActorFrameFact, BuildDiagnosticFact, FrameFact, ReviewFacts, TargetFacts,
};
use crate::review::markers::MarkerSet;
use crate::review::types::{
    Policy, RenderFacts, RenderSampleFacts, RenderStatus, Severity, TargetKind,
};

fn base_facts() -> ReviewFacts {
    ReviewFacts {
        target: TargetFacts {
            kind: TargetKind::SingleScene,
            duration_s: 2.0,
            width: 100,
            height: 100,
        },
        frames: Vec::new(),
        actors: Vec::new(),
        actions: Vec::new(),
        probe_frames: Vec::new(),
        build_diagnostics: Vec::new(),
        redundant_keyframes: Vec::new(),
        has_modifier_programs: false,
        render: None,
        site_rest: None,
    }
}

fn actor(label: &str) -> ActorFact {
    ActorFact {
        scene: "main".into(),
        label: label.into(),
        source_line: Some(3),
        parent: None,
        ancestors: Vec::new(),
        group_ancestors: Vec::new(),
        stack_ancestors: Vec::new(),
        effect_scope_ancestors: Vec::new(),
        parent_is_stack: false,
        max_effective_opacity: 0.0,
        visual: true,
        is_group_like: false,
        category: animatix::timeline::ActorCategory::Shape,
        is_text: false,
        is_plot: false,
        is_effect_scope: false,
        is_callout: false,
        is_legend: false,
        is_non_content: false,
        is_background: false,
        in_stack: false,
    }
}

fn actor_frame(label: &str, opacity: f32, bounds: Option<Box2>) -> ActorFrameFact {
    ActorFrameFact {
        scene: "main".into(),
        label: label.into(),
        opacity: Some(opacity),
        bounds,
        on_frame_fraction: if bounds.is_some() { 1.0 } else { 0.0 },
        padded_on_frame_fraction: if bounds.is_some() { 1.0 } else { 0.0 },
        draw_items: bounds.map(|_| 1),
        draw_commands: bounds.map(|_| 1),
        text_max_width: None,
        text_overflow: None,
        measured_text_width: None,
    }
}

fn frame(time_s: f64, actors: Vec<ActorFrameFact>) -> FrameFact {
    FrameFact {
        time_s,
        scene: "main".into(),
        ink_fraction: None,
        state_digest: None,
        cast: actors.len(),
        actors,
        draw_order: Vec::new(),
    }
}

#[test]
fn opacity_rule_flags_actors_never_positive() {
    let mut facts = base_facts();
    facts.actors = vec![actor("ghost"), actor("solid")];
    facts.frames = vec![frame(
        0.0,
        vec![
            actor_frame("ghost", 0.0, None),
            actor_frame("solid", 1.0, None),
        ],
    )];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "actor.opacity-never-positive"
        && issue.actor.as_deref() == Some("ghost")));
    assert!(!run.issues.iter().any(|issue| issue.actor.as_deref() == Some("solid")));
}

#[test]
fn never_on_frame_flags_fully_off_canvas_bounds() {
    let mut facts = base_facts();
    facts.actors = vec![actor("offscreen")];
    let mut offscreen = actor_frame("offscreen", 1.0, Some(Box2::new(200.0, 200.0, 250.0, 250.0)));
    offscreen.on_frame_fraction = 0.0;
    offscreen.padded_on_frame_fraction = 0.0;
    facts.frames = vec![frame(0.0, vec![offscreen])];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "actor.never-on-frame"));
}

#[test]
fn clipped_rule_prefers_warning_for_text() {
    let mut facts = base_facts();
    let mut text = actor("caption");
    text.is_text = true;
    facts.actors = vec![text];
    let mut frame_actor = actor_frame("caption", 1.0, Some(Box2::new(-10.0, 10.0, 50.0, 50.0)));
    frame_actor.on_frame_fraction = 0.8;
    facts.frames = vec![
        frame(0.0, vec![frame_actor.clone()]),
        frame(1.0, vec![frame_actor]),
    ];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    let issue = run
        .issues
        .iter()
        .find(|issue| issue.rule_id == "actor.clipped")
        .expect("clipped issue");
    assert_eq!(issue.severity, Severity::Warning);
}

#[test]
fn overlap_rule_skips_ancestor_and_same_stack_pairs() {
    let mut facts = base_facts();
    let mut parent = actor("card");
    parent.visual = false;
    let mut child = actor("label");
    child.ancestors = vec!["card".into()];
    let mut sibling = actor("badge");
    sibling.parent = Some("card".into());
    sibling.parent_is_stack = true;
    child.parent = Some("card".into());
    facts.actors = vec![parent, child, sibling];
    facts.frames = vec![frame(
        0.0,
        vec![
            actor_frame("label", 1.0, Some(Box2::new(0.0, 0.0, 50.0, 50.0))),
            actor_frame("badge", 1.0, Some(Box2::new(10.0, 10.0, 40.0, 40.0))),
        ],
    )];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(!run.issues.iter().any(|issue| issue.rule_id == "overlap.candidates"));
}

#[test]
fn overlap_rule_flags_independent_content() {
    let mut facts = base_facts();
    facts.actors = vec![actor("a"), actor("b")];
    facts.frames = vec![
        frame(
            0.0,
            vec![
                actor_frame("a", 1.0, Some(Box2::new(0.0, 0.0, 50.0, 50.0))),
                actor_frame("b", 1.0, Some(Box2::new(10.0, 10.0, 40.0, 40.0))),
            ],
        ),
        frame(
            1.0,
            vec![
                actor_frame("a", 1.0, Some(Box2::new(0.0, 0.0, 50.0, 50.0))),
                actor_frame("b", 1.0, Some(Box2::new(10.0, 10.0, 40.0, 40.0))),
            ],
        ),
    ];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "overlap.undeclared"));
}

#[test]
fn text_overflow_rule_uses_max_width_and_overflow_mode() {
    let mut facts = base_facts();
    let mut text = actor("title");
    text.is_text = true;
    facts.actors = vec![text];
    let mut frame_actor = actor_frame("title", 1.0, Some(Box2::new(0.0, 0.0, 200.0, 20.0)));
    frame_actor.text_max_width = Some(100.0);
    frame_actor.text_overflow = Some("visible".into());
    frame_actor.measured_text_width = Some(200.0);
    facts.frames = vec![
        frame(0.0, vec![frame_actor.clone()]),
        frame(1.0, vec![frame_actor]),
    ];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    let issue = run
        .issues
        .iter()
        .find(|issue| issue.rule_id == "text.overflow")
        .expect("text overflow");
    assert_eq!(issue.severity, Severity::Warning);
}

#[test]
fn entrance_rule_flags_actor_not_visible_after_action() {
    let mut facts = base_facts();
    facts.actors = vec![actor("title")];
    facts.actions = vec![crate::review::facts::ActionFact {
        scene: "main".into(),
        verb: "fade-in".into(),
        targets: vec!["title".into()],
        start_s: 0.0,
        duration_s: 1.0,
        category: animatix::timeline::ActionCategory::Entrance,
    }];
    facts.frames = vec![
        frame(0.0, vec![actor_frame("title", 0.0, None)]),
        frame(1.05, vec![actor_frame("title", 0.0, None)]),
    ];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "entrance.not-visible"));
}

#[test]
fn never_revealed_rule_maps_build_diagnostic() {
    let mut facts = base_facts();
    facts.build_diagnostics = vec![BuildDiagnosticFact {
        code: "never-revealed".into(),
        severity: Severity::Warning,
        message: "Actor `ghost` is declared before any keyframe".into(),
        actor: Some("ghost".into()),
        source_line: None,
    }];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "build.never-revealed"));
}

#[test]
fn render_rules_skip_without_render_facts() {
    let facts = base_facts();
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert_eq!(run.skipped, 6);
    assert_eq!(
        run.skipped_rules,
        vec![
            "actor.ink-missing",
            "actor.reveal-missing",
            "frame.blank",
            "beat.static",
            "transition.endpoint",
            "text.occluded",
        ]
    );
    assert!(run.issues.iter().all(|issue| !issue.rule_id.starts_with("frame.")));
}

#[test]
fn frame_blank_rule_uses_render_ink() {
    let mut facts = base_facts();
    facts.render = Some(RenderFacts {
        status: RenderStatus::Rendered,
        reason: "test".into(),
        samples: vec![RenderSampleFacts {
            time_s: 1.0,
            ink_fraction: 0.0,
            changed_from_prev: None,
            actor_ink: Default::default(),
            transition: None,
            occlusions: Vec::new(),
        }],
        site_rest: None,
    });
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "frame.blank"));
}

#[test]
fn verify_draw_missing_flags_empty_commands() {
    let mut facts = base_facts();
    facts.actors = vec![actor("box")];
    let mut box_frame = actor_frame("box", 1.0, Some(Box2::new(0.0, 0.0, 50.0, 50.0)));
    box_frame.draw_items = Some(1);
    box_frame.draw_commands = Some(0);
    facts.frames = vec![frame(0.0, vec![box_frame])];
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "actor.draw-missing"));
}

#[test]
fn verify_ink_missing_flags_positive_opacity_without_ink() {
    let mut facts = base_facts();
    facts.actors = vec![actor("box")];
    let mut box_frame = actor_frame("box", 1.0, Some(Box2::new(0.0, 0.0, 50.0, 50.0)));
    // Engine emitted no draw commands: this is the "declared but paints
    // nothing" case the rule exists for (a drawing actor is trusted instead).
    box_frame.draw_commands = Some(0);
    facts.frames = vec![frame(0.0, vec![box_frame])];
    facts.render = Some(RenderFacts {
        status: RenderStatus::Rendered,
        reason: "test".into(),
        samples: vec![RenderSampleFacts {
            time_s: 0.0,
            ink_fraction: 0.0,
            changed_from_prev: None,
            actor_ink: Default::default(),
            transition: None,
            occlusions: Vec::new(),
        }],
        site_rest: None,
    });
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "actor.ink-missing"));
}

#[test]
fn site_rest_rule_ports_the_035_plateau() {
    let mut facts = base_facts();
    facts.render = Some(RenderFacts {
        status: RenderStatus::Rendered,
        reason: "test".into(),
        samples: vec![],
        site_rest: Some(crate::review::types::SiteRestFacts {
            held_time_s: 2.0,
            held_ink_fraction: 0.01,
            peak_ink_fraction: 0.10,
            peak_time_s: 1.0,
        }),
    });
    facts.site_rest = facts.render.as_ref().and_then(|render| render.site_rest.clone());
    let run = evaluate(&facts, &MarkerSet::default(), Policy::Site);
    assert!(run.issues.iter().any(|issue| issue.rule_id == "rest.cast-erased"));
}

#[test]
fn test_render_not_rendered_zero_output() {
    for status in [RenderStatus::Skipped, RenderStatus::Failed] {
        let mut facts = base_facts();
        facts.actors = vec![actor("box")];
        let box_frame = actor_frame("box", 1.0, Some(Box2::new(0.0, 0.0, 50.0, 50.0)));
        facts.frames = vec![frame(0.0, vec![box_frame])];
        facts.render = Some(RenderFacts {
            status,
            reason: "test".into(),
            samples: vec![RenderSampleFacts {
                time_s: 0.0,
                ink_fraction: 0.0,
                changed_from_prev: None,
                actor_ink: Default::default(),
                transition: None,
                occlusions: Vec::new(),
            }],
            site_rest: None,
        });
        let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);
        let render_rule_ids = [
            "frame.blank",
            "beat.static",
            "transition.endpoint",
            "text.occluded",
            "text.occluded-full",
            "actor.ink-missing",
            "actor.reveal-missing",
        ];
        for id in render_rule_ids {
            assert!(
                !run.issues.iter().any(|issue| issue.rule_id == id),
                "expected no issue for {id} when render status is {status:?}"
            );
        }
    }
}

#[test]
fn test_overlap_aggregation_cap_24_and_cluster_warning_in_generated_policy() {
    let mut facts = base_facts();
    let mut actors = Vec::new();
    let mut actor_frames = Vec::new();
    for i in 0..10 {
        let name = format!("act_{i}");
        let mut a = actor(&name);
        a.source_line = Some(10 + i);
        actors.push(a);
        actor_frames.push(actor_frame(
            &name,
            1.0,
            Some(Box2::new(0.0, 0.0, 50.0 + i as f64, 50.0 + i as f64)),
        ));
    }
    facts.actors = actors;
    facts.frames = vec![frame(0.5, actor_frames)];

    let run = evaluate(&facts, &MarkerSet::default(), Policy::Default);

    let undeclared: Vec<_> =
        run.issues.iter().filter(|i| i.rule_id == "overlap.undeclared").collect();
    assert_eq!(undeclared.len(), 24);
    for u in &undeclared {
        assert_eq!(u.severity, Severity::Info);
    }

    let clusters: Vec<_> = run.issues.iter().filter(|i| i.rule_id == "overlap.cluster").collect();
    assert_eq!(clusters.len(), 1);
    let cluster = clusters[0];
    assert_eq!(cluster.severity, Severity::Info);
    assert_eq!(cluster.actor, None);
    assert_eq!(cluster.evidence["pair_count"], 45);
    assert_eq!(cluster.evidence["kept"], 24);
    assert_eq!(cluster.evidence["relation"], "independent");
    assert_eq!(
        cluster.evidence["sample_pairs"].as_array().expect("sample_pairs array").len(),
        5
    );
}

#[test]
fn test_generated_batch_relation() {
    let mut comp_child = actor("comp.child");
    comp_child.source_line = Some(10);
    let mut plain_box = actor("box");
    plain_box.source_line = Some(20);
    let dummy_frame = frame(0.0, vec![]);

    assert_eq!(
        structural::overlap_relation(&comp_child, &plain_box, &dummy_frame),
        "generated_batch"
    );
    assert_eq!(
        structural::overlap_relation(&plain_box, &comp_child, &dummy_frame),
        "generated_batch"
    );

    let mut none_a = actor("spark_0");
    none_a.source_line = None;
    let mut none_b = actor("spark_1");
    none_b.source_line = None;
    assert_eq!(structural::overlap_relation(&none_a, &none_b, &dummy_frame), "generated_batch");

    let mut plain_a = actor("box_a");
    plain_a.source_line = Some(1);
    let mut plain_b = actor("box_b");
    plain_b.source_line = Some(2);
    assert_eq!(structural::overlap_relation(&plain_a, &plain_b, &dummy_frame), "independent");

    let mut stack_actor = actor("stack_item");
    stack_actor.in_stack = true;
    let mut background_actor = actor("bg");
    background_actor.is_background = true;
    assert_eq!(
        structural::overlap_relation(&stack_actor, &background_actor, &dummy_frame),
        "stack_layer"
    );

    let content_actor = actor("content");
    let mut decoration_actor = actor("decoration");
    decoration_actor.is_non_content = true;
    assert_eq!(
        structural::overlap_relation(&content_actor, &decoration_actor, &dummy_frame),
        "decoration_layer"
    );
}

#[test]
fn test_brief_json_filters_info_explained_and_redundant_unless_waived() {
    let report = crate::review::report::AuditReport {
        input: "demo.amx".into(),
        kind: "audit",
        target: TargetFacts {
            kind: TargetKind::SingleScene,
            duration_s: 1.0,
            width: 64,
            height: 32,
        },
        render: RenderFacts {
            status: RenderStatus::Skipped,
            reason: "--render never".into(),
            samples: Vec::new(),
            site_rest: None,
        },
        sample_times_s: vec![0.0],
        frames: Vec::new(),
        findings: vec![
            RuleIssue {
                rule_id: "overlap.explained",
                severity: Severity::Info,
                persistence: crate::review::types::Persistence::Held,
                scene: None,
                time_s: Some(0.0),
                actor: Some("a, b".into()),
                source_line: None,
                message: "explained overlap".into(),
                evidence: serde_json::json!({}),
                fix_hint: None,
                waived_by: None,
                suggestion: "".into(),
                baseline_state: None,
            },
            RuleIssue {
                rule_id: "keyframe.redundant",
                severity: Severity::Info,
                persistence: crate::review::types::Persistence::Held,
                scene: None,
                time_s: Some(0.0),
                actor: Some("a".into()),
                source_line: None,
                message: "redundant keyframe".into(),
                evidence: serde_json::json!({}),
                fix_hint: None,
                waived_by: None,
                suggestion: "".into(),
                baseline_state: None,
            },
            RuleIssue {
                rule_id: "overlap.explained",
                severity: Severity::Info,
                persistence: crate::review::types::Persistence::Held,
                scene: None,
                time_s: Some(0.0),
                actor: Some("c, d".into()),
                source_line: None,
                message: "waived explained overlap".into(),
                evidence: serde_json::json!({}),
                fix_hint: None,
                waived_by: Some("// layout: allow-overlap".into()),
                suggestion: "".into(),
                baseline_state: None,
            },
            RuleIssue {
                rule_id: "actor.clipped",
                severity: Severity::Warning,
                persistence: crate::review::types::Persistence::Held,
                scene: None,
                time_s: Some(0.0),
                actor: Some("e".into()),
                source_line: None,
                message: "clipped".into(),
                evidence: serde_json::json!({}),
                fix_hint: None,
                waived_by: None,
                suggestion: "".into(),
                baseline_state: None,
            },
        ],
        summary: crate::review::report::Summary {
            errors: 0,
            warnings: 1,
            infos: 3,
            candidates: 3,
            skipped: 0,
            waived: 1,
        },
        skipped: 0,
        coverage: crate::review::report::Coverage {
            structural: "complete",
            render: "skipped",
            evidence: "skipped",
            complete: false,
            skipped_rules: Vec::new(),
            checks_run: 10,
            checks_skipped: 0,
        },
        delta: None,
        budget: None,
    };

    let brief = report.to_brief_json(10, false);
    let findings = brief["anchors"].as_array().expect("anchors array");
    assert_eq!(findings.len(), 2);
    let codes: Vec<_> = findings.iter().map(|f| f["code"].as_str().unwrap()).collect();
    assert!(codes.contains(&"overlap.explained"));
    assert!(codes.contains(&"actor.clipped"));
    assert!(!codes.contains(&"keyframe.redundant"));
    assert_eq!(brief["summary"]["infos"], 3);
}
