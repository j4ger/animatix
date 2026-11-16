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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use animatix::renderer::text::FontContext;
use animatix::timeline::BuildQuality;
use animatix::timeline::assets::AssetCache;
use animatix_web::asset_urls::collect_asset_urls_from_source;
use animatix_web::host::build_document_with_modules;

/// A scene the site plays, and the pages that play it.
#[derive(Default)]
struct SceneRefs {
    pages: Vec<String>,
}

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

/// Historical scene coverage is an explicit regression corpus, independent of
/// whether its former HTML presentation still exists. Current site coverage is
/// tested separately from canonical Astro sources below.
fn collect_regressions(root: &Path, into: &mut BTreeMap<PathBuf, SceneRefs>) {
    for line in include_str!("fixtures/legacy-scenes.txt").lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        assert!(!line.starts_with('/') && !line.split('/').any(|part| part == ".."));
        let scene = root.join(line);
        assert!(
            into.insert(
                scene,
                SceneRefs {
                    pages: vec!["legacy regression corpus".into()]
                }
            )
            .is_none(),
            "duplicate regression: {line}"
        );
    }
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
fn seed_assets(cache: &mut AssetCache, sources: &[Fetched]) -> Vec<String> {
    let mut unfetchable = Vec::new();
    for source in sources {
        for url in literal_asset_urls(&source.text) {
            let path = resolve(&source.dir, &url);
            let svg = url.to_lowercase().ends_with(".svg");
            let payload = if svg {
                std::fs::read_to_string(&path).map(|text| text.into_bytes())
            } else {
                std::fs::read(&path)
            };
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
fn build_scene(scene: &Path) -> Result<(), Vec<String>> {
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
        let unfetchable = seed_assets(&mut cache, &sources);
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
                match std::fs::read_to_string(&path) {
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
            Ok(())
        } else {
            Err(vec!["build produced no target and no error diagnostic".to_string()])
        };
    }
    Err(vec!["import retry did not converge in 32 rounds".to_string()])
}

#[test]
fn legacy_regression_corpus_builds_with_explicit_modules() {
    let root = web_root();
    let mut refs: BTreeMap<PathBuf, SceneRefs> = BTreeMap::new();
    collect_regressions(&root, &mut refs);
    assert!(!refs.is_empty(), "legacy regression corpus is empty");

    let mut failures = Vec::new();
    let mut missing_files = Vec::new();
    let mut checked = 0usize;

    for (scene, r) in &refs {
        if !scene.exists() {
            missing_files.push(format!("{} (played by {:?})", scene.display(), r.pages));
            continue;
        }
        checked += 1;
        if let Err(errors) = build_scene(scene) {
            failures.push(format!(
                "{} [{}] failed to build: {}",
                scene.display(),
                r.pages.join(", "),
                errors.join("; ")
            ));
        }
    }

    eprintln!("site scenes: {} referenced, {} built with the unified SDK", refs.len(), checked);
    assert!(
        missing_files.is_empty(),
        "players point at scene files that are not on disk:\n{}",
        missing_files.join("\n")
    );
    assert!(
        failures.is_empty(),
        "scenes the site plays do not build cleanly (warning or worse):\n{}",
        failures.join("\n")
    );
    assert!(checked > 0, "no scene was exercised");
}

#[test]
fn nested_module_smoke_scene_builds_through_the_explicit_fetch_protocol() {
    let scene = web_root().join("dev/smoke/imports/scene.amx");
    assert!(
        build_scene(&scene).is_ok(),
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
    let directory = web_root().join("content/scenes");
    let mut scenes: Vec<_> = std::fs::read_dir(&directory)
        .expect("canonical site sources exist")
        .map(|entry| entry.expect("readable scene entry").path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("amx"))
        .collect();
    scenes.sort();
    assert!(
        scenes.len() >= 16,
        "the curated works, lessons, techniques and brand scene must be covered"
    );
    let failures: Vec<_> = scenes
        .iter()
        .filter_map(|scene| {
            let source = std::fs::read_to_string(scene).expect("canonical source is readable");
            let mut errors = authored_source_errors(scene, &source);
            if let Err(build_errors) = build_scene(scene) {
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
