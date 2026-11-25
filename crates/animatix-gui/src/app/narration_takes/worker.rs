use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::app::i18n::StatusText;

use super::assets::{self, Pair};
use super::bindings::Binding;
use super::history::{History, Mix, Take};
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

#[derive(Clone, PartialEq, Eq, Serialize)]
pub(super) struct ManifestParagraph {
    pub id: String,
    pub text: String,
}

pub(super) struct Request {
    pub folder: PathBuf,
    pub name: String,
    pub paragraphs: Vec<ManifestParagraph>,
    pub bindings: Vec<Binding>,
    pub base: Option<Pair>,
    pub python: OsString,
    pub operation: Operation,
    pub take: Take,
    pub selector: String,
    pub history: Arc<History>,
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
    Missing,
}

pub(super) enum Operation {
    Synthesize {
        voice: String,
        rate: String,
        provider: Provider,
    },
    Import {
        audio: Option<PathBuf>,
        timing: ImportTiming,
    },
    Use,
}

impl Operation {
    fn status(&self) -> &'static str {
        match self {
            Self::Synthesize { .. } => "gui-narration-generating",
            Self::Import { .. } => "gui-narration-aligning",
            Self::Use => "gui-narration-updating",
        }
    }
}

#[derive(Deserialize)]
struct Candidate {
    candidate: PathBuf,
}

#[derive(Deserialize)]
pub(super) struct Adopted {
    pub audio: PathBuf,
    pub transcript: PathBuf,
}

pub(super) struct Finished {
    pub adopted: Adopted,
    pub base_hashes: Option<(String, String)>,
}

pub(super) struct Job {
    cancel: Arc<AtomicBool>,
    result: Receiver<Result<Finished, StatusText>>,
    thread: Option<JoinHandle<()>>,
    pub status: &'static str,
}

impl Job {
    pub fn start(request: Request, ctx: egui::Context) -> Result<Self, StatusText> {
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
            .map_err(|error| {
                StatusText::message("gui-narration-worker-start", [("detail", error.to_string())])
            })?;
        Ok(Self {
            cancel,
            result,
            thread: Some(thread),
            status,
        })
    }

    fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn cancelling(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn poll(&self) -> Option<Result<Finished, StatusText>> {
        match self.result.try_recv() {
            Ok(_) if self.cancelling() => {
                Some(Err(StatusText::message("gui-narration-job-cancelled", [])))
            },
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err(StatusText::message("gui-narration-worker-stopped", [])))
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

fn run(request: Request, cancel: &AtomicBool) -> Result<Finished, StatusText> {
    let outcome = run_update(&request, cancel);
    if let Err(error) = &outcome {
        tracing::debug!(
            error = error.canonical(),
            take = request.take.id,
            "Voiceover update did not finish"
        );
        if let Err(error) = request.history.issue(&request.take.id, Some(error.canonical())) {
            tracing::warn!(%error, "Could not save voiceover issue");
        }
    }
    outcome
}

fn run_update(request: &Request, cancel: &AtomicBool) -> Result<Finished, StatusText> {
    let scratch = tempfile::tempdir().map_err(narration_detail)?;
    for (name, contents) in TOOLS {
        fs::write(scratch.path().join(name), contents).map_err(narration_detail)?;
    }
    let manifest = scratch.path().join("script.json");
    fs::write(
        &manifest,
        serde_json::to_vec(&serde_json::json!({ "sentences": request.paragraphs }))
            .map_err(narration_detail)?,
    )
    .map_err(narration_detail)?;
    let prefix = scratch.path().join("chapter");
    let cache = scratch.path().join("chapter.takes");

    // Even when an authored pair is missing or changes, keep the take.
    let original = request.base.as_ref().map(assets::fingerprint).transpose();
    let cli = Cli {
        request,
        scripts: scratch.path(),
        manifest: &manifest,
        prefix: &prefix,
        cancel,
    };
    prepare(&cli, scratch.path())?;

    let base_hashes = original.map_err(|error| {
        StatusText::message("gui-narration-read-current", [("detail", error.to_string())])
    })?;
    let base = stage_baseline(request, &cache, base_hashes.as_ref())?;
    let candidate = cache.join("candidates").join(&request.take.id);
    request
        .history
        .candidate(&request.take.id, &request.selector, &candidate)
        .map_err(narration_detail)?;
    let bindings_path = scratch.path().join("bindings.json");
    let selectors: HashMap<_, _> = request
        .bindings
        .iter()
        .map(|binding| (&binding.id, &binding.selector))
        .collect();
    let takes = request.history.takes().map_err(narration_detail)?;
    let bindings: HashMap<_, _> = takes
        .iter()
        .filter_map(|take| selectors.get(&take.paragraph).map(|selector| (&take.id, *selector)))
        .collect();
    fs::write(&bindings_path, serde_json::to_vec(&bindings).map_err(narration_detail)?)
        .map_err(narration_detail)?;
    let mut cmd = cli.command("adopt", false);
    cmd.arg("--candidate")
        .arg(candidate.join("take.json"))
        .arg("--workbench-bindings")
        .arg(bindings_path);
    if let Some((audio, transcript)) = &base {
        cmd.arg("--base-audio").arg(audio).arg("--base-transcript").arg(transcript);
    } else {
        cmd.arg("--empty-base");
    }
    let output: Adopted = cli.run(cmd)?;
    if request
        .base
        .as_ref()
        .map(assets::fingerprint)
        .transpose()
        .map_err(narration_detail)?
        != base_hashes
    {
        return Err(StatusText::message("gui-narration-worker-stale", []));
    }
    let layout = fs::read(output.audio.with_file_name("state.json")).map_err(narration_detail)?;
    let (pair, (audio_digest, transcript_digest)) =
        assets::publish(&request.folder, &request.name, &(output.audio, output.transcript))
            .map_err(narration_detail)?;
    let relative = |path: &Path| {
        path.strip_prefix(&request.folder)
            .map(Path::to_path_buf)
            .map_err(narration_detail)
    };
    let mut mix = Mix {
        audio_digest,
        transcript_digest,
        layout,
        parts: HashMap::new(),
    };
    let state: serde_json::Value = serde_json::from_slice(&mix.layout).map_err(narration_detail)?;
    if let Some(takes) = state["chapter"]["takes"].as_object() {
        mix.parts.extend(takes.iter().filter_map(|(selector, take)| {
            take["candidate"].as_str().map(|id| (selector.clone(), id.to_owned()))
        }));
    }
    request
        .history
        .store_mix(&relative(&pair.0)?, &relative(&pair.1)?, &mix)
        .map_err(narration_detail)?;
    request.history.issue(&request.take.id, None).map_err(narration_detail)?;
    Ok(Finished {
        adopted: Adopted {
            audio: pair.0,
            transcript: pair.1,
        },
        base_hashes,
    })
}

fn prepare(cli: &Cli<'_>, scratch: &Path) -> Result<(), StatusText> {
    let request = cli.request;
    let mut cmd = match &request.operation {
        Operation::Use => return Ok(()),
        Operation::Synthesize {
            voice,
            rate,
            provider,
        } => {
            let mut cmd = cli.command("synthesize", matches!(provider, Provider::Edge));
            cmd.args(["--voice", voice]).arg(format!("--rate={rate}"));
            provider.add_args(&mut cmd, &request.python, scratch);
            cmd
        },
        Operation::Import { audio, timing } => {
            let raw = scratch.join("original.wav");
            if let Some(audio) = audio {
                cli.decode(audio, &raw)?;
                request.history.import_wave(&request.take, &raw).map_err(narration_detail)?;
            } else {
                request.history.raw_wave(&request.take.id, &raw).map_err(narration_detail)?;
            }
            let mut cmd = cli.command("import", false);
            cmd.arg("--audio").arg(&raw);
            match timing {
                ImportTiming::Sidecar(path) => {
                    cmd.arg("--timing").arg(path);
                },
                ImportTiming::Align(alignment) => alignment.add_args(&mut cmd, false),
                ImportTiming::Missing => {
                    return Err(StatusText::message("gui-narration-alignment-required", []));
                },
            }
            cmd
        },
    };
    cmd.arg("--empty-base");
    let output: Candidate = cli.run(cmd)?;
    if matches!(&request.operation, Operation::Synthesize { .. }) {
        request
            .history
            .import_wave(&request.take, &output.candidate.with_file_name("take.wav"))
            .map_err(narration_detail)?;
    }
    request
        .history
        .store_alignment(&request.take.id, &output.candidate)
        .map_err(narration_detail)
}

fn stage_baseline(
    request: &Request,
    cache: &Path,
    hashes: Option<&(String, String)>,
) -> Result<Option<Pair>, StatusText> {
    let Some((original, hashes)) = request.base.as_ref().zip(hashes) else {
        return Ok(None);
    };
    let directory = cache.join("revisions").join("00000000000000000000000000000000");
    let local = if let (Ok(audio), Ok(transcript)) = (
        original.0.strip_prefix(&request.folder),
        original.1.strip_prefix(&request.folder),
    ) {
        request.history.mix(audio, transcript).map_err(narration_detail)?
    } else {
        None
    };
    if let Some(mix) =
        local.filter(|mix| mix.audio_digest == hashes.0 && mix.transcript_digest == hashes.1)
    {
        fs::create_dir_all(&directory).map_err(narration_detail)?;
        let audio = directory.join("chapter.wav");
        let transcript = directory.join("chapter.speech.json");
        fs::copy(&original.0, &audio).map_err(narration_detail)?;
        fs::copy(&original.1, &transcript).map_err(narration_detail)?;
        fs::write(directory.join("state.json"), mix.layout).map_err(narration_detail)?;
        Ok(Some((audio, transcript)))
    } else {
        Ok(Some(original.clone()))
    }
}

struct Cli<'a> {
    request: &'a Request,
    scripts: &'a Path,
    manifest: &'a Path,
    prefix: &'a Path,
    cancel: &'a AtomicBool,
}

impl Cli<'_> {
    fn command(&self, action: &str, edge: bool) -> Command {
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
        cmd.arg(self.scripts.join(TAKES))
            .arg(action)
            .arg(self.manifest)
            .arg(&self.request.selector)
            .arg("-o")
            .arg(self.prefix)
            .current_dir(&self.request.folder)
            .stdin(Stdio::null())
            .env("PYTHONUTF8", "1");
        if let Some(path) = tool_search_path() {
            cmd.env("PATH", path);
        }
        cmd
    }

    fn decode(&self, audio: &Path, raw: &Path) -> Result<(), StatusText> {
        if hound::WavReader::open(audio).is_ok_and(|wave| {
            wave.spec().bits_per_sample == 16
                && wave.spec().sample_format == hound::SampleFormat::Int
        }) {
            fs::copy(audio, raw).map_err(narration_detail)?;
            return Ok(());
        }
        let ffmpeg = crate::app::audio::packaged_ffmpeg().unwrap_or_else(|| "ffmpeg".into());
        let mut cmd = Command::new(ffmpeg);
        cmd.args(["-nostdin", "-v", "error", "-xerror", "-i"])
            .arg(audio)
            .args(["-map", "0:a:0", "-c:a", "pcm_s16le"])
            .arg(raw);
        self.run_process(cmd)?;
        Ok(())
    }

    fn run<T: for<'de> Deserialize<'de>>(&self, cmd: Command) -> Result<T, StatusText> {
        let output = self.run_process(cmd)?;
        let json = output.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or_default();
        serde_json::from_str(json).map_err(|error| {
            StatusText::message("gui-narration-tool-response", [("detail", error.to_string())])
        })
    }

    fn run_process(&self, mut cmd: Command) -> Result<String, StatusText> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(StatusText::message("gui-narration-job-cancelled", []));
        }

        let mut out = tempfile::tempfile().map_err(narration_detail)?;
        let mut err = tempfile::tempfile().map_err(narration_detail)?;
        cmd.stdout(out.try_clone().map_err(narration_detail)?)
            .stderr(err.try_clone().map_err(narration_detail)?);
        configure_process(&mut cmd);
        let mut child = cmd.spawn().map_err(|error| {
            StatusText::message(
                "gui-narration-tool-start",
                [
                    ("program", cmd.get_program().to_string_lossy().to_string()),
                    ("detail", error.to_string()),
                ],
            )
        })?;
        let status = loop {
            if self.cancel.load(Ordering::Relaxed) {
                terminate(&mut child);
                return Err(StatusText::message("gui-narration-job-cancelled", []));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(40)),
                Err(error) => {
                    terminate(&mut child);
                    return Err(StatusText::message(
                        "gui-narration-tool-wait",
                        [("detail", error.to_string())],
                    ));
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
            return Err(StatusText::message(
                "gui-narration-tool-exit",
                [("status", status.to_string()), ("detail", shortened)],
            ));
        }
        Ok(output)
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

fn narration_detail(error: impl std::fmt::Display) -> StatusText {
    StatusText::message("gui-narration-worker-detail", [("detail", error.to_string())])
}

fn read_tail(file: &mut File, limit: u64) -> Result<String, StatusText> {
    let length = file.metadata().map_err(narration_detail)?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(limit)))
        .map_err(narration_detail)?;
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes).map_err(narration_detail)?;
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
