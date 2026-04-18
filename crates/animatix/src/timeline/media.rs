use super::{AnimationTrack, Diagnostic, Easing, Stmt, Timeline};

impl Timeline {
    pub(super) fn process_media_statement(
        &mut self,
        stmt: &Stmt,
        time_ms: f64,
        parent_label: Option<&str>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        match stmt {
            Stmt::Svg {
                label,
                url,
                at,
                scale,
            } => {
                let label_str = label.clone().unwrap_or_else(|| "unnamed_svg".to_string());
                self.add_node(label_str.clone(), parent_label);
                let track = self
                    .tracks
                    .entry(label_str.clone())
                    .or_insert_with(|| AnimationTrack::new(label_str));

                track
                    .position
                    .add_keyframe(time_ms as u64, [at.0, at.1], Easing::Linear);

                let svg_content = std::fs::read_to_string(url).unwrap_or_else(|e| {
                    eprintln!("Failed to read SVG file {}: {}", url, e);
                    String::new()
                });

                if !svg_content.is_empty() {
                    let mut parsed_paths = crate::timeline::svg::parse_svg(&svg_content);
                    if *scale != 1.0 {
                        let affine = kurbo::Affine::scale(*scale as f64);
                        for path in &mut parsed_paths {
                            path.path.apply_affine(affine);
                        }
                    }
                    let measured_half_size = crate::timeline::svg::measure_svg_paths(&parsed_paths);
                    track
                        .size
                        .add_keyframe(time_ms as u64, measured_half_size, Easing::Linear);
                    track.svg_paths = parsed_paths;
                }
            }
            Stmt::Image {
                label,
                url,
                at,
                size,
            } => {
                let label_str = label.clone().unwrap_or_else(|| "unnamed_image".to_string());
                self.add_node(label_str.clone(), parent_label);
                let track = self
                    .tracks
                    .entry(label_str.clone())
                    .or_insert_with(|| AnimationTrack::new(label_str));

                track
                    .position
                    .add_keyframe(time_ms as u64, [at.0, at.1], Easing::Linear);

                if let Some(image) = crate::timeline::image::load_image(url) {
                    let display_size = size
                        .map(|(width, height)| [width / 2.0, height / 2.0])
                        .unwrap_or([image.natural_size[0] / 2.0, image.natural_size[1] / 2.0]);

                    track
                        .size
                        .add_keyframe(time_ms as u64, display_size, Easing::Linear);
                    track
                        .image
                        .add_keyframe(time_ms as u64, Some(image), Easing::Linear);
                } else {
                    eprintln!("Failed to load image file {}", url);
                }
            }
            _ => unreachable!("process_media_statement only handles svg/image statements"),
        }
    }
}
