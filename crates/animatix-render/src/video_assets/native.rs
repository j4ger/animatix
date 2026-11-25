use super::{decode, probe};

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use animatix::timeline::assets::AssetCache;
use animatix::timeline::image::SceneImage;
use animatix::timeline::video::{VideoAsset, VideoLoader, VideoMetadata};

use decode::Decoder;

const DECODER_LIMIT: usize = 4;
type SharedReaders = Arc<Mutex<VecDeque<(u64, Decoder)>>>;
static NEXT_ASSET: AtomicU64 = AtomicU64::new(0);

/// Create the cache used by a native CLI or workbench document.
pub fn native_asset_cache() -> Arc<AssetCache> {
    let mut cache = AssetCache::new();
    cache.set_video_loader(Arc::new(NativeLoader::default()));
    Arc::new(cache)
}

#[derive(Default)]
pub(super) struct NativeLoader {
    readers: SharedReaders,
}

impl VideoLoader for NativeLoader {
    fn open(&self, path: &Path) -> Result<Arc<dyn VideoAsset>, String> {
        let path = path
            .canonicalize()
            .map_err(|err| format!("Could not open local video '{}': {err}", path.display()))?;
        if !path.is_file()
            || !path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
                ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("mov")
            })
        {
            return Err(format!("Video expects a local MP4 or MOV file: '{}'", path.display()));
        }
        let media = probe::read(&path)?;
        Ok(Arc::new(NativeAsset {
            id: NEXT_ASSET.fetch_add(1, Ordering::Relaxed),
            path,
            media,
            readers: Arc::clone(&self.readers),
        }))
    }
}

struct NativeAsset {
    id: u64,
    path: PathBuf,
    media: probe::Media,
    readers: SharedReaders,
}

impl std::fmt::Debug for NativeAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeAsset").field("path", &self.path).finish_non_exhaustive()
    }
}

impl Drop for NativeAsset {
    fn drop(&mut self) {
        if let Ok(mut readers) = self.readers.lock() {
            readers.retain(|(id, _)| *id != self.id);
        }
    }
}

impl VideoAsset for NativeAsset {
    fn metadata(&self) -> VideoMetadata {
        self.media.metadata
    }

    fn frame_at(&self, time_s: f64) -> Result<Option<SceneImage>, String> {
        if time_s < self.media.video_start_s || time_s >= self.media.metadata.duration_s {
            return Ok(None);
        }
        let mut readers = self.readers.lock().map_err(|_| "Video decoder cache was interrupted")?;
        let existing = readers
            .iter()
            .rposition(|(id, decoder)| *id == self.id && decoder.can_sample(time_s));
        let mut decoder = match existing.and_then(|i| readers.remove(i)) {
            Some((_, decoder)) => decoder,
            None => {
                if readers.len() >= DECODER_LIMIT {
                    readers.pop_front();
                }
                Decoder::open(&self.path, self.media, time_s)?
            },
        };
        let mut image = decoder.sample(time_s)?;
        if image.is_none() && decoder.started_after_zero() {
            // Some containers index the nearest keyframe after the target.
            // A decode from the beginning preserves the exact selection in that case.
            decoder = Decoder::open(&self.path, self.media, 0.0)?;
            image = decoder.sample(time_s)?;
        }
        readers.push_back((self.id, decoder));
        Ok(image)
    }
}
