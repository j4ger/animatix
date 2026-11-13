//! Use the library API: the upstream example CLI emits harness-specific URLs.

use std::{collections::BTreeMap, fs, path::Path};

use animatix_core::features::FeatureMask;
use serde_json::{Value, json};

pub fn emit(
    original: &str,
    bindgen_dir: &str,
    out: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = fs::read(original)?;
    let bindgen_dir = Path::new(bindgen_dir);
    let bindgened = fs::read(bindgen_dir.join("animatix_web_bg.wasm"))?;
    let output = wasm_split_cli::Splitter::new(&input, &bindgened)?.emit()?;
    let out = Path::new(out);
    fs::create_dir_all(out)?;
    fs::write(out.join("animatix_web_bg.wasm"), &output.main.bytes)?;
    fs::copy(bindgen_dir.join("animatix_web.js"), out.join("animatix_web.js"))?;
    let mut chunks = Vec::new();
    for (index, chunk) in output.chunks.iter().enumerate() {
        let file = format!("shared_{index}.wasm");
        fs::write(out.join(&file), &chunk.bytes)?;
        chunks.push(json!({"id": format!("shared_{index}"), "file": file, "dependencies": []}));
    }
    let mut capabilities = BTreeMap::new();
    for module in &output.modules {
        let (name, mask) = capability(&module.module_name)?;
        if capabilities.contains_key(name) {
            return Err(format!("duplicate capability factory: {name}").into());
        }
        let file = format!("capability_{name}.wasm");
        fs::write(out.join(&file), &module.bytes)?;
        let mut dependencies: Vec<_> = module.relies_on_chunks.iter().copied().collect();
        dependencies.sort_unstable();
        let import = format!(
            "__wasm_split_load_{}_{}_{}",
            module.module_name,
            module.hash_id.as_deref().ok_or("missing split hash")?,
            module.component_name.as_deref().ok_or("missing factory name")?,
        );
        chunks.push(json!({"id":name, "file":file, "dependencies":dependencies.iter().map(|i| format!("shared_{i}")).collect::<Vec<_>>(), "import": import}));
        capabilities.insert(name, json!({"mask":mask,"files":[file],"dependencies":dependencies}));
    }
    fs::write(
        out.join("split-manifest.json"),
        serde_json::to_vec_pretty(&json!({"chunks":chunks,"capabilities":capabilities}))?,
    )?;
    emit_glue(out, &chunks, &capabilities)?;
    Ok(())
}

fn capability(name: &str) -> Result<(&'static str, u32), Box<dyn std::error::Error>> {
    match name {
        "rich" => Ok(("rich_text", FeatureMask::RICH_TEXT.bits())),
        "media" => Ok(("media", (FeatureMask::IMAGE_DECODE | FeatureMask::SVG).bits())),
        "encode" => Ok(("raster_encode", FeatureMask::RASTER_ENCODE.bits())),
        other => Err(format!("unknown capability partition '{other}'").into()),
    }
}

fn emit_glue(
    out: &Path,
    chunks: &[Value],
    capabilities: &BTreeMap<&str, Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = serde_json::to_string(&json!({"chunks":chunks,"capabilities":capabilities}))?;
    let mut glue = include_str!("runtime.js").replace("__SPLIT_MANIFEST__", &data);
    for chunk in chunks {
        if let Some(import) = chunk["import"].as_str() {
            glue.push_str(&format!(
                "\nexport const {import} = callbackLoader({});\n",
                serde_json::to_string(&chunk["id"])?
            ));
        }
    }
    fs::write(out.join("__wasm_split.js"), glue)?;
    let entry = out.join("animatix_web.js");
    let mut js = fs::read_to_string(&entry)?;
    js.push_str("\nexport { ensure_capabilities } from './__wasm_split.js';\n");
    fs::write(entry, js)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_and_duplicate_partitions_cannot_silently_ship() {
        assert_eq!(capability("rich").unwrap(), ("rich_text", 1));
        assert_eq!(capability("media").unwrap(), ("media", 6));
        assert!(capability("typo").is_err());
    }
}
