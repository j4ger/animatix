# Linux video-enabled distribution

The Linux release is an x86_64 GNU/Linux bundle built in the digest-pinned
Ubuntu 22.04 container. Ubuntu 22.04 is the runtime baseline; Ubuntu 24.04 is
also exercised by `scripts/test-dist-linux.sh`.

The base container digest, Rust toolchain, and FFmpeg/codec source archives are
fixed. The image installs generic compiler/build tools from Ubuntu's signed apt
repositories. Those package indexes move over time, so the tool layer is not
claimed to be byte-for-byte reproducible; each package records its actual image
ID, compiler/binutils versions, and complete installed apt package manifest.

Build the relocatable package tree with:

```bash
scripts/dist-linux.sh [target/or-dist/output-directory]
# default: target/dist-linux-package
```

The resulting tree has `bin/animatix`, `bin/animatix-gui`, `bin/ffmpeg`, and
`bin/ffprobe`.
The two launchers set `PATH` so Animatix's audio mux step uses the bundled
FFmpeg CLI. Shared FFmpeg libraries and the remaining non-system ELF runtime
dependencies live in `lib/`; executable ELF files use `$ORIGIN` RUNPATH entries.
The package contains FFmpeg 8.0.3, x264, libvpx 1.16.0, and Opus source archives,
their license files, the source hash lock, and build metadata.
The package also includes Animatix's MIT license. Build metadata records the
Git commit, whether the checkout was dirty, and a SHA-256 identity over the
allowlisted source files used for the build, including dirty tracked changes
and non-ignored untracked project files. The Linux distribution build snapshots
those files first, builds from that copy, and packages the same copy as a
separate corresponding-source archive beside the binary archive. It contains
the exact source snapshot and identity, `Cargo.lock`, an offline Cargo source
configuration and vendored Rust dependencies, the build recipe, and the pinned
FFmpeg/x264/libvpx/Opus source archives. It omits `.git`, build output, and
files outside the source allowlist. The archive's `SHA256SUMS` and
`SOURCE-IDENTITY.json` are checked by the packaging script.

The source archive includes `REBUILD.md` with the Linux rebuild command. Cargo
dependencies and codec sources are supplied locally for an offline Cargo/media
build. Docker's pinned Ubuntu image and its signed apt package inputs still need
to be available; their resolved versions are recorded in the binary package.
This build information describes the artifact contents and recipe; it is not a
legal compliance certification.

The build uses `FFMPEG_INCLUDE_DIR`, `FFMPEG_LIBS_DIR`, and
`FFMPEG_LINK_MODE=dynamic` for `rusty_ffmpeg`. It uses a dedicated Cargo target
directory and clears it before building, so generated bindings come from the
locked FFmpeg headers and cannot be reused from Nix or a host FFmpeg install.
It does not set the general `PKG_CONFIG_PATH` variable. The static codec
libraries are configured with the logical `/usr` prefix and installed below
the private staging root with `DESTDIR`. For the FFmpeg configure step, the
build scopes `PKG_CONFIG_LIBDIR` to the staged `.pc` files and uses
`C_INCLUDE_PATH` and `LIBRARY_PATH` to locate staged headers and libraries; it
does not set `PKG_CONFIG_SYSROOT_DIR`. The logical prefix keeps codec and FFmpeg
build metadata from recording the checkout's temporary build paths.

## Runtime requirements

- x86_64 Ubuntu 22.04 or 24.04, or a compatible distribution with glibc at least
  as new as Ubuntu 22.04. The package intentionally uses the host's glibc and
  dynamic loader.
- A working Vulkan/graphics stack for rendering. Vulkan loader, Mesa/NVIDIA/AMD
  ICDs, vendor driver libraries, and the kernel driver are host-provided and are
  deliberately excluded from the package. A headless software render needs a
  host-installed Mesa lavapipe driver.
- GUI audio playback uses the bundled ALSA core library and the host's ALSA
  runtime setup. The host must provide `/usr/share/alsa/alsa.conf`, any ALSA
  PCM plugins needed by its output setup (for example, its PulseAudio or
  PipeWire bridge), and an accessible audio device/session. The package does
  not include host ALSA configuration, plugins, or sound devices. Without
  those host facilities the GUI still opens, but audio playback is unavailable.
- The video-enabled CLI bundles FFmpeg and its non-system runtime closure. The
  GUI and CLI are built with video enabled on Linux. macOS and Windows release
  builds remain no-video.

The package checker verifies the bundled media libraries, relative RUNPATHs,
absence of `/nix/store` references, and absence of GPU driver/loader files.
Run the clean-host compatibility and media export check with:

```bash
scripts/test-dist-linux.sh target/dist-linux-package
```

The `dist-linux` CI gate also creates the release `.tar.gz`, extracts it into a
fresh directory, and runs the same clean-host check against that extracted
tree. It also verifies and publishes the matching `*-source.tar.gz` archive.
This exercises the binary artifact that is uploaded for release and checks that
its corresponding-source digest matches the build metadata.

That check uses pinned Ubuntu 22.04 and 24.04 runtime images and installs only
the host Vulkan/lavapipe driver and virtual display tools. It asserts that Nix,
Rust, and a system FFmpeg are absent, then uses the bundled media tools to render
and inspect MP4 with AAC audio, WebM with Opus audio, and GIF from a self-contained
fixture. It also launches a real GUI window on the virtual display. Runtime logs, actual
encoded media, stream metadata, GUI window-tree evidence, and resolved runtime
package versions are retained below
`target/perf/distribution-refactor/native/runtime/` (or the optional second
argument to the checker).

The media check decodes all five expected video frames and verifies the
container playback duration is within 20 ms of the one-second fixture. It also
decodes audio to PCM and checks the 440 Hz test tone and nonzero level. The raw
PCM sample-count budget allows one final encoded frame: AAC uses 1024 samples
at 44.1 kHz (about 23.2 ms), and Opus uses 960 samples at 48 kHz (882 samples
after the check's 44.1 kHz resampling). This accounts for codec frame padding
while still rejecting the prior WebM output that decoded to only about 0.79 s.
RMS and tone checks use only the PCM samples within the probed container
playback duration, so AAC decoder padding does not affect the measured tone.
