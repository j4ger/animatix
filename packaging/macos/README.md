# macOS application bundle

`scripts/dist-package.sh macos BIN_DIR` retains the existing CLI/GUI executables
and also creates `Animatix.app` with a real `Contents/MacOS/animatix-gui`,
`Info.plist`, and `Contents/Resources/Animatix.icns`. The application can be moved
independently of the command-line binaries. Its identifier is
`io.github.j4ger.animatix`; the version comes from the workspace manifest.

The ICNS is generated from the same canonical SVG as the other platforms (see
[branding](../../docs/branding.md)). Packaging validates the bundle metadata and
icon. This does not add signing, notarization, an installer, automatic file
associations, or a universal binary: the archive still reports and contains the
architecture actually built by the release runner. Finder/Dock appearance must
be checked on macOS; structural fixture tests on Linux are not that test.

On Windows, `crates/animatix-gui/build.rs` uses `winresource` to embed the ICO in
PE resources at build time, including when cross-compiling. MSVC builds need the
Windows SDK resource compiler; GNU builds need MinGW windres. Packaging checks
that the resulting executable contains both image and group-icon resources and
that every group entry resolves to an embedded image. Explorer and runtime
window icons are distinct: the GUI also sets its viewport PNG on startup.
