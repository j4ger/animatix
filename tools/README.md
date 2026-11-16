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
