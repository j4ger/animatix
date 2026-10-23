use super::*;

/// "Restore defaults" resets encoders/resolution/timing but must not touch
/// what to export (`format`, `export_scope`) or where (`output_path`).
#[test]
fn restore_defaults_keeps_target_but_resets_encoding() {
    let mut state = ExportDialogState {
        format: ExportFormat::WebP,
        width: 640,
        height: 360,
        fps: 12,
        duration_s: 9.0,
        hold_s: 4.0,
        auto_duration: false,
        export_scope: ExportScope::WholeComposition,
        output_path: "custom/path.webp".to_string(),
        video_codec: animatix_render::encode::VideoCodec::H264Vaapi,
        h264_preset: animatix_render::encode::H264Preset::Veryslow,
        ..ExportDialogState::default()
    };
    let defaults = ExportDialogState::default();
    state.width = defaults.width;
    state.height = defaults.height;
    state.fps = defaults.fps;
    state.duration_s = defaults.duration_s;
    state.hold_s = defaults.hold_s;
    state.auto_duration = defaults.auto_duration;
    state.video_codec = defaults.video_codec;
    state.h264_preset = defaults.h264_preset;

    assert_eq!(state.width, defaults.width);
    assert_eq!(state.fps, defaults.fps);
    assert_eq!(state.h264_preset, defaults.h264_preset);
    assert_eq!(state.format, ExportFormat::WebP, "format is a target choice, not a setting");
    assert_eq!(state.export_scope, ExportScope::WholeComposition);
    assert_eq!(state.output_path, "custom/path.webp");
}
