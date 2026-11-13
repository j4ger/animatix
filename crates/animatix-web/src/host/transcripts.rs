//! Validate fetched transcripts before resolving the final font closure.

use std::collections::HashSet;

use super::PreparedProgram;

impl PreparedProgram {
    /// Enrich this immutable source snapshot without parsing or building it again.
    /// Every declared transcript must be present, including audio-only transcripts
    /// needed by speech anchors. Only displayable cues contribute font demands.
    pub fn with_transcript_sources(&self, sources: &[(String, String)]) -> Result<Self, String> {
        let mut prepared = self.clone();
        let mut seen = HashSet::new();
        for (id, source) in sources {
            if !seen.insert(id.as_str()) {
                return Err(format!("duplicate transcript resource: {id}"));
            }
            if !self.analysis.assets.iter().any(|asset| asset.id == *id && asset.kind == "text") {
                return Err(format!("transcript is outside this prepared snapshot: {id}"));
            }
            let transcript = animatix::renderer::text::transcript::parse_transcript_auto(source)
                .map_err(|error| format!("invalid transcript '{id}': {error}"))?;
            if self.analysis.fonts.transcript_ids.contains(id) {
                for segment in transcript.segments {
                    prepared.analysis.fonts.text.push_str(&segment.text);
                    prepared.analysis.fonts.text.push('\n');
                    for word in segment.words {
                        prepared.analysis.fonts.text.push_str(&word.word);
                        prepared.analysis.fonts.text.push('\n');
                    }
                }
            }
        }
        for asset in self.analysis.assets.iter().filter(|asset| asset.kind == "text") {
            if !seen.contains(asset.id.as_str()) {
                return Err(format!("transcript resource was not prepared: {}", asset.id));
            }
        }
        Ok(prepared)
    }
}

#[cfg(test)]
mod tests {
    use super::super::prepare_document;

    const VTT: &str = "WEBVTT\n\n00:00.000 --> 00:01.000\n中文 subtitle\n";

    #[test]
    fn vector_tuple_loop_bindings_prepare_the_same_resources_as_the_builder() {
        let source = r#"
#0.1s
for (x, y), i in {(1, 2), (3, 4)} {
  icon[i]: Svg, url: format("point-{}-{}.svg", x, y)
  label[i]: Text, text: format("{} {}", x, y)
}
"#;
        let prepared = prepare_document(source, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert_eq!(prepared.analysis.assets.len(), 2);
        for url in ["point-1-2.svg", "point-3-4.svg"] {
            assert!(
                prepared
                    .analysis
                    .assets
                    .iter()
                    .any(|asset| asset.url == url && asset.kind == "svg")
            );
        }
        assert!(prepared.analysis.fonts.text.contains("1 2"));
        assert!(prepared.analysis.fonts.text.contains("3 4"));
        assert!(!prepared.analysis.fonts.dynamic);
    }

    #[test]
    fn inline_palette_loops_resolve_colorscheme_values_and_derived_resource_urls() {
        let source = r#"
let test_palette = Colorscheme {
  extends: "editorial-dark",
  accent.primary: (0.91, 0.12, 0.23, 1),
  text.primary: (0.21, 0.32, 0.43, 1)
}
config { colorscheme: "test_palette" }
#0.1s
row: Row {
  for color, i in {accent.primary, text.primary} {
    icon[i]: Svg, url: format("swatch-{}.svg", color)
  }
}
"#;
        let prepared = prepare_document(source, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert_eq!(prepared.analysis.assets.len(), 2);
        assert!(prepared.analysis.assets.iter().all(|asset| asset.kind == "svg"));
        assert!(prepared.analysis.assets.iter().all(|asset| asset.url.starts_with("swatch-(")));
        assert_ne!(prepared.analysis.assets[0].url, prepared.analysis.assets[1].url);
    }

    #[test]
    fn fetched_caption_cues_extend_fonts_without_changing_the_source_snapshot() {
        let source = "let track = \"./words.data\"\n#0.1s\nsubs: Captions, transcript: track\n";
        let original =
            prepare_document(source, &[], &[], Some("https://host.test/scenes/main.amx"));
        assert!(original.analysis.ok, "{:?}", original.analysis.diagnostics);
        let id = "https://host.test/scenes/words.data".to_string();
        assert!(
            original
                .analysis
                .assets
                .iter()
                .any(|asset| asset.id == id && asset.kind == "text")
        );
        let prepared = original.with_transcript_sources(&[(id, VTT.into())]).unwrap();
        assert!(prepared.analysis.fonts.text.contains("中文 subtitle"));
        assert!(!original.analysis.fonts.text.contains("中文"));
        assert!(!prepared.analysis.fonts.rich_text);
        assert!(!prepared.analysis.fonts.dynamic);
    }

    #[test]
    fn audio_only_transcripts_do_not_request_fonts_but_caption_sources_do() {
        let audio = "#0.1s\nvoice: Audio, source: \"voice.mp3\", transcript: \"words.vtt\"\n";
        let audio_only = prepare_document(audio, &[], &[], None);
        let prepared =
            audio_only.with_transcript_sources(&[("words.vtt".into(), VTT.into())]).unwrap();
        assert!(prepared.analysis.fonts.text.is_empty());
        assert!(prepared.analysis.fonts.families.is_empty());
        let source = format!("{audio}\nsubs: Captions, source: \"voice\"\n");
        let captions = prepare_document(&source, &[], &[], None);
        let prepared =
            captions.with_transcript_sources(&[("words.vtt".into(), VTT.into())]).unwrap();
        assert!(prepared.analysis.fonts.text.contains("中文"));
    }

    #[test]
    fn array_actor_labels_keep_caption_and_svg_resource_kinds_aligned() {
        let captions = r#"
let tracks = {"first.vtt", "second.vtt"}
#0.1s
for track, i in tracks {
  voice[i]: Audio, source: "voice.mp3", transcript: track
}
subs: Captions, source: "voice__0"
"#;
        let prepared = prepare_document(captions, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert_eq!(prepared.analysis.fonts.transcript_ids, ["first.vtt"]);
        let prepared = prepared
            .with_transcript_sources(&[
                ("first.vtt".into(), VTT.into()),
                ("second.vtt".into(), "WEBVTT\n\n00:00.000 --> 00:01.000\nsecond 中文\n".into()),
            ])
            .unwrap();
        assert!(prepared.analysis.fonts.text.contains("中文 subtitle"));
        assert!(!prepared.analysis.fonts.text.contains("second 中文"));

        let svg = r#"
let urls = {"first.svg", "second.svg"}
#0.1s
for url, i in urls {
  icon[i]: Svg, url: url
}
icon[0].url = "replacement.svg"
"#;
        let prepared = prepare_document(svg, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert_eq!(
            prepared
                .analysis
                .assets
                .iter()
                .map(|asset| asset.kind.as_str())
                .collect::<Vec<_>>(),
            ["svg", "svg", "svg"]
        );

        let nested_svg = r#"
#0.1s
group: Group {
  svgChild: Svg
}
group.svgChild.url = "nested.svg"
"#;
        let prepared = prepare_document(nested_svg, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert_eq!(prepared.analysis.assets.len(), 1);
        assert_eq!(prepared.analysis.assets[0].kind, "svg");
    }

    #[test]
    fn known_branches_only_collect_selected_font_and_resource_requirements() {
        let conditional = r#"
let choose_rich = false
#0.1s
if choose_rich {
  rich: Typst, text: "中文"
  unused: Image, url: unresolved_url
} else {
  label: Text, text: "English"
}
"#;
        let prepared = prepare_document(conditional, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert!(prepared.analysis.fonts.text.contains("English"));
        assert!(!prepared.analysis.fonts.text.contains("中文"));
        assert!(prepared.analysis.assets.is_empty());
        assert_eq!(prepared.analysis.required_features, 0);

        for source in [
            r#"
let choice = (1, 2)
#0.1s
match choice {
  (1, 2) => {
    icon: Svg, url: "tuple.svg"
  }
  _ => {
    pic: Image, url: unresolved_url
  }
}
"#,
            r#"
let choice = 2
#0.1s
match choice {
  1..=3 => {
    icon: Svg, url: "range.svg"
  }
  _ => {
    pic: Image, url: unresolved_url
  }
}
"#,
        ] {
            let prepared = prepare_document(source, &[], &[], None);
            assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
            assert_eq!(prepared.analysis.assets.len(), 1);
            assert_eq!(prepared.analysis.assets[0].kind, "svg");
            assert_eq!(prepared.analysis.required_features, 1 << 2);
        }
    }

    #[test]
    fn inline_row_loop_binds_values_for_static_text_and_resource_closure() {
        let source = r#"
let urls = {"first.svg", "second.svg"}
#0.1s
row: Row {
  for url, i in urls {
    icon[i]: Svg, url: url
    label[i]: Text, text: url
  }
}
"#;
        let prepared = prepare_document(source, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert!(!prepared.analysis.fonts.dynamic);
        assert!(prepared.analysis.fonts.text.contains("first.svg"));
        assert!(prepared.analysis.fonts.text.contains("second.svg"));
        assert_eq!(
            prepared
                .analysis
                .assets
                .iter()
                .map(|asset| (asset.id.as_str(), asset.kind.as_str()))
                .collect::<Vec<_>>(),
            [("first.svg", "svg"), ("second.svg", "svg")]
        );
    }

    #[test]
    fn unknown_condition_conservatively_collects_possible_text_without_forcing_dynamic_fonts() {
        let source = r#"
#0.1s
gate: Rect
if gate.opacity > 0 {
  positive: Rect
} else {
  negative: Rect
}
"#;
        let prepared = prepare_document(source, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert!(prepared.analysis.fonts.text.is_empty());
        assert!(prepared.analysis.fonts.families.is_empty());
        assert!(!prepared.analysis.fonts.dynamic);

        let source = r#"
#0.1s
gate: Rect
if gate.opacity > 0 {
  positive: Text, text: "First English"
} else {
  negative: Text, text: "Second English"
}
"#;
        let prepared = prepare_document(source, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert!(prepared.analysis.fonts.text.contains("First English"));
        assert!(prepared.analysis.fonts.text.contains("Second English"));
        assert!(!prepared.analysis.fonts.dynamic);
    }

    #[test]
    fn invalid_missing_and_unrelated_transcripts_fail_preparation() {
        let prepared =
            prepare_document("#0.1s\nsubs: Captions, transcript: \"words.vtt\"\n", &[], &[], None);
        assert!(prepared.with_transcript_sources(&[]).is_err());
        assert!(
            prepared
                .with_transcript_sources(&[("words.vtt".into(), "<html>bad</html>".into())])
                .is_err()
        );
        assert!(prepared.with_transcript_sources(&[("other.vtt".into(), VTT.into())]).is_err());
        let input = ("words.vtt".into(), VTT.into());
        assert!(prepared.with_transcript_sources(&[input.clone(), input]).is_err());
    }
}
