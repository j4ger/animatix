# Property System Implementation Guide

> **Purpose:** Step-by-step guide for implementing the property system redesign.
> **Prerequisite:** Read `docs/design/property_system.md` first.

---

## Phase 0: Inventory

Before touching code, understand the full surface area:

```
grep -rn "match prop.name.as_str()" crates/animatix/src/ --include="*.rs"
grep -rn "match property {" crates/animatix/src/ --include="*.rs"
grep -rn "PropertySchema\|PropertyDef\|property_registry" crates/animatix/src/ --include="*.rs"
```

---

## Phase 1: New files

### 1.1 `timeline/property_registry.rs`

Create the core types:

```rust
// ValueType enum — one variant per property value shape
// PropertyFlags bitflags — ANIMATED, ASSIGNABLE, INJECTABLE, LAYOUT_AFFECTING
// GroupMembership struct — { group_id: GroupHandlerId }
// PropertySchema struct — { name, value_type, flags, field, group }
// ActorField enum — flat enum over all storage fields
// GroupHandlerId enum — PositionBinding, VectorShapeState, PlotDomain, ContainerLayout
```

Then define `PROPERTY_REGISTRY` — the static sorted slice.

Implement `lookup_property(name: &str) -> Option<&'static PropertySchema>` via binary search.

### 1.2 `timeline/property_groups.rs`

Implement the group handlers:

```rust
// resolve_position_binding()
//   Takes collected at/anchor/offset, calls existing resolve_position_binding_with_lookup_diagnostic()
//   Exactly the same logic as today's process_body lines 1543-1560, but extracted

// resolve_vector_shape_state()
//   Takes radius/sides/from/to/start_angle/sweep_angle/points/commands
//   Calls existing apply_vector_shape_property() and apply_vector_shape_defaults()
//   Produces size/line_from/line_to/arc_angles/vector_paths

// resolve_plot_domain()
//   Takes x_domain/y_domain/t_domain/func/tolerance/max_depth/resolution
//   Stores in env via set(); consumed by build_plot_curve_paths()

// resolve_container_layout()
//   Takes gap/align/cols
//   Calls existing register_container_metadata_and_apply_layout()
```

### 1.3 `timeline/value_parser.rs` (optional but recommended)

Extract all the `evaluate_expr → unwrap → convert to target type` boilerplate into one dispatch:

```rust
pub(crate) fn parse_value(
    value_type: ValueType,
    expr: &Expr,
    env: &Environment,
    diag: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<PropertyValue> {
    match value_type {
        ValueType::F32 => {
            evaluate_expr_with_lookup_diagnostic(expr, env, diag, subject)
                .map(|v| PropertyValue::F32(v.as_num() as f32))
        }
        ValueType::Vec2 => {
            evaluate_expr_with_lookup_diagnostic(expr, env, diag, subject)
                .and_then(|v| match v {
                    Value::Vec2([x, y]) => Some(PropertyValue::Vec2([x as f32, y as f32])),
                    _ => None,
                })
        }
        ValueType::Color => {
            parse_color_in_env_with_lookup_diagnostic(..)
                .map(PropertyValue::Color)
        }
        // ... one arm per ValueType variant
    }
}
```

---

## Phase 2: Storage restructure

### 2.1 In `track.rs`

1. Define `ActorKindId` enum + `ShapeKind` enum (at top of file)
2. Replace `LayoutSizeState` with `Option<PropertyTrack<[f32;2]>>`:
   - Remove the `LayoutSizeState` enum and its `impl`
   - Remove `ensure_layout_size()`, `layout_size_get()`, `preserve_instant_delayed_value()` from AnimationTrack
   - These are now handled by the generic `Option<PropertyTrack<T>>` `TrackAccessor` trait
3. Define `GeometryTier` struct:
```rust
pub(crate) struct GeometryTier {
    pub position: Option<PropertyTrack<[f32; 2]>>,
    pub motion_offset: Option<PropertyTrack<[f32; 2]>>,
    pub size: Option<PropertyTrack<[f32; 2]>>,
    pub layout_size: Option<PropertyTrack<[f32; 2]>>,
    pub rotation: Option<PropertyTrack<f32>>,
    pub scale: Option<PropertyTrack<f32>>,
    pub placement_mode: Option<PropertyTrack<PlacementMode>>,
    pub position_binding: Option<PropertyTrack<PositionBinding>>,
}
```
4. Define `StyleTier` struct
5. Define `ActorPayload` enum with variants: `Empty`, `Shape{..}`, `Text{..}`, `Image{..}`, `Svg{..}`, `Plot{..}`
6. Define updated `AnimationTrack`:
```rust
pub struct AnimationTrack {
    pub header: ActorHeader,
    pub geometry: GeometryTier,
    pub style: StyleTier,
    pub payload: ActorPayload,
}
```
7. Add backward-compat accessor methods:
```rust
impl AnimationTrack {
    // Old callers use these; they forward to the new structure
    pub fn position(&self) -> &Option<PropertyTrack<[f32; 2]>> { &self.geometry.position }
    pub fn position_mut(&mut self) -> &mut Option<PropertyTrack<[f32; 2]>> { &mut self.geometry.position }
    pub fn color(&self) -> &Option<PropertyTrack<[f32; 4]>> { &self.style.color }
    // ... one per moved field
}
```
8. **Test that it compiles** — all existing code should compile via the accessor methods

### 2.2 Fix up all call sites

Search for `track.` field accesses and replace them with the correct tier path:

```rust
// Before:
track.position.get(t, [0.0, 0.0])
track.color.ensure([1.0;4]).add_keyframe(t, c, e)

// After:
track.geometry.position.get(t, [0.0, 0.0])
track.style.color.ensure([1.0;4]).add_keyframe(t, c, e)
```

This is the most mechanical part. Do it in one focused pass.

### 2.3 Remove `LayoutSizeState` bitrot

Remove the three ad-hoc AnimationTrack methods:
- `ensure_layout_size()` → use `geometry.layout_size.ensure()`
- `layout_size_get()` → use `geometry.layout_size.as_ref().map(|t| t.evaluate(t))`
- `preserve_instant_delayed_value(default, t)` → use `preserve_instant_delayed_value(&mut geometry.layout_size, t)`

Search for all call sites of these methods and inline the new pattern.

---

## Phase 3: Generic engine + switch-over

### 3.1 Implement the declaration engine

In a new file or in `build.rs`:

```rust
pub(crate) fn process_declaration_property(
    track: &mut AnimationTrack,
    prop: &Property,
    env: &Environment,
    ctx: &mut DeclarationContext,
    diag: &mut Vec<Diagnostic>,
) {
    let schema = match lookup_property(&prop.name) {
        Some(s) => s,
        None => { /* unknown property diagnostic */ return; }
    };

    // Validate for actor kind
    if !ctx.allowed_properties.contains(schema) {
        /* invalid property diagnostic */ return;
    }

    // Group properties → defer
    if let Some(group) = &schema.group {
        ctx.deferred_groups.entry(group.group_id).or_default().push(prop);
        return;
    }

    // Build-time only → execute now
    if !schema.flags.contains(PropertyFlags::ANIMATED) {
        execute_build_time_property(track, schema, &prop.value, env, ctx, diag);
        return;
    }

    // Standard animated property
    let target = parse_value(schema.value_type, &prop.value, env, diag, &format!("{}.{}", track.label, prop.name));
    let Some(target) = target else { return; };

    apply_animated_property(track, schema, target, ctx);
}

fn apply_animated_property(track: &mut AnimationTrack, schema: &PropertySchema, target: PropertyValue, ctx: &DeclarationContext) {
    // Write start keyframe if animating
    if ctx.duration_ms > 0.0 {
        snapshot_existing_value(track, schema, ctx.t_start_ms);
    } else if ctx.delay_ms > 0.0 {
        preserve_delayed_value(track, schema, ctx.t_start_ms);
    }

    // Write end keyframe
    write_property_value(track, schema, ctx.t_end_ms, target, ctx.easing);
}
```

### 3.2 Migrate `build.rs` — the big match blocks

**Strategy:** Migrate one property at a time. For each property:

1. Look up its `PropertySchema`
2. Verify the generated keyframes match the old code
3. Replace the match arm with a comment: `// Handled by property engine`
4. After all arms are migrated, delete the entire `match prop.name.as_str()` block

**Step A — Plot pre-pass (build.rs:1298-1370)**:

Replace the first loop that reads domain properties with:
```rust
for prop in props {
    let schema = lookup_property(&prop.name);
    if let Some(group) = schema.and_then(|s| s.group.as_ref()) {
        if group.group_id == GroupHandlerId::PlotDomain {
            ctx.deferred_groups.entry(PlotDomain).or_default().push(prop);
        }
    }
}
// After loop, resolve groups:
resolve_groups(track, ctx.deferred_groups, env, ctx, diag);
```

**Step B — The big property pass (build.rs:1501-1490)**:

Replace the second loop with:
```rust
for prop in props {
    process_declaration_property(track, prop, &eval_env, &mut ctx, diag);
}
```

### 3.3 Migrate `declarations_text.rs`

The text property match (lines 145-210) becomes:
```rust
for prop in props {
    process_declaration_property(track, prop, &eval_env, &mut ctx, diag);
}
```

Remove the special-cased `font_size`, `text`/`latex`/`code`, `color` → these are now in the registry.

**Special handling needed:** The text declaration reads `color` to set both the track *and* an immediate typst `Color` for text shaping. The `content_matches()` property maps `"text"`, `"latex"`, `"math"`, `"code"` into `TextContent`. These are now in the registry with `field: ActorField::TextContent`.

Add a post-loop hook for text actors:
```rust
// After all properties are processed via the engine,
// extract the current text_content + font_size + color for text shaping:
if let ActorPayload::Text { ref content, .. } = track.payload {
    let text = content.get(t, String::new());
    let font_size = track.style ... // need font_size stored somewhere
    // Trigger typesetting
}
```

### 3.4 Migrate `assignments.rs`

Replace the giant `match property { "color" => .., "size" => .., ... }` with:
```rust
let schema = lookup_property(property).unwrap_or_else(|| {
    /* unknown property diagnostic */
    return;
});
if !schema.flags.contains(PropertyFlags::ASSIGNABLE) {
    /* not assignable diagnostic */
    return;
}
process_assignment_property(track, schema, value, &eval_env, &timing, diag);
```

### 3.5 Migrate `media.rs`

Same pattern — `process_declaration_property()` for each property in Svg/Image declarations.

### 3.6 Migrate `runtime.rs` — `inject_runtime_lookup_values`

Replace the per-property-name injection with the `inject_properties_into_env()` function.

Verify that every property with `INJECTABLE` flag is injected (compile-time check: the `ActorField` match is exhaustive).

---

## Phase 4: Cleanup

### 4.1 Remove backward-compat accessors

After all call sites have been migrated to use the tiered paths:
```rust
// Delete:
impl AnimationTrack {
    pub fn position(&self) -> &Option<...> { &self.geometry.position }
    // ... all other accessors
}
```

### 4.2 Remove `LayoutSizeState` fully

Delete the enum definition, its impl, and the three associated methods on `AnimationTrack`.

### 4.3 Remove `PrimitiveDescriptor::for_actor_type()`

Replace with `ActorKindId` stored in `track.header.kind`.

### 4.4 Remove `VectorShapeState` (if fully subsumed)

If the group handler `VectorShapeState` implements the full logic, delete the old struct.

### 4.5 Delete the old match-block functions

Any `process_*` functions in `build.rs`, `declarations_text.rs`, `assignments.rs`, `media.rs` that are dead code.

---

## Phase 5: Clean compile + tests

Run the full test suite:

```bash
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

Verify no regressions in existing `.amx` examples:

```bash
./scripts/check_examples.sh
```

---

## Checklist for adding a *new* property (future use)

```
□ 1. Add `ActorField` variant        → track.rs
□ 2. Add storage field                → GeometryTier / StyleTier / ActorPayload variant
□ 3. Add ValueType variant?           → property_registry.rs (rare — only if new value kind)
□ 4. Add `PropertySchema` row         → property_registry.rs
□ 5. Add index to allowed_properties  → actor_kind.rs
□ 6. Add injection (if INJECTABLE)    → runtime.rs / inject_properties_into_env()
□ 7. Add renderer handling (if render-relevant) → scene_eval.rs
□ 8. Add GUI widget mapping (if custom) → gui/source_edit.rs
□ 9. Add test                         → timeline/tests/
```

Items 3, 6, 7, and 8 are only needed for non-standard properties. For 80% of cases, steps 1-2 + 4-5 + 9 is the full set.
