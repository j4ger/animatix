use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use animatix::composition::{BuildTarget, Composition};
use animatix::timeline::AudioSegment;
use animatix_render::encode::{self, ExportSettings, H264Preset, VideoCodec, VideoFrameRange};
use animatix_syntax::ast::Stmt;
use animatix_syntax::module::ModuleGraph;
use animatix_syntax::token::LineIndex;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::render::{effective_video_settings, resolve_duration};
use crate::build::{print_build_diagnostics, try_load_and_build_in};
use crate::extensions::CliExtensions;

#[derive(clap::Args, Debug)]
pub struct PlanArgs {
    /// Input Animatix composition (.amx).
    input: PathBuf,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long, default_value_t = 30)]
    fps: u32,
    #[arg(long)]
    duration: Option<f64>,
    #[arg(long, default_value_t = 1.0)]
    hold: f64,
    #[arg(long, default_value = "auto")]
    codec: VideoCodec,
    #[arg(long, default_value = "medium")]
    preset: H264Preset,
    #[arg(long)]
    export_preset: Option<String>,
}

#[derive(Serialize)]
struct ChapterPlan<'a> {
    version: u32,
    width: u32,
    height: u32,
    fps: u32,
    duration_s: f64,
    codec: String,
    preset: &'static str,
    files: BTreeMap<PathBuf, String>,
    shared_source: String,
    scenes: BTreeMap<&'a str, SceneInput>,
    chapters: Vec<Chapter<'a>>,
    audio: Vec<AudioInput>,
}

#[derive(Serialize)]
struct Chapter<'a> {
    name: &'a str,
    start_frame: u32,
    end_frame: u32,
    sampling: String,
    scenes: Vec<&'a str>,
}

#[derive(Serialize)]
struct SceneInput {
    source: String,
    assets: BTreeSet<String>,
}

#[derive(Serialize)]
struct AudioInput {
    source: String,
    start_time_s: f64,
    duration_s: Option<f64>,
    volume: f32,
    volume_envelope: Option<Vec<(f64, f32)>>,
}

impl From<AudioSegment> for AudioInput {
    fn from(segment: AudioSegment) -> Self {
        Self {
            source: segment.source,
            start_time_s: segment.start_time_s,
            duration_s: segment.duration_s,
            volume: segment.volume,
            volume_envelope: segment.volume_envelope,
        }
    }
}

pub fn run_plan(args: PlanArgs, extensions: &CliExtensions) -> Result<(), String> {
    let (target, graph) = load(&args.input, extensions)?;
    let BuildTarget::MultiScene(composition) = &target else {
        return Err("Chapter export requires a multi-scene composition".into());
    };
    let (width, height, fps, settings) = effective_video_settings(
        &target,
        args.width,
        args.height,
        args.fps,
        ExportSettings {
            video_codec: args.codec,
            h264_preset: args.preset,
            ..Default::default()
        },
        args.export_preset.as_deref(),
    )?;
    let duration_s = resolve_duration(args.duration, &target, args.hold, 0.5);
    let ranges = encode::video_chapter_frame_ranges(composition, fps, duration_s)
        .map_err(|err| err.to_string())?;
    let sources = SourceInputs::from_graph(&graph)?;
    let scenes = collect_scene_inputs(composition, &sources, &args.input)?;
    let carry = carry_dependencies(composition);
    let chapters = ranges
        .into_iter()
        .filter(|(_, range)| range.start_frame() < range.end_frame())
        .map(|(name, range)| chapter(composition, fps, name, range, &carry))
        .collect();
    let plan = ChapterPlan {
        version: 1,
        width,
        height,
        fps,
        duration_s,
        codec: settings.video_codec.to_string(),
        preset: settings.h264_preset.as_str(),
        files: sources.files,
        shared_source: format!("{:x}", sources.shared.finalize()),
        scenes,
        chapters,
        audio: scheduled_audio(&target)
            .into_iter()
            .filter(|segment| segment.start_time_s < f64::from(duration_s))
            .map(AudioInput::from)
            .collect(),
    };
    serde_json::to_writer(std::io::stdout().lock(), &plan).map_err(|err| err.to_string())
}

pub fn run_audio(
    input: &Path,
    duration: Option<f64>,
    output: &Path,
    extensions: &CliExtensions,
) -> Result<(), String> {
    let (target, _) = load(input, extensions)?;
    let duration = resolve_duration(duration, &target, 1.0, 0.5);
    encode::render_mixed_audio(&scheduled_audio(&target), f64::from(duration), output, |warning| {
        tracing::warn!("{warning}");
    })
    .map_err(|err| err.to_string())
}

fn load(input: &Path, extensions: &CliExtensions) -> Result<(BuildTarget, ModuleGraph), String> {
    let mut graph = ModuleGraph::new();
    let (target, diagnostics) = try_load_and_build_in(&mut graph, input, extensions, &[])?;
    print_build_diagnostics(&diagnostics);
    if diagnostics.iter().any(|diagnostic| diagnostic.is_error()) {
        return Err("The program contains build errors".into());
    }
    Ok((target, graph))
}

fn scheduled_audio(target: &BuildTarget) -> Vec<AudioSegment> {
    match target {
        BuildTarget::SingleScene(timeline) => timeline.audio_segments().to_vec(),
        BuildTarget::MultiScene(composition) => composition.audio_segments(),
    }
}

struct SourceInputs {
    files: BTreeMap<PathBuf, String>,
    shared: Sha256,
    scenes: BTreeMap<(PathBuf, usize, usize), String>,
}

impl SourceInputs {
    fn from_graph(graph: &ModuleGraph) -> Result<Self, String> {
        let mut result = SourceInputs {
            files: BTreeMap::new(),
            shared: Sha256::new(),
            scenes: BTreeMap::new(),
        };
        for path in graph.loaded_paths().collect::<BTreeSet<_>>() {
            let text = std::fs::read_to_string(path)
                .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
            let statements = graph
                .file_ast(path)
                .ok_or_else(|| format!("Missing AST for {}", path.display()))?;
            let path = path.to_path_buf();
            result.record_module(path, &text, statements)?;
        }
        Ok(result)
    }

    fn record_module(
        &mut self,
        path: PathBuf,
        text: &str,
        statements: &[Stmt],
    ) -> Result<(), String> {
        let lines = LineIndex::new(text);
        let mut sections = Vec::new();
        let mut has_nested_shared_definitions = false;
        for stmt in statements {
            if let Stmt::Scene {
                span: Some(span),
                body,
                ..
            } = stmt
            {
                let offset = lines.line_col_to_byte(span.start_line - 1, span.start_col - 1);
                if text.as_bytes().get(offset) != Some(&b'#') {
                    return Err(format!("Scene source changed while loading {}", path.display()));
                }
                sections.push((span.start_line, span.start_col, offset));
                animatix_syntax::walk::walk_stmts(body, &mut |item| {
                    if matches!(item, Stmt::Import { .. } | Stmt::ComponentDef(..)) {
                        has_nested_shared_definitions = true;
                    }
                });
            }
        }
        // A nested import or component can enter the module-wide definition scope.
        let shared_end = if has_nested_shared_definitions {
            text.len()
        } else {
            sections.first().map_or(text.len(), |(_, _, offset)| *offset)
        };
        hash_field(&mut self.shared, path.as_os_str().as_encoded_bytes());
        hash_field(&mut self.shared, &text.as_bytes()[..shared_end]);
        for (index, (line, column, start)) in sections.iter().copied().enumerate() {
            let end = sections.get(index + 1).map_or(text.len(), |(_, _, offset)| *offset);
            if end < start {
                return Err(format!("Scene source changed while loading {}", path.display()));
            }
            let mut hash = Sha256::new();
            hash_field(&mut hash, path.as_os_str().as_encoded_bytes());
            hash_field(&mut hash, &text.as_bytes()[start..end]);
            self.scenes
                .insert((path.clone(), line, column), format!("{:x}", hash.finalize()));
        }
        self.files.insert(path, format!("{:x}", Sha256::digest(text)));
        Ok(())
    }
}

fn collect_scene_inputs<'a>(
    composition: &'a Composition,
    sources: &SourceInputs,
    entry: &Path,
) -> Result<BTreeMap<&'a str, SceneInput>, String> {
    let entry = entry.canonicalize().map_err(|err| err.to_string())?;
    composition
        .scenes
        .iter()
        .map(|(name, scene)| {
            let path = scene.source_path.as_ref().unwrap_or(&entry);
            let (line, column) =
                scene.source_span.map_or((0, 0), |span| (span.start_line, span.start_col));
            let source = sources
                .scenes
                .get(&(path.clone(), line, column))
                .ok_or_else(|| {
                    format!("Could not locate source for scene '{name}' in {}", path.display())
                })?
                .clone();
            let assets = scene.timeline.asset_usage().map(|(path, _)| path.clone()).collect();
            Ok((name.as_str(), SceneInput { source, assets }))
        })
        .collect()
}

fn carry_dependencies(composition: &Composition) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut dependencies = BTreeMap::new();
    let mut previous = BTreeSet::new();
    for name in composition.playback_order().iter().map(String::as_str) {
        previous.insert(name);
        dependencies.insert(name, previous.clone());
        if !composition.scenes[name].timeline.persistence_flags_iter().any(|(_, flag)| flag) {
            previous.clear();
        }
    }
    dependencies
}

fn chapter<'a>(
    composition: &'a Composition,
    fps: u32,
    name: &'a str,
    range: VideoFrameRange,
    carry: &BTreeMap<&'a str, BTreeSet<&'a str>>,
) -> Chapter<'a> {
    let mut sampling = Sha256::new();
    let mut scenes = BTreeSet::new();
    for frame in range.start_frame()..range.end_frame() {
        let (active, local, blend) =
            composition.evaluate(animatix::time::frame_seconds(frame, f64::from(fps)));
        if let Some(blend) = blend {
            sampling.update([1]);
            let from = composition.scenes[&blend.from_scene].name.as_str();
            let to = composition.scenes[&blend.to_scene].name.as_str();
            sample(&mut sampling, &mut scenes, carry, from, blend.from_local);
            sample(&mut sampling, &mut scenes, carry, to, blend.to_local);
            hash_field(&mut sampling, blend.id.as_bytes());
            hash_field(&mut sampling, format!("{:?}", blend.easing).as_bytes());
            sampling.update((blend.progress as f32).to_le_bytes());
        } else {
            sampling.update([0]);
            let active = composition.scenes[&active].name.as_str();
            sample(&mut sampling, &mut scenes, carry, active, local);
        }
    }
    Chapter {
        name,
        start_frame: range.start_frame(),
        end_frame: range.end_frame(),
        sampling: format!("{:x}", sampling.finalize()),
        scenes: scenes.into_iter().collect(),
    }
}

fn sample<'a>(
    hash: &mut Sha256,
    scenes: &mut BTreeSet<&'a str>,
    carry: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    name: &'a str,
    local: f64,
) {
    hash_field(hash, name.as_bytes());
    // Ignore sub-nanosecond cancellation error when an earlier chapter shifts
    // this scene by a whole frame. Fractional-frame shifts still change the hash.
    hash.update(((local * 1_000_000_000.0).round() as i64).to_le_bytes());
    // Keyframe lookup truncates to whole milliseconds. Preserve its boundary
    // choice even when two local times fall within the same rounded nanosecond.
    hash.update(animatix::time::sample_tick_millis(local).to_le_bytes());
    scenes.insert(name);
    if let Some(dependencies) = carry.get(name) {
        scenes.extend(dependencies);
    }
}

fn hash_field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
