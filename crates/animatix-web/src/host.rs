//! Source → diagnostics → buildable document, shared by the wasm driver and
//! native tests.
//!
//! This is the web analogue of the GUI's `DocumentSession::rebuild` pipeline,
//! minus GUI-specific caching: parse/typecheck/expand through the module
//! system in `SourcesOnly` mode (no disk access — the editor owns the source
//! text), then build through the font-context-aware engine entry points.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use animatix::composition::{BuildTarget, Composition};
use animatix::extension_context::ExtensionContext;
use animatix::renderer::text::FontContext;
use animatix::timeline::assets::AssetCache;
use animatix::timeline::{BuildQuality, SceneDimensions, Timeline};
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use animatix_syntax::module::{ModuleError, ModuleGraph, Namespace, SourceAccess};

use crate::dto::{DiagnosticDto, LoadResultDto, MarkerDto};

/// Virtual path the single edited document is registered under. Imports of
/// other files resolve relative to it inside the in-memory source map: the
/// bundled library covers `../lib/*`, the shell supplies everything else via
/// `add_module` (learning the keys from `LoadResultDto::missing_imports`),
/// and anything still absent surfaces as a `FileNotFound` module error.
pub const ENTRY_PATH: &str = "main.amx";

/// The repo's shared example library (`examples/lib/*.amx`), embedded at
/// compile time and registered in the in-memory source map so example
/// imports (`import "../lib/theme.amx"`) resolve without any disk access.
///
/// Keys are the *resolved* import paths: `resolve_import` joins the import
/// string onto the importing file's directory and normalizes, which maps
/// every library reference onto the `../lib/<name>.amx` shape.
///
/// A `const` rather than a function body because [`identity_digest`] has to read
/// it at compile time: the embedded text is part of the artifact's fingerprint.
const BUNDLED_LIBRARY: &[(&str, &str)] = &[
    ("../lib/actions.amx", include_str!("../../../examples/lib/actions.amx")),
    ("../lib/card.amx", include_str!("../../../examples/lib/card.amx")),
    ("../lib/charts.amx", include_str!("../../../examples/lib/charts.amx")),
    (
        "../lib/colorschemes.amx",
        include_str!("../../../examples/lib/colorschemes.amx"),
    ),
    ("../lib/components.amx", include_str!("../../../examples/lib/components.amx")),
    ("../lib/light.amx", include_str!("../../../examples/lib/light.amx")),
    ("../lib/palette.amx", include_str!("../../../examples/lib/palette.amx")),
    ("../lib/reexport.amx", include_str!("../../../examples/lib/reexport.amx")),
    ("../lib/slide.amx", include_str!("../../../examples/lib/slide.amx")),
    ("../lib/theme.amx", include_str!("../../../examples/lib/theme.amx")),
    ("../lib/tokens.amx", include_str!("../../../examples/lib/tokens.amx")),
    ("../lib/ui.amx", include_str!("../../../examples/lib/ui.amx")),
];

pub fn bundled_library() -> &'static [(&'static str, &'static str)] {
    BUNDLED_LIBRARY
}

const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

/// FNV-1a over the version string and every embedded library file.
const fn fold(mut hash: u32, bytes: &[u8]) -> u32 {
    let mut i = 0;
    while i < bytes.len() {
        hash = hash.wrapping_mul(FNV_PRIME) ^ (bytes[i] as u32);
        i += 1;
    }
    hash
}

/// Length-prefix helper, so a rename that moves bytes from the key into the
/// source cannot produce the same digest.
const fn fold_len(mut hash: u32, len: u32) -> u32 {
    let mut shift = 0;
    while shift < 32 {
        hash = hash.wrapping_mul(FNV_PRIME) ^ ((len >> shift) & 0xff);
        shift += 8;
    }
    hash
}

/// The fingerprint of *this* engine bundle: its version and the exact text of
/// every `.amx` compiled into it.
///
/// This replaces a hand-incremented literal (`build_id() -> u32 { 54 }`) that
/// nothing verified. A number someone types by hand says nothing about what is
/// in the artifact, and the failure it was supposed to catch is precisely the one
/// it did not: a stale `web/pkg` served next to fresh scenes, which looked current
/// because the literal had been bumped in the same commit as the source change.
/// Here, editing any bundled `.amx` changes the digest, so an old bundle cannot
/// masquerade as a new one — and no `build.rs` is involved, which would make
/// `cargo test --workspace` depend on `.git` and make the artifact nondeterministic.
pub const fn identity_digest(version: &str, library: &[(&str, &str)]) -> u32 {
    let mut hash = fold(FNV_OFFSET_BASIS, version.as_bytes());
    let mut i = 0;
    while i < library.len() {
        let (key, source) = library[i];
        hash = fold_len(hash, key.len() as u32);
        hash = fold(hash, key.as_bytes());
        hash = fold_len(hash, source.len() as u32);
        hash = fold(hash, source.as_bytes());
        i += 1;
    }
    hash
}

/// The digest this build was compiled with.
pub const BUILD_ID: u32 = identity_digest(env!("CARGO_PKG_VERSION"), BUNDLED_LIBRARY);

/// Duration floor for scrubbing/looping behaviour, mirroring the GUI.
const MIN_DURATION_S: f64 = 0.1;

/// A parsed-and-built document plus everything the JS shell needs to update
/// the UI. `target` is `None` when the current source cannot be built; the
/// caller keeps rendering the previous document (last-known-good behaviour).
pub struct BuiltDocument {
    pub target: Option<BuildTarget>,
    pub result: LoadResultDto,
}

/// Parse, typecheck, expand, and build `source`. Never fails hard: failures
/// come back as diagnostics with `ok == false` and no target.
pub fn build_document(
    source: &str,
    font_context: Arc<FontContext>,
    quality: BuildQuality,
) -> BuiltDocument {
    build_document_with_assets(source, font_context, quality, None)
}

/// `assets` pre-seeds the scene's asset cache (the web player fetches the
/// bytes), so build-time asset loads resolve from memory instead of the
/// filesystem.
pub fn build_document_with_assets(
    source: &str,
    font_context: Arc<FontContext>,
    quality: BuildQuality,
    assets: Option<Arc<AssetCache>>,
) -> BuiltDocument {
    build_document_with_modules(source, &[], font_context, quality, assets)
}

/// Parse, typecheck, expand, and build `source` with `modules` pre-registered
/// in the source map.
///
/// `modules` are the `.amx` files the shell fetched for this scene beyond the
/// bundled library, keyed by the *resolved* import path (what the module
/// system would join + normalize — the shell learns the keys from
/// `LoadResultDto::missing_imports`). When the load stops on a file that was
/// never supplied, the result reports that key in `missing_imports` instead of
/// surfacing only a generic not-found error: the shell fetches it and retries.
pub fn build_document_with_modules(
    source: &str,
    modules: &[(PathBuf, String)],
    font_context: Arc<FontContext>,
    quality: BuildQuality,
    assets: Option<Arc<AssetCache>>,
) -> BuiltDocument {
    let path = PathBuf::from(ENTRY_PATH);
    let mut diagnostics: Vec<DiagnosticDto> = Vec::new();

    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.add_source(path.clone(), source);
    for (lib_path, lib_source) in bundled_library() {
        graph.add_source(PathBuf::from(lib_path), *lib_source);
    }
    for (module_path, module_source) in modules {
        graph.add_source(module_path.clone(), module_source.clone());
    }

    let mut program = match graph.load_program_with_source(&path, Some(source)) {
        Ok(program) => program,
        // A missing file is not a diagnostic to render — it is the shell's
        // cue to fetch one more module and retry. Report the resolved key so
        // the fetch target is unambiguous.
        Err(ModuleError::FileNotFound(missing)) => {
            return BuiltDocument {
                target: None,
                result: LoadResultDto {
                    ok: false,
                    duration_s: MIN_DURATION_S,
                    width: SceneDimensions::default().width,
                    height: SceneDimensions::default().height,
                    diagnostics: vec![DiagnosticDto {
                        severity: "error".to_string(),
                        code: "module-error".to_string(),
                        message: format!(
                            "Imported module '{}' is not loaded yet; fetch it relative to the \
                             scene, register it with add_module, and call load_source again.",
                            missing.display()
                        ),
                        subject: None,
                        line: None,
                        column: None,
                        span: None,
                    }],
                    missing_imports: vec![missing.display().to_string()],
                    markers: Vec::new(),
                },
            };
        },
        Err(err) => return built_failure(module_error_diagnostics(&err, &path)),
    };

    diagnostics.extend(program.typecheck().iter().map(DiagnosticDto::from_diagnostic));

    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    for error in &expansion_errors {
        diagnostics.push(DiagnosticDto::from_diagnostic(&Diagnostic::error(
            DiagnosticCode::UnknownAction,
            DiagnosticPhase::Build,
            error.clone(),
        )));
    }

    let namespaces: HashMap<String, Namespace> = program.namespaces;
    let context = Arc::new(ExtensionContext::new());
    let has_scenes = expanded.iter().any(|s| matches!(s, Stmt::Scene { .. }));

    let (target, build_diagnostics) = if has_scenes {
        let report = Composition::build_with_font_context_and_asset_cache_and_extension_context(
            &expanded,
            &namespaces,
            font_context,
            quality,
            assets,
            context,
        );
        (BuildTarget::MultiScene(report.output), report.diagnostics)
    } else {
        let report =
            Timeline::build_with_diagnostics_and_font_context_and_asset_cache_and_extension_context(
                &expanded,
                &namespaces,
                font_context,
                quality,
                assets,
                context,
            );
        // `BuildTarget::from_ast*` adds a `PersistTargetNotCarried` warning
        // here for persisted actors with no successor scene, but reads the
        // private `persistence_flags` to do it. The web driver skips that
        // nicety; persistence in a single-scene web document degrades to
        // "state simply resets between loops".
        (BuildTarget::SingleScene(report.output), report.diagnostics)
    };

    diagnostics.extend(build_diagnostics.iter().map(DiagnosticDto::from_diagnostic));

    let (duration_s, width, height) = document_extent(&target);
    let markers = timeline_markers(&expanded, &target);
    let ok = !diagnostics.iter().any(|d| d.severity == "error");
    // A build that produced error diagnostics still parses, so a `BuildTarget`
    // exists — but it is not a document the shell may install. Handing it over
    // would replace the figure with the half-built scene (black, at whatever
    // default resolution the broken source implied) even though the caller
    // treats `ok == false` as "keep the last known good document": that is the
    // live editor's "a typo never blanks the figure". `ok == false` therefore
    // yields no target, matching this module's contract. The extent and
    // markers still describe what the build saw, so a caller can report where
    // it stopped.
    let target = ok.then_some(target);
    BuiltDocument {
        target,
        result: LoadResultDto {
            ok,
            duration_s,
            width,
            height,
            diagnostics,
            missing_imports: Vec::new(),
            markers,
        },
    }
}

/// Timeline landmarks for the embed's scrubber: the `#2s` keyframe
/// declarations the author wrote (per scene, offset by the scene's global
/// start in compositions), plus scene starts and transition windows. Sorted
/// by time, deduplicated — deliberately *not* every property-track time,
/// which stagger/assignments would explode into noise.
fn timeline_markers(stmts: &[Stmt], target: &BuildTarget) -> Vec<MarkerDto> {
    let mut markers: Vec<MarkerDto> = Vec::new();
    match target {
        BuildTarget::SingleScene(_) => {
            let mut times = Vec::new();
            collect_keyframe_stmt_times(stmts, 0.0, &mut times);
            for t in times {
                markers.push(MarkerDto {
                    t,
                    kind: "keyframe".to_string(),
                    dur: 0.0,
                });
            }
        },
        BuildTarget::MultiScene(composition) => {
            for (name, start) in &composition.scene_start_times {
                if *start > 0.0 {
                    markers.push(MarkerDto {
                        t: *start,
                        kind: "scene".to_string(),
                        dur: 0.0,
                    });
                }
                // A `play` edge into this scene carries its transition; the
                // blend window is `[to_scene_start, +duration]` (the from
                // scene's tail overlaps it — composition/time.rs).
                if let Some(edge) = composition.edges.get(name)
                    && edge.transition.duration_ms > 0
                {
                    markers.push(MarkerDto {
                        t: *start,
                        kind: "transition".to_string(),
                        dur: edge.transition.duration_ms as f64 / 1000.0,
                    });
                }
            }
            for stmt in stmts {
                let Stmt::Scene { name, body, .. } = stmt else {
                    continue;
                };
                let Some(start) = composition.scene_start_times.get(name) else {
                    continue;
                };
                let mut times = Vec::new();
                collect_keyframe_stmt_times(body, *start, &mut times);
                for t in times {
                    markers.push(MarkerDto {
                        t,
                        kind: "keyframe".to_string(),
                        dur: 0.0,
                    });
                }
            }
        },
    }
    markers.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    markers.dedup_by(|a, b| a.kind == b.kind && (a.t - b.t).abs() < 1e-6);
    markers
}

/// Collect absolute `#Ns` keyframe-statement times, offset by `base` (the
/// owning scene's global start). Relative (`#+Ns`) keyframes are skipped:
/// their absolute time depends on playback order, and the author's beats are
/// the absolute marks.
fn collect_keyframe_stmt_times(stmts: &[Stmt], base: f64, out: &mut Vec<f64>) {
    for stmt in stmts {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                let t = match time {
                    animatix_syntax::ast::Time::Seconds(s) => *s,
                    animatix_syntax::ast::Time::Milliseconds(ms) => *ms as f64 / 1000.0,
                    // Marker placement only: beat stamps land at their
                    // default-tempo position, the same convention the outline
                    // and analyzer views use.
                    animatix_syntax::ast::Time::Beats(b) => {
                        b * animatix_syntax::ast::beat_seconds(animatix_syntax::ast::DEFAULT_BPM)
                    },
                };
                out.push(base + t);
                collect_keyframe_stmt_times(body, base, out);
            },
            Stmt::Scene { body, .. } => collect_keyframe_stmt_times(body, base, out),
            _ => {},
        }
    }
}

/// Duration + dimensions of a built target, with the same floors as the GUI.
fn document_extent(target: &BuildTarget) -> (f64, u32, u32) {
    let default_dims = SceneDimensions::default();
    let default_dims = (default_dims.width, default_dims.height);
    match target {
        BuildTarget::SingleScene(timeline) => {
            let (w, h) = timeline.resolution().unwrap_or(default_dims);
            // Playback length, not the inferred keyframe extent: a declared
            // `config { duration: N }` overrides it, so the player stops there.
            (timeline.playback_duration_seconds().max(MIN_DURATION_S), w, h)
        },
        BuildTarget::MultiScene(composition) => {
            let (w, h) = composition
                .scenes
                .values()
                .next()
                .and_then(|scene| scene.timeline.resolution())
                .unwrap_or(default_dims);
            (composition.global_duration_s.max(MIN_DURATION_S), w, h)
        },
    }
}

fn built_failure(diagnostics: Vec<DiagnosticDto>) -> BuiltDocument {
    BuiltDocument {
        target: None,
        result: LoadResultDto {
            ok: false,
            duration_s: MIN_DURATION_S,
            width: SceneDimensions::default().width,
            height: SceneDimensions::default().height,
            diagnostics,
            missing_imports: Vec::new(),
            markers: Vec::new(),
        },
    }
}

fn module_error_diagnostics(err: &ModuleError, path: &Path) -> Vec<DiagnosticDto> {
    match err {
        ModuleError::ParseErrors(errors) => errors
            .iter()
            .map(|e| DiagnosticDto::from_diagnostic(&e.to_diagnostic().with_path(path)))
            .collect(),
        other => vec![DiagnosticDto {
            severity: "error".to_string(),
            code: "module-error".to_string(),
            message: other.to_string(),
            subject: None,
            line: None,
            column: None,
            span: None,
        }],
    }
}

#[cfg(test)]
mod tests {
    /// A web build that pre-seeds the asset cache must satisfy an `Image`
    /// actor's build-time load from memory: the sandbox has no filesystem, so
    /// without the pre-seed this same scene fails with `MediaLoadFailure`.
    #[cfg(feature = "image-decode")]
    #[test]
    fn preseeded_asset_cache_satisfies_build_time_loads() {
        let png: &[u8] = include_bytes!("../../../examples/assets/checker.png");
        let mut cache = animatix::timeline::assets::AssetCache::new();
        cache.insert_image_bytes("dot.png", png).expect("checker.png decodes");

        let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0.1s
pic: Image, url: "dot.png", size: (400, 240), at: (320, 180)
fade-in pic [300ms]
"#;
        let built = super::build_document_with_assets(
            source,
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
            Some(Arc::new(cache)),
        );

        assert!(
            built.result.ok,
            "the pre-seeded asset must satisfy the build: {:?}",
            built.result.diagnostics
        );
        assert!(
            !built.result.diagnostics.iter().any(|d| d.code == "media-load-failure"),
            "no media-load failure may remain: {:?}",
            built.result.diagnostics
        );
    }

    /// Same for an `Svg` actor: the pre-registered source must satisfy the
    /// build-time parse.
    #[cfg(feature = "svg")]
    #[test]
    fn preseeded_svg_source_satisfies_build_time_parse() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80"><circle cx="40" cy="40" r="30" fill="tomato"/></svg>"#;
        let mut cache = animatix::timeline::assets::AssetCache::new();
        cache.insert_svg_source("mark.svg", svg).expect("the inline svg parses");

        let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0.1s
pic: Svg, url: "mark.svg", size: (300, 300), at: (320, 180)
fade-in pic [300ms]
"#;
        let built = super::build_document_with_assets(
            source,
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
            Some(Arc::new(cache)),
        );

        assert!(
            built.result.ok,
            "the pre-seeded svg must satisfy the build: {:?}",
            built.result.diagnostics
        );
    }

    use super::*;

    const HELLO: &str = include_str!("../../../examples/basics/00_hello.amx");
    const MOTION: &str = include_str!("../../../examples/basics/03_timing.amx");
    const MULTISCENE: &str = include_str!("../../../examples/composition/14_multiscene.amx");

    #[test]
    fn builds_self_contained_example_without_errors() {
        let doc = build_document(HELLO, Arc::new(FontContext::new()), BuildQuality::Draft);
        assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
        assert!(doc.result.diagnostics.is_empty());
        assert!(matches!(doc.target, Some(BuildTarget::SingleScene(_))));
        assert!(doc.result.duration_s >= MIN_DURATION_S);
        assert!(doc.result.width > 0 && doc.result.height > 0);
    }

    #[test]
    fn builds_animation_example_with_duration() {
        let doc = build_document(MOTION, Arc::new(FontContext::new()), BuildQuality::Draft);
        assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
        assert!(
            doc.result.duration_s > MIN_DURATION_S,
            "timed example should have real duration"
        );
    }

    #[test]
    fn builds_multiscene_example_as_composition() {
        let doc = build_document(MULTISCENE, Arc::new(FontContext::new()), BuildQuality::Draft);
        assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
        assert!(matches!(doc.target, Some(BuildTarget::MultiScene(_))));
    }

    #[test]
    fn parse_failure_reports_diagnostics_without_target() {
        let doc = build_document(
            "actor this is not valid amx {{{",
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
        );
        assert!(!doc.result.ok);
        assert!(doc.target.is_none());
        assert!(doc.result.diagnostics.iter().any(|d| d.severity == "error"));
    }

    // A *build* that produced error diagnostics still parses, so it used to
    // hand the shell a half-built target. The embed installs whatever target
    // it is given, so a typo in the live editor blanked the figure it was told
    // to keep — `ok == false` must mean "no document to install".
    #[test]
    fn build_failure_reports_diagnostics_without_target() {
        let doc = build_document(
            "config { colorscheme: \"editorial-dark\", resolution: (320, 180) }\n\
             ghost: DoesNotExist, size: (10, 10)\n\
             seen: Rect, size: (10, 10), color: accent.primary, at: (20, 20)\n\
             #0.1s\nfade-in seen [100ms]\n",
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
        );
        assert!(!doc.result.ok);
        assert!(
            doc.result
                .diagnostics
                .iter()
                .any(|d| d.severity == "error" && d.code == "unknown-actor-type"),
            "expected the unknown-actor-type error, got {:?}",
            doc.result.diagnostics
        );
        assert!(
            doc.target.is_none(),
            "a failed build must not hand the shell a document to install"
        );
        // The reported extent still describes what the build saw, so the shell
        // can say where it stopped.
        assert!(doc.result.width > 0 && doc.result.height > 0);
    }

    // The gate is on errors only. Warnings are ordinary — every shipped demo
    // scene has some — and must keep the document installable.
    #[test]
    fn warnings_still_yield_a_document() {
        let doc = build_document(
            "config { colorscheme: \"editorial-dark\", resolution: (320, 180) }\n\
             ghost: Rect, size: (10, 10), color: accent.primary, at: (20, 20)\n\
             seen: Rect, size: (10, 10), color: accent.success, at: (60, 20)\n\
             #0.1s\nfade-in seen [100ms]\n",
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
        );
        assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
        assert!(
            doc.result.diagnostics.iter().any(|d| d.severity == "warning"),
            "the fixture should exercise the warning path: {:?}",
            doc.result.diagnostics
        );
        assert!(matches!(doc.target, Some(BuildTarget::SingleScene(_))));
    }

    // The transformer demo scenes (web/demos/transformer/scenes) must always
    // build cleanly — they are embedded in the shipped demo page.
    //
    // The scenes import the site theme two directories up (`../../lib/
    // theme.amx` -> web/demos/lib/theme.amx), which the browser fetches and
    // registers through the missing-imports retry protocol. The test helper
    // drives that same protocol: build, register every reported key from the
    // theme source, rebuild until the graph closes.
    fn build_demo_scene(source: &str) -> BuiltDocument {
        let mut modules: Vec<(PathBuf, String)> = Vec::new();
        for _ in 0..4 {
            let mut doc = build_document_with_modules(
                source,
                &modules,
                Arc::new(FontContext::new()),
                BuildQuality::Draft,
                None,
            );
            if doc.result.missing_imports.is_empty() {
                return doc;
            }
            for key in std::mem::take(&mut doc.result.missing_imports) {
                assert!(
                    key.ends_with("theme.amx"),
                    "demo scenes should only import the site theme, got {key}"
                );
                let path = PathBuf::from(&key);
                if !modules.iter().any(|(p, _)| *p == path) {
                    modules.push((path, include_str!("../../../web/demos/lib/theme.amx").into()));
                }
            }
        }
        panic!("the demo scene's import graph never closed: {source:?}");
    }

    #[test]
    fn transformer_demo_scenes_build_cleanly() {
        const SCENES: &[(&str, &str)] = &[
            ("overview", include_str!("../../../web/demos/transformer/scenes/overview.amx")),
            ("tokens", include_str!("../../../web/demos/transformer/scenes/tokens.amx")),
            (
                "positional",
                include_str!("../../../web/demos/transformer/scenes/positional.amx"),
            ),
            ("attention", include_str!("../../../web/demos/transformer/scenes/attention.amx")),
            ("multihead", include_str!("../../../web/demos/transformer/scenes/multihead.amx")),
            (
                "feedforward",
                include_str!("../../../web/demos/transformer/scenes/feedforward.amx"),
            ),
            ("pipeline", include_str!("../../../web/demos/transformer/scenes/pipeline.amx")),
        ];
        for (name, source) in SCENES {
            let doc = build_demo_scene(source);
            assert!(
                doc.result.ok,
                "demo scene '{name}' failed to build: {:?}",
                doc.result.diagnostics
            );
            assert!(
                doc.result.diagnostics.is_empty(),
                "demo scene '{name}' has diagnostics: {:?}",
                doc.result.diagnostics
            );
            assert!(doc.result.duration_s > MIN_DURATION_S, "scene '{name}' has no duration");
        }
    }

    /// The scrubber's landmarks: single-scene documents report their
    /// scene-level keyframe times, compositions report scene starts and
    /// transition windows.
    #[test]
    fn load_result_markers_expose_timeline_landmarks() {
        const TOKENS: &str = include_str!("../../../web/demos/transformer/scenes/tokens.amx");
        let doc = build_demo_scene(TOKENS);
        assert!(doc.result.ok, "tokens should build: {:?}", doc.result.diagnostics);
        let ts: Vec<f64> = doc
            .result
            .markers
            .iter()
            .filter(|m| m.kind == "keyframe")
            .map(|m| m.t)
            .collect();
        for expected in [
            0.15, 0.4, 0.75, 1.3, 1.65, 2.45, 2.85, 4.15, 4.55, 4.85, 5.35,
        ] {
            assert!(
                ts.iter().any(|t| (t - expected).abs() < 1e-3),
                "tokens keyframes must include {expected}s: {ts:?}"
            );
        }
        assert!(
            doc.result.markers.iter().all(|m| m.dur == 0.0),
            "single-scene markers are points: {:?}",
            doc.result.markers
        );

        const MULTISCENE: &str = include_str!("../../../examples/composition/14_multiscene.amx");
        let doc = build_document(MULTISCENE, Arc::new(FontContext::new()), BuildQuality::Draft);
        assert!(doc.result.ok);
        let kinds: Vec<&str> = doc.result.markers.iter().map(|m| m.kind.as_str()).collect();
        assert!(
            kinds.contains(&"scene"),
            "a composition must mark scene starts: {:?}",
            doc.result.markers
        );
        // Sorted, and all markers inside the document's duration.
        let ts: Vec<f64> = doc.result.markers.iter().map(|m| m.t).collect();
        let mut sorted = ts.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(ts, sorted);
        assert!(ts.iter().all(|t| *t >= 0.0 && *t <= doc.result.duration_s + 1e-6));
        let _ = kinds;
    }

    #[test]
    fn missing_import_is_reported_as_a_fetch_key_not_read_from_disk() {
        let doc = build_document(
            "import \"../lib/nonexistent.amx\" as ghost\n",
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
        );
        assert!(!doc.result.ok);
        assert!(doc.target.is_none(), "an unsupplied import builds no document");
        assert!(
            doc.result.diagnostics.iter().any(|d| d.message.contains("nonexistent.amx")),
            "the diagnostic names the missing module: {:?}",
            doc.result.diagnostics
        );
    }

    /// The shell-side import protocol: a load that stops on an unsupplied
    /// import reports the resolved key in `missing_imports`; registering the
    /// fetched text under that key and retrying eventually closes the graph —
    /// including transitively (a supplied module importing another module).
    #[test]
    fn missing_imports_close_the_graph_through_the_retry_protocol() {
        let entry = "import \"./mods/greet.amx\" as greet\n";
        let greet = "import \"./word.amx\" as word\n\nexport component Hello { }\n";
        let word = "export component Word { }\n";

        // Round 1: nothing supplied — the entry's own import is missing.
        let doc = build_document(entry, Arc::new(FontContext::new()), BuildQuality::Draft);
        assert!(!doc.result.ok);
        assert_eq!(doc.result.missing_imports, ["mods/greet.amx".to_string()]);

        // Round 2: greet.amx supplied — its own import surfaces next.
        let modules = vec![(PathBuf::from("mods/greet.amx"), greet.to_string())];
        let doc = build_document_with_modules(
            entry,
            &modules,
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
            None,
        );
        assert!(!doc.result.ok);
        assert_eq!(doc.result.missing_imports, ["mods/word.amx".to_string()]);

        // Round 3: closure — the graph loads.
        let modules = vec![
            (PathBuf::from("mods/greet.amx"), greet.to_string()),
            (PathBuf::from("mods/word.amx"), word.to_string()),
        ];
        let doc = build_document_with_modules(
            entry,
            &modules,
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
            None,
        );
        assert!(doc.result.ok, "the closed graph must build: {:?}", doc.result.diagnostics);
        assert!(doc.result.missing_imports.is_empty());
    }

    /// The point of the digest: it is a function of the bundle's contents. With
    /// the old hand-typed literal this test could not exist, because nothing about
    /// `54` depended on anything.
    #[test]
    fn identity_digest_follows_version_and_contents() {
        // BUILD_ID must be *this* digest, not a number that happens to be stable.
        assert_eq!(BUILD_ID, identity_digest(env!("CARGO_PKG_VERSION"), BUNDLED_LIBRARY));
        // The embedded library is part of the input, not just the version.
        assert_ne!(BUILD_ID, identity_digest(env!("CARGO_PKG_VERSION"), &[]));
        // A version bump alone moves it.
        assert_ne!(
            identity_digest("0.1.0", BUNDLED_LIBRARY),
            identity_digest("0.2.0", BUNDLED_LIBRARY)
        );
        // One byte of scene text moves it.
        assert_ne!(
            identity_digest("0.1.0", &[("a", "x")]),
            identity_digest("0.1.0", &[("a", "y")])
        );
        // Length-prefixed, so bytes moving between key and source is not neutral.
        assert_ne!(
            identity_digest("0.1.0", &[("ab", "c")]),
            identity_digest("0.1.0", &[("a", "bc")])
        );
    }

    /// Two entries sharing a resolved key would make the second invisible in the
    /// browser: the source map is keyed by that path.
    #[test]
    fn bundled_library_keys_are_unique_and_resolved() {
        let mut seen = std::collections::HashSet::new();
        for (key, _) in bundled_library() {
            assert!(key.starts_with("../lib/"), "unexpected key shape: {key}");
            assert!(seen.insert(*key), "duplicate bundled library entry for {key}");
        }
    }
}
