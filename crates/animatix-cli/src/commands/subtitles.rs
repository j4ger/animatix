use std::path::Path;

use animatix::renderer::text::transcript::SubtitleFormat;

pub fn run(
    input: &Path,
    defines: &[(String, String)],
    track: Option<&str>,
    output: &Path,
    extensions: &crate::extensions::CliExtensions,
) -> Result<(), String> {
    let format = match output.extension().and_then(|extension| extension.to_str()) {
        Some("srt") => SubtitleFormat::Srt,
        Some("vtt") => SubtitleFormat::Vtt,
        _ => return Err("subtitle output must end in .srt or .vtt".to_owned()),
    };
    let (target, diagnostics) = crate::build::load_and_build(input, extensions, defines);
    if diagnostics.iter().any(|diagnostic| diagnostic.is_error()) {
        return Err("could not build subtitle timeline".to_owned());
    }
    let cues = target.subtitle_cues(track);
    if cues.is_empty() {
        return Err("no subtitle cues within the selected timeline and track".to_owned());
    }
    std::fs::write(output, format.encode(&cues))
        .map_err(|error| format!("could not write {}: {error}", output.display()))
}
