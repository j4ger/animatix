//! URI/path conversion and file classification shared by the server and the
//! handlers.

use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::Url;

/// Convert a file:// URI to a PathBuf.
pub(crate) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let url = url::Url::parse(uri).ok()?;
    url.to_file_path().ok()
}

pub(crate) fn path_to_uri(path: &str) -> Option<Url> {
    Url::parse(&format!("file://{path}")).ok()
}

/// True when a path names an Animatix source file.
pub(crate) fn has_amx_extension(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "amx")
}

/// True when a path names an `*.amx-plugin.toml` extension manifest.
pub(crate) fn is_manifest_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".amx-plugin.toml"))
}
