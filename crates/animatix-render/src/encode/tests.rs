//! Tests for the shared export types.

use super::*;

#[test]
fn export_presets_are_named_and_resolvable() {
    assert_eq!(ExportPreset::ALL.len(), 4);
    let preset = ExportPreset::by_name("1080p30").expect("1080p30 should exist");
    assert_eq!(preset.width, 1920);
    assert_eq!(preset.height, 1080);
    assert_eq!(preset.fps, 30);
    assert!(ExportPreset::by_name("missing").is_none());
}
