fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let mut perf_log: Option<std::path::PathBuf> = None;
    let mut script: Option<std::path::PathBuf> = None;
    let mut file: Option<std::path::PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--review" => {
                let run = args.next().map(std::path::PathBuf::from);
                if let Some(run) = run {
                    if perf_log.is_some() {
                        tracing::warn!("--perf-log is ignored together with --review");
                    }
                    animatix_gui::run_review(run);
                } else {
                    tracing::error!("--review requires a dogfood run directory");
                    std::process::exit(2);
                }
                return;
            },
            "--perf-log" => match args.next() {
                Some(path) => perf_log = Some(std::path::PathBuf::from(path)),
                None => {
                    tracing::error!("--perf-log requires a .jsonl file path");
                    std::process::exit(2);
                },
            },
            "--demo-script" => match args.next() {
                Some(path) => script = Some(std::path::PathBuf::from(path)),
                None => {
                    tracing::error!("--demo-script requires a file path");
                    std::process::exit(2);
                },
            },
            other => {
                if file.is_none() {
                    file = Some(std::path::PathBuf::from(other));
                } else {
                    tracing::error!("Unexpected argument: {other}");
                    std::process::exit(2);
                }
            },
        }
    }

    animatix_gui::run_gui(file, perf_log, script);
}
