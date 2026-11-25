use super::*;
use crate::composition::BuildTarget;
use crate::timeline::{BuildOptions, DebugRenderOptions, SceneDimensions};

#[derive(Debug)]
struct Fixture;

impl VideoAsset for Fixture {
    fn metadata(&self) -> VideoMetadata {
        VideoMetadata {
            display_size: [100, 50],
            duration_s: 4.0,
            audio_start_s: Some(0.3),
        }
    }

    fn frame_at(&self, source_time_s: f64) -> Result<Option<SceneImage>, String> {
        let pixels = vec![(source_time_s * 1000.0) as u8, 0, 0, 255];
        Ok(Some(SceneImage {
            data: peniko::ImageData {
                data: pixels.into(),
                format: peniko::ImageFormat::Rgba8,
                alpha_type: peniko::ImageAlphaType::Alpha,
                width: 1,
                height: 1,
            },
            natural_size: [1.0, 1.0],
        }))
    }
}

struct FixtureLoader;
impl VideoLoader for FixtureLoader {
    fn open(&self, _: &Path) -> Result<Arc<dyn VideoAsset>, String> {
        Ok(Arc::new(Fixture))
    }
}

fn build(source: &str) -> Timeline {
    let mut cache = crate::timeline::assets::AssetCache::new();
    cache.set_video_loader(Arc::new(FixtureLoader));
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}");
    let report = BuildTarget::build_with_options(
        &ast.unwrap(),
        &Default::default(),
        None,
        BuildOptions::default().with_asset_cache(Arc::new(cache)),
    );
    assert!(
        report.diagnostics.iter().all(|item| !item.is_error()),
        "{:?}",
        report.diagnostics
    );
    let BuildTarget::SingleScene(timeline) = report.output else {
        panic!("expected one scene")
    };
    timeline
}

#[test]
fn clip_clock_crop_and_audio_use_the_declared_range() {
    let timeline = build(
        r#"
        config { duration: 2 }
        #1s
          film: Video, source: "fixture.mp4", size: (80, 80),
            in_point: 500ms, out_point: 2s, fit: "contain",
            crop: (0.25, 0, 0.5, 1), volume: 0.2, end_behavior: "hold"

        #1.5s
          film.volume = 0.5
    "#,
    );
    let audio = timeline.audio_segments();
    assert_eq!(audio.len(), 1);
    assert!((audio[0].source_offset_s - 0.2).abs() < 1e-9);
    assert_eq!(audio[0].start_time_s, 1.0);
    assert_eq!(audio[0].duration_s, Some(1.0));
    assert_eq!(audio[0].volume_at(1.5), 0.5);
    let id = timeline.world.get_id("film").unwrap();
    let clip = timeline.world.video_clips.get(id).unwrap();
    assert_eq!(clip.source_time(0.99), None);
    assert_eq!(clip.source_time(1.0), Some(0.5));
    assert_eq!(clip.source_time(2.5), Some(2.0_f64.next_down()));

    let program = timeline.evaluate_program_with_debug(
        1.0,
        SceneDimensions::default(),
        DebugRenderOptions::default(),
        &mut None,
    );
    let command = &program.items.iter().find(|item| item.label == "film").unwrap().commands[0];
    let RenderCommand::ClippedImage {
        size, offset, clip, ..
    } = command
    else {
        panic!("{command:?}")
    };
    assert_eq!(*size, [160.0, 80.0]);
    assert_eq!(*offset, [-80.0, -40.0]);
    assert_eq!(*clip, kurbo::Rect::new(-40.0, -40.0, 40.0, 40.0));
}

#[test]
fn clip_defaults_to_hidden_after_its_inferred_end() {
    let timeline = build(
        r#"
        #2s
          film: Video, source: "fixture.mp4", in_point: 1s, out_point: 2s
    "#,
    );
    assert_eq!(timeline.playback_duration_seconds(), 3.0);
    let clip = timeline.world.video_clips.get(timeline.world.get_id("film").unwrap()).unwrap();
    assert_eq!(clip.source_time(2.0), Some(1.0));
    assert_eq!(clip.source_time(3.0), None);
}
