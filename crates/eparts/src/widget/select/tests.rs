use super::options::*;
use super::*;

#[test]
fn builder_defaults() {
    let mut sel = None;
    let s = Select::new("test", &mut sel, &["A", "B"]);
    assert_eq!(s.placeholder, "Select…");
    assert!(!s.searchable);
    assert!(!s.clearable);
    assert_eq!(s.flat_options.len(), 2);
}

#[test]
fn builder_placeholder() {
    let mut sel = None;
    let s = Select::new("test", &mut sel, &["A"]).placeholder("Pick one");
    assert_eq!(s.placeholder, "Pick one");
}

#[test]
fn builder_searchable() {
    let mut sel = None;
    let s = Select::new("test", &mut sel, &["A"]).searchable(true);
    assert!(s.searchable);
}

#[test]
fn builder_clearable() {
    let mut sel = None;
    let s = Select::new("test", &mut sel, &["A"]).clearable(true);
    assert!(s.clearable);
}

#[test]
fn grouped_constructor() {
    let groups: &[(&str, &[&str])] = &[("Fruits", &["Apple", "Banana"]), ("Veg", &["Carrot"])];
    let mut sel = None;
    let s = Select::grouped("test", &mut sel, groups);
    assert_eq!(s.flat_options.len(), 5);
    assert!(matches!(
        s.flat_options[0],
        FlatOption::Header(ref h) if h == "Fruits"
    ));
    assert!(matches!(
        s.flat_options[1],
        FlatOption::Item { ref label, .. } if label == "Apple"
    ));
    assert!(matches!(
        s.flat_options[2],
        FlatOption::Item { ref label, .. } if label == "Banana"
    ));
    assert!(matches!(
        s.flat_options[3],
        FlatOption::Header(ref h) if h == "Veg"
    ));
    assert!(matches!(
        s.flat_options[4],
        FlatOption::Item { ref label, .. } if label == "Carrot"
    ));
    assert_eq!(s.flat_options[4].index(), Some(2));
}

// ── Filtering ───────────────────────────────────────────────────────

fn flat(labels: &[&str]) -> Vec<FlatOption> {
    labels
        .iter()
        .enumerate()
        .map(|(i, l)| FlatOption::Item {
            label: l.to_string(),
            index: i,
        })
        .collect()
}

#[test]
fn visible_positions_empty_filter_keeps_all() {
    let opts = flat(&["A", "B", "C"]);
    assert_eq!(visible_positions(&opts, ""), vec![0, 1, 2]);
}

#[test]
fn visible_positions_suppresses_empty_headers() {
    let groups: &[(&str, &[&str])] = &[("Fruits", &["Apple", "Banana"]), ("Veg", &["Carrot"])];
    let mut sel = None;
    let s = Select::grouped("g", &mut sel, groups);
    // "Car" only matches the Carrot item → the Fruits header is dropped but
    // the Veg header stays.
    let vis = visible_positions(&s.flat_options, "car");
    assert_eq!(vis, vec![3, 4]);
}

#[test]
fn visible_positions_case_insensitive_substring() {
    let opts = flat(&["Alpha", "Beta", "Gamma", "Delta"]);
    // "ta" is in Be*ta* and Del*ta*; matching is case-insensitive.
    assert_eq!(visible_positions(&opts, "TA"), vec![1, 3]);
}

// ── Highlight navigation ────────────────────────────────────────────

#[test]
fn step_highlight_starts_at_first() {
    assert_eq!(step_highlight(&[0, 1, 2], None, 1), Some(0));
    assert_eq!(step_highlight(&[0, 1, 2], None, -1), Some(0));
}

#[test]
fn step_highlight_moves_and_clamps() {
    assert_eq!(step_highlight(&[0, 1, 2], Some(0), 1), Some(1));
    assert_eq!(step_highlight(&[0, 1, 2], Some(2), 1), Some(2));
    assert_eq!(step_highlight(&[0, 1, 2], Some(0), -1), Some(0));
    assert_eq!(step_highlight(&[0, 1, 2], Some(1), -1), Some(0));
}

#[test]
fn step_highlight_skips_filtered_rows() {
    // Only positions 1 and 3 are visible.
    assert_eq!(step_highlight(&[1, 3], Some(1), 1), Some(3));
    assert_eq!(step_highlight(&[1, 3], Some(3), -1), Some(1));
}

#[test]
fn step_highlight_empty_is_none() {
    assert_eq!(step_highlight(&[], None, 1), None);
}

#[test]
fn step_highlight_stale_current_resets_to_first() {
    assert_eq!(step_highlight(&[2, 5], Some(0), 1), Some(2));
}

// ── Trigger rect / layout ───────────────────────────────────────────

/// The trigger must fill the rect supplied by `ui.add_sized`.
#[test]
fn trigger_fills_add_sized_rect() {
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 120.0));
    let mut sel = None;
    let mut rect = None;
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ui| {
            let resp = ui.add_sized(
                egui::vec2(180.0, 22.0),
                Select::new("sized_select", &mut sel, &["A", "B"]),
            );
            rect = Some(resp.rect);
        },
    );
    let rect = rect.expect("trigger response");
    assert!(
        (rect.width() - 180.0).abs() < 1.0,
        "trigger width should match add_sized: {rect:?}"
    );
    assert!(
        (rect.height() - 22.0).abs() < 1.0,
        "trigger height should match add_sized: {rect:?}"
    );
}

// ── Keyboard navigation ─────────────────────────────────────────────

fn base_input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 240.0))),
        ..Default::default()
    }
}

fn key_event(key: Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

fn run_select(
    ctx: &egui::Context,
    input: egui::RawInput,
    id: &'static str,
    selected: &mut Option<usize>,
) {
    let _ = ctx.run_ui(input, |ui| {
        ui.add(Select::new(id, selected, &["A", "B", "C"]));
    });
}

/// ArrowDown must move the keyboard highlight and consume the event while
/// the trigger is focused and the dropdown is open.
#[test]
fn arrow_down_moves_highlight_and_consumes_event() {
    let ctx = egui::Context::default();
    let select_id = egui::Id::new("kb_select");
    let trigger_id = select_id.with("__trigger");
    let highlight_key = select_id.with("__highlight");
    let popover_id = select_id.with("__popover");

    Popover::new(popover_id).open(&ctx, true);
    ctx.memory_mut(|m| m.request_focus(trigger_id));
    ctx.data_mut(|d| d.insert_temp(highlight_key, Some(0usize)));

    let mut selected: Option<usize> = None;
    let mut remaining = Vec::new();
    let mut input = base_input();
    input.events.push(key_event(Key::ArrowDown));
    let _ = ctx.run_ui(input, |ui| {
        ui.add(Select::new("kb_select", &mut selected, &["A", "B", "C"]));
        remaining = ui.input(|i| i.events.clone());
    });

    let highlight: Option<usize> = ctx.data(|d| d.get_temp(highlight_key)).unwrap_or(None);
    assert_eq!(highlight, Some(1), "ArrowDown should advance the highlight");
    assert!(
        !remaining.iter().any(|e| matches!(
            e,
            egui::Event::Key {
                key: Key::ArrowDown,
                ..
            }
        )),
        "focused open Select must consume the arrow key"
    );
    assert_eq!(selected, None, "arrow navigation must not commit a selection");
}

/// Enter commits the highlighted option and closes the popover.
#[test]
fn enter_commits_highlight_and_closes() {
    let ctx = egui::Context::default();
    let select_id = egui::Id::new("commit_select");
    let trigger_id = select_id.with("__trigger");
    let highlight_key = select_id.with("__highlight");
    let popover_id = select_id.with("__popover");

    Popover::new(popover_id).open(&ctx, true);
    ctx.memory_mut(|m| m.request_focus(trigger_id));
    ctx.data_mut(|d| d.insert_temp(highlight_key, Some(1usize)));

    let mut selected: Option<usize> = None;
    let mut input = base_input();
    input.events.push(key_event(Key::Enter));
    run_select(&ctx, input, "commit_select", &mut selected);

    assert_eq!(selected, Some(1), "Enter should commit the highlighted option");
    assert!(
        !Popover::is_open(&ctx, popover_id),
        "Enter must close the dropdown after committing"
    );
}

/// Escape closes the open dropdown without changing the selection.
///
/// A warm-up frame is required because egui surrenders focus on Escape
/// *before* widget code runs unless the focused widget has already
/// registered an `escape: true` lock filter (which happens the first frame
/// the trigger holds focus, i.e. the frame the user clicked it).
#[test]
fn escape_closes_without_commit() {
    let ctx = egui::Context::default();
    let select_id = egui::Id::new("esc_select");
    let trigger_id = select_id.with("__trigger");
    let popover_id = select_id.with("__popover");

    Popover::new(popover_id).open(&ctx, true);
    ctx.memory_mut(|m| m.request_focus(trigger_id));

    let mut selected: Option<usize> = None;
    // Warm-up frame registers the escape focus lock filter.
    run_select(&ctx, base_input(), "esc_select", &mut selected);
    assert!(
        ctx.memory(|m| m.has_focus(trigger_id)),
        "trigger should still hold focus after the warm-up frame"
    );

    let mut input = base_input();
    input.events.push(key_event(Key::Escape));
    run_select(&ctx, input, "esc_select", &mut selected);

    assert_eq!(selected, None, "Escape must not commit a selection");
    assert!(!Popover::is_open(&ctx, popover_id), "Escape must close the dropdown");
}

/// Keyboard navigation is inert while the trigger is not focused.
#[test]
fn arrow_ignored_when_trigger_unfocused() {
    let ctx = egui::Context::default();
    let select_id = egui::Id::new("unfocused_select");
    let highlight_key = select_id.with("__highlight");

    let mut selected: Option<usize> = None;
    let mut remaining = Vec::new();
    let mut input = base_input();
    input.events.push(key_event(Key::ArrowDown));
    let _ = ctx.run_ui(input, |ui| {
        ui.add(Select::new("unfocused_select", &mut selected, &["A", "B", "C"]));
        remaining = ui.input(|i| i.events.clone());
    });

    assert!(
        ctx.data(|d| d.get_temp::<Option<usize>>(highlight_key))
            .unwrap_or(None)
            .is_none(),
        "unfocused Select must not set a keyboard highlight"
    );
    assert!(
        remaining.iter().any(|e| matches!(
            e,
            egui::Event::Key {
                key: Key::ArrowDown,
                ..
            }
        )),
        "unfocused Select must not consume the arrow key"
    );
}

/// Unconstrained, the trigger is one row tall (not the whole panel height).
#[test]
fn trigger_default_height_is_one_row() {
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 400.0));
    let mut sel = None;
    let mut rect = None;
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ui| {
            rect = Some(ui.add(Select::new("plain_select", &mut sel, &["A", "B"])).rect);
        },
    );
    let rect = rect.expect("trigger response");
    assert!(
        rect.height() <= 40.0,
        "unconstrained trigger should be roughly one row tall: {rect:?}"
    );
}
