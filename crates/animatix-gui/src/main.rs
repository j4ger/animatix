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
            "-V" | "--version" => {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                return;
            },
            "-h" | "--help" => {
                println!(
                    "{} {}\n\nUsage: animatix-gui [OPTIONS] [FILE]\n\nOptions:\n  --perf-log <PATH>      Drain stage tracer telemetry into a .jsonl file\n  --demo-script <PATH>   Run scripted GUI performance sessions\n  -h, --help             Print help\n  -V, --version          Print version",
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_VERSION")
                );
                return;
            },
            "--perf-log" => match args.next() {
                Some(path) => perf_log = Some(std::path::PathBuf::from(path)),
                None => {
                    eprintln!("error: --perf-log requires a .jsonl file path");
                    std::process::exit(2);
                },
            },
            "--demo-script" => match args.next() {
                Some(path) => script = Some(std::path::PathBuf::from(path)),
                None => {
                    eprintln!("error: --demo-script requires a file path");
                    std::process::exit(2);
                },
            },
            other => {
                if other.starts_with('-') {
                    eprintln!("error: unexpected option '{other}'");
                    std::process::exit(2);
                } else if file.is_none() {
                    file = Some(std::path::PathBuf::from(other));
                } else {
                    eprintln!("error: unexpected argument '{other}'");
                    std::process::exit(2);
                }
            },
        }
    }

    animatix_gui::run_gui(file, perf_log, script);
}
