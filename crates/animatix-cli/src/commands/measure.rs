//! `animatix measure` command: render-free fact gathering.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use tracing::error;

use crate::diagnostics::OutputFormat;
use crate::extensions::CliExtensions;
use crate::review::{
    Detail, FailOn, Policy, RenderMode, ReviewOptions, SampleSpec, parse_sample_spec, run_review,
};

/// CLI argument bundle for `animatix measure`.
#[derive(ClapArgs, Debug)]
pub(crate) struct MeasureArgs {
    /// Path to the .amx file
    pub(crate) input: PathBuf,
    /// Override a build-time variable: `--set NAME=VALUE` (repeatable)
    #[arg(long = "set", value_parser = crate::build::parse_set_pair)]
    pub(crate) set: Vec<(String, String)>,
    /// Output format (json or text)
    #[arg(long, default_value = "json")]
    pub(crate) format: OutputFormat,
    /// Sample times: auto, keyframes, or a frame count
    #[arg(long, default_value = "auto", value_parser = parse_sample_spec)]
    pub(crate) samples: SampleSpec,
    /// Upper bound on the automatic/regular sample grid
    #[arg(long, default_value_t = 24)]
    pub(crate) max_frames: usize,
    /// Render tier: auto, always, or never
    #[arg(long, value_enum, default_value = "auto")]
    pub(crate) render: RenderMode,
    /// Precise sample times in seconds (comma-separated, overrides --samples)
    #[arg(long, value_delimiter = ',')]
    pub(crate) times: Option<Vec<f64>>,
}

/// Run one measure command and own its exit code.
pub(crate) fn run(args: MeasureArgs, extensions: &CliExtensions) {
    let MeasureArgs {
        input,
        set,
        format,
        samples,
        max_frames,
        render,
        times,
    } = args;
    let options = ReviewOptions {
        policy: Policy::Default,
        samples,
        max_frames,
        render,
        detail: Detail::Full,
        fail_on: FailOn::Never,
        frames_dir: None,
        times,
    };
    let report = match run_review(&input, &set, extensions, &options, "measure", false, None) {
        Ok(report) => report,
        Err(e) => {
            error!("measure could not run: {e}");
            std::process::exit(2);
        },
    };

    match format {
        OutputFormat::Json => println!("{}", report.to_json(options.detail)),
        OutputFormat::Text => println!("{:?}", report.summary),
    }
}
