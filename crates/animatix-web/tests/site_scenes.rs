//! Every scene the deployed site plays must build with the unified web SDK.
//!
//! This is the gate for browser-only failures: a scene whose import, asset,
//! or runtime capability cannot be prepared is fine on the desktop but shows
//! "Scene error" in the page. The test drives `animatix_web::host` — the same
//! parse/build pipeline the wasm module exposes to JS — and follows the
//! embed's own fetch protocol, so the module keys it learns, the assets it
//! seeds and the builds it runs are the ones the player would perform.
//!
//! The site-scene gate runs once against the SDK's default feature set, which
//! includes every capability. Partition loading is covered by SDK tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use animatix::composition::BuildTarget;
use animatix::renderer::text::FontContext;
use animatix::timeline::BuildQuality;
use animatix::timeline::assets::AssetCache;
use animatix_web::asset_urls::collect_asset_urls_from_source;
use animatix_web::host::{BuiltDocument, build_document_with_modules};

/// One source the player has in hand, with the directory its urls and imports
/// resolve against — the equivalent of `new URL(key, sourceUrl)`.
#[derive(Clone)]
struct Fetched {
    key: String,
    dir: PathBuf,
    text: String,
}

fn web_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../web")
        .canonicalize()
        .expect("web/ is part of the repository")
}

/// Resolve `src` against the page's directory the way `new URL(src, pageUrl)`
/// does, then collapse `.`/`..` lexically (URL resolution has no symlinks to
/// follow either).
fn resolve(dir: &Path, src: &str) -> PathBuf {
    let mut out = dir.to_path_buf();
    for segment in src.split('/') {
        match segment {
            "" | "." => {},
            ".." => {
                out.pop();
            },
            other => out.push(other),
        }
    }
    out
}

/// The literal `url: "…"` properties in a source, in order, deduplicated.
///
/// This drives the same AST collector as `list_asset_urls` on the wasm class,
/// so the site test fetches exactly the assets the embed fetches.
fn literal_asset_urls(text: &str) -> Vec<String> {
    collect_asset_urls_from_source(text)
}

/// Seed the cache exactly as `_fetchAssets` does: each source's urls resolve
/// against that source's own location, `.svg` crosses as text and anything
/// else as encoded bytes, and the literal url string is the cache key. A url
/// with no file behind it is the page's own 404, so it is reported.
fn read_resource(path: &Path, published: Option<&Path>) -> std::io::Result<Vec<u8>> {
    if let Some(root) = published
        && !path.canonicalize()?.starts_with(root)
    {
        return Err(std::io::Error::other("outside the published content tree"));
    }
    std::fs::read(path)
}

fn seed_assets(
    cache: &mut AssetCache,
    sources: &[Fetched],
    published: Option<&Path>,
) -> Vec<String> {
    let mut unfetchable = Vec::new();
    for source in sources {
        for url in literal_asset_urls(&source.text) {
            let path = resolve(&source.dir, &url);
            let svg = url.to_lowercase().ends_with(".svg");
            let payload = read_resource(&path, published);
            match payload {
                Err(e) => unfetchable.push(format!("{url} (resolved to {path:?}): {e}")),
                Ok(bytes) => {
                    if svg {
                        let text = String::from_utf8_lossy(&bytes);
                        if let Err(e) = cache.insert_svg_source(&url, &text) {
                            unfetchable.push(format!("{url} ({path:?}): {e}"));
                        }
                    }
                    if !svg && let Err(e) = cache.insert_image_bytes(&url, &bytes) {
                        unfetchable.push(format!("{url} ({path:?}): {e}"));
                    }
                },
            }
        }
    }
    unfetchable
}

/// Build `scene` the way the player does: imports through the embed's retry
/// loop (the engine names the unresolved keys, each is read relative to the
/// scene, the build runs again — 32 rounds as in JS) and assets pre-seeded
/// into the cache, because the sandboxed build never touches a filesystem.
fn build_scene(scene: &Path, published: Option<&Path>) -> Result<BuiltDocument, Vec<String>> {
    let source = std::fs::read_to_string(scene).map_err(|e| vec![format!("{scene:?}: {e}")])?;
    let scene_dir = scene.parent().unwrap().to_path_buf();
    let mut modules: Vec<Fetched> = Vec::new();

    for _round in 0..32 {
        let mut sources = vec![Fetched {
            key: String::new(),
            dir: scene_dir.clone(),
            text: source.clone(),
        }];
        sources.extend(modules.iter().cloned());
        let mut cache = AssetCache::new();
        let unfetchable = seed_assets(&mut cache, &sources, published);
        if !unfetchable.is_empty() {
            return Err(unfetchable);
        }

        let built = build_document_with_modules(
            &source,
            &modules
                .iter()
                .map(|m| (PathBuf::from(&m.key), m.text.clone()))
                .collect::<Vec<_>>(),
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
            Some(Arc::new(cache)),
        );

        if !built.result.missing_imports.is_empty() {
            let mut added = false;
            for key in &built.result.missing_imports {
                if modules.iter().any(|m| &m.key == key) {
                    continue;
                }
                let path = resolve(&scene_dir, key);
                let text = read_resource(&path, published)
                    .and_then(|bytes| String::from_utf8(bytes).map_err(std::io::Error::other));
                match text {
                    Ok(text) => {
                        modules.push(Fetched {
                            dir: resolve(&scene_dir, key)
                                .parent()
                                .unwrap_or(&scene_dir)
                                .to_path_buf(),
                            key: key.clone(),
                            text,
                        });
                        added = true;
                    },
                    Err(e) => {
                        return Err(vec![format!(
                            "import '{key}' (resolved to {path:?}) is not fetchable: {e}"
                        )]);
                    },
                }
            }
            if added {
                continue;
            }
        }

        // Anything warning-or-worse is a failure, not just an error. The palette
        // defect this catches is invisible to a "does it build" test: an imported
        // `Colorscheme` that the embed's module path does not register still
        // builds, and the page quietly renders in the fallback palette while
        // `animatix check` on disk is clean. Only the browser console showed it,
        // and only for the scenes whose import specifier escapes the entry dir.
        let errors: Vec<String> = built
            .result
            .diagnostics
            .iter()
            .filter(|d| d.severity != "hint" && d.severity != "info")
            .map(|d| format!("{} {} [{}] line {:?}", d.severity, d.message, d.code, d.line))
            .collect();
        return if !errors.is_empty() {
            Err(errors)
        } else if built.result.ok {
            Ok(built)
        } else {
            Err(vec!["build produced no target and no error diagnostic".to_string()])
        };
    }
    Err(vec!["import retry did not converge in 32 rounds".to_string()])
}

#[test]
fn graph_trace_composition_builds_with_explicit_imports() {
    let scene = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/amx/composition/curve-handoff.amx");
    let built = build_scene(&scene, None).expect("the graph and its harmonic module build");
    assert!((built.result.duration_s - 4.5).abs() < 1e-6);
    let Some(BuildTarget::MultiScene(composition)) = built.target else {
        panic!("the fixture must build a composition");
    };
    let (_, _, blend) = composition.evaluate(2.75);
    let blend = blend.expect("the graph hands off to the spectrum during the wipe");
    assert_eq!((blend.from_scene.as_str(), blend.to_scene.as_str()), ("Trace", "Spectrum"));
    assert!((blend.progress - 0.5).abs() < 1e-6);
}

#[test]
fn svg_probe_prepares_assets_and_nested_imports() {
    let root = web_root().join("dev/svg-probe");
    for file in ["scene-import.amx", "scene_rect.amx", "scene-quality.amx"] {
        if let Err(errors) = build_scene(&root.join(file), None) {
            panic!("{file}: {errors:?}");
        }
    }
}

#[test]
fn nested_module_smoke_scene_builds_through_the_explicit_fetch_protocol() {
    let scene = web_root().join("dev/smoke/imports/scene.amx");
    assert!(
        build_scene(&scene, None).is_ok(),
        "nested module smoke scene must close its import graph"
    );
}

/// The runtime build is deliberately permissive about authored property types.
/// Pair it with the shared language checker when certifying teaching sources.
fn authored_source_errors(path: &Path, source: &str) -> Vec<String> {
    use animatix_analyzer::DiagnosticSeverity;
    let mut analyzer = animatix_analyzer::Analyzer::new_with_path(source, Some(path.to_path_buf()));
    analyzer.merge_import_symbols();
    analyzer
        .diagnostics()
        .into_iter()
        .filter(|diagnostic| {
            matches!(diagnostic.severity, DiagnosticSeverity::Warning | DiagnosticSeverity::Error)
        })
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Astro loads canonical sources dynamically rather than through literal HTML
/// players. Keep these in the workspace test gate as well as manual site builds.
#[test]
fn every_canonical_astro_scene_builds_with_the_sdk() {
    let published = web_root().join("content");
    let directory = published.join("scenes");
    let mut scenes: Vec<_> = std::fs::read_dir(&directory)
        .expect("canonical site sources exist")
        .map(|entry| entry.expect("readable scene entry").path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("amx"))
        .collect();
    scenes.sort();
    assert!(!scenes.is_empty(), "public scene inventory must be nonempty");
    let failures: Vec<_> = scenes
        .iter()
        .filter_map(|scene| {
            let source = std::fs::read_to_string(scene).expect("canonical source is readable");
            let mut errors = authored_source_errors(scene, &source);
            if let Err(build_errors) = build_scene(scene, Some(&published)) {
                errors.extend(build_errors);
            }
            (!errors.is_empty()).then(|| format!("{}: {}", scene.display(), errors.join("; ")))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "canonical sources must parse, type-check and build through the SDK: {}",
        failures.join("\n")
    );
}

#[test]
fn canonical_source_preflight_rejects_syntax_and_property_type_errors() {
    for source in [
        "#0s\nbroken: Rect, size: (",
        "#0s\nbroken: Rect, size: \"wide\"",
    ] {
        assert!(
            !authored_source_errors(Path::new("negative.amx"), source).is_empty(),
            "invalid source must not be certified by the site preflight: {source}"
        );
    }
}
