fn main() {
    let path = std::env::args().nth(1).map(std::path::PathBuf::from);
    animatix_gui::run_gui(path);
}
