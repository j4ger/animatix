fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let mut args = std::env::args().skip(1);
    if let Some(first) = args.next() {
        if first == "--review" {
            let run = args.next().map(std::path::PathBuf::from);
            if let Some(run) = run {
                animatix_gui::run_review(run);
            } else {
                tracing::error!("--review requires a dogfood run directory");
                std::process::exit(2);
            }
        } else {
            animatix_gui::run_gui(Some(std::path::PathBuf::from(first)));
        }
    } else {
        animatix_gui::run_gui(None);
    }
}
