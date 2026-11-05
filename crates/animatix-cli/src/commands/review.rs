//! `animatix review` command: render-free structural review by default, with
//! an optional GPU pixel tier and optional storyboard emission.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use tracing::error;

use crate::diagnostics::OutputFormat;
use crate::extensions::CliExtensions;
use crate::review::{
    Detail, FailOn, Policy, RenderMode, ReviewOptions, SampleSpec, parse_sample_spec, run_review,
    write_verify_file,
};

/// CLI argument bundle for `animatix review`.
#[derive(ClapArgs, Debug)]
pub(crate) struct ReviewArgs {
    /// Path to the .amx file
    pub(crate) input: PathBuf,
    /// Override a build-time variable: `--set NAME=VALUE` (repeatable)
    #[arg(long = "set", value_parser = crate::build::parse_set_pair)]
    pub(crate) set: Vec<(String, String)>,
    /// Output format (text or json)
    #[arg(long, default_value = "text")]
    pub(crate) format: OutputFormat,
    /// Rule policy profile
    #[arg(long, value_enum, default_value = "default")]
    pub(crate) policy: Policy,
    /// Sample times: auto, keyframes, or a frame count
    #[arg(long, default_value = "auto", value_parser = parse_sample_spec)]
    pub(crate) samples: SampleSpec,
    /// Upper bound on the automatic/regular sample grid
    #[arg(long, default_value_t = 24)]
    pub(crate) max_frames: usize,
    /// Render tier: auto, always, or never
    #[arg(long, value_enum, default_value = "auto")]
    pub(crate) render: RenderMode,
    /// Report detail: summary or full text (JSON is always complete)
    #[arg(long, value_enum, default_value = "summary")]
    pub(crate) detail: Detail,
    /// Optional storyboard PNG to render after the review
    #[arg(long)]
    pub(crate) storyboard: Option<PathBuf>,
    /// Optional directory for rendered review PNG samples
    #[arg(long)]
    pub(crate) frames_dir: Option<PathBuf>,
    /// Exit policy: error, warning, or never
    #[arg(long, value_enum, default_value = "error")]
    pub(crate) fail_on: FailOn,
    /// Optional path for suggested verify.txt lines (never overwritten)
    #[arg(long)]
    pub(crate) emit_verify: Option<PathBuf>,
}

/// Run one review and own its exit code.
pub(crate) fn run(args: ReviewArgs, extensions: &CliExtensions) {
    let ReviewArgs {
        input,
        set,
        format,
        policy,
        samples,
        max_frames,
        render,
        detail,
        storyboard,
        frames_dir,
        fail_on,
        emit_verify,
    } = args;
    let options = ReviewOptions {
        policy,
        samples,
        max_frames,
        render,
        detail,
        fail_on,
        frames_dir,
    };
    let report = match run_review(&input, &set, extensions, &options) {
        Ok(report) => report,
        Err(e) => {
            error!("review could not run: {e}");
            std::process::exit(2);
        },
    };

    if let Some(path) = storyboard {
        let sample_times = report.sample_times_s.clone();
        match crate::commands::storyboard::render_storyboard(
            &input,
            &set,
            None,
            None,
            None,
            Some(sample_times.clone()),
            sample_times.len().max(2),
            false,
            Some(path.clone()),
            extensions,
        ) {
            Ok((output, frames)) => {
                if format == OutputFormat::Text {
                    println!("Storyboard saved to {} ({} frames)", output.display(), frames);
                }
            },
            Err(e) => {
                error!("review could not render storyboard: {e}");
                std::process::exit(2);
            },
        }
    }

    match format {
        OutputFormat::Json => println!("{}", report.to_json(options.detail)),
        OutputFormat::Text => report.print_text(options.detail),
    }

    if let Some(path) = emit_verify
        && let Err(e) = write_verify_file(&path, &report)
    {
        error!("{e}");
        std::process::exit(2);
    }

    let code = report.exit_code(options.fail_on);
    if code != 0 {
        std::process::exit(code);
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::ReviewArgs;

    #[derive(Parser)]
    struct ReviewParser {
        #[command(flatten)]
        args: ReviewArgs,
    }

    #[test]
    fn review_args_parse_all_documented_flags() {
        let parsed = ReviewParser::try_parse_from([
            "review",
            "demo.amx",
            "--format",
            "json",
            "--policy",
            "site",
            "--samples",
            "8",
            "--max-frames",
            "8",
            "--render",
            "never",
            "--detail",
            "full",
            "--storyboard",
            "board.png",
            "--frames-dir",
            "frames",
            "--fail-on",
            "warning",
            "--emit-verify",
            "suggested.verify.txt",
            "--set",
            "answer=42",
        ])
        .expect("review arguments must parse");
        assert_eq!(parsed.args.samples, crate::review::SampleSpec::Count(8));
        assert_eq!(parsed.args.render, crate::review::RenderMode::Never);
        assert_eq!(parsed.args.fail_on, crate::review::FailOn::Warning);
        assert_eq!(parsed.args.max_frames, 8);
        assert_eq!(parsed.args.set, vec![("answer".to_string(), "42".to_string())]);
    }

    #[test]
    fn review_args_reject_unknown_sample_mode() {
        assert!(
            ReviewParser::try_parse_from(["review", "demo.amx", "--samples", "bogus"]).is_err()
        );
    }
}
