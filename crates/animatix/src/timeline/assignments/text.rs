use super::*;
use crate::timeline::animation_track::{GeometryTracks, TextTracks};

pub(crate) fn recompile_text_at_assignment(
    track: &mut AnimationTrack,
    text: &mut TextTracks,
    geometry: &mut GeometryTracks,
    target_text: String,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    instant_delayed: bool,
    duration_ms: f64,
    font_ctx: &crate::renderer::text::FontContext,
    text_compiler: &mut crate::renderer::text::TextCompiler,
) -> Result<(), RenderError> {
    if duration_ms > 0.0 {
        let start_val = text.text_content.get(t_start_ms, String::new());
        text.text_content
            .ensure(String::new())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else {
        // A step has to be fenced too. With no keyframe just before the stamp,
        // the previously declared string interpolates all the way into the new
        // one, and the cross-fade branch in `primitives` then compiles and draws
        // *both* strings for the whole preceding gap — the roadmap's "static
        // keyframe `.text =` assignments overprint". `preserve_instant_delayed_value`
        // already does the fencing for delayed steps; an undated assignment needs
        // exactly the same thing one millisecond earlier.
        preserve_instant_delayed_value(&mut text.text_content, t_start_ms);
    }
    text.text_content
        .ensure(String::new())
        .add_keyframe(t_end_ms, target_text.clone(), easing);

    // This helper receives a bare track plus the text and geometry tiers (the primitive
    // assignment path owns no `World`), so derive the projection from the
    // actor type, which is the same derivation the built-in catalog uses.
    // Extension overrides live in `World` and are applied by callers that
    // have one.
    let caps = animatix_std::caps_for_type(&track.actor_type).unwrap_or_default();
    let text_kind = match caps.text {
        Some(crate::timeline::TextKind::Text) => crate::renderer::text::TextKind::Text,
        Some(crate::timeline::TextKind::Code) => crate::renderer::text::TextKind::Code,
        Some(crate::timeline::TextKind::Typst) => crate::renderer::text::TextKind::Typst,
        // `Math` never recompiled on `text` assignment (no plain-text body).
        _ => return Ok(()),
    };

    let font_family = text.font_family.get(t_end_ms, String::new());
    let font_size = text.font_size.get(t_end_ms, 48.0);
    let font_weight = text.font_weight.get(t_end_ms, 400.0);
    let font_style = text.font_style.get(t_end_ms, "normal".to_string());
    let line_height = text.line_height.get(t_end_ms, 1.2);
    let letter_spacing = text.letter_spacing.get(t_end_ms, 0.0);
    let word_spacing = text.word_spacing.get(t_end_ms, 0.0);
    let color = track.style.color.get(t_end_ms, [1.0, 1.0, 1.0, 1.0]);
    let language = text.language.get(t_end_ms, String::new());

    let new_paths = text_compiler.compile(
        &target_text,
        &font_family,
        font_size,
        font_weight,
        &font_style,
        line_height,
        letter_spacing,
        word_spacing,
        color,
        text_kind,
        &language,
        font_ctx,
        0.0,
        "left",
        "visible",
    )?;
    let new_half_size = crate::renderer::text::measure_text_paths(&new_paths);
    let text_anchor = text.text_anchor.get(t_end_ms, "center".to_string());
    let mut new_paths_vec = new_paths.to_vec();
    let anchor_offset_x = match text_anchor.as_str() {
        "left" => new_half_size[0],
        "right" => -new_half_size[0],
        _ => 0.0,
    };
    if anchor_offset_x != 0.0 {
        let affine = kurbo::Affine::translate((anchor_offset_x as f64, 0.0));
        for p in &mut new_paths_vec {
            p.path.apply_affine(affine);
            p.glyph_center[0] += anchor_offset_x;
        }
    }

    if duration_ms > 0.0 {
        let start_val = track.evaluate_text_paths_and_clip(text, t_start_ms).0;
        text.text_paths
            .ensure(Vec::new())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
        let start_size = geometry.size.get(t_start_ms, DEFAULT_LAYOUT_HALF_SIZE);
        let start_layout_size =
            track.layout_size_get(geometry, t_start_ms).unwrap_or(DEFAULT_LAYOUT_HALF_SIZE);
        geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_start_ms,
            start_size,
            Easing::Linear,
        );
        track.ensure_layout_size(geometry, DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_start_ms,
            start_layout_size,
            Easing::Linear,
        );
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut text.text_paths, t_start_ms);
        preserve_instant_delayed_value(&mut geometry.size, t_start_ms);
        preserve_instant_delayed_value(&mut geometry.layout_size, t_start_ms);
    }

    // Content swaps cross-fade the source and target glyph sets instead of
    // morphing arbitrary strings through a midpoint string snapshot.
    if duration_ms > 0.0 {
        track
            .style
            .morph_options
            .ensure(crate::timeline::MorphOptions::default())
            .add_keyframe(
                t_end_ms,
                crate::timeline::MorphOptions {
                    strategy: crate::timeline::MorphStrategy::Fade,
                    ..Default::default()
                },
                easing,
            );
    }

    // Compute font metrics for baseline alignment. The inputs are identical to
    // the `text_compiler.compile` call above, so this is a process-wide cache
    // hit rather than a second Typst compilation.
    let (ascent, descent, baseline_offset) = match crate::renderer::text::compile_text_cached(
        text_kind,
        &target_text,
        &language,
        // Same palette the `text_compiler.compile` call above used, so this is
        // still a cache hit rather than a second compilation.
        text_compiler.highlight_palette.as_ref(),
        &font_family,
        font_size,
        font_weight,
        &font_style,
        line_height,
        letter_spacing,
        word_spacing,
        color,
        0.0,
        None,
        "left",
        "visible",
        text_compiler.text_fast_path,
        font_ctx,
    ) {
        Ok(cached) => (cached.ascent, cached.descent, cached.baseline_offset),
        // The same inputs already compiled successfully above; keep the
        // previous behavior of defaulting metrics when that ever fails.
        Err(_) => (0.0, 0.0, 0.0),
    };

    text.text_paths.ensure(Vec::new()).add_keyframe(t_end_ms, new_paths_vec, easing);
    geometry
        .size
        .ensure(DEFAULT_LAYOUT_HALF_SIZE)
        .add_keyframe(t_end_ms, new_half_size, easing);
    track.ensure_layout_size(geometry, DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
        t_end_ms,
        new_half_size,
        easing,
    );
    track.set_metrics(text, t_end_ms, ascent, descent, baseline_offset);
    Ok(())
}
