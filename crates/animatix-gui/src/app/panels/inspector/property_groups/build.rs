use super::*;

// ─── Group Builder (generic via registry) ─────────────────────────────────

pub(crate) fn build_property_groups(
    timeline: &animatix::timeline::Timeline,
    id: EntityId,
    time_ms: u64,
) -> Vec<PropertyGroup> {
    let caps = timeline.world().caps(id);
    let actor_type = timeline.world().actor_type(id);
    let indices = allowed_property_indices(&caps, actor_type);
    let actor_label = timeline.world().label(id).unwrap_or("");
    let legend = timeline.entity(actor_label).and_then(|entity| entity.legend());
    let effects = timeline
        .entity(actor_label)
        .and_then(|entity| entity.effects())
        .cloned()
        .unwrap_or_default();

    let mut buckets = GroupBuckets::default();
    collect_registry_entries(&mut buckets, timeline, id, time_ms, &indices, actor_type);

    let mut legend_props = std::mem::take(&mut buckets.legend);
    legend_props.extend(collect_legend_extras(timeline, id, caps.legend_host, legend));

    let mut groups = buckets.into_groups();
    if !legend_props.is_empty() {
        groups.push(PropertyGroup {
            name: "Legend",
            icon: egui_phosphor::regular::CHART_LINE,
            properties: legend_props,
        });
    }

    let extension_props = collect_extension_properties(timeline, id, actor_type, time_ms);
    if !extension_props.is_empty() {
        groups.push(PropertyGroup {
            name: "Extensions",
            icon: egui_phosphor::regular::PLUG,
            properties: extension_props,
        });
    }

    let effect_props = collect_effect_properties(&effects, time_ms);
    if !effect_props.is_empty() {
        groups.push(PropertyGroup {
            name: "Effects",
            icon: egui_phosphor::regular::MAGIC_WAND,
            properties: effect_props,
        });
    }

    groups
}

/// Property entries bucketed by the inspector group they render in.
#[derive(Default)]
struct GroupBuckets {
    geometry: Vec<PropertyEntry>,
    layout: Vec<PropertyEntry>,
    style: Vec<PropertyEntry>,
    shape: Vec<PropertyEntry>,
    text: Vec<PropertyEntry>,
    plot: Vec<PropertyEntry>,
    annotation: Vec<PropertyEntry>,
    media: Vec<PropertyEntry>,
    audio: Vec<PropertyEntry>,
    captions: Vec<PropertyEntry>,
    visibility: Vec<PropertyEntry>,
    legend: Vec<PropertyEntry>,
}

impl GroupBuckets {
    /// Turn the buckets into groups, in inspector display order.
    fn into_groups(self) -> Vec<PropertyGroup> {
        let mut groups = Vec::new();
        push_group(
            &mut groups,
            "Transform",
            egui_phosphor::regular::ARROWS_OUT_CARDINAL,
            self.geometry,
        );
        push_group(&mut groups, "Layout", egui_phosphor::regular::ROWS, self.layout);
        push_group(&mut groups, "Style", egui_phosphor::regular::PAINT_BRUSH, self.style);
        push_group(&mut groups, "Shape", egui_phosphor::regular::SHAPES, self.shape);
        push_group(&mut groups, "Text", egui_phosphor::regular::TEXT_T, self.text);
        push_group(
            &mut groups,
            "Plot & Coordinates",
            egui_phosphor::regular::CHART_LINE,
            self.plot,
        );
        push_group(&mut groups, "Annotation", egui_phosphor::regular::TEXT_T, self.annotation);
        push_group(&mut groups, "Media", egui_phosphor::regular::FILM_STRIP, self.media);
        push_group(&mut groups, "Audio", egui_phosphor::regular::SPEAKER_HIGH, self.audio);
        push_group(
            &mut groups,
            "Captions",
            egui_phosphor::regular::TEXT_ALIGN_CENTER,
            self.captions,
        );
        // `solo` is a scene-visibility switch, not a text/counter knob: every actor
        // type carries it (`Applicable::Everything`), so it gets its own group
        // instead of growing a bogus "Text" group on shapes.
        push_group(&mut groups, "Visibility", egui_phosphor::regular::EYE, self.visibility);
        groups
    }
}

/// Push `properties` as a group when the bucket is non-empty.
fn push_group(
    groups: &mut Vec<PropertyGroup>,
    name: &'static str,
    icon: &'static str,
    properties: Vec<PropertyEntry>,
) {
    if !properties.is_empty() {
        groups.push(PropertyGroup {
            name,
            icon,
            properties,
        });
    }
}

/// Route every registry-backed property into its group bucket.
fn collect_registry_entries(
    buckets: &mut GroupBuckets,
    timeline: &animatix::timeline::Timeline,
    id: EntityId,
    time_ms: u64,
    indices: &[usize],
    actor_type: &str,
) {
    for &idx in indices {
        let schema = &PROPERTY_REGISTRY[idx];
        // Skip group-resolution fields and build-time-only properties
        if is_inspector_skipped(schema) {
            continue;
        }
        let entry = registry_entry(timeline, id, time_ms, schema);
        route_entry(buckets, entry, schema.field, actor_type);
    }
}

fn is_inspector_skipped(schema: &animatix::timeline::PropertySchema) -> bool {
    schema.value_type == ValueType::BuildTimeOnly
        || schema.value_type == ValueType::CommandList
        || schema.value_type == ValueType::PointList
        || matches!(schema.field, ActorField::PositionBindingGroup | ActorField::VectorShapeGroup)
}

/// Build the inspector entry (value, edit kind, keyframe state) for one schema row.
fn registry_entry(
    timeline: &animatix::timeline::Timeline,
    id: EntityId,
    time_ms: u64,
    schema: &animatix::timeline::PropertySchema,
) -> PropertyEntry {
    let value = read_inspector_property_value(timeline, id, schema, time_ms);
    // One store: the World field bundle is the shape-aware view — it joins
    // the property plan for tagged fields with the tier tracks (including
    // the shape component), so shape-tier keyframes are not invisible here.
    let has_kf = timeline.world().property_has_keyframes(id, schema.field);
    let has_kf_now = timeline.world().property_has_keyframe_at(id, schema.field, time_ms);
    let kf_count = timeline.world().property_keyframe_count(id, schema.field);

    let value = convert_for_display(value, schema.name);
    let kind = match schema.value_type {
        ValueType::Union(variants) => PropertyKind::Union { variants, value },
        ValueType::Sum(variants) => PropertyKind::Sum { variants, value },
        ValueType::Enum(variants) => PropertyKind::Enum {
            variants,
            value: match value {
                PropertyValue::Enum(s) | PropertyValue::String(s) => s,
                other => format!("{other:?}"),
            },
        },
        _ => value_to_kind(value, schema.value_type, schema.name),
    };
    PropertyEntry {
        name: schema.name.to_string(),
        kind,
        has_keyframes: has_kf,
        has_keyframe_at_current_time: has_kf_now,
        keyframe_count: kf_count,
    }
}

/// Send one registry entry to the bucket for its actor field.
fn route_entry(
    buckets: &mut GroupBuckets,
    entry: PropertyEntry,
    field: ActorField,
    actor_type: &str,
) {
    match field {
        // Geometry tier
        ActorField::Position
        | ActorField::MotionOffset
        | ActorField::Size
        | ActorField::LayoutSize
        | ActorField::Rotation
        | ActorField::Scale
        | ActorField::PlacementMode
        | ActorField::PositionBinding
        | ActorField::Transform
        | ActorField::MinWidth
        | ActorField::MinHeight
        | ActorField::MaxHeight => buckets.geometry.push(entry),

        // Container & Layout
        ActorField::ContainerLayoutGroup => buckets.layout.push(entry),

        // Plot & Domain
        ActorField::PlotDomainGroup => buckets.plot.push(entry),

        // Style tier
        ActorField::Color
        | ActorField::Opacity
        | ActorField::StrokeWidth
        | ActorField::StrokeColor
        | ActorField::StrokeProgress
        | ActorField::FillOpacity
        | ActorField::MorphOptions
        | ActorField::LineCap
        | ActorField::LineJoin
        | ActorField::DashPattern
        | ActorField::DashOffset
        | ActorField::Blend
        | ActorField::FillGradient
        | ActorField::StrokeGradient
        | ActorField::GradientExtend
        | ActorField::GradientSpace => buckets.style.push(entry),

        // Shape tier
        ActorField::ShapeType
        | ActorField::LineFrom
        | ActorField::LineTo
        | ActorField::ArcAngles
        | ActorField::Points
        | ActorField::VectorPaths
        | ActorField::HeadSize
        | ActorField::CornerRadius => buckets.shape.push(entry),

        // Text tier
        ActorField::TextContent
        | ActorField::FontFamily
        | ActorField::FontSize
        | ActorField::FontWeight
        | ActorField::FontStyle
        | ActorField::LineHeight
        | ActorField::LetterSpacing
        | ActorField::WordSpacing
        | ActorField::TextMaxWidth
        | ActorField::TextAlign
        | ActorField::Overflow
        | ActorField::Language
        | ActorField::CharProgress
        | ActorField::TextPaths
        | ActorField::Ascent
        | ActorField::Descent
        | ActorField::Baseline
        | ActorField::HighlightColor
        | ActorField::HighlightOpacity
        | ActorField::HighlightPadding
        | ActorField::HighlightRadius => buckets.text.push(entry),

        // Annotation tier
        ActorField::LabelAt
        | ActorField::CalloutTarget
        | ActorField::CalloutPlace
        | ActorField::CalloutStandoff
        | ActorField::CalloutToOffset => buckets.annotation.push(entry),

        // Media & Audio tier
        ActorField::ImageData | ActorField::SvgPaths => buckets.media.push(entry),
        ActorField::AudioSource | ActorField::AudioVolume => buckets.audio.push(entry),

        // Tagged union properties
        ActorField::Tagged(tag) => route_tagged_entry(buckets, entry, tag, actor_type),
        _ => {},
    }
}

/// Tagged union properties, routed by their tag (some are actor-type specific).
fn route_tagged_entry(
    buckets: &mut GroupBuckets,
    entry: PropertyEntry,
    tag: &'static str,
    actor_type: &str,
) {
    match tag {
        "arrow" | "routing" => buckets.shape.push(entry),
        "callout_place" => buckets.annotation.push(entry),
        "shadow" | "shadow_color" => buckets.style.push(entry),
        "rotate_x" | "rotate_y" | "perspective" | "idle" => buckets.geometry.push(entry),
        "solo" => buckets.visibility.push(entry),
        "comma" | "decimals" | "prefix" | "suffix" | "value" | "split_by" | "split_mask"
        | "split_offset_y" | "split_stagger" => buckets.text.push(entry),
        "transcript" if actor_type == "Audio" => buckets.audio.push(entry),
        "caption_mode" | "active_color" | "inactive_color" | "backdrop" | "backdrop_color"
        | "backdrop_radius" | "backdrop_padding" | "pop_scale" | "max_lines"
        | "transition_time" | "transcript" => buckets.captions.push(entry),
        s if s.starts_with("legend_") => buckets.legend.push(entry),
        _ => buckets.style.push(entry),
    }
}

/// Legend entries that are not registry properties: style tracks and the
/// actor's legend participation.
fn collect_legend_extras(
    timeline: &animatix::timeline::Timeline,
    id: EntityId,
    legend_host: bool,
    legend: Option<&animatix::timeline::LegendTracks>,
) -> Vec<PropertyEntry> {
    let mut props = Vec::new();
    if legend_host {
        props.extend(legend_style_entries(legend));
    }
    let mode = animatix::timeline::legend::legend_mode_for_track(
        id,
        timeline.world().meta(id).first_seen_ms,
        timeline.world(),
    );
    if legend.and_then(|legend| legend.color).is_some()
        || mode != animatix::timeline::LegendMode::Auto
    {
        props.push(legend_participation_entry(mode));
    }
    props
}

/// Properties contributed by loaded extension descriptors that target this actor type.
fn collect_extension_properties(
    timeline: &animatix::timeline::Timeline,
    id: EntityId,
    actor_type: &str,
    time_ms: u64,
) -> Vec<PropertyEntry> {
    let mut props = Vec::new();
    let plan = timeline.world().property_plan(id);
    for descriptor in timeline.extension_property_descriptors() {
        if !descriptor.actor_types.iter().any(|ty| ty.as_str() == actor_type) {
            continue;
        }
        let id = animatix::property_descriptor::runtime_id(&descriptor);
        let Some(value) = animatix::timeline::read_property_plan_slot(plan, id, time_ms) else {
            continue;
        };
        props.push(PropertyEntry {
            name: descriptor.name.clone(),
            kind: extension_value_to_kind(value, &descriptor.ty),
            has_keyframes: plan.keyframe_count(id) > 0,
            has_keyframe_at_current_time: plan.has_keyframe_at(id, time_ms),
            keyframe_count: plan.keyframe_count(id),
        });
    }
    props
}

/// Effect stages on a compositing scope. Entry names are `stage.param`;
/// the property-edit handler routes the dotted form to the effect chain.
fn collect_effect_properties(
    effects: &animatix::timeline::effects::EffectChainTrack,
    time_ms: u64,
) -> Vec<PropertyEntry> {
    let mut props = Vec::new();
    for stage in &effects.stages {
        let Some(effect) = animatix::timeline::effects::effect(&stage.kind) else {
            continue;
        };
        for spec in effect.params() {
            let track_opt = stage.params.get(spec.name.as_ref());
            let value = track_opt.and_then(|param| param.sample(time_ms)).unwrap_or_else(|| {
                animatix::timeline::effects::identity_to_property(spec.identity)
            });
            props.push(PropertyEntry {
                name: format!("{}.{}", stage.label, spec.name),
                kind: effect_value_to_kind(spec.kind, value),
                has_keyframes: track_opt.is_some_and(|param| param.keyframe_count() > 0),
                has_keyframe_at_current_time: track_opt
                    .is_some_and(|param| param.has_keyframe_at(time_ms)),
                keyframe_count: track_opt.map_or(0, |param| param.keyframe_count()),
            });
        }
        props.push(PropertyEntry {
            name: format!("{}.enabled", stage.label),
            kind: PropertyKind::Bool(matches!(
                stage.enabled.sample(time_ms),
                Some(PropertyValue::Bool(true))
            )),
            has_keyframes: stage.enabled.keyframe_count() > 0,
            has_keyframe_at_current_time: stage.enabled.has_keyframe_at(time_ms),
            keyframe_count: stage.enabled.keyframe_count(),
        });
    }
    props
}
