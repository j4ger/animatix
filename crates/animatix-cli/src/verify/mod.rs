//! `animatix verify`: strict generated-scene contract checks.
//!
//! `auto` owns the zero-config mode built on shared `review` facts. The
//! line-based checker below remains for projects with an existing
//! `verify.txt`; both feed the same CLI, but auto mode has its own JSON report.

pub(crate) mod auto;
mod checks;
pub(crate) mod report;

pub(crate) use checks::{parse_verify_checks, run_verify_checks};

pub(crate) struct VerifyCheck {
    kind: VerifyKind,
    time_s: f64,
    second_time_s: Option<f64>,
    label: Option<String>,
    threshold: Option<f64>,
    pub line: usize,
    pub raw: String,
}

/// The four supported check kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifyKind {
    /// The actor put ink on screen inside its evaluated bounds.
    Visible,
    /// The actor did not (e.g. a hidden-by-default declaration never revealed).
    Invisible,
    /// The whole frame is not blank.
    Ink,
    /// Two times render differently — the animation actually animates.
    Differs,
    /// The actor's region changed between a hidden time and a shown time — the
    /// robust "did it actually appear?" check for actors whose bounds contain
    /// other painted content (e.g. a curve inside a Graph that paints axes).
    Reveals,
}

/// Result of running one check.
pub(crate) struct VerifyOutcome {
    pub line: usize,
    pub raw: String,
    pub passed: bool,
    pub detail: String,
}
