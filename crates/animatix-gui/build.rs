fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Cargo executes build.rs on the host, so cfg!(windows) would be wrong
    // when cross-compiling. Embed the icon into the target's PE resources.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("ProductName", "Animatix")
            .set("FileDescription", "Animatix animation studio")
            .set("OriginalFilename", "animatix-gui.exe")
            .compile()?;
    }
    Ok(())
}
