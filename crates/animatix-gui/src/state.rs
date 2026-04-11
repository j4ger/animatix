use animatix::ast::Stmt;
use animatix::module::ModuleGraph;
use animatix::renderer::render_image;
use animatix::timeline::{AnimationTrack, PropertyTrack, Timeline};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct PreviewState {
    pub image_path: Option<PathBuf>,
    pub status: String,
    pub error: Option<String>,
}

pub struct SessionState {
    pub file_path: PathBuf,
    pub source_text: String,
    pub ast: Option<Vec<Stmt>>,
    pub current_time_s: f64,
    pub duration_s: f64,
    pub is_dirty: bool,
    pub is_playing: bool,
    pub preview: PreviewState,
    pub(crate) preview_revision: u64,
}

impl SessionState {
    pub fn load(file_path: PathBuf) -> Result<Self, String> {
        let source_text = fs::read_to_string(&file_path)
            .map_err(|err| format!("Failed to read {}: {err}", file_path.display()))?;

        let mut state = Self {
            file_path,
            source_text,
            ast: None,
            current_time_s: 0.0,
            duration_s: 5.0,
            is_dirty: false,
            is_playing: false,
            preview: PreviewState {
                image_path: None,
                status: "Loaded file".to_string(),
                error: None,
            },
            preview_revision: 0,
        };

        state.rebuild()?;
        Ok(state)
    }

    pub fn from_error(file_path: PathBuf, error: String) -> Self {
        Self {
            file_path,
            source_text: String::new(),
            ast: None,
            current_time_s: 0.0,
            duration_s: 5.0,
            is_dirty: false,
            is_playing: false,
            preview: PreviewState {
                image_path: None,
                status: "Failed to initialize session".to_string(),
                error: Some(error),
            },
            preview_revision: 0,
        }
    }

    pub fn set_source_text(&mut self, source_text: String) {
        self.source_text = source_text;
        self.is_dirty = true;
    }

    pub fn reload_from_disk(&mut self) -> Result<(), String> {
        self.source_text = fs::read_to_string(&self.file_path)
            .map_err(|err| format!("Failed to reload {}: {err}", self.file_path.display()))?;
        self.is_dirty = false;
        self.rebuild()
    }

    pub fn save_to_disk(&mut self) -> Result<(), String> {
        fs::write(&self.file_path, &self.source_text)
            .map_err(|err| format!("Failed to save {}: {err}", self.file_path.display()))?;
        self.is_dirty = false;
        self.preview.status = format!("Saved {}", self.file_path.display());
        Ok(())
    }

    pub fn rebuild(&mut self) -> Result<(), String> {
        let mut graph = ModuleGraph::new();
        let ast = graph
            .load_entry_with_source(&self.file_path, Some(&self.source_text))
            .map_err(|err| err.to_string())?;
        let timeline = Timeline::build(&ast);
        self.duration_s = timeline_duration_seconds(&timeline).max(0.1);
        self.ast = Some(ast);
        self.preview.error = None;
        self.preview.status = format!("Built timeline • {:.2}s total duration", self.duration_s);
        self.render_preview()?;
        Ok(())
    }

    pub fn set_current_time(&mut self, next_time_s: f64) -> Result<(), String> {
        self.current_time_s = next_time_s.clamp(0.0, self.duration_s.max(0.0));
        self.render_preview()
    }

    pub fn tick_playback(&mut self, delta: Duration) -> Result<(), String> {
        if !self.is_playing {
            return Ok(());
        }

        let next_time = self.current_time_s + delta.as_secs_f64();
        if next_time >= self.duration_s {
            self.current_time_s = self.duration_s;
            self.is_playing = false;
        } else {
            self.current_time_s = next_time;
        }

        self.render_preview()
    }

    pub fn toggle_playback(&mut self) {
        if self.current_time_s >= self.duration_s {
            self.current_time_s = 0.0;
        }
        self.is_playing = !self.is_playing;
    }

    fn render_preview(&mut self) -> Result<(), String> {
        let ast = self
            .ast
            .as_ref()
            .ok_or_else(|| "No compiled scene available for preview".to_string())?;

        self.preview_revision += 1;
        let preview_path = preview_output_path(self.preview_revision);
        render_image(ast, 1280, 720, self.current_time_s as f32, &preview_path);

        self.preview.status = format!(
            "Preview rendered • t = {:.2}s / {:.2}s",
            self.current_time_s, self.duration_s
        );
        self.preview.image_path = Some(preview_path);
        self.preview.error = None;
        Ok(())
    }
}

fn preview_output_path(revision: u64) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!("animatix_gui_preview_{stamp}_{revision}.png"))
}

fn latest_keyframe_ms<T>(track: &PropertyTrack<T>) -> Option<u64> {
    track.keyframes.keys().next_back().copied()
}

fn track_max_ms(track: &AnimationTrack) -> u64 {
    let mut max_ms = 0u64;

    for time in [
        latest_keyframe_ms(&track.position),
        latest_keyframe_ms(&track.placement_mode),
        latest_keyframe_ms(&track.position_binding),
        latest_keyframe_ms(&track.size),
        latest_keyframe_ms(&track.line_from),
        latest_keyframe_ms(&track.line_to),
        latest_keyframe_ms(&track.arc_angles),
        latest_keyframe_ms(&track.color),
        latest_keyframe_ms(&track.shape_type),
        latest_keyframe_ms(&track.opacity),
        latest_keyframe_ms(&track.stroke_width),
        latest_keyframe_ms(&track.stroke_color),
        latest_keyframe_ms(&track.stroke_progress),
        latest_keyframe_ms(&track.fill_opacity),
        latest_keyframe_ms(&track.text_paths),
        latest_keyframe_ms(&track.vector_paths),
        latest_keyframe_ms(&track.image),
    ]
    .into_iter()
    .flatten()
    {
        max_ms = max_ms.max(time);
    }

    max_ms
}

pub fn timeline_duration_seconds(timeline: &Timeline) -> f64 {
    timeline
        .tracks
        .values()
        .map(track_max_ms)
        .max()
        .unwrap_or(0) as f64
        / 1000.0
}

pub fn default_file_path() -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| Path::new("."));
    repo_root.join("examples/showcase.amx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix::ast::{Property, Time};

    #[test]
    fn duration_scans_latest_track_keyframe() {
        let ast = vec![
            Stmt::Keyframe {
                time: Time::Seconds(0.0),
                body: vec![Stmt::ActorDecl {
                    is_pub: false,
                    label: "box".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: animatix::ast::Expr::Tuple(vec![
                            animatix::ast::Expr::Num(100.0),
                            animatix::ast::Expr::Num(100.0),
                        ]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                }],
            },
            Stmt::RelativeKeyframe {
                offset: Time::Seconds(2.0),
                body: vec![Stmt::Assignment {
                    target: "box".to_string(),
                    property: "opacity".to_string(),
                    value: animatix::ast::Expr::Num(0.5),
                    modifiers: vec![],
                }],
            },
        ];

        let timeline = Timeline::build(&ast);
        assert_eq!(timeline_duration_seconds(&timeline), 2.0);
    }
}
