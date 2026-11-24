use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::capture::Recording;
use super::source::{BoundAnchor, review_anchors};

const TAKES: &str = "narration_takes.py";
const QWEN: &str = "qwen_chapter_provider.py";
const TOOLS: &[(&str, &str)] = &[
    (TAKES, include_str!("../../../../../tools/quality/narration_takes.py")),
    (QWEN, include_str!("../../../../../tools/quality/qwen_chapter_provider.py")),
    (
        "narration_timeline.py",
        include_str!("../../../../../tools/quality/narration_timeline.py"),
    ),
    (
        "speech_chapter.py",
        include_str!("../../../../../tools/quality/speech_chapter.py"),
    ),
    ("speech_sync.py", include_str!("../../../../../tools/quality/speech_sync.py")),
];

#[derive(Serialize)]
pub(super) struct ManifestParagraph {
    pub id: String,
    pub text: String,
}

pub(super) struct Request {
    pub folder: PathBuf,
    pub output_prefix: PathBuf,
    pub paragraphs: Vec<ManifestParagraph>,
    pub base: Option<(PathBuf, PathBuf)>,
    pub anchors: Vec<BoundAnchor>,
    pub python: OsString,
    pub operation: Operation,
}

pub(super) struct LocalAlignment {
    pub model: PathBuf,
    pub tokens: PathBuf,
    pub lexicon: Option<PathBuf>,
}

pub(super) enum Provider {
    Edge,
    Qwen {
        model: PathBuf,
        alignment: LocalAlignment,
    },
    Local {
        executable: OsString,
        arguments: Vec<String>,
        revision: String,
    },
}

pub(super) enum ImportTiming {
    Sidecar(PathBuf),
    Align(LocalAlignment),
}

pub(super) enum RecordingInput {
    File(PathBuf),
    Microphone(Arc<Recording>),
}

impl RecordingInput {
    fn path(&self) -> &Path {
        match self {
            Self::File(path) => path,
            Self::Microphone(recording) => recording.path(),
        }
    }
}

pub(super) enum Operation {
    Inspect,
    Synthesize {
        paragraph: String,
        voice: String,
        rate: String,
        provider: Provider,
        apply_revision: Option<String>,
    },
    Import {
        paragraph: String,
        audio: RecordingInput,
        timing: ImportTiming,
        apply_revision: Option<String>,
    },
    Adopt {
        paragraph: String,
        candidate: PathBuf,
        expected_revision: String,
    },
}

impl Operation {
    pub fn updates_scene(&self) -> bool {
        match self {
            Self::Synthesize { apply_revision, .. } | Self::Import { apply_revision, .. } => {
                apply_revision.is_some()
            },
            Self::Adopt { .. } => true,
            Self::Inspect => false,
        }
    }

    pub fn status(&self) -> &'static str {
        match self {
            Self::Inspect => "Reading scene takes…",
            Self::Synthesize { .. } => "Synthesizing and aligning take…",
            Self::Import { .. } => "Loading and aligning recording…",
            Self::Adopt { .. } => "Assembling scene narration…",
        }
    }
}

#[derive(Deserialize)]
pub(super) struct SceneTakes {
    pub revision: String,
    pub paragraphs: Vec<ParagraphTakes>,
}

#[derive(Deserialize)]
pub(super) struct ParagraphTakes {
    pub id: String,
    pub status: String,
    pub active_take: Option<PathBuf>,
    pub candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
pub(super) struct Candidate {
    pub candidate: PathBuf,
    pub audio: PathBuf,
    pub kind: String,
    pub voice: Option<String>,
    pub rate: Option<String>,
    pub duration: f64,
    pub word_count: usize,
}

#[derive(Deserialize)]
pub(super) struct Adopted {
    pub audio: PathBuf,
    pub transcript: PathBuf,
    pub affected_markers: Vec<MarkerChange>,
}

#[derive(Deserialize)]
pub(super) struct MarkerChange {
    pub marker: String,
    pub before: Option<f64>,
    pub after: Option<f64>,
}

pub(super) enum Output {
    Scene(SceneTakes, Option<PathBuf>),
    Adopted(Adopted),
}

pub(super) struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Output, String>>,
    thread: Option<JoinHandle<()>>,
    pub status: &'static str,
}

impl Job {
    pub fn start(request: Request, ctx: egui::Context) -> Result<Self, String> {
        let status = request.operation.status();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (send, result) = channel();
        let thread = thread::Builder::new()
            .name("narration-take".into())
            .spawn(move || {
                let _ = send.send(run(request, &worker_cancel));
                ctx.request_repaint();
            })
            .map_err(|error| format!("Could not start narration worker: {error}"))?;
        Ok(Self {
            cancel,
            result,
            thread: Some(thread),
            status,
        })
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancelling(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn poll(&self) -> Option<Result<Output, String>> {
        match self.result.try_recv() {
            Ok(_) if self.cancelling() => Some(Err("Narration job cancelled.".into())),
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("The narration worker stopped unexpectedly.".into()))
            },
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(request: Request, cancel: &AtomicBool) -> Result<Output, String> {
    let scratch = tempfile::tempdir()
        .map_err(|error| format!("Could not prepare narration tools: {error}"))?;
    for (name, contents) in TOOLS {
        fs::write(scratch.path().join(name), contents)
            .map_err(|error| format!("Could not prepare {name}: {error}"))?;
    }

    let mut manifest = tempfile::Builder::new()
        .prefix(".animatix-narration-")
        .suffix(".json")
        .tempfile_in(&request.folder)
        .map_err(|error| format!("Could not prepare narration inputs beside the scene: {error}"))?;
    serde_json::to_writer(&mut manifest, &serde_json::json!({ "sentences": request.paragraphs }))
        .map_err(|error| format!("Could not write narration inputs: {error}"))?;
    manifest
        .flush()
        .map_err(|error| format!("Could not write narration inputs: {error}"))?;

    let cli = Cli {
        request: &request,
        scripts: scratch.path(),
        manifest: manifest.path(),
        cancel,
    };
    let mut latest = None;
    match &request.operation {
        Operation::Inspect => {},
        Operation::Synthesize {
            paragraph,
            voice,
            rate,
            provider,
            apply_revision,
        } => {
            let using_edge = matches!(provider, Provider::Edge);
            let mut cmd = cli.command("synthesize", Some(paragraph), using_edge);
            cmd.args(["--voice", voice]).arg(format!("--rate={rate}"));
            provider.add_args(&mut cmd, &request.python, scratch.path());
            latest = Some(cli.run::<Candidate>(cmd)?.candidate);
            if let (Some(candidate), Some(revision)) = (&latest, apply_revision) {
                return cli.adopt(paragraph, candidate, revision);
            }
        },
        Operation::Import {
            paragraph,
            audio,
            timing,
            apply_revision,
        } => {
            let mut cmd = cli.command("import", Some(paragraph), false);
            cmd.arg("--audio").arg(audio.path());
            match timing {
                ImportTiming::Sidecar(path) => {
                    cmd.arg("--timing").arg(path);
                },
                ImportTiming::Align(alignment) => alignment.add_args(&mut cmd, false),
            }
            latest = Some(cli.run::<Candidate>(cmd)?.candidate);
            if let (Some(candidate), Some(revision)) = (&latest, apply_revision) {
                return cli.adopt(paragraph, candidate, revision);
            }
        },
        Operation::Adopt {
            paragraph,
            candidate,
            expected_revision,
        } => return cli.adopt(paragraph, candidate, expected_revision),
    }
    cli.run(cli.command("inspect", None, false))
        .map(|scene| Output::Scene(scene, latest))
}

struct Cli<'a> {
    request: &'a Request,
    scripts: &'a Path,
    manifest: &'a Path,
    cancel: &'a AtomicBool,
}

impl Cli<'_> {
    fn adopt(&self, paragraph: &str, candidate: &Path, revision: &str) -> Result<Output, String> {
        let mut cmd = self.command("adopt", Some(paragraph), false);
        cmd.arg("--candidate").arg(candidate).arg("--expected-revision").arg(revision);
        let mut adopted: Adopted = self.run(cmd)?;
        let before = self.request.base.as_ref().map(|(_, transcript)| transcript.as_path());
        let reviewed = review_anchors(&self.request.anchors, before, &adopted.transcript);
        adopted.affected_markers.retain(|change| {
            !reviewed.iter().any(|reviewed| {
                reviewed
                    .marker
                    .rsplit_once('@')
                    .is_some_and(|(_, marker)| marker == change.marker)
            })
        });
        adopted.affected_markers.extend(reviewed);
        Ok(Output::Adopted(adopted))
    }

    fn command(&self, action: &str, paragraph: Option<&str>, edge: bool) -> Command {
        let mut cmd = if edge {
            let mut uv = Command::new("uv");
            uv.args([
                "run",
                "--no-project",
                "--with",
                "edge-tts==7.3.1",
                "--python",
            ]);
            uv.arg(&self.request.python).arg("python");
            uv
        } else {
            Command::new(&self.request.python)
        };
        cmd.arg(self.scripts.join(TAKES)).arg(action).arg(self.manifest);
        if let Some(paragraph) = paragraph {
            cmd.arg(paragraph);
        }
        cmd.arg("-o").arg(&self.request.output_prefix);
        if let Some((audio, transcript)) = &self.request.base {
            cmd.arg("--base-audio").arg(audio).arg("--base-transcript").arg(transcript);
        } else {
            cmd.arg("--empty-base");
        }
        cmd.current_dir(&self.request.folder)
            .stdin(Stdio::null())
            .env("PYTHONUTF8", "1");
        if let Some(path) = tool_search_path() {
            cmd.env("PATH", path);
        }
        cmd
    }

    fn run<T: for<'de> Deserialize<'de>>(&self, mut cmd: Command) -> Result<T, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("Narration job cancelled.".into());
        }

        let mut out = tempfile::tempfile().map_err(|error| error.to_string())?;
        let mut err = tempfile::tempfile().map_err(|error| error.to_string())?;
        cmd.stdout(out.try_clone().map_err(|error| error.to_string())?)
            .stderr(err.try_clone().map_err(|error| error.to_string())?);
        configure_process(&mut cmd);
        let mut child = cmd.spawn().map_err(|error| {
            format!("Could not start {}: {error}", cmd.get_program().to_string_lossy())
        })?;
        let status = loop {
            if self.cancel.load(Ordering::Relaxed) {
                terminate(&mut child);
                return Err("Narration job cancelled.".into());
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(40)),
                Err(error) => {
                    terminate(&mut child);
                    return Err(format!("Could not wait for narration tools: {error}"));
                },
            }
        };

        let output = read_tail(&mut out, 32 * 1024 * 1024)?;
        if !status.success() {
            let failure = read_tail(&mut err, 4000)?;
            let details = if failure.trim().is_empty() {
                output.trim()
            } else {
                failure.trim()
            };
            let shortened = details.chars().take(4000).collect::<String>();
            return Err(format!("Narration tools exited with {status}. {shortened}"));
        }
        let json = output.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or_default();
        serde_json::from_str(json)
            .map_err(|error| format!("Invalid narration tool response: {error}"))
    }
}

impl Provider {
    fn add_args(&self, cmd: &mut Command, python: &OsStr, scripts: &Path) {
        match self {
            Self::Edge => {},
            Self::Qwen { model, alignment } => {
                cmd.arg("--provider-command").arg(python);
                provider_arg(cmd, scripts.join(QWEN));
                provider_arg(cmd, "--tts-model");
                provider_arg(cmd, model);
                alignment.add_args(cmd, true);
                cmd.args(["--provider-revision", "qwen"]);
            },
            Self::Local {
                executable,
                arguments,
                revision,
            } => {
                cmd.arg("--provider-command").arg(executable);
                for argument in arguments {
                    provider_arg(cmd, argument);
                }
                if !revision.is_empty() {
                    cmd.arg("--provider-revision").arg(revision);
                }
            },
        }
    }
}

impl LocalAlignment {
    fn add_args(&self, cmd: &mut Command, provider: bool) {
        let mut push = |option: &str, value: &Path| {
            if provider {
                provider_arg(cmd, option);
                provider_arg(cmd, value);
            } else {
                cmd.arg(option).arg(value);
            }
        };
        push("--asr-model", &self.model);
        push("--asr-tokens", &self.tokens);
        if let Some(lexicon) = &self.lexicon {
            push("--lexicon", lexicon);
        }
    }
}

fn provider_arg(cmd: &mut Command, value: impl AsRef<OsStr>) {
    let mut prefixed = OsString::from("--provider-arg=");
    prefixed.push(value);
    cmd.arg(prefixed);
}

fn read_tail(file: &mut File, limit: u64) -> Result<String, String> {
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(limit)))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn configure_process(_cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        _cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
}

fn tool_search_path() -> Option<OsString> {
    let ffmpeg = crate::app::audio::packaged_ffmpeg()?;
    let packaged_dir = ffmpeg.parent()?;
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(
        std::iter::once(packaged_dir.to_path_buf()).chain(std::env::split_paths(&path)),
    )
    .ok()
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        if let Ok(group) = i32::try_from(child.id()) {
            // A negative PID signals the process group we created above, including any provider.
            unsafe { kill(-group, 9) };
        }
    }
    #[cfg(windows)]
    {
        let mut taskkill = Command::new("taskkill");
        configure_process(&mut taskkill);
        let _ = taskkill.args(["/T", "/F", "/PID", &child.id().to_string()]).output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod smoke;
