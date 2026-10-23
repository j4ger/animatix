use super::*;

/// Simulate what a caller would produce: a flat list of 5 items
/// with expanded flags set according to the given set.
fn build_flat(expanded: &HashSet<String>) -> Vec<TreeItem> {
    vec![
        TreeItem {
            id: "a".into(),
            label: "A".into(),
            depth: 0,
            has_children: true,
            expanded: expanded.contains("a"),
        },
        TreeItem {
            id: "a1".into(),
            label: "A.1".into(),
            depth: 1,
            has_children: false,
            expanded: false,
        },
        TreeItem {
            id: "a2".into(),
            label: "A.2".into(),
            depth: 1,
            has_children: true,
            expanded: expanded.contains("a2"),
        },
        TreeItem {
            id: "a2i".into(),
            label: "A.2.i".into(),
            depth: 2,
            has_children: false,
            expanded: false,
        },
        TreeItem {
            id: "b".into(),
            label: "B".into(),
            depth: 0,
            has_children: false,
            expanded: false,
        },
    ]
}

#[test]
fn flat_list_includes_children_when_expanded() {
    // When both A and A.2 are in the expanded set, all 5 items are present.
    let expanded: HashSet<String> = ["a".into(), "a2".into()].iter().cloned().collect();
    let flat = build_flat(&expanded);
    assert_eq!(flat.len(), 5);
    assert_eq!(flat[0].id, "a");
    assert_eq!(flat[3].id, "a2i");
    assert_eq!(flat[4].id, "b");
}

#[test]
fn flat_list_all_items_always_present() {
    // build_flat always returns all 5 items; the caller controls which
    // are visible by setting expanded flags on their parents.
    let flat = build_flat(&HashSet::new());
    assert_eq!(flat.len(), 5);
}

#[test]
fn expanded_flags_reflect_expanded_set() {
    let expanded: HashSet<String> = ["a".into()].iter().cloned().collect();
    let flat = build_flat(&expanded);
    assert!(flat[0].expanded); // A in expanded set
    assert!(!flat[2].expanded); // A.2 not in set
}

#[test]
fn root_nodes_are_depth_zero() {
    let flat = build_flat(&HashSet::new());
    let roots: Vec<_> = flat.iter().filter(|i| i.depth == 0).collect();
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].id, "a");
    assert_eq!(roots[1].id, "b");
}

#[test]
fn tree_walk_order_preserved() {
    let flat = build_flat(&HashSet::new());
    assert_eq!(flat[0].id, "a"); // first root
    assert_eq!(flat[4].id, "b"); // last root
}

// ── Pure navigation helper tests ────────────────────────────────

#[test]
fn nav_next_prev_edge() {
    let items = build_flat(&HashSet::new());
    assert_eq!(super::nav_next_idx(0, &items), Some(1));
    assert_eq!(super::nav_prev_idx(0, &items), None);
    assert_eq!(super::nav_next_idx(items.len() - 1, &items), None);
    assert_eq!(super::nav_prev_idx(items.len() - 1, &items), Some(3));
}

#[test]
fn nav_home_end() {
    let items = build_flat(&HashSet::new());
    assert_eq!(super::nav_home_idx(&items), Some(0));
    assert_eq!(super::nav_end_idx(&items), Some(4));
    assert_eq!(super::nav_home_idx(&[]), None);
    assert_eq!(super::nav_end_idx(&[]), None);
}

#[test]
fn type_ahead_exact_match() {
    let items = build_flat(&HashSet::new());
    assert_eq!(super::type_ahead_match("A.1", 0, &items), Some(1));
}

#[test]
fn type_ahead_case_insensitive() {
    let items = build_flat(&HashSet::new());
    assert_eq!(super::type_ahead_match("b", 0, &items), Some(4));
}

#[test]
fn type_ahead_wraps() {
    let items = build_flat(&HashSet::new());
    // "A.1" (index 1) matches prefix "A" from start=1.
    assert_eq!(super::type_ahead_match("A", 1, &items), Some(1));
    // Searching from 4 (item "B") wraps to index 0 ("A").
    assert_eq!(super::type_ahead_match("A", 4, &items), Some(0));
}

#[test]
fn type_ahead_no_match() {
    let items = build_flat(&HashSet::new());
    assert_eq!(super::type_ahead_match("ZZZ", 0, &items), None);
}

#[test]
fn type_ahead_empty_prefix() {
    let items = build_flat(&HashSet::new());
    assert_eq!(super::type_ahead_match("", 0, &items), None);
}

#[test]
fn tree_builder_row_height() {
    let items: Vec<TreeItem> = vec![];
    let tree = Tree::new(&items).row_height(32.0);
    assert_eq!(tree.row_height, 32.0);
}

#[test]
fn tree_builder_indent_step() {
    let items: Vec<TreeItem> = vec![];
    let tree = Tree::new(&items).indent_step(24.0);
    assert_eq!(tree.indent_step, 24.0);
}

// ── Keyboard focus gating ───────────────────────────────────────

fn screen() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))
}

fn base_input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(screen()),
        ..Default::default()
    }
}

fn arrow_down() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::ArrowDown,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

fn run_tree_frame(
    ctx: &egui::Context,
    input: egui::RawInput,
    id: &str,
    items: &[TreeItem],
) -> (Option<usize>, Vec<egui::Event>) {
    let mut selected = None;
    let mut remaining = Vec::new();
    let _ = ctx.run_ui(input, |ui| {
        selected = Tree::new(items).show(ui, id).selected_index;
        remaining = ui.input(|i| i.events.clone());
    });
    (selected, remaining)
}

#[test]
fn tree_keyboard_ignored_when_unfocused() {
    let ctx = egui::Context::default();
    let items = build_flat(&HashSet::new());

    let mut input = base_input();
    input.events.push(arrow_down());
    let (sel, remaining) = run_tree_frame(&ctx, input, "unfocused_tree", &items);

    assert_eq!(sel, None, "unfocused tree must not react to arrow keys");
    assert!(
        remaining.iter().any(|e| matches!(
            e,
            egui::Event::Key {
                key: egui::Key::ArrowDown,
                ..
            }
        )),
        "unfocused tree must not consume the arrow key"
    );
}

#[test]
fn tree_keyboard_navigates_when_focused_and_consumes_event() {
    let ctx = egui::Context::default();
    let tree_id = Id::new("focused_tree");
    let items = build_flat(&HashSet::new());

    ctx.memory_mut(|m| m.request_focus(tree_id));

    let mut input = base_input();
    input.events.push(arrow_down());
    let (sel, remaining) = run_tree_frame(&ctx, input, "focused_tree", &items);

    assert_eq!(sel, Some(0), "focused tree should select the first item");
    assert!(
        !remaining.iter().any(|e| matches!(
            e,
            egui::Event::Key {
                key: egui::Key::ArrowDown,
                ..
            }
        )),
        "focused tree must consume the arrow key it handled"
    );
}

/// A tree taller than its viewport must scroll rather than growing to the
/// full content height, and `viewport_response` must describe only the
/// visible slice (distinct from the full-content `response`).
#[test]
fn tree_scrolls_and_exposes_viewport_response() {
    let ctx = egui::Context::default();
    let items: Vec<TreeItem> = (0..50)
        .map(|i| TreeItem {
            id: format!("n{i}"),
            label: format!("Node {i}"),
            depth: 0,
            has_children: false,
            expanded: false,
        })
        .collect();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 120.0));

    let mut used_height = 0.0;
    let mut content_rect = egui::Rect::NOTHING;
    let mut viewport_rect = egui::Rect::NOTHING;
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ui| {
            let resp = Tree::new(&items).row_height(24.0).show(ui, "tall_tree");
            content_rect = resp.response.rect;
            viewport_rect = resp.viewport_response.rect;
            used_height = ui.min_rect().height();
        },
    );

    assert!(
        used_height <= 160.0,
        "scrolling tree should be capped to the viewport, got {used_height}"
    );
    assert!(
        content_rect.height() > viewport_rect.height() + 1.0,
        "content response ({}px) should be taller than the viewport response ({}px)",
        content_rect.height(),
        viewport_rect.height()
    );
}

/// Regression: the viewport interaction must be registered *before* the row
/// interactions, otherwise it sits on top of them and swallows their hover.
#[test]
fn tree_row_hover_survives_viewport_response() {
    let ctx = egui::Context::default();
    let items = build_flat(&HashSet::new());
    let pointer = screen().left_top() + egui::vec2(30.0, 5.0);

    // Warm-up so widget rects are registered for hit-testing.
    let _ = ctx.run_ui(base_input(), |ui| {
        let _ = Tree::new(&items).show(ui, "hover_tree");
    });

    let mut row_hovered = None;
    let mut input = base_input();
    input.events.push(egui::Event::PointerMoved(pointer));
    let _ = ctx.run_ui(input, |ui| {
        let _ = Tree::new(&items).show(ui, "hover_tree");
        row_hovered = ui.ctx().read_response(Id::new("hover_tree").with(0)).map(|r| r.hovered());
    });
    assert_eq!(row_hovered, Some(true), "viewport_response must not steal row hover");
}

/// A tree that fits inside the viewport: content and viewport responses
/// cover the same on-screen region, which is only as tall as the rows.
#[test]
fn tree_viewport_is_on_screen_when_content_fits() {
    let ctx = egui::Context::default();
    let items = build_flat(&HashSet::new());
    let mut content_rect = egui::Rect::NOTHING;
    let mut viewport_rect = egui::Rect::NOTHING;
    let mut selected = None;
    let _ = ctx.run_ui(base_input(), |ui| {
        let resp = Tree::new(&items).show(ui, "short_tree");
        content_rect = resp.response.rect;
        viewport_rect = resp.viewport_response.rect;
        selected = resp.selected_index;
    });
    assert_eq!(selected, None);
    // Content is exactly the 5 rows; the viewport response covers the same
    // on-screen region (no scrolling needed, so it isn't padded out).
    assert_eq!(content_rect.height(), 5.0 * ROW_M);
    assert!(
        (viewport_rect.height() - content_rect.height()).abs() <= 1.0,
        "fitting tree: viewport ({}) should match content ({})",
        viewport_rect.height(),
        content_rect.height()
    );
    assert!(
        screen().contains_rect(viewport_rect),
        "viewport response must stay within the screen: {viewport_rect:?}"
    );
}

#[test]
fn tree_row_click_claims_focus() {
    let ctx = egui::Context::default();
    let items = build_flat(&HashSet::new());
    let row_pos = screen().left_top() + egui::vec2(20.0, 5.0);

    // Warm-up frame registers widget rects for hit-testing.
    let _ = ctx.run_ui(base_input(), |ui| {
        let _ = Tree::new(&items).show(ui, "click_tree");
    });

    let mut sel = None;
    let mut input = base_input();
    input.events.push(egui::Event::PointerButton {
        pos: row_pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    input.events.push(egui::Event::PointerButton {
        pos: row_pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    let _ = ctx.run_ui(input, |ui| {
        sel = Tree::new(&items).show(ui, "click_tree").selected_index;
    });

    assert_eq!(sel, Some(0), "clicking a row should select it");
    assert!(
        ctx.memory(|m| m.has_focus(Id::new("click_tree"))),
        "clicking a tree row must claim keyboard focus for the tree"
    );
}
