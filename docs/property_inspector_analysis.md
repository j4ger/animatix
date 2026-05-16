# Property Inspector Design Analysis

## Current Architecture

The property inspector lives in:
- `crates/animatix-gui/src/app/panels/inspector/mod.rs` — main panel orchestration
- `crates/animatix-gui/src/app/panels/inspector/property_groups.rs` — property grouping & rendering
- `crates/animatix-gui/src/app/panels/inspector/keyframe_table.rs` — dope sheet / keyframe timeline
- `crates/animatix-gui/src/app/components/mod.rs` — shared UI components (`Row`, `field`, `card`, `section_header`)
- `crates/animatix-gui/src/app/theme.rs` — design tokens

## Issues Found

### 1. **Input boxes are not aligned across rows** (Critical)

**Location:** `property_groups.rs:314-318`

Each property row computes its input area independently:

```rust
let input_width = 110.0_f32.min(available * 0.45);
let input_rect = egui::Rect::from_min_size(
    egui::pos2(row_rect.max.x - input_width - SPACE_S, row_rect.min.y),
    Vec2::new(input_width, row_height),
);
```

**Problem:** The input box is always right-aligned with a variable width (`110.0.min(available * 0.45)`). This means:
- The left edge of every input box shifts depending on available width
- Labels have variable length ("position" vs "rotation" vs "stroke_width")
- There is **no shared column boundary** between labels and inputs
- The visual result is a ragged left edge on all input boxes — they appear to "float" at different positions

**Impact:** This is the #1 visual issue. In a professional tool, input fields should form a clean vertical column.

---

### 2. **Label and input compete for the same horizontal space**

**Location:** `property_groups.rs:285-311`

```rust
let label_x = row_rect.min.x + SPACE_L;   // ~8px from left
// ...
let input_width = 110.0_f32.min(available * 0.45);
let input_rect = egui::Rect::from_min_size(
    egui::pos2(row_rect.max.x - input_width - SPACE_S, row_rect.min.y),
    Vec2::new(input_width, row_height),
);
```

**Problem:** There is no explicit gap reservation between label text and input box. The label paints at `min.x + 8` and the input sits at `max.x - input_width - 4`. On narrow panels or with long property names, these can visually collide or feel cramped. The label is drawn via `painter().text()` so it doesn't participate in egui's layout system — it can paint *over* the input area if the name is long.

---

### 3. **Inconsistent input widths per property type**

**Location:** `property_groups.rs` various match arms

| Property Type | Input Behavior |
|---------------|----------------|
| `Vec2` (x, y) | Two DragValues inside a `field()` frame, width determined by content |
| `Float` (normal) | Single DragValue, fills the `field()` frame |
| `Float` (0-1) | Slider + label, wider than a plain DragValue |
| `Color` | Color button only, much narrower than other inputs |
| `Text` | TextEdit or ComboBox, variable width |

**Problem:** Every input type has a different visual footprint. The `field()` wrapper tries to provide consistency, but the contents vary wildly:
- A color swatch is ~28px wide
- A Vec2 pair is ~90px wide  
- A slider+value is ~110px wide
- A ComboBox is the full `input_width`

This creates a "staircase" effect where inputs don't line up even though they're all in the same right-aligned area.

---

### 4. **No label truncation / overflow handling**

**Location:** `property_groups.rs:305-310`

```rust
ui.painter().text(
    egui::pos2(label_x + 12.0, baseline_y),
    egui::Align2::LEFT_CENTER,
    entry.name,   // e.g., "stroke_progress" — 15 chars
    egui::TextStyle::Small.resolve(ui.style()),
    TEXT_SECONDARY,
);
```

**Problem:** Labels are painted directly with no width constraint. A long name like `stroke_progress` or `position_binding` will extend arbitrarily far to the right, potentially overlapping the input area. There is no `max_width`, no ellipsis, no clipping rect.

---

### 5. **Keyframe dots are positioned inconsistently**

**Location:** `property_groups.rs:288-302`

```rust
let dot_x = label_x;  // same as label start!
```

**Problem:** The keyframe dot is placed at `label_x` (the left edge), then the label text is offset `+12.0` from that. This means:
- The dot is not visually associated with the label
- It sits in "dead space" between the row edge and the text
- When there's no keyframe, an invisible gap remains where the dot would be
- The label text starts at different effective positions depending on whether a dot is present

**Expected:** Dots should either be in a dedicated fixed-width column (like in the dope sheet) or inline with the label.

---

### 6. **Row height is too small for comfortable interaction**

**Location:** `property_groups.rs:276`

```rust
let row_height = ROW_S;  // 20px
```

**Problem:** 20px is tight for DragValue widgets which have their own padding. The `field()` frame adds `SPACE_XS` (2px) vertical padding, leaving only ~16px for the actual widget. DragValues feel cramped. Modern design tools typically use 24-28px rows.

---

### 7. **The `field()` component doesn't constrain its children**

**Location:** `components/mod.rs:327-356`

```rust
pub fn field(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) -> Response {
    let frame = egui::Frame::new()
        .fill(BG_WIDGET)
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(Margin::symmetric(SPACE_S as i8, SPACE_XS as i8));

    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            add_contents,
        )
    });
    // ... stroke
}
```

**Problem:** `field()` sets `ui.available_width()` but doesn't enforce a minimum or maximum. When placed inside `scope_builder` with `max_rect(input_rect)`, the outer rect is fixed but the inner contents can still overflow visually (egui doesn't clip by default). The `left_to_right` layout also means contents push beyond the right edge if too wide.

---

### 8. **Group headers and property rows use different visual hierarchies**

**Location:** `property_groups.rs:227-267` (group), `269-508` (rows)

**Problem:** 
- Group headers use the `Row` component with an icon, chevron, and count badge
- Property rows are hand-rolled with `painter().text()` and `allocate_exact_size()`

This means:
- Group headers have hover backgrounds, proper click targets, and consistent indentation
- Property rows have hover backgrounds but no consistent indentation system
- The visual language differs: group headers feel "app-like" while rows feel "debug-like"

---

### 9. **Inconsistent spacing between property groups**

**Location:** `property_groups.rs:266`

```rust
ui.add_space(SPACE_S);  // 4px after each group
```

**Problem:** Groups are separated by only 4px. Combined with the card's `inner_margin(SPACE_M)` and the 1px row spacing, groups visually bleed into each other. There's no clear separation between "Transform" and "Style" sections.

---

### 10. **Opacity slider label is not aligned with other float inputs**

**Location:** `property_groups.rs:366-396`

```rust
ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
    let slider = ui.add(
        egui::Slider::new(&mut nv, 0.0..=1.0)
            .show_value(false)
            .trailing_fill(true),
    );
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!("{:.2}", nv))
                .monospace()
                .size(FONT_SIZE_XS)
                .color(TEXT_PRIMARY),
        )
        .selectable(false),
    );
    // ...
});
```

**Problem:** The slider+value layout is a different structure from the plain DragValue used for other floats. The value label is a separate widget with its own size, causing the total width to differ from a single DragValue. This is part of the "inconsistent widths" issue but specifically affects the 0-1 range properties.

---

### 11. **Missing visual hierarchy within the card**

**Location:** `inspector/mod.rs:119-151`

```rust
components::card(ui, |ui| {
    components::section_header(ui, egui_phosphor::regular::WRENCH, "Properties", None);
    // ... all property groups directly follow
});
```

**Problem:** Inside the Properties card, all groups sit at the same visual level. There is no subtle divider or extra spacing between group sections. Compare to Blender, Figma, or After Effects where property groups have:
- Subtle horizontal rules between groups
- Or extra vertical padding
- Or indented rows under headers

---

### 12. **Actor header uses raw painter calls instead of structured layout**

**Location:** `inspector/mod.rs:224-312`

**Problem:** The actor header at the top of the inspector uses manual `painter().text()` calls and approximate width calculations (`right_x -= 60.0`). The label edit mode and display mode have different sizes. The shape type text is right-aligned with hardcoded offsets. This feels fragile and doesn't adapt well to resizing.

---

### 13. **No tooltip / unit display for most numeric inputs**

**Location:** `property_groups.rs` DragValue calls

**Problem:** Only `rotation` has a suffix (`°`). Other properties like `size`, `position`, `stroke_width` have no unit indication. A user can't tell if position is in pixels, percent, or normalized coordinates without prior knowledge.

---

### 14. **Color picker is just a button, no hex display**

**Location:** `property_groups.rs:423-452`

**Problem:** The color input is only `color_edit_button_srgba()`. There's no accompanying hex text display (e.g., `#ff3366`) like in most design tools. Users can't see the exact color value or copy/paste it.

---

## Summary Table

| # | Issue | Severity | File:Line |
|---|-------|----------|-----------|
| 1 | Input boxes not aligned — no shared column boundary | **Critical** | `property_groups.rs:314` |
| 2 | Label/input space competition, no gap reservation | **High** | `property_groups.rs:285-318` |
| 3 | Inconsistent input widths per type | **High** | `property_groups.rs:321-506` |
| 4 | No label truncation/overflow | **Medium** | `property_groups.rs:305` |
| 5 | Keyframe dot positioning inconsistent | **Medium** | `property_groups.rs:288` |
| 6 | Row height too tight (20px) | **Medium** | `property_groups.rs:276` |
| 7 | `field()` doesn't constrain children | **Medium** | `components/mod.rs:327` |
| 8 | Group headers vs rows: different visual systems | **Medium** | `property_groups.rs:227-508` |
| 9 | Group spacing too tight | **Low** | `property_groups.rs:266` |
| 10 | Opacity slider layout differs from other floats | **Medium** | `property_groups.rs:366` |
| 11 | Missing visual hierarchy in card | **Low** | `inspector/mod.rs:119` |
| 12 | Actor header fragile manual layout | **Medium** | `inspector/mod.rs:224` |
| 13 | No unit tooltips on numeric inputs | **Low** | `property_groups.rs:328-401` |
| 14 | Color has no hex display | **Low** | `property_groups.rs:434` |

## Proposed Redesign Direction

1. **Adopt a two-column grid layout:** Reserve a fixed left column (~45-50% width) for labels, and a fixed right column for inputs. All inputs align to the same left edge.

2. **Standardize input widths by type:** Define fixed widths:
   - `Vec2`: 90px (two DragValues with small gap)
   - `Float`: 70px (single DragValue)
   - `0-1 Float`: 90px (slider + value)
   - `Color`: 90px (swatch + hex label)
   - `Text`: fill remaining width

3. **Add a dedicated keyframe column:** A fixed 16px-wide column at the far left for keyframe dots, separate from the label area.

4. **Increase row height to 24px** (`ROW_M`) for comfortable widget fitting.

5. **Use egui's `Grid` or `Table` for alignment** instead of hand-rolled `allocate_exact_size` + `painter().text()`.

6. **Add group dividers:** A subtle 1px `BORDER` line or extra `SPACE_M` padding between property groups.

7. **Add unit suffixes/tooltips** to DragValues (`px`, `°`, `%` where appropriate).

8. **Show hex next to color swatch** for precision editing.

9. **Label truncation:** Use `Galley` layout with max_width to clip long labels with ellipsis.

10. **Unify row rendering:** Make property rows use a similar component system as `Row` for consistency.
