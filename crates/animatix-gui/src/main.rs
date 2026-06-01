fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let path = std::env::args().nth(1).map(std::path::PathBuf::from);
    animatix_gui::run_gui(path);
}
