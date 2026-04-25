use super::{
    AnimationTrack, Diagnostic, Easing, Expr, Timeline, apply_explicit_position_binding,
    evaluate_expr_with_lookup_diagnostic, resolve_position_binding_with_lookup_diagnostic,
};
use crate::ast::{Property, Stmt};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::timeline::Value;

fn push_media_load_failure_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    url: &str,
    message: String,
) {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::MediaLoadFailure,
            DiagnosticPhase::Build,
            message,
        )
        .with_subject(subject)
        .with_path(url),
    );
}

fn push_unsupported_media_modifier_diagnostics(
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
    actor_type: &str,
    modifiers: &[crate::ast::Modifier],
) {
    for modifier in modifiers {
        let modifier_name = modifier.name.as_deref().unwrap_or("duration-shorthand");
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::UnsupportedModifierKey,
                DiagnosticPhase::Build,
                format!(
                    "Unsupported modifier key '{modifier_name}' on {actor_type} actor declaration '{label}'; media declarations currently keep an instant-only contract."
                ),
            )
            .with_subject(label),
        );
    }
}

impl Timeline {
    pub(super) fn process_media_actor_decl(
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
        if !modifiers.is_empty() {
            push_unsupported_media_modifier_diagnostics(diagnostics, label, actor_type, modifiers);
        }
        let is_new_track = !self.tracks.contains_key(label);
        let track = self
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| AnimationTrack::new(label.to_string()));

        // Record first declaration time so scene evaluation can hide
        // actors before they are declared
        if is_new_track {
            track.first_seen_ms = time_ms as u64;
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

        match actor_type {
            "Svg" => {
                if url.is_empty() {
                    return;
                }
                match std::fs::read_to_string(&url) {
                    Ok(svg_content) => match crate::timeline::svg::parse_svg(&svg_content) {
                        Ok(mut parsed_paths) => {
                            if scale != 1.0 {
                                let affine = kurbo::Affine::scale(scale as f64);
                                for path in &mut parsed_paths {
                                    path.path.apply_affine(affine);
                                }
                            }
                            let measured_half_size =
                                crate::timeline::svg::measure_svg_paths(&parsed_paths);
                            track.size.add_keyframe(
                                time_ms as u64,
                                measured_half_size,
                                Easing::Linear,
                            );
                            track.svg_paths = parsed_paths;
                        }
                        Err(error) => push_media_load_failure_diagnostic(
                            diagnostics,
                            &format!("{}.url", label),
                            &url,
                            format!("Failed to parse SVG file '{url}': {error}"),
                        ),
                    },
                    Err(error) => push_media_load_failure_diagnostic(
                        diagnostics,
                        &format!("{}.url", label),
                        &url,
                        format!("Failed to read SVG file '{url}': {error}"),
                    ),
                }
            }
            "Image" => {
                if url.is_empty() {
                    return;
                }
                match crate::timeline::image::load_image(&url) {
                    Ok(image) => {
                        let display_size = authored_size
                            .unwrap_or([image.natural_size[0] / 2.0, image.natural_size[1] / 2.0]);
                        track
                            .size
                            .add_keyframe(time_ms as u64, display_size, Easing::Linear);
                        track
                            .image
                            .add_keyframe(time_ms as u64, Some(image), Easing::Linear);
                    }
                    Err(error) => push_media_load_failure_diagnostic(
                        diagnostics,
                        &format!("{}.url", label),
                        &url,
                        format!("Failed to load image file '{url}': {error}"),
                    ),
                }
            }
            _ => {}
        }
    }

    pub(super) fn process_media_statement(
        &mut self,
        stmt: &Stmt,
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match stmt {
            Stmt::Svg {
                label,
                url,
                at,
                anchor,
                offset,
                scale,
            } => {
                let label_str = label.clone().unwrap_or_else(|| "unnamed_svg".to_string());
                let eval_env = self.build_eval_env(time_ms as u64);
                self.add_node(label_str.clone(), parent_label);
                let is_new_track = !self.tracks.contains_key(&label_str);
                let track = self
                    .tracks
                    .entry(label_str.clone())
                    .or_insert_with(|| AnimationTrack::new(label_str.clone()));

                // Record first declaration time so scene evaluation can hide
                // actors before they are declared
                if is_new_track {
                    track.first_seen_ms = time_ms as u64;
                }

                let binding_subject = format!("{}.at", label_str);
                if let Some((binding, position)) = resolve_position_binding_with_lookup_diagnostic(
                    at.as_ref(),
                    anchor.as_ref(),
                    offset.as_ref(),
                    &eval_env,
                    diagnostics,
                    &binding_subject,
                ) {
                    apply_explicit_position_binding(track, time_ms as u64, binding, position);
                } else {
                    track
                        .position
                        .add_keyframe(time_ms as u64, [0.0, 0.0], Easing::Linear);
                }

                match std::fs::read_to_string(url) {
                    Ok(svg_content) => match crate::timeline::svg::parse_svg(&svg_content) {
                        Ok(mut parsed_paths) => {
                            if *scale != 1.0 {
                                let affine = kurbo::Affine::scale(*scale as f64);
                                for path in &mut parsed_paths {
                                    path.path.apply_affine(affine);
                                }
                            }
                            let measured_half_size =
                                crate::timeline::svg::measure_svg_paths(&parsed_paths);
                            track.size.add_keyframe(
                                time_ms as u64,
                                measured_half_size,
                                Easing::Linear,
                            );
                            track.svg_paths = parsed_paths;
                        }
                        Err(error) => push_media_load_failure_diagnostic(
                            diagnostics,
                            &format!("{}.url", label_str),
                            url,
                            format!("Failed to parse SVG file '{url}': {error}"),
                        ),
                    },
                    Err(error) => push_media_load_failure_diagnostic(
                        diagnostics,
                        &format!("{}.url", label_str),
                        url,
                        format!("Failed to read SVG file '{url}': {error}"),
                    ),
                }
            }
            Stmt::Image {
                label,
                url,
                at,
                anchor,
                offset,
                size,
            } => {
                let label_str = label.clone().unwrap_or_else(|| "unnamed_image".to_string());
                let eval_env = self.build_eval_env(time_ms as u64);
                self.add_node(label_str.clone(), parent_label);
                let is_new_track = !self.tracks.contains_key(&label_str);
                let track = self
                    .tracks
                    .entry(label_str.clone())
                    .or_insert_with(|| AnimationTrack::new(label_str.clone()));

                // Record first declaration time so scene evaluation can hide
                // actors before they are declared
                if is_new_track {
                    track.first_seen_ms = time_ms as u64;
                }

                let binding_subject = format!("{}.at", label_str);
                if let Some((binding, position)) = resolve_position_binding_with_lookup_diagnostic(
                    at.as_ref(),
                    anchor.as_ref(),
                    offset.as_ref(),
                    &eval_env,
                    diagnostics,
                    &binding_subject,
                ) {
                    apply_explicit_position_binding(track, time_ms as u64, binding, position);
                } else {
                    track
                        .position
                        .add_keyframe(time_ms as u64, [0.0, 0.0], Easing::Linear);
                }

                match crate::timeline::image::load_image(url) {
                    Ok(image) => {
                        let display_size = size
                            .map(|(width, height)| [width / 2.0, height / 2.0])
                            .unwrap_or([image.natural_size[0] / 2.0, image.natural_size[1] / 2.0]);

                        track
                            .size
                            .add_keyframe(time_ms as u64, display_size, Easing::Linear);
                        track
                            .image
                            .add_keyframe(time_ms as u64, Some(image), Easing::Linear);
                    }
                    Err(error) => push_media_load_failure_diagnostic(
                        diagnostics,
                        &format!("{}.url", label_str),
                        url,
                        format!("Failed to load image file '{url}': {error}"),
                    ),
                }
            }
            _ => unreachable!("process_media_statement only handles svg/image statements"),
        }
    }
}
