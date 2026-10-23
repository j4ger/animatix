use super::*;

const ITEMS: [&str; 5] = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"];

// ── Interaction test harness ────────────────────────────────────

fn screen() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))
}

fn base_input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(screen()),
        ..Default::default()
    }
}

fn press_release(pos: egui::Pos2) -> [egui::Event; 2] {
    [
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ]
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

fn has_arrow_down(events: &[egui::Event]) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            egui::Event::Key {
                key: egui::Key::ArrowDown,
                ..
            }
        )
    })
}

/// Run one frame for a `List`; returns `(selected_index, leftover_events)`.
fn run_list_frame(
    ctx: &egui::Context,
    input: egui::RawInput,
    id: &str,
    items: &[&str],
) -> (Option<usize>, Vec<egui::Event>) {
    let mut selected = None;
    let mut remaining = Vec::new();
    let _ = ctx.run_ui(input, |ui| {
        selected = List::new(items).show(ui, id).selected_index;
        remaining = ui.input(|i| i.events.clone());
    });
    (selected, remaining)
}

#[test]
fn list_keyboard_ignored_when_unfocused() {
    let ctx = egui::Context::default();
    let (sel0, _) = run_list_frame(&ctx, base_input(), "unfocused_list", &ITEMS);
    assert_eq!(sel0, None);

    // Arrow key with nothing focused must not touch the list.
    let mut input = base_input();
    input.events.push(arrow_down());
    let (sel1, remaining) = run_list_frame(&ctx, input, "unfocused_list", &ITEMS);
    assert_eq!(sel1, None, "unfocused list must not react to arrow keys");
    assert!(has_arrow_down(&remaining), "unfocused list must not consume the arrow key");
}

#[test]
fn list_keyboard_navigates_when_focused_and_consumes_event() {
    let ctx = egui::Context::default();
    let list_id = Id::new("focused_list");

    // Give the list keyboard focus.
    ctx.memory_mut(|m| m.request_focus(list_id));

    let mut input = base_input();
    input.events.push(arrow_down());
    let (sel, remaining) = run_list_frame(&ctx, input, "focused_list", &ITEMS);
    assert_eq!(sel, Some(0), "focused list should move selection down");
    assert!(
        !has_arrow_down(&remaining),
        "focused list must consume the arrow key it handled"
    );

    let mut input = base_input();
    input.events.push(arrow_down());
    let (sel, _) = run_list_frame(&ctx, input, "focused_list", &ITEMS);
    assert_eq!(sel, Some(1));
}

#[test]
fn list_type_ahead_ignored_while_a_text_field_is_focused() {
    let ctx = egui::Context::default();
    let pos = screen().left_top() + egui::vec2(20.0, 5.0);
    let mut text = String::new();
    let mut selected = None;

    // Frame 1: warm up so the TextField's widgets are registered.
    let _ = ctx.run_ui(base_input(), |ui| {
        let _ = TextField::new(&mut text).desired_width(200.0).show(ui);
    });
    // Frame 2: click the TextField to focus it.
    let mut input = base_input();
    input.events.extend(press_release(pos));
    let _ = ctx.run_ui(input, |ui| {
        let _ = TextField::new(&mut text).desired_width(200.0).show(ui);
    });

    // Frame 3: type — the list rendered alongside must not type-ahead.
    let mut input = base_input();
    input.events.push(egui::Event::Text("Esto".to_owned()));
    let _ = ctx.run_ui(input, |ui| {
        let _ = TextField::new(&mut text).desired_width(200.0).show(ui);
        selected = List::new(&ITEMS).show(ui, "typeahead_list").selected_index;
    });
    assert_eq!(selected, None, "typing in a TextField must not drive list type-ahead");
}

#[test]
fn list_click_claims_focus_and_routes_selection() {
    let ctx = egui::Context::default();
    let row_pos = screen().left_top() + egui::vec2(20.0, 5.0);

    // Warm-up frame registers the widget rects needed for hit-testing.
    let _ = ctx.run_ui(base_input(), |ui| {
        let _ = List::new(&ITEMS).show(ui, "click_list");
    });

    let mut sel = None;
    let mut input = base_input();
    input.events.extend(press_release(row_pos));
    let _ = ctx.run_ui(input, |ui| {
        sel = List::new(&ITEMS).show(ui, "click_list").selected_index;
    });
    assert_eq!(sel, Some(0), "clicking a row should select it");
    assert!(
        ctx.memory(|m| m.has_focus(Id::new("click_list"))),
        "clicking the list must claim keyboard focus"
    );
}

#[test]
fn two_lists_only_the_focused_one_reacts() {
    let ctx = egui::Context::default();
    ctx.memory_mut(|m| m.request_focus(Id::new("list_b")));

    let mut first = None;
    let mut second = None;
    let run = |first: &mut Option<usize>, second: &mut Option<usize>| {
        let _ = ctx.run_ui(input_with(arrow_down()), |ui| {
            *first = List::new(&ITEMS).row_height(24.0).show(ui, "list_a").selected_index;
            ui.add_space(4.0);
            *second = List::new(&ITEMS).row_height(24.0).show(ui, "list_b").selected_index;
        });
    };
    run(&mut first, &mut second);

    assert_eq!(first, None, "unfocused list must not react to the arrow key");
    assert_eq!(second, Some(0), "focused list should react to the arrow key");
}

fn input_with(event: egui::Event) -> egui::RawInput {
    let mut input = base_input();
    input.events.push(event);
    input
}

// ── SearchableList filter ───────────────────────────────────────

const FILTER_ITEMS: [&str; 3] = ["Alpha", "Beta", "Gamma"];

#[test]
fn searchable_list_respects_persisted_filter() {
    let ctx = egui::Context::default();
    let root = Id::new("searchable_persisted");
    ctx.data_mut(|d| d.insert_temp(root.with("filter"), "Beta".to_owned()));

    let mut resp = None;
    let _ = ctx.run_ui(base_input(), |ui| {
        resp = Some(SearchableList::new(&FILTER_ITEMS).show(ui, "searchable_persisted"));
    });
    let resp = resp.expect("response");
    assert_eq!(resp.filtered_count, 1, "filtering must use the persisted filter");
}

#[test]
fn searchable_list_typing_persists_and_narrows_filter() {
    let ctx = egui::Context::default();
    // The filter field occupies the first row of the widget.
    let field_pos = screen().left_top() + egui::vec2(60.0, 8.0);

    let show = |ctx: &egui::Context, input: egui::RawInput| -> SearchableListResponse {
        let mut resp = None;
        let _ = ctx.run_ui(input, |ui| {
            resp = Some(SearchableList::new(&FILTER_ITEMS).show(ui, "searchable_typed"));
        });
        resp.expect("response")
    };

    // Warm-up, then click the filter field to focus its TextEdit.
    let _ = show(&ctx, base_input());
    let mut input = base_input();
    input.events.extend(press_release(field_pos));
    let _ = show(&ctx, input);

    // Now type "Gamma" and verify the filter narrows + is persisted.
    let mut input = base_input();
    input.events.push(egui::Event::Text("Gamma".to_owned()));
    let resp = show(&ctx, input);

    assert_eq!(resp.filtered_count, 1, "typing in the filter must narrow the list");
    assert_eq!(resp.confirmed, None);
    let stored: String = ctx
        .data(|d| d.get_temp(Id::new("searchable_typed").with("filter")))
        .unwrap_or_default();
    assert_eq!(stored, "Gamma", "edited filter must be persisted to Memory");
}

#[test]
fn next_index_basic() {
    assert_eq!(next_index(0, 5), Some(1));
    assert_eq!(next_index(4, 5), None);
}

#[test]
fn prev_index_basic() {
    assert_eq!(prev_index(0), None);
    assert_eq!(prev_index(4), Some(3));
}

#[test]
fn home_end() {
    assert_eq!(home_index(5), Some(0));
    assert_eq!(end_index(5), Some(4));
    assert_eq!(home_index(0), None);
    assert_eq!(end_index(0), None);
}

#[test]
fn type_ahead_exact() {
    assert_eq!(type_ahead_prefix_match("Beta", 0, &ITEMS), Some(1));
}

#[test]
fn type_ahead_case_insensitive() {
    assert_eq!(type_ahead_prefix_match("gamma", 0, &ITEMS), Some(2));
    assert_eq!(type_ahead_prefix_match("DELTA", 0, &ITEMS), Some(3));
}

#[test]
fn type_ahead_wraps() {
    // "Alpha" is at index 0; searching from 1 should wrap.
    assert_eq!(type_ahead_prefix_match("Alpha", 1, &ITEMS), Some(0));
}

#[test]
fn type_ahead_no_match() {
    assert_eq!(type_ahead_prefix_match("ZZZ", 0, &ITEMS), None);
}

#[test]
fn type_ahead_empty_prefix() {
    assert_eq!(type_ahead_prefix_match("", 0, &ITEMS), None);
}

#[test]
fn filter_items_empty_returns_all() {
    let r = filter_items("", &ITEMS);
    assert_eq!(r.len(), 5);
    assert_eq!(r[0], (0, "Alpha"));
}

#[test]
fn filter_items_substring() {
    // "ta" appears in Beta (B**ta**) and Delta (De**ta**)
    let r = filter_items("ta", &ITEMS);
    assert_eq!(r.len(), 2);
    assert_eq!(r[0], (1, "Beta"));
    assert_eq!(r[1], (3, "Delta"));
}

#[test]
fn filter_items_case_insensitive() {
    let r = filter_items("DELTA", &ITEMS);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0], (3, "Delta"));
}

#[test]
fn filter_items_no_match() {
    assert!(filter_items("ZZZ", &ITEMS).is_empty());
}

#[test]
fn filter_preserves_original_indices() {
    // "a" is in Alpha (0), Beta (1), Gamma (2), Delta (3)
    let r = filter_items("a", &ITEMS);
    assert_eq!(r.len(), 4);
    assert_eq!(r[0], (0, "Alpha"));
    assert_eq!(r[1], (1, "Beta"));
    assert_eq!(r[2], (2, "Gamma"));
    assert_eq!(r[3], (3, "Delta"));
}

// ── Scrolling / virtualization ──────────────────────────────────

/// Screen smaller than the content: the list must scroll rather than
/// growing to the full content height.
#[test]
fn list_scrolls_instead_of_growing() {
    let ctx = egui::Context::default();
    let owned: Vec<String> = (0..50).map(|i| format!("Row {i}")).collect();
    let items: Vec<&str> = owned.iter().map(String::as_str).collect();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 120.0));

    let mut used_height = 0.0;
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ui| {
            let _ = List::new(&items).row_height(24.0).show(ui, "tall_list");
            used_height = ui.min_rect().height();
        },
    );

    // 50 rows × 24 px = 1200 px of content; the widget must stay ≈ viewport tall.
    assert!(
        used_height <= 160.0,
        "scrolling list should be capped to the viewport, got {used_height}"
    );
}

/// Clicking a row inside the viewport still routes the click to the right
/// index through the scroll area.
#[test]
fn list_click_selects_visible_row_under_scroll() {
    let ctx = egui::Context::default();
    let owned: Vec<String> = (0..50).map(|i| format!("Row {i}")).collect();
    let items: Vec<&str> = owned.iter().map(String::as_str).collect();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 120.0));
    let input = || egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let run = |input: egui::RawInput| -> Option<usize> {
        let mut sel = None;
        let _ = ctx.run_ui(input, |ui| {
            sel = List::new(&items).row_height(24.0).show(ui, "scroll_click").selected_index;
        });
        sel
    };

    // Warm-up registers the scroll area + widget rects.
    let _ = run(input());

    // Second visible row (y ≈ 24..48) → index 1.
    let pos = egui::pos2(20.0, 30.0);
    let mut click = input();
    click.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    click.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    assert_eq!(run(click), Some(1), "clicking the 2nd visible row selects index 1");
}

/// Keyboard navigation must scroll the newly selected row into view: after
/// `End`, a click near the bottom of the viewport lands on the last row.
#[test]
fn list_end_scrolls_selection_into_view() {
    let ctx = egui::Context::default();
    let owned: Vec<String> = (0..50).map(|i| format!("Row {i}")).collect();
    let items: Vec<&str> = owned.iter().map(String::as_str).collect();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 120.0));
    let list_id = Id::new("end_scroll");

    let base = || egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let run = |input: egui::RawInput| -> Option<usize> {
        let mut sel = None;
        let _ = ctx.run_ui(input, |ui| {
            sel = List::new(&items).row_height(24.0).show(ui, "end_scroll").selected_index;
        });
        sel
    };

    // Warm up, focus the list, then press End.
    let _ = run(base());
    ctx.memory_mut(|m| m.request_focus(list_id));
    let mut input = base();
    input.events.push(egui::Event::Key {
        key: egui::Key::End,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    });
    assert_eq!(run(input), Some(49), "End should select the last item");

    // Let the scroll animation settle.
    for _ in 0..60 {
        let _ = run(base());
    }

    // Bottom-most visible row should now be the last item.
    let pos = egui::pos2(20.0, 110.0);
    let mut click = base();
    click.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    click.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    assert_eq!(run(click), Some(49), "after End the last row must be scrolled into view");
}

/// `visible_row_range` spans the viewport with one row of overscan each side.
#[test]
fn visible_row_range_basic() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
    let range = visible_row_range(viewport, 20.0, 50);
    assert_eq!(range.start, 0);
    // 100/20 = 5 rows visible + 1 overscan.
    assert_eq!(range.end, 6);
}

#[test]
fn visible_row_range_scrolled_clamps_to_count() {
    let viewport = egui::Rect::from_min_max(egui::pos2(0.0, 980.0), egui::pos2(100.0, 1080.0));
    let range = visible_row_range(viewport, 20.0, 50);
    assert_eq!(range.start, 49);
    assert_eq!(range.end, 50, "range must not exceed the item count");
}

#[test]
fn visible_row_range_empty_or_zero_height() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
    assert_eq!(visible_row_range(viewport, 20.0, 0), 0..0);
    assert_eq!(visible_row_range(viewport, 0.0, 10), 0..0);
}

#[test]
fn row_rect_for_offsets_by_row_height() {
    let outer = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 240.0));
    let r0 = row_rect_for(outer, 24.0, 0);
    let r2 = row_rect_for(outer, 24.0, 2);
    assert_eq!(r0.min, egui::pos2(10.0, 20.0));
    assert_eq!(r0.height(), 24.0);
    assert_eq!(r2.min.y, 20.0 + 48.0);
    assert_eq!(r2.width(), 100.0);
}

#[test]
fn builder_row_height() {
    let list = List::new(&ITEMS).row_height(30.0);
    assert_eq!(list.row_height, 30.0);
}

#[test]
fn builder_type_ahead_timeout() {
    let d = Duration::from_secs(2);
    let list = List::new(&ITEMS).type_ahead_timeout(d);
    assert_eq!(list.type_ahead_timeout, d);
}
