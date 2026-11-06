//! Background document-load worker.
//!
//! Reads, parses, expands imports, typechecks, and builds the timeline off the
//! UI thread so frame handling stays responsive. Only the finished
//! `DocumentSession` is handed back for installation.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use animatix::extension_context::ExtensionContext;
use animatix_analyzer::ExtensionManifest;
use crossbeam_channel::{Receiver, Sender};

use crate::document::DocumentSession;
use crate::error::GuiError;

/// Token identifying one load request so stale completions can be ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoadToken(pub u64);

/// A request to load and rebuild one file off the UI thread.
pub struct LoadRequest {
    token: LoadToken,
    path: PathBuf,
    extension_context: Arc<ExtensionContext>,
    extension_manifest: ExtensionManifest,
    workspace_root: Option<PathBuf>,
    plugin_epoch: u64,
}

/// The result of one background load.
pub struct LoadResponse {
    pub token: LoadToken,
    pub path: PathBuf,
    pub plugin_epoch: u64,
    pub result: Result<DocumentSession, GuiError>,
}

/// Owns the background load thread and its request/response channels.
pub struct LoadWorker {
    request_tx: Option<Sender<LoadRequest>>,
    response_rx: Receiver<LoadResponse>,
    next_token: u64,
    handle: Option<thread::JoinHandle<()>>,
}

impl LoadWorker {
    /// Start the load worker thread.
    pub fn start() -> Self {
        let (request_tx, request_rx) = crossbeam_channel::unbounded::<LoadRequest>();
        let (response_tx, response_rx) = crossbeam_channel::unbounded::<LoadResponse>();
        let handle = thread::Builder::new()
            .name("animatix-load".into())
            .spawn(move || Self::worker_loop(request_rx, response_tx));
        match handle {
            Ok(handle) => Self {
                request_tx: Some(request_tx),
                response_rx,
                next_token: 0,
                handle: Some(handle),
            },
            Err(err) => {
                tracing::error!("Failed to spawn load worker thread: {err}");
                Self {
                    request_tx: None,
                    response_rx,
                    next_token: 0,
                    handle: None,
                }
            },
        }
    }

    /// Submit one load request. The returned token identifies its response.
    pub fn submit(
        &mut self,
        path: PathBuf,
        extension_context: Arc<ExtensionContext>,
        extension_manifest: ExtensionManifest,
        workspace_root: Option<PathBuf>,
        plugin_epoch: u64,
    ) -> Result<LoadToken, String> {
        self.next_token += 1;
        let token = LoadToken(self.next_token);
        let request = LoadRequest {
            token,
            path,
            extension_context,
            extension_manifest,
            workspace_root,
            plugin_epoch,
        };
        let Some(tx) = self.request_tx.as_ref() else {
            return Err("load worker is not running".to_string());
        };
        tx.send(request)
            .map_err(|err| format!("Failed to submit load request: {err}"))?;
        Ok(token)
    }

    /// Poll for completed loads without blocking.
    pub fn poll(&mut self) -> Vec<LoadResponse> {
        let mut responses = Vec::new();
        while let Ok(response) = self.response_rx.try_recv() {
            responses.push(response);
        }
        responses
    }

    #[cfg(test)]
    fn wait_for_response(&mut self, timeout: std::time::Duration) -> Option<LoadResponse> {
        self.response_rx.recv_timeout(timeout).ok()
    }

    fn worker_loop(request_rx: Receiver<LoadRequest>, response_tx: Sender<LoadResponse>) {
        while let Ok(request) = request_rx.recv() {
            let result = DocumentSession::load_with_extension(
                request.path.clone(),
                request.extension_context,
                request.extension_manifest,
                request.workspace_root,
            );
            let response = LoadResponse {
                token: request.token,
                path: request.path,
                plugin_epoch: request.plugin_epoch,
                result,
            };
            if response_tx.send(response).is_err() {
                break;
            }
        }
    }
}

impl Drop for LoadWorker {
    fn drop(&mut self) {
        // Detach the worker: a load in progress cannot be interrupted safely.
        self.request_tx.take();
        self.handle.take();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn document_session_is_send_for_background_load() {
        assert_send::<DocumentSession>();
    }

    #[test]
    fn load_worker_returns_a_built_document() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "animatix_load_worker_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.amx");
        std::fs::write(&path, "#0s\nbox: Rect, size: (100, 100)\n").unwrap();

        let mut worker = LoadWorker::start();
        let token = worker
            .submit(
                path.clone(),
                Arc::new(ExtensionContext::new()),
                ExtensionManifest::default(),
                Some(dir.clone()),
                7,
            )
            .expect("submit load");
        let response = worker.wait_for_response(Duration::from_secs(10)).expect("load response");
        assert_eq!(response.token, token);
        assert_eq!(response.path, path);
        assert_eq!(response.plugin_epoch, 7);
        let document = response.result.expect("valid document");
        assert!(document.timeline.is_some());
        assert!(document.source_text.contains("box:"));

        drop(worker);
        std::fs::remove_dir_all(dir).ok();
    }
}
