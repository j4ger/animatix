//! Read face metadata from the same TTF/OTF sources native builds embed.

use std::{collections::BTreeSet, fs, io::Write, path::Path};

use serde_json::json;
use ttf_parser::{Face, name_id};

pub fn emit(directory: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries: Vec<_> = fs::read_dir(directory)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut faces = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("ttf" | "otf")) {
            continue;
        }
        faces.push(face(&path)?);
    }
    // stdout is the machine-readable font catalog consumed by the SDK builder.
    writeln!(std::io::stdout(), "{}", serde_json::to_string_pretty(&faces)?)?;
    Ok(())
}

fn face(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let face = Face::parse(&bytes, 0)?;
    let family = [name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY]
        .into_iter()
        .find_map(|id| {
            face.names()
                .into_iter()
                .filter(|name| name.name_id == id)
                .find_map(|name| name.to_string())
        })
        .ok_or("font has no Unicode family name")?;
    let mut codes = BTreeSet::new();
    if let Some(cmap) = face.tables().cmap {
        for table in cmap.subtables {
            if table.is_unicode() {
                table.codepoints(|code| {
                    if table.glyph_index(code).is_some() {
                        codes.insert(code);
                    }
                });
            }
        }
    }
    let mut ranges: Vec<[u32; 2]> = Vec::new();
    for code in codes {
        match ranges.last_mut() {
            Some(range) if range[1] + 1 == code => range[1] = code,
            _ => ranges.push([code, code]),
        }
    }
    let axes: Vec<_> = face.variation_axes().into_iter().map(|axis| json!({
        "tag":axis.tag.to_string(),"min":axis.min_value,"default":axis.def_value,"max":axis.max_value,
    })).collect();
    Ok(json!({
        "file":path.file_name().ok_or("font has no filename")?.to_string_lossy(),
        "family":family,"weight":face.weight().to_number(),
        "style":if face.is_italic() {"italic"} else {"normal"},
        "axes":axes,"coverage":ranges,
    }))
}
