use super::support::assign;
use super::support::{actor, assert_clean, build_ast, build_clean, keyframe, parse, prop, vec2};
use super::*;
use crate::ast::BinaryOp;

// ── Fixtures ──────────────────────────────────────────────────────────────

fn let_decl(name: &str, value: Expr) -> Stmt {
    Stmt::LetDecl {
        is_pub: false,
        name: name.to_string(),
        value,
        span: None,
    }
}

fn always(body: Vec<Stmt>) -> Stmt {
    Stmt::Always { body, span: None }
}

fn bin(a: Expr, op: BinaryOp, b: Expr) -> Expr {
    Expr::Binary(Box::new(a), op, Box::new(b))
}

fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Call(name.to_string(), args)
}

fn clean_ast(ast: Vec<Stmt>) -> Timeline {
    let report = build_ast(&ast);
    assert_clean(&report);
    report.output
}

/// Expand a source loaded through the module graph, then build it.
fn build_module(path: &str, source: &str) -> Timeline {
    let mut graph = animatix_syntax::module::ModuleGraph::new();
    let program = graph
        .load_program_with_source(std::path::Path::new(path), Some(source))
        .expect("program loads");
    let expanded = program.expand_components(&mut Vec::new());
    clean_ast(expanded)
}

fn build_module_file(path: &std::path::Path) -> Option<Timeline> {
    if !path.exists() {
        return None;
    }
    let mut graph = animatix_syntax::module::ModuleGraph::new();
    let program = graph.load_program(path).expect("program loads");
    let expanded = program.expand_components(&mut Vec::new());
    Some(clean_ast(expanded))
}

type Overrides = std::collections::HashMap<String, std::collections::HashMap<String, Value>>;

/// Execute the compiled `always` programs against one frame environment.
fn run_frame(timeline: &Timeline, time_ms: u64, dims: SceneDimensions) -> (Environment, Overrides) {
    let mut overrides = Overrides::new();
    let mut env = timeline.build_frame_env(time_ms, dims, &overrides);
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(program, time_ms, dims, &mut env, &mut overrides)
            .expect("modifier IR execution should succeed");
    }
    (env, overrides)
}

/// The overrides an `always` program wrote for one frame.
fn run_modifiers(timeline: &Timeline, time_ms: u64, dims: SceneDimensions) -> Overrides {
    run_frame(timeline, time_ms, dims).1
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[test]
fn test_keyframe_scoped_variables_create_tracks() {
    let ast = vec![
        keyframe(0.0, vec![let_decl("freq", Expr::Num(1.0))]),
        keyframe(3.0, vec![let_decl("freq", Expr::Num(1.7))]),
    ];
    let timeline = clean_ast(ast);

    assert!(
        timeline.variable_tracks.contains_key("freq"),
        "Expected variable track for 'freq'"
    );
    let freq_track = timeline.variable_tracks.get("freq").unwrap();
    assert_eq!(freq_track.keyframes.len(), 2, "Expected 2 keyframes for freq");

    // Piecewise-constant: 1.0 until the 3s stamp, 1.7 afterwards.
    for (ms, expected) in [(0, 1.0), (1500, 1.0), (3000, 1.7), (5000, 1.7)] {
        let value = freq_track.evaluate(ms);
        assert!(value.is_some(), "Expected freq value at t={ms}ms");
        assert_eq!(value.unwrap(), Value::Num(expected));
    }
}

#[test]
fn test_keyframe_scoped_variables_injected_into_frame_env() {
    let ast = vec![keyframe(
        0.0,
        vec![
            actor(
                "tracker",
                "Ellipse",
                vec![
                    prop("radius_x", Expr::Num(10.0)),
                    prop("radius_y", Expr::Num(10.0)),
                    prop("at", vec2(0.0, 0.0)),
                ],
            ),
            let_decl("freq", Expr::Num(2.0)),
            always(vec![assign(
                "tracker",
                "at",
                Expr::Tuple(vec![
                    bin(
                        Expr::Num(640.0),
                        BinaryOp::Add,
                        bin(
                            Expr::Num(100.0),
                            BinaryOp::Mul,
                            call(
                                "cos",
                                vec![bin(
                                    Expr::Ident("freq".to_string()),
                                    BinaryOp::Mul,
                                    Expr::Ident("t".to_string()),
                                )],
                            ),
                        ),
                    ),
                    bin(
                        Expr::Num(360.0),
                        BinaryOp::Add,
                        bin(
                            Expr::Num(100.0),
                            BinaryOp::Mul,
                            call(
                                "sin",
                                vec![bin(
                                    Expr::Ident("freq".to_string()),
                                    BinaryOp::Mul,
                                    Expr::Ident("t".to_string()),
                                )],
                            ),
                        ),
                    ),
                ]),
            )]),
        ],
    )];
    let timeline = clean_ast(ast);

    // Evaluate at t=0s — with freq=2.0, cos(2*0)=1, sin(2*0)=0.
    let overrides = run_modifiers(
        &timeline,
        0,
        SceneDimensions {
            width: 1280,
            height: 720,
        },
    );
    let tracker_at = overrides.get("tracker").and_then(|m| m.get("at"));
    assert!(
        tracker_at.is_some(),
        "Expected tracker.at override from always block using freq variable"
    );
    if let Some(Value::Vec2([x, y])) = tracker_at {
        assert!((x - 740.0).abs() < 0.1, "Expected x≈740, got {}", x);
        assert!((y - 360.0).abs() < 0.1, "Expected y≈360, got {}", y);
    } else {
        panic!("Expected Vec2 override for tracker.at, got {:?}", tracker_at);
    }
}

#[test]
fn always_object_field_writes_update_frame_environment() {
    let ast = vec![keyframe(
        0.0,
        vec![
            let_decl(
                "p",
                Expr::Construct(
                    "Point".to_string(),
                    vec![prop("x", Expr::Num(10.0)), prop("y", Expr::Num(20.0))],
                ),
            ),
            always(vec![
                assign("p", "x", Expr::Num(30.0)),
                let_decl(
                    "q",
                    bin(
                        Expr::Path(vec!["p".to_string(), "x".to_string()]),
                        BinaryOp::Add,
                        Expr::Num(1.0),
                    ),
                ),
            ]),
        ],
    )];
    let timeline = clean_ast(ast);
    let (env, _) = run_frame(&timeline, 0, SceneDimensions::default());

    match env.get("p") {
        Some(Value::Object(name, fields)) => {
            assert_eq!(name, "Point");
            assert_eq!(fields["x"], Value::Num(30.0));
            assert_eq!(fields["y"], Value::Num(20.0));
        },
        other => panic!("Expected Point object, got {:?}", other),
    }
    assert_eq!(env.get("q"), Some(Value::Num(31.0)));
}

#[test]
fn build_time_for_loop_let_shadowing_carries_algorithm_state() {
    let timeline = build_clean(
        r#"
#0s
let arr = {2, 1}
for i in {0} {
  if arr[0] > arr[1] {
    let arr = list_swap(arr, 0, 1)
  }
  let done = arr[0] == 1
}
"#,
    );

    let arr = timeline.variable_tracks.get("arr").expect("arr track");
    assert_eq!(
        arr.evaluate(0),
        Some(Value::List(vec![Value::Num(1.0), Value::Num(2.0)].into())),
        "the shadowing let should carry list_swap state into the same build pass"
    );
    let done = timeline.variable_tracks.get("done").expect("done track");
    assert_eq!(
        done.evaluate(0),
        Some(Value::Num(1.0)),
        "later statements in the same keyframe should read the shadowed value"
    );
}

#[test]
fn build_time_for_loop_step_sequences_swap_actions() {
    // A `[step: 300ms]` for loop advances the build-time clock per iteration,
    // so variable-index swap actions land on distinct keyframe times instead
    // of colliding at the loop start.
    let timeline = build_clean(
        r#"
config { dynamic_layout: true }
row: Row, at: (640, 440), gap: 16 {
  for k in {0, 1, 2} {
    b[k]: Rect, size: (70, 80), color: blue, opacity: 1
  }
}
#0s
let arr = {3, 2, 1}
for i in {0, 1} [step: 300ms] {
  if arr[i] > arr[i+1] {
    swap row.b[i], row.b[i+1] [300ms]
    let arr = list_swap(arr, i, i+1)
  }
}
"#,
    );

    // Two swaps: t=0..300 (b0<->b1) and t=300..600 (b1<->b2).
    let track = &timeline.child_order("row").expect("row child_orders track").track;
    let orders = track.keyframes.keys().copied().collect::<Vec<_>>();
    assert_eq!(orders, vec![0, 300, 600], "swap keyframes must advance per step");

    // The algorithm state carried through the loop updates per iteration:
    // {3,2,1} -> swap(0,1) -> {2,3,1} at t=0, then swap(1,2) -> {2,1,3} at t=300.
    let arr = timeline.variable_tracks.get("arr").expect("arr track");
    assert_eq!(
        arr.evaluate(0),
        Some(Value::List(vec![Value::Num(2.0), Value::Num(3.0), Value::Num(1.0)].into())),
        "the first iteration should swap indices 0 and 1"
    );
    assert_eq!(
        arr.evaluate(600),
        Some(Value::List(vec![Value::Num(2.0), Value::Num(1.0), Value::Num(3.0)].into())),
        "the second iteration should swap indices 1 and 2"
    );
}

#[test]
fn build_time_indexed_assignment_resolves_against_loop_variables() {
    // `row.b[j].color = red` inside a for loop resolves `j` against the build
    // environment into the concrete `b__N` track.
    let timeline = build_clean(
        r#"
config { dynamic_layout: true }
row: Row, at: (640, 440), gap: 16 {
  for k in {0, 1, 2} {
    b[k]: Rect, size: (70, 80), color: blue, opacity: 1
  }
}
#0s
for j in {0, 2} {
  row.b[j].color = red [200ms]
}
"#,
    );
    for key in ["b__0", "b__2"] {
        let track = timeline.world.tracks.get_id(key).unwrap_or_else(|| panic!("{key} track"));
        assert_eq!(
            timeline.world.style(track).color.get(200, [1.0, 1.0, 1.0, 1.0]),
            [1.0, 0.0, 0.0, 1.0],
            "indexed assignment should write the resolved actor by the end of its duration"
        );
    }
}

#[test]
fn always_nested_object_field_writes_update_frame_environment() {
    let ast = vec![keyframe(
        0.0,
        vec![
            let_decl(
                "p",
                Expr::Construct(
                    "Point".to_string(),
                    vec![prop(
                        "a",
                        Expr::Construct(
                            "Inner".to_string(),
                            vec![prop("b", Expr::Num(10.0)), prop("c", Expr::Num(20.0))],
                        ),
                    )],
                ),
            ),
            always(vec![
                Stmt::Assignment {
                    target: vec![
                        crate::ast::TargetSegment::Static("p".to_string()),
                        crate::ast::TargetSegment::Static("a".to_string()),
                    ],
                    property: "b".to_string(),
                    value: Expr::Num(30.0),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                },
                let_decl(
                    "q",
                    bin(
                        Expr::Path(vec!["p".to_string(), "a".to_string(), "b".to_string()]),
                        BinaryOp::Add,
                        Expr::Num(1.0),
                    ),
                ),
            ]),
        ],
    )];
    let timeline = clean_ast(ast);
    let (env, _) = run_frame(&timeline, 0, SceneDimensions::default());

    assert_eq!(env.get("q"), Some(Value::Num(31.0)));
    match env.get("p") {
        Some(Value::Object(_, fields)) => match &fields["a"] {
            Value::Object(name, inner) => {
                assert_eq!(name, "Inner");
                assert_eq!(inner["b"], Value::Num(30.0));
                assert_eq!(inner["c"], Value::Num(20.0));
            },
            other => panic!("Expected Inner object, got {:?}", other),
        },
        other => panic!("Expected Point object, got {:?}", other),
    }
}

#[test]
fn pure_fn_calls_evaluate_in_expressions() {
    // `fn` declarations with `-> Type` are evaluated at build time when their
    // calls appear in `let` expressions; `return` unwinds the body.
    let timeline = build_clean(
        r#"
fn dnf(arr: List<Num>) -> List<Num> {
  let arr = list_swap(arr, 0, 2)
  return arr
}
fn total(xs: List<Num>) -> Num {
  let a = xs[0]
  let b = xs[1]
  if a > b {
    return a + b
  } else {
    return a * b
  }
}
#0s
let sorted = dnf({3, 2, 1})
let summed = total(sorted)
"#,
    );
    let sorted = timeline.variable_tracks.get("sorted").expect("sorted track");
    assert_eq!(
        sorted.evaluate(0),
        Some(Value::List(vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)].into())),
        "dnf should swap indices 0 and 2"
    );
    let summed = timeline.variable_tracks.get("summed").expect("summed track");
    assert_eq!(
        summed.evaluate(0),
        Some(Value::Num(2.0)),
        "total(1,2,3) should return 1*2 via the else branch"
    );
}

#[test]
fn timeline_fn_expands_with_block_scope_without_leaking_locals() {
    // A timeline function body's local `let` must not leak into the scene
    // after the call, and must not write scene variable tracks.
    let timeline = build_module(
        "fn_scope.amx",
        r#"
config { colorscheme: "editorial-dark" }
fn pulse(strength: Num = 1.0) {
  let local = strength * 2
  self.scale = local [100ms]
}
btn: Rect, size: (100, 50), color: blue
#0s
fade-in btn [300ms]
#1s
pulse btn [strength: 1.5]
"#,
    );
    assert!(
        !timeline.variable_tracks.contains_key("local"),
        "function-local let must not create a scene variable track"
    );
    // The scale keyframe written through the fn body targets the actor;
    // scale is stored in the transform track as [s,0,0,s,0,0].
    let track = timeline.world.tracks.get_id("btn").expect("btn track");
    let geometry = timeline.world.geometry(track);
    assert_eq!(
        geometry.scale.get(1100, 1.0),
        3.0,
        "pulse body should write scale = strength * 2 = 3.0 at t=1.1s"
    );
}

#[test]
fn timeline_fn_drives_algorithm_sort() {
    // The insertion-sort loop extracted into a timeline function produces the
    // same swap sequence as the inlined version (module expansion + fn blocks).
    let timeline = build_module(
        "fn_sort.amx",
        r#"
config { colorscheme: "editorial-dark", dynamic_layout: true }
fn bubble_sort(bars, values: List<Num>) {
  let arr = values
  for key in {1, 2, 3} [step: 1200ms] {
    for j in {0, 1, 2} [step: 300ms] {
      if j < key && arr[key - j] < arr[key - j - 1] {
        swap bars[j], bars[j+1] [300ms]
        let arr = list_swap(arr, key - j, key - j - 1)
      }
    }
  }
}
row: Row, at: (640, 440), gap: 16 {
  for k in {0,1,2,3} {
    b[k]: Rect, size: (70, 80), color: blue, opacity: 1
  }
}
#0s
let vals = {4, 1, 3, 2}
bubble_sort(b, vals)
"#,
    );
    // 4 swaps (1 + 1 + 2) land on the step-sequenced timeline: times are
    // pass_start + j*300 for the swaps in each pass.
    let track = &timeline.child_order("row").expect("row child_orders").track;
    let orders = track.keyframes.keys().copied().collect::<Vec<_>>();
    assert_eq!(
        orders,
        vec![0, 300, 1200, 1500, 2400, 2700, 3000],
        "fn-emitted swaps must follow the step-sequenced passes"
    );
}

#[test]
fn timeline_fn_dnf_swaps_resolve_index_params() {
    // DNF partition inside a timeline function: `zero`/`i`/`two` are value
    // parameters let-bound into the block, and `swap row.b[zero], row.b[i]`
    // target indices resolve against those locals at build time.
    let timeline = build_module(
        "dnf_fn.amx",
        r#"
config { colorscheme: "editorial-dark", dynamic_layout: true }
fn dnf_pass(arr: List<Num>, zero: Num, i: Num, two: Num) {
  let arr = arr
  let zero = zero
  let i = i
  let two = two
  for step in {0, 1, 2, 3, 4} [step: 700ms] {
    if i <= two {
      match arr[i] {
        0 => {
          swap row.b[zero], row.b[i] [600ms]
          let arr = list_swap(arr, zero, i)
          let zero = zero + 1
          let i = i + 1
        }
        1 => { let i = i + 1 }
        2 => {
          swap row.b[i], row.b[two] [600ms]
          let arr = list_swap(arr, i, two)
          let two = two - 1
        }
        _ => {}
      }
    }
  }
}
row: Row, at: (640, 440), gap: 16 {
  for k in {0,1,2,3,4,5} {
    b[k]: Rect, size: (70, 80), color: blue, opacity: 1
  }
}
#0s
let arr = {2, 0, 1, 2, 1, 0}
dnf_pass(arr, 0, 0, 5)
"#,
    );
    // Effective swaps: (0,5) at t=0 and (3,4) at t=2800; the same-index
    // swaps are no-ops but still emit child-order keyframes.
    let track = &timeline.child_order("row").expect("row child_orders").track;
    let orders = track.keyframes.keys().copied().collect::<Vec<_>>();
    assert!(!orders.is_empty(), "dnf fn must emit swap keyframes");
    // Final child order after the swaps: [0,0,1,1,2,2] values map to slots
    // b__0..b__5.
    let final_order = timeline.get_child_order("row", *orders.last().expect("last keyframe"));
    assert_eq!(
        final_order,
        vec!["b__5", "b__1", "b__2", "b__4", "b__3", "b__0"],
        "dnf swaps (0,5) then (3,4) must leave values 0,0,1,1,2,2 left to right"
    );
}

#[test]
fn timeline_fn_dotted_targets_resolve_against_instance() {
    // Timeline function whose targets are dotted (`bars.bar[j]`) against a
    // component instance: the label parameter substitutes into the target
    // string and indices resolve against the loop locals.
    let timeline = build_module(
        "fn_dotted.amx",
        r#"
config { colorscheme: "editorial-dark", dynamic_layout: true }
fn bubble_sort(bars, values: List<Num>) {
  let arr = values
  for key in {1, 2, 3} [step: 1200ms] {
    for j in {0, 1, 2} [step: 300ms] {
      if j < key && arr[key - j] < arr[key - j - 1] {
        swap bars.bar[key - j], bars.bar[key - j - 1] [300ms]
        let arr = list_swap(arr, key - j, key - j - 1)
      }
    }
  }
}
pub component Bars(values: List<Num>, colors: List<Color>, at: Vec2 = (640, 440)) {
  row: Row, at: at, gap: 8 {
    for v, i in values {
      bar[i]: Rect, size: (60, v * 30), color: colors[i], opacity: 1
    }
  }
}
bars: Bars, values: {4, 1, 3, 2}, colors: {(0.4, 0.8, 1.0, 1.0), (1.0, 0.7, 0.3, 1.0), (0.4, 0.8, 1.0, 1.0), (0.4, 0.8, 1.0, 1.0)}
#0s
let vals = {4, 1, 3, 2}
bubble_sort(bars, vals)
"#,
    );
    // The component instance's row track receives the swaps.
    let (row_id, _) = timeline.child_orders_iter().next().expect("row child_orders");
    let row_key = timeline.get_entity_label(row_id).expect("row label").to_string();
    let track = &timeline.child_order(&row_key).expect("row child_orders").track;
    let times = track.keyframes.keys().copied().collect::<Vec<_>>();
    assert_eq!(
        times,
        vec![0, 300, 1200, 1500, 2400, 2700, 3000],
        "dotted-target fn swaps must follow the insertion-sort passes"
    );
}

#[test]
fn pure_fn_tail_expression_returns_value() {
    // A pure function's final expression is its return value (Rust style);
    // `return` statements still work for early unwinding.
    let timeline = build_clean(
        r#"
fn sum2(xs: List<Num>) -> Num {
  let a = xs[0]
  let b = xs[1]
  a + b
}
fn guard(n: Num) -> Num {
  if n < 0 {
    return 0
  }
  n * 2
}
#0s
let s = sum2({3, 4})
let g = guard(-5)
let h = guard(21)
"#,
    );
    let s = timeline.variable_tracks.get("s").expect("s track");
    assert_eq!(s.evaluate(0), Some(Value::Num(7.0)), "tail expr 3+4");
    let g = timeline.variable_tracks.get("g").expect("g track");
    assert_eq!(g.evaluate(0), Some(Value::Num(0.0)), "early return 0");
    let h = timeline.variable_tracks.get("h").expect("h track");
    assert_eq!(h.evaluate(0), Some(Value::Num(42.0)), "tail expr 21*2");
}

#[test]
fn always_block_calls_pure_fn_at_frame_time() {
    // Pure functions are callable from `always` bodies: the IR executor
    // resolves the UserFn through the frame environment.
    let timeline = build_clean(
        r#"
fn double(n: Num) -> Num {
  n * 2
}
box: Rect, size: (100, 50), color: blue
#0s
fade-in box [300ms]
always {
  box.scale = double(3) [0ms]
}
"#,
    );
    let overrides = run_modifiers(&timeline, 500, SceneDimensions::default());
    let box_overrides = overrides.get("box").expect("box overrides");
    let scale = box_overrides.get("scale").expect("box.scale override");
    assert_eq!(scale, &Value::Num(6.0), "always must evaluate double(3) = 6 at frame time");
}

#[test]
fn pub_fn_imports_across_files() {
    // `pub fn` declarations in an imported module are available to the entry
    // file through the flattened module statements.
    let lib = r#"
pub fn dnf(arr: List<Num>) -> List<Num> {
  let arr = list_swap(arr, 0, 2)
  return arr
}
pub fn bump(v: Num, f: Num) -> Num {
  v * f
}
"#;
    let scene = r#"
import "./lib.amx"
#0s
let sorted = dnf({3, 2, 1})
let scaled = bump(2, 4)
"#;
    let dir = std::env::temp_dir().join(format!("animatix_pubfn_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");
    std::fs::write(dir.join("lib.amx"), lib).expect("write lib");
    let scene_path = dir.join("scene.amx");
    std::fs::write(&scene_path, scene).expect("write scene");
    let timeline = build_module_file(&scene_path).expect("scene builds");
    let _ = std::fs::remove_dir_all(&dir);

    let sorted = timeline.variable_tracks.get("sorted").expect("sorted");
    assert_eq!(
        sorted.evaluate(0),
        Some(Value::List(vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)].into())),
        "imported dnf must swap indices 0 and 2"
    );
    let scaled = timeline.variable_tracks.get("scaled").expect("scaled");
    assert_eq!(scaled.evaluate(0), Some(Value::Num(8.0)), "imported bump(2,4)");
}

#[test]
fn sort_colors_demo_file_produces_dnf_sequence() {
    // Guard the actual demo file: the DNF timeline function must emit the
    // partition swaps on the step-sequenced timeline and end with the
    // values 0,0,1,1,2,2 left to right.
    let workspace = std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
    let demo_path =
        std::path::Path::new(&workspace).join("../../examples/projects/leetcode_sort_colors.amx");
    let Some(timeline) = build_module_file(&demo_path) else {
        eprintln!("skipping: demo file not found at {demo_path:?}");
        return;
    };
    let track = &timeline.child_order("row").expect("row child_orders from the demo").track;
    let last = *track.keyframes.keys().max().expect("last keyframe");
    let final_order = timeline.get_child_order("row", last);
    assert_eq!(
        final_order,
        vec!["b__5", "b__1", "b__2", "b__4", "b__3", "b__0"],
        "DNF partition must leave values 0,0,1,1,2,2 left to right"
    );
}

#[test]
fn extension_actor_at_position_regression() {
    // A stub extension primitive with an `at` position: does the position
    // track get written for extension types?
    struct StubPrim;
    impl crate::primitives::Primitive for StubPrim {
        fn type_name(&self) -> &str {
            "StubPulse"
        }
        fn build(
            &self,
            ctx: &mut crate::primitives::BuildCtx,
            label: &str,
            _props: &[animatix_syntax::ast::Property],
            _modifiers: &[animatix_syntax::ast::Modifier],
            _children: &[animatix_syntax::ast::InlineItem],
        ) -> Result<(), Vec<animatix_syntax::diagnostics::Diagnostic>> {
            let track_id = ctx.timeline.world.spawn(label, "StubPulse");
            ctx.timeline.world.rebuild_property_plan(track_id);
            Ok(())
        }
    }
    let source = r#"
config { colorscheme: "editorial-dark" }
p: StubPulse, at: (100, 100), size: (50, 50)
"#;
    let ast = parse(source);
    let mut ctx = crate::extension_context::ExtensionContext::new();
    ctx.register_primitive(
        std::sync::Arc::new(StubPrim),
        animatix_std::PrimitiveInfo::extension(
            "StubPrim",
            crate::timeline::ActorCategory::Annotation,
        ),
    )
    .expect("register");
    let report = crate::composition::BuildTarget::build_with_options(
        &ast,
        &std::collections::HashMap::new(),
        None,
        crate::timeline::BuildOptions::default().with_context(std::sync::Arc::new(ctx)),
    );
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let crate::composition::BuildTarget::SingleScene(timeline) = report.output else {
        panic!("expected single scene");
    };
    let id = timeline.world.get_id("p").expect("p track");
    let geometry = timeline.world.geometry(id);
    // Regression: extension actors must honor the common `at` property like
    // built-ins (previously the position stayed [0,0], so native text and
    // image commands rendered at the scene origin).
    assert_eq!(
        geometry.position.get(0, [0.0, 0.0]),
        [100.0, 100.0],
        "extension actor must apply `at`"
    );
}
