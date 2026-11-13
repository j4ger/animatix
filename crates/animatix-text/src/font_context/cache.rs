//! Bounded, content-isolated face, shaping coverage and rich-font caches.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

#[cfg(feature = "rich-text")]
use super::{Bytes, Font};
use super::{FontContext, FontSetId};

// System glyph-fallback fonts

type CharFaceCache = std::sync::Mutex<HashMap<(FontSetId, char), Option<usize>>>;

/// Process-wide cache: character → ordered face position covering it. `None`
/// caches a known-uncovered char so repeated compiles skip the scan.
fn char_face_cache() -> &'static CharFaceCache {
    static CACHE: std::sync::OnceLock<CharFaceCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Shared face data keyed by fontdb face id.
type FaceData = Option<Arc<[u8]>>;

/// A binary source's address is a compact lookup key, but can be reused after
/// its owning database is dropped. Keep a weak identity witness so a later
/// allocation at the same address cannot reuse stale face bytes.
type BinarySource = dyn AsRef<[u8]> + Send + Sync;

struct CachedFaceData {
    source: Option<Weak<BinarySource>>,
    bytes: FaceData,
}

fn cached_source_matches(
    cached: Option<&Weak<BinarySource>>,
    current: Option<&Weak<BinarySource>>,
) -> bool {
    match (cached, current) {
        (Some(cached), Some(current)) => cached.upgrade().is_some_and(|cached| {
            current.upgrade().is_some_and(|current| Arc::ptr_eq(&cached, &current))
        }),
        (None, None) => true,
        _ => false,
    }
}

const FACE_DATA_CACHE_CAP: usize = 512;
const CHAR_FACE_CACHE_CAP: usize = 8192;
#[cfg(feature = "rich-text")]
const TYPST_FONT_CACHE_CAP: usize = 512;

fn evict_font_cache<K, V>(cache: &mut HashMap<K, V>, cap: usize)
where
    K: Clone + Eq + std::hash::Hash,
{
    if cache.len() > cap {
        let remove: Vec<_> = cache.keys().take(cache.len() / 2).cloned().collect();
        for key in remove {
            cache.remove(&key);
        }
    }
}

/// Process-wide cache: fontdb face id → shared face data.
fn face_data_cache() -> &'static std::sync::Mutex<HashMap<(FontSetId, String), CachedFaceData>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<HashMap<(FontSetId, String), CachedFaceData>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

#[cfg(feature = "rich-text")]
/// Process-wide cache: content font environment and stable ordered position → Typst font.
fn typst_font_cache() -> &'static std::sync::Mutex<HashMap<(FontSetId, usize), Font>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<(FontSetId, usize), Font>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Shared copy of a system face's bytes (loaded once per process).
pub(super) fn cached_face_data(font_ctx: &FontContext, id: fontdb::ID) -> Option<Arc<[u8]>> {
    let (source, _) = font_ctx.db.face_source(id)?;
    let (source_key, source_witness) = match source {
        fontdb::Source::Binary(data) => {
            (format!("binary:{:p}", Arc::as_ptr(&data)), Some(Arc::downgrade(&data)))
        },
        fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
            (format!("file:{}", path.display()), None)
        },
    };
    let key = (font_ctx.font_set_id, source_key);
    let mut cache = face_data_cache().lock().unwrap();
    if let Some(entry) = cache.get(&key) {
        if cached_source_matches(entry.source.as_ref(), source_witness.as_ref()) {
            return entry.bytes.clone();
        }
        // The raw pointer key has been recycled for another binary source.
        cache.remove(&key);
    }
    let data = FontContext::face_data(&font_ctx.db, id).map(Arc::from);
    cache.insert(
        key,
        CachedFaceData {
            source: source_witness,
            bytes: data.clone(),
        },
    );
    evict_font_cache(&mut cache, FACE_DATA_CACHE_CAP);
    // TTC faces share one entry. Bound retained bytes as well as entry count;
    // a live layout owns its Arc even if the reusable cache evicts it.
    const BYTE_CAP: usize = 128 * 1024 * 1024;
    let mut retained: usize = cache
        .values()
        .filter_map(|entry| entry.bytes.as_ref())
        .map(|bytes| bytes.len())
        .sum();
    if retained > BYTE_CAP {
        let keys: Vec<_> = cache.keys().cloned().collect();
        for key in keys {
            if retained <= BYTE_CAP {
                break;
            }
            if let Some(CachedFaceData {
                bytes: Some(bytes), ..
            }) = cache.remove(&key)
            {
                retained -= bytes.len();
            }
        }
    }
    data
}

/// Whether a parsed face can render `ch`.
fn face_covers_char(data: &[u8], index: u32, ch: char) -> bool {
    ttf_parser::Face::parse(data, index)
        .map(|face| face.glyph_index(ch).is_some())
        .unwrap_or(false)
}

/// Find the first installed face covering `ch`, preferring normal weight and
/// style, non-monospaced faces. Cached per character.
pub(super) fn probe_char_face(font_ctx: &FontContext, ch: char) -> Option<fontdb::ID> {
    let key = (font_ctx.font_set_id, ch);
    if let Some(cached) = char_face_cache().lock().unwrap().get(&key) {
        return cached.and_then(|position| font_ctx.db.faces().nth(position).map(|face| face.id));
    }
    let db = &font_ctx.db;
    let mut found: Option<usize> = None;
    'passes: for prefer_plain in [true, false] {
        for (position, face) in db.faces().enumerate() {
            let matches = if prefer_plain {
                face.style == fontdb::Style::Normal
                    && face.weight == fontdb::Weight::NORMAL
                    && !face.monospaced
            } else {
                true // Last pass also permits monospace and style-only fallback faces.
            };
            if !matches {
                continue;
            }
            let Some(data) = cached_face_data(font_ctx, face.id) else {
                continue;
            };
            if face_covers_char(data.as_ref(), face.index, ch) {
                found = Some(position);
                break 'passes;
            }
        }
    }
    let mut cache = char_face_cache().lock().unwrap();
    cache.insert(key, found);
    evict_font_cache(&mut cache, CHAR_FACE_CACHE_CAP);
    found.and_then(|position| db.faces().nth(position).map(|face| face.id))
}

#[cfg(feature = "rich-text")]
/// Construct (or fetch from cache) a Typst font for a system face.
pub(super) fn cached_typst_font(font_ctx: &FontContext, id: fontdb::ID) -> Option<Font> {
    let mut cache = typst_font_cache().lock().unwrap();
    let position = font_ctx.db.faces().position(|face| face.id == id)?;
    let key = (font_ctx.font_set_id, position);
    if let Some(font) = cache.get(&key) {
        return Some(font.clone());
    }
    let data = cached_face_data(font_ctx, id)?;
    let index = font_ctx.db.face(id)?.index;
    let font = Font::new(Bytes::new(data), index)?;
    cache.insert(key, font.clone());
    evict_font_cache(&mut cache, TYPST_FONT_CACHE_CAP);
    let mut retained: usize = cache.values().map(|font| font.data().len()).sum();
    const BYTE_CAP: usize = 128 * 1024 * 1024;
    if retained > BYTE_CAP {
        let keys: Vec<_> = cache.keys().cloned().collect();
        for key in keys {
            if retained <= BYTE_CAP {
                break;
            }
            if let Some(font) = cache.remove(&key) {
                retained -= font.data().len();
            }
        }
    }
    Some(font)
}

#[cfg(feature = "rich-text")]
/// Collect Typst fonts covering characters of `content` the primary family
/// cannot render (CJK, Cyrillic, ...), without requiring `font_family`.
/// Bounded at 8 fallback faces per compile; results are cached per face id.
pub(crate) fn collect_fallback_fonts(
    font_ctx: &FontContext,
    content: &str,
    primary_family: &str,
    primary_weight: f32,
    primary_style: &str,
) -> Vec<Font> {
    // Resolve the primary face through shared, bounded cached bytes.
    let fontdb_style = match primary_style {
        "italic" | "oblique" => fontdb::Style::Italic,
        _ => fontdb::Style::Normal,
    };
    let primary_id = font_ctx.db.query(&fontdb::Query {
        families: &[fontdb::Family::Name(primary_family)],
        weight: fontdb::Weight(primary_weight.round() as u16),
        style: fontdb_style,
        ..Default::default()
    });
    let primary = primary_id.and_then(|id| {
        let index = font_ctx.db.face(id)?.index;
        let data = cached_face_data(font_ctx, id)?;
        Some((data, index))
    });

    let mut ids: Vec<fontdb::ID> = Vec::new();
    for ch in content.chars() {
        if ch.is_whitespace() {
            continue;
        }
        let covered = primary
            .as_ref()
            .map(|(data, index)| face_covers_char(data, *index, ch))
            .unwrap_or(false);
        if covered {
            continue;
        }
        if let Some(id) = probe_char_face(font_ctx, ch)
            && !ids.contains(&id)
        {
            ids.push(id);
        }
        if ids.len() >= 8 {
            break;
        }
    }
    ids.iter().filter_map(|id| cached_typst_font(font_ctx, *id)).collect()
}

/// Resolve an entire grapheme against one face. A scalar-by-scalar probe can
/// otherwise pick two individually valid fonts that cannot shape the cluster.
pub(super) fn probe_cluster_face(ctx: &FontContext, chars: &[char]) -> Option<fontdb::ID> {
    type ClusterKey = (FontSetId, String);
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<ClusterKey, Option<usize>>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let key = (ctx.font_set_id, chars.iter().collect());
    if let Some(position) = cache.lock().unwrap().get(&key) {
        return position.and_then(|position| ctx.db.faces().nth(position).map(|face| face.id));
    }
    let mut found = None;
    'passes: for prefer_plain in [true, false] {
        for (position, info) in ctx.db.faces().enumerate() {
            if prefer_plain && (info.style != fontdb::Style::Normal || info.monospaced) {
                continue;
            }
            let Some(data) = cached_face_data(ctx, info.id) else {
                continue;
            };
            let Ok(face) = ttf_parser::Face::parse(&data, info.index) else {
                continue;
            };
            if chars.iter().all(|ch| face.glyph_index(*ch).is_some()) {
                found = Some(position);
                break 'passes;
            }
        }
    }
    let mut cache = cache.lock().unwrap();
    cache.insert(key, found);
    evict_font_cache(&mut cache, 2048);
    found.and_then(|position| ctx.db.faces().nth(position).map(|face| face.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_binary_source_is_not_a_cache_hit_for_a_new_source() {
        let old_source: Arc<BinarySource> = Arc::new(b"old font bytes".to_vec());
        let old_witness = Arc::downgrade(&old_source);
        let stale_entry = CachedFaceData {
            source: Some(old_witness),
            bytes: Some(Arc::from(&b"old font bytes"[..])),
        };
        drop(old_source);

        let current_source: Arc<BinarySource> = Arc::new(b"different font bytes".to_vec());
        let current_witness = Arc::downgrade(&current_source);
        assert!(!cached_source_matches(stale_entry.source.as_ref(), Some(&current_witness)));
        assert_eq!(stale_entry.bytes.as_deref().unwrap(), b"old font bytes");
    }
}
