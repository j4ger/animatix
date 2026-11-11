//! Review command value types shared by the structural, render, and rule tiers.

use std::path::PathBuf;

use clap::ValueEnum;

/// Policy profile selected by `--policy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Policy {
    /// Built-in tolerant defaults.
    Default,
    /// Website embedding policy (`scripts/site-rest-check.py` semantics).
    Site,
    /// LLM-generated scenes: layout defects are hard errors.
    Generated,
}

/// Sample-time selection mode from `--samples`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum SampleSpec {
    /// Storyboard-style automatic beat selection.
    #[default]
    Auto,
    /// Every keyframe time plus the start and rest frame.
    Keyframes,
    /// `N` evenly spaced times including start and rest.
    Count(usize),
}

/// Parse `auto`, `keyframes`, or a positive integer count.
pub(crate) fn parse_sample_spec(raw: &str) -> Result<SampleSpec, String> {
    match raw.trim() {
        "auto" => Ok(SampleSpec::Auto),
        "keyframes" => Ok(SampleSpec::Keyframes),
        other => {
            let count = other.parse::<usize>().map_err(|_| {
                format!("expected auto, keyframes, or a frame count, got '{other}'")
            })?;
            if count < 2 {
                return Err("sample count must be at least 2".to_string());
            }
            Ok(SampleSpec::Count(count))
        },
    }
}

/// Render-tier behavior from `--render`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum RenderMode {
    /// Render when a GPU adapter is available; otherwise skip structurally.
    Auto,
    /// Require rendering; failure to initialize is a cannot-run error.
    Always,
    /// Never render.
    Never,
}

/// Report verbosity from `--detail`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Detail {
    /// Counts and issues only.
    Summary,
    /// Counts, issues, and per-sample frame facts.
    Full,
}

/// Exit policy from `--fail-on`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum FailOn {
    /// Exit 1 only when an issue has `error` severity.
    Error,
    /// Exit 1 when any `error` or `warning` issue exists.
    Warning,
    /// Never fail because of issues (cannot-run still exits 2).
    Never,
}

/// Issue severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Persistence {
    #[default]
    Transient,
    Held,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FixHint {
    pub kind: &'static str,
    pub target: Option<String>,
    pub source_line: Option<usize>,
    pub snippet: Option<String>,
    pub marker: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) enum Severity {
    #[default]
    /// Informational observation.
    Info,
    /// Likely defect; non-fatal by default.
    Warning,
    /// Hard failure.
    Error,
}

impl Severity {
    /// Stable JSON spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// How much the rule trusts its own result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Confidence {
    #[default]
    Low,
    /// Direct measurement.
    High,
    /// Geometry heuristic.
    Medium,
}

impl Confidence {
    /// Stable JSON spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::Medium => "medium",
        }
    }
}

/// Which build shape produced the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetKind {
    /// One `Timeline`.
    SingleScene,
    /// A `Composition`.
    MultiScene,
}

impl TargetKind {
    /// Stable JSON spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SingleScene => "single_scene",
            Self::MultiScene => "multi_scene",
        }
    }
}

/// Render-tier status reported in `render.status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderStatus {
    /// Every requested sample was rendered.
    Rendered,
    /// Rendering was not attempted (--render never or no adapter).
    Skipped,
    /// An adapter existed but a frame failed; structural facts remain valid.
    Failed,
}

impl RenderStatus {
    /// Stable JSON spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Rendered => "rendered",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// One rule finding.
#[derive(Clone, Debug, Default)]
pub(crate) struct RuleIssue {
    /// Stable rule identifier, e.g. `actor.clipped`.
    pub rule_id: &'static str,
    /// Severity used by `--fail-on`.
    pub severity: Severity,
    /// Rule confidence.
    pub confidence: Confidence,
    pub persistence: Persistence,
    /// Scene owning the finding, when known.
    pub scene: Option<String>,
    /// Sample time the finding refers to, if any.
    pub time_s: Option<f64>,
    /// Actor label the finding refers to, if any.
    pub actor: Option<String>,
    /// 1-based declaration line when known.
    pub source_line: Option<usize>,
    /// Human-readable description.
    pub message: String,
    /// Structured measurements backing the finding.
    pub evidence: serde_json::Value,
    pub fix_hint: Option<FixHint>,
    pub waived_by: Option<String>,
    /// Suggested edit direction.
    pub suggestion: String,
}

/// Render measurements for one sample.
#[derive(Clone, Debug)]
pub(crate) struct RenderSampleFacts {
    /// Sample time in seconds.
    pub time_s: f64,
    /// Whole-frame ink fraction (`0..1`).
    pub ink_fraction: f32,
    /// Fraction of pixels changed from the previous rendered sample.
    pub changed_from_prev: Option<f32>,
    /// Per-actor ink fraction inside the actor's bounds, keyed by
    /// `(scene, label)` so multi-scene compositions cannot collide.
    pub actor_ink: std::collections::BTreeMap<(String, String), f32>,
    /// Active transition, when this sample landed inside a blend.
    pub transition: Option<crate::frame_source::TransitionFact>,
    pub occlusions: Vec<crate::review::probe::OcclusionFact>,
}

/// Site-policy resting-frame measurement, mirroring
/// `scripts/site-rest-check.py`.
#[derive(Clone, Debug)]
pub(crate) struct SiteRestFacts {
    /// Time of the held/rest frame.
    pub held_time_s: f64,
    /// Ink fraction of the held frame.
    pub held_ink_fraction: f32,
    /// Peak ink fraction over the measured plateau.
    pub peak_ink_fraction: f32,
    /// Time of the peak.
    pub peak_time_s: f64,
}

/// Aggregate render-tier result.
#[derive(Clone, Debug)]
pub(crate) struct RenderFacts {
    /// Status reported in JSON.
    pub status: RenderStatus,
    /// Why rendering was skipped/failed, or a short success note.
    pub reason: String,
    /// Per-sample measurements, ordered by time.
    pub samples: Vec<RenderSampleFacts>,
    /// Site-policy resting-frame facts, when measured.
    pub site_rest: Option<SiteRestFacts>,
}

/// Everything the command layer needs to know about a review run.
#[derive(Clone, Debug)]
pub(crate) struct ReviewOptions {
    /// Rule policy.
    pub policy: Policy,
    /// Sample selection mode.
    pub samples: SampleSpec,
    /// Upper bound on the automatic/regular sample grid.
    pub max_frames: usize,
    /// Render tier selection.
    pub render: RenderMode,
    /// Report verbosity.
    pub detail: Detail,
    /// Exit policy.
    pub fail_on: FailOn,
    /// Optional directory for rendered PNG samples.
    pub frames_dir: Option<PathBuf>,
    pub times: Option<Vec<f64>>,
}
