# Workspace tools

Tool ownership follows the subsystem, not the implementation language.

- `ci/`: workspace gate registry, selection policy, workflow validation and test orchestration. The public entry point is `tools/ci/ci.sh`.
- `release/`: shared CLI/GUI distribution, native container builds, source archives and release assembly.
- `quality/`: workspace structure, audit, rendering and performance checks.
- `content/`: source catalog, shared scene contracts and authoring helpers.
- `docs/`: documentation validation.
- `branding/`: assets shared by native applications and the website.

Web build, test, site, development and deployment tooling belongs to `web/tools/`. GUI-only packaging and screenshots belong to `crates/animatix-gui/tools/`; the hybrid benchmark owns its helpers in its own crate.

Cargo workspace and toolchain manifests stay at the repository root. Moving a tool must also update its CI ownership, SDK input fingerprint (where applicable), source archive inputs, and documentation. Run gates from the repository root in the shell specified by `tools/ci/ci.sh gates`.

## Speech transcript conversion

Convert Whisper JSON, Universal Speech JSON, or a SubRip `.srt` file to Animatix `.speech.json`.

```sh
python3 tools/quality/speech_sync.py convert captions.srt -o narration.speech.json
```

SRT timestamps apply to entire cues. Each word in the converted transcript inherits its cue's start and end times.

## Chapter narration

Use `speech_chapter.py` to audition voices, assemble chapter audio and word timings,
and retake individual sentences. [Chapter narration workflow](../docs/narration.md)

## Chapter video

Use `chapter-export.py` to rebuild changed scenes and assemble the MP4.
[Incremental chapter export](../docs/contributing.md#incremental-chapter-export)
