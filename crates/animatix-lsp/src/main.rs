//! LSP server for the Animatix DSL.
//!
//! This binary provides language intelligence (completions, diagnostics, hover, go-to-definition)
//! to external editors like VS Code and Neovim via the Language Server Protocol.

mod handlers;
mod paths;
mod server;

use tower_lsp::{LspService, Server};

use crate::server::Backend;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use animatix_analyzer::discover_manifest_sources;

    use crate::paths::{has_amx_extension, is_manifest_file_name, uri_to_path};

    #[test]
    fn uri_to_path_strips_file_prefix() {
        assert_eq!(
            uri_to_path("file:///home/user/project/main.amx"),
            Some(PathBuf::from("/home/user/project/main.amx"))
        );
    }

    #[test]
    fn uri_to_path_returns_none_for_non_file_uri() {
        assert_eq!(uri_to_path("http://example.com/file.amx"), None);
    }

    #[test]
    fn uri_to_path_handles_empty_string() {
        assert_eq!(uri_to_path(""), None);
    }

    #[test]
    fn manifest_and_source_paths_are_classified() {
        assert!(is_manifest_file_name(Path::new("/p/demo.amx-plugin.toml")));
        assert!(!is_manifest_file_name(Path::new("/p/demo.amx")));
        assert!(!is_manifest_file_name(Path::new("/p/plain.toml")));

        assert!(has_amx_extension(Path::new("/p/main.amx")));
        assert!(!has_amx_extension(Path::new("/p/demo.amx-plugin.toml")));
        assert!(!has_amx_extension(Path::new("/p/notes.txt")));
    }

    #[test]
    fn loads_amx_plugin_manifests_from_directory() {
        let dir = std::env::temp_dir().join(format!(
            "animatix-lsp-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("demo.amx-plugin.toml"), "[[primitives]]\ntype_name = \"Gauge\"\n")
            .expect("write manifest");
        std::fs::write(dir.join("ignored.toml"), "").expect("write ignored file");

        let sources = discover_manifest_sources(Some(&dir), None, &[]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].manifest.primitives[0].type_name, "Gauge");

        std::fs::remove_dir_all(&dir).ok();
    }
}
