//! Native Video actor backend.

#[cfg(feature = "video")]
mod decode;
#[cfg(feature = "video")]
mod native;
#[cfg(feature = "video")]
mod probe;

#[cfg(all(test, feature = "video"))]
mod tests;

#[cfg(feature = "video")]
pub use native::native_asset_cache;

/// Create the document cache when the native video backend is disabled.
#[cfg(not(feature = "video"))]
pub fn native_asset_cache() -> std::sync::Arc<animatix::timeline::assets::AssetCache> {
    std::sync::Arc::new(animatix::timeline::assets::AssetCache::new())
}
