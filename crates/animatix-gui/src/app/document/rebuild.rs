//! Background rebuild worker that runs parse/typecheck/build off the UI thread.
//!
//! The worker receives source text snapshots, runs `DocumentSession::rebuild()`
//! on a background thread, and sends the result back for acceptance on the UI thread.

use std::path::PathBuf;
use std::thread;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};

use crate::app::document::rebuild_output::{RebuildFailure, RebuildOutput};
use crate::app::document::version::{
    CancellationSource, CancellationToken, SourceEpoch, SourceHash,
};
use crate::document::DocumentSession;

/// Token identifying a specific rebuild request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RebuildToken(pub u64);

/// A request sent to the rebuild worker.
pub struct RebuildRequest {
    pub token: RebuildToken,
    pub source_epoch: SourceEpoch,
    pub source_hash: SourceHash,
    pub file_path: PathBuf,
    pub source_text: String,
    pub cancellation: CancellationToken,
}

/// A response from the rebuild worker.
pub struct RebuildResponse {
    pub token: RebuildToken,
    pub source_epoch: SourceEpoch,
    pub source_hash: SourceHash,
    pub result: Result<RebuildOutput, RebuildFailure>,
    pub elapsed_ms: f32,
}

/// The rebuild worker runs on a dedicated thread.
pub struct RebuildWorker {
    request_tx: Option<Sender<RebuildRequest>>,
    response_rx: Receiver<RebuildResponse>,
    cancel_source: CancellationSource,
    next_token: u64,
    handle: Option<thread::JoinHandle<()>>,
}

impl RebuildWorker {
    /// Start a new rebuild worker on a background thread.
    pub fn start() -> Self {
        let (req_tx, req_rx) = crossbeam_channel::unbounded::<RebuildRequest>();
        let (res_tx, res_rx) = crossbeam_channel::bounded::<RebuildResponse>(4);

        let handle = thread::Builder::new().name("animatix-rebuild".into()).spawn(move || {
            Self::worker_loop(req_rx, res_tx);
        });

        match handle {
            Ok(handle) => Self {
                request_tx: Some(req_tx),
                response_rx: res_rx,
                cancel_source: CancellationSource::new(),
                next_token: 0,
                handle: Some(handle),
            },
            Err(err) => {
                tracing::error!("Failed to spawn rebuild worker thread: {err}");
                Self {
                    request_tx: None,
                    response_rx: res_rx,
                    cancel_source: CancellationSource::new(),
                    next_token: 0,
                    handle: None,
                }
            },
        }
    }

    /// Submit a rebuild request. Previous requests with lower tokens are
    /// automatically cancelled.
    pub fn submit(
        &mut self,
        source: &crate::app::stores::SourceStore,
    ) -> Result<RebuildToken, String> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.text().hash(&mut hasher);
        let hash = hasher.finish();

        self.next_token += 1;
        self.cancel_source.cancel(self.next_token);

        let request = RebuildRequest {
            token: RebuildToken(self.next_token),
            source_epoch: source.epoch(),
            source_hash: SourceHash(hash),
            file_path: source.file_path().to_path_buf(),
            source_text: source.text().to_string(),
            cancellation: self.cancel_source.token(),
        };

        let Some(tx) = self.request_tx.as_ref() else {
            return Err("rebuild worker is not running".to_string());
        };

        tx.send(request)
            .map_err(|err| format!("Failed to submit rebuild request: {err}"))?;
        Ok(RebuildToken(self.next_token))
    }

    /// Poll for completed rebuild responses. Returns all available responses.
    pub fn poll(&mut self) -> Vec<RebuildResponse> {
        let mut responses = Vec::new();
        while let Ok(response) = self.response_rx.try_recv() {
            responses.push(response);
        }
        responses
    }

    fn worker_loop(req_rx: Receiver<RebuildRequest>, res_tx: Sender<RebuildResponse>) {
        while let Ok(request) = req_rx.recv() {
            let start = Instant::now();

            // Check cancellation before starting work
            if request.cancellation.is_cancelled() {
                continue;
            }

            // Build a temporary DocumentSession and run rebuild
            let mut session = match DocumentSession::from_source(
                request.file_path.clone(),
                request.source_text.clone(),
            ) {
                Ok(s) => s,
                Err(_) => {
                    let _ = res_tx.send(RebuildResponse {
                        token: request.token,
                        source_epoch: request.source_epoch,
                        source_hash: request.source_hash,
                        result: Err(RebuildFailure {
                            error: "failed to create document session".into(),
                            diagnostics: Vec::new(),
                            partial_source_index: None,
                        }),
                        elapsed_ms: start.elapsed().as_secs_f32() * 1000.0,
                    });
                    continue;
                },
            };

            // Check cancellation again before running rebuild
            if request.cancellation.is_cancelled() {
                continue;
            }

            let rebuild_result = session.rebuild();

            // Check cancellation after rebuild (don't send stale data)
            if request.cancellation.is_cancelled() {
                continue;
            }

            let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;

            let result = match rebuild_result {
                Ok(()) => {
                    // Get timeline duration
                    let duration_s = session.duration_s;
                    let scene_dimensions = session.scene_dimensions;

                    Ok(RebuildOutput {
                        raw_statements: session.raw_statements.unwrap_or_default(),
                        expanded_statements: session.expanded_statements.unwrap_or_default(),
                        namespaces: session.namespaces,
                        components: session.components,
                        module_actions: session.module_actions,
                        source_index: session.source_index.unwrap_or_default(),
                        timeline: session.timeline,
                        composition: session.composition,
                        diagnostics: session.diagnostics,
                        timeline_index: session.timeline_index,
                        keyframe_lines: session.keyframe_lines,
                        duration_s,
                        scene_dimensions,
                    })
                },
                Err(e) => Err(RebuildFailure {
                    error: e.to_string(),
                    diagnostics: session.diagnostics,
                    partial_source_index: session.source_index,
                }),
            };

            let _ = res_tx.send(RebuildResponse {
                token: request.token,
                source_epoch: request.source_epoch,
                source_hash: request.source_hash,
                result,
                elapsed_ms,
            });
        }
    }
}

impl Drop for RebuildWorker {
    fn drop(&mut self) {
        // Cancel any in-flight rebuild and close the request channel.
        // The worker thread is deliberately detached: joining here could block
        // app shutdown while a long rebuild is not cancellation-aware.
        self.cancel_source.cancel(self.next_token + 1);
        self.request_tx.take();
        self.handle.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::stores::SourceStore;
    use crate::document::DocumentSession;
    use crate::editor::EditorBuffer;

    #[test]
    fn test_submit_returns_token() {
        let mut worker = RebuildWorker::start();

        let doc =
            DocumentSession::from_source(std::path::PathBuf::from("test.amx"), "#0s\n".to_string())
                .expect("create session");
        let editor = EditorBuffer::new(&doc.file_path, doc.source_text.clone());
        let source = SourceStore::new(doc, editor);

        let token = worker.submit(&source).expect("submit should succeed");
        assert!(token.0 > 0, "submit should return a token with positive value");
    }

    #[test]
    fn submit_reports_error_when_worker_is_not_running() {
        let mut worker = RebuildWorker::start();
        worker.request_tx = None;

        let doc =
            DocumentSession::from_source(std::path::PathBuf::from("test.amx"), "#0s\n".to_string())
                .expect("create session");
        let editor = EditorBuffer::new(&doc.file_path, doc.source_text.clone());
        let source = SourceStore::new(doc, editor);

        assert!(worker.submit(&source).is_err());
    }

    #[test]
    fn test_worker_drop_does_not_hang() {
        let mut worker = RebuildWorker::start();

        let doc =
            DocumentSession::from_source(std::path::PathBuf::from("test.amx"), "#0s\n".to_string())
                .expect("create session");
        let editor = EditorBuffer::new(&doc.file_path, doc.source_text.clone());
        let source = SourceStore::new(doc, editor);

        let _token = worker.submit(&source).expect("submit should succeed");

        // Drop the worker — this should not deadlock
        drop(worker);
    }
}
