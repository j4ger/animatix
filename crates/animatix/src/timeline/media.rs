use super::{
    AnimationTrack, AudioSegment, Diagnostic, Easing, Expr, Timeline, apply_explicit_position_binding,
    evaluate_expr_with_lookup_diagnostic, resolve_position_binding_with_lookup_diagnostic,
    TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE,
};
use super::property_engine::{parse_property_value, write_property_field};
use super::property_registry::lookup_property;
use crate::ast::Property;
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::timeline::Value;

fn push_media_load_failure_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    url: &str,
    message: String,
) {
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::MediaLoadFailure,
            DiagnosticPhase::Build,
            message,
        )
        .with_subject(subject)
        .with_path(url),
    );
}


fn seed_svg_track(
    track: &mut AnimationTrack,
    diagnostics: &mut Vec<Diagnostic>,
    subject_label: &str,
    url: &str,
    scale: f32,
    time_ms: u64,
) {
    if url.is_empty() {
        return;
    }

    match std::fs::read_to_string(url) {
        Ok(svg_content) => match crate::timeline::svg::parse_svg(&svg_content) {
            Ok(mut parsed_paths) => {
                if scale != 1.0 {
                    let affine = kurbo::Affine::scale(scale as f64);
                    for path in &mut parsed_paths {
                        path.path.apply_affine(affine);
                    }
                }
                let measured_half_size = crate::timeline::svg::measure_svg_paths(&parsed_paths);
                track.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                    time_ms,
                    measured_half_size,
                    Easing::Linear,
                );
                track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                    time_ms,
                    measured_half_size,
                    Easing::Linear,
                );
                track.svg_paths = parsed_paths;
            }
            Err(error) => push_media_load_failure_diagnostic(
                diagnostics,
                &format!("{}.url", subject_label),
                url,
                format!("Failed to parse SVG file '{url}': {error}"),
            ),
        },
        Err(error) => push_media_load_failure_diagnostic(
            diagnostics,
            &format!("{}.url", subject_label),
            url,
            format!("Failed to read SVG file '{url}': {error}"),
        ),
    }
}

fn seed_image_track(
    track: &mut AnimationTrack,
    diagnostics: &mut Vec<Diagnostic>,
    subject_label: &str,
    url: &str,
    authored_half_size: Option<[f32; 2]>,
    time_ms: u64,
) {
    if url.is_empty() {
        return;
    }

    match crate::timeline::image::load_image(url) {
        Ok(image) => {
            let display_size = authored_half_size
                .unwrap_or([image.natural_size[0] / 2.0, image.natural_size[1] / 2.0]);
            track
                .size
                .ensure(DEFAULT_LAYOUT_HALF_SIZE)
                .add_keyframe(time_ms, display_size, Easing::Linear);
            track
                .ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE)
                .add_keyframe(time_ms, display_size, Easing::Linear);
            track
                .image
                .ensure(None)
                .add_keyframe(time_ms, Some(image), Easing::Linear);
        }
        Err(error) => push_media_load_failure_diagnostic(
            diagnostics,
            &format!("{}.url", subject_label),
            url,
            format!("Failed to load image file '{url}': {error}"),
        ),
    }
}

impl Timeline {
    /// Process a media actor declaration (Image, Audio, Video) and add it to the timeline.
    pub fn process_media_actor_decl(
        &mut self,
        actor_type: &str,
        label: &str,
        props: &[Property],
        modifiers: &[crate::ast::Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let eval_env = self.build_eval_env(time_ms as u64);
        self.add_node(label.to_string(), parent_label);
        // Parse timing modifiers for Image/Svg declarations (timed re-declarations).
        use crate::timeline::timing::{
            parse_timing_modifiers, ModifierHost, ParsedTimingModifiers,
        };
        let ParsedTimingModifiers {
            delay_ms,
            ..
        } = parse_timing_modifiers(modifiers, ModifierHost::ActorDeclaration, Some(label), diagnostics);
        // Pre-seed opacity for pre-keyframe first declarations without explicit opacity.
        let has_explicit_opacity = props.iter().any(|p| p.name == "opacity");
        let is_first_decl = !self.tracks.contains_key(label);

        let track = self
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| AnimationTrack::new(label.to_string()));

        // Ensure the track kind matches the declaration type.
        track.kind = match actor_type {
            "Svg" => super::ActorKindId::Svg,
            "Image" => super::ActorKindId::Image,
            _ => track.kind,
        };

        // Record first declaration time so scene evaluation can hide
        // actors before they are declared
        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = time_ms as u64;
        }

        if is_first_decl && !has_explicit_opacity && self.default_opacity != 1.0 {
            track.opacity.ensure(1.0).add_keyframe(0, self.default_opacity, Easing::Linear);
        }

        let mut url = String::new();
        let mut scale = 1.0f32;
        let mut authored_size: Option<[f32; 2]> = None;
        let mut at_expr: Option<Expr> = None;
        let mut anchor_expr: Option<Expr> = None;
        let mut offset_expr: Option<Expr> = None;

        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "url" => {
                    url = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_default();
                }
                "scale" => {
                    scale = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(1.0))
                    .as_num() as f32;
                }
                "size" => {
                    if let Value::Vec2([width, height]) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0))
                    {
                        authored_size = Some([width as f32 / 2.0, height as f32 / 2.0]);
                    }
                }
                "at" => at_expr = Some(prop.value.clone()),
                "anchor" => anchor_expr = Some(prop.value.clone()),
                "offset" => offset_expr = Some(prop.value.clone()),
                _ => {}
            }
        }

        if let Some((binding, position)) = resolve_position_binding_with_lookup_diagnostic(
            at_expr.as_ref(),
            anchor_expr.as_ref(),
            offset_expr.as_ref(),
            &eval_env,
            diagnostics,
            label,
        ) {
            apply_explicit_position_binding(track, time_ms as u64, binding, position);
        }

        // Dispatch remaining (unhandled) properties through the generic engine so
        // universal properties like rotation and opacity work on media actors.
        for prop in props {
            let already_handled = match prop.name.as_str() {
                "url" | "size" => true,
                "scale" if actor_type == "Svg" => true, // Pre-parse scale, not transform scale
                "at" | "anchor" | "offset" => true,
                _ => false,
            };
            if already_handled {
                continue;
            }
            let prop_subject = format!("{}.{}", label, prop.name);
            if let Some(schema) = lookup_property(&prop.name) {
                if let Some(pv) = parse_property_value(schema.value_type, &prop.value, &eval_env, diagnostics, &prop_subject) {
                    write_property_field(track, schema.field, pv, time_ms as u64, time_ms as u64, Easing::Linear, diagnostics);
                }
            }
        }

        let seed_time_ms = (time_ms as u64).saturating_add(delay_ms as u64);

        match actor_type {
            "Svg" => seed_svg_track(track, diagnostics, label, &url, scale, seed_time_ms),
            "Image" => {
                seed_image_track(track, diagnostics, label, &url, authored_size, seed_time_ms)
            }
            _ => {}
        }
    }

    /// Process an Audio actor declaration. Audio is non-visual — it stores
    /// metadata on `Timeline::audio_segments` for muxing during export.
    pub fn process_audio_actor_decl(
        &mut self,
        label: &str,
        props: &[Property],
        modifiers: &[crate::ast::Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        self.add_node(label.to_string(), parent_label);
        let track = self
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| AnimationTrack::new(label.to_string()));
        track.kind = super::ActorKindId::Audio;

        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = time_ms as u64;
        }

        let eval_env = self.build_eval_env(time_ms as u64);
        let mut source = String::new();
        let mut volume = 1.0f32;

        // TODO: Eventually sample `source` and `volume` from the property track,
        //       via the property engine, to support keyframe-based animation.
        //       Currently these are read directly from the declaration props.
        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "source" => {
                    source = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_default();
                }
                "volume" => {
                    volume = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(1.0))
                    .as_num() as f32;
                }
                _ => {}
            }
        }

        if source.is_empty() {
            diagnostics.push(
                Diagnostic::warning(
                    crate::diagnostics::DiagnosticCode::InvalidConfigValue,
                    crate::diagnostics::DiagnosticPhase::Build,
                    format!("Audio actor '{label}' has no 'source' property; no audio will be played."),
                )
                .with_subject(label),
            );
            return;
        }

        // Parse timing modifiers to determine start time and duration
        use crate::timeline::timing::{
            parse_timing_modifiers, ModifierHost, ParsedTimingModifiers,
        };
        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            ..
        } = parse_timing_modifiers(modifiers, ModifierHost::ActorDeclaration, Some(label), diagnostics);

        let start_time_s = (time_ms + delay_ms) / 1000.0;
        let duration_s = if duration_ms > 0.0 { Some(duration_ms / 1000.0) } else { None };

        self.audio_segments.push(AudioSegment {
            source,
            start_time_s,
            duration_s,
            volume: volume.clamp(0.0, 1.0),
        });
    }
}
