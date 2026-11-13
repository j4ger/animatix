# Animatix application identity

`logo.svg` is the canonical approved vector mark: three flat shapes inside a
keyframe boundary on a black rounded tile. The area outside the tile remains
transparent. Regenerate all consumers with `python3 scripts/update-icons.py`
(requires Inkscape). Commit generated assets with the source SVG; normal Rust
builds do not require graphics tools.

| Consumer | Asset/integration |
| --- | --- |
| README | Root `logo.svg` |
| Website header and favicons | Identical `web/logo.svg`, included in site artifacts |
| Native window | Embedded 256 px RGBA PNG, set through eframe |
| Linux application menu / desktop shell | `io.github.j4ger.animatix.desktop`, hicolor SVG + 256/512 px PNG, matching window app ID |
| Windows Explorer / executable | Six-resolution ICO (16–256 px), compiled into the GUI PE resources by `build.rs` |
| macOS Finder / Dock | `Animatix.app`, `Info.plist`, and Retina-capable `Animatix.icns` (16–1024 px) |

All platforms use the stable application identity `io.github.j4ger.animatix`.
Existing toolbar/action glyphs and shapes inside demo scenes are not brand assets.

## Linux

The release package carries `install-desktop.py`. Run `python3 install-desktop.py`
from the unpacked package to register the current user's launcher and icons.
See [Linux packaging](../packaging/linux/README.md) for lifecycle details. Do not
confuse a Nix development build with the portable Ubuntu-baseline release bundle.
Launching a bare executable need not produce the installed desktop's Dock icon;
the desktop entry, app ID, and icon theme registration are a separate integration.

## Windows

The standard GUI build embeds its application icon when the target OS is Windows.
This uses the platform resource compiler through `winresource`; native MSVC builds
need their Windows SDK and GNU cross-builds need a compatible windres toolchain.
Build failure is not silently ignored. Runtime window icons are still set through
eframe, while Explorer reads the executable resource. Windows may cache Explorer
icons; validate freshly built artifacts rather than judging an older cached file.

## macOS

The existing macOS archive now also contains `Animatix.app`, with a real executable
inside `Contents/MacOS` and icons in `Contents/Resources`. Move that bundle into
Applications and launch it through Finder for normal Dock identity. Existing raw
CLI/GUI files remain for compatibility. Icon packaging does not add Developer ID
signing or notarization; no Gatekeeper settings should be weakened to test it.

## Checks

`scripts/ci.sh gate native-package-tests` covers the existing package validation,
icon directory/payload structure, macOS bundle fixtures, Windows resource wiring,
Linux desktop identity, and user-local installer behavior. `desktop-file-validate`
is used when available. Run the normal GUI compile/build gates as well.

Static validation of ICO/ICNS/PE or bundle metadata is not equivalent to checking
Explorer, Finder, Dock, X11, and Wayland on each operating system. Record the OS,
desktop, build type, and whether a real launcher/window was observed in release QA.
