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
    /// Asset cache carried from the previous successful build. `None` on the
    /// first build or when the previous build had no renderable timeline.
    pub asset_cache: Option<std::sync::Arc<animatix::timeline::assets::AssetCache>>,
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

    /// Restart the worker thread after a panic, failed initial spawn, or lost
    /// request channel.
    ///
    /// Returns true when a request channel is available. Callers should retry
    /// the submit when this returns false; the previous worker may have died.
    fn ensure_worker(&mut self) -> bool {
        let request_channel_lost = self.request_tx.is_none();
        let thread_finished = self.handle.as_ref().is_some_and(|handle| handle.is_finished());
        if !request_channel_lost && !thread_finished {
            return true;
        }

        // Detach the old handle without joining. A worker that lost its request
        // channel may still be draining in flight work; joining here could block
        // the UI thread while the worker exits.
        self.handle.take();

        let (req_tx, req_rx) = crossbeam_channel::unbounded::<RebuildRequest>();
        let (res_tx, res_rx) = crossbeam_channel::bounded::<RebuildResponse>(4);
        match thread::Builder::new().name("animatix-rebuild".into()).spawn(move || {
            Self::worker_loop(req_rx, res_tx);
        }) {
            Ok(handle) => {
                self.request_tx = Some(req_tx);
                self.response_rx = res_rx;
                self.handle = Some(handle);
                true
            },
            Err(err) => {
                tracing::error!("Failed to restart rebuild worker thread: {err}");
                self.request_tx = None;
                self.response_rx = res_rx;
                self.handle = None;
                false
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
            asset_cache: source.document.asset_cache(),
            cancellation: self.cancel_source.token(),
        };

        if !self.ensure_worker() {
            return Err("rebuild worker is not running".to_string());
        }

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
                let _ = res_tx.send(cancelled_response(request, start));
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
                let _ = res_tx.send(cancelled_response(request, start));
                continue;
            }

            let asset_cache = request.asset_cache.clone();
            let rebuild_result = session.rebuild_with_asset_cache(asset_cache);

            // Check cancellation after rebuild (don't send stale data)
            if request.cancellation.is_cancelled() {
                let _ = res_tx.send(cancelled_response(request, start));
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

fn cancelled_response(request: RebuildRequest, start: Instant) -> RebuildResponse {
    RebuildResponse {
        token: request.token,
        source_epoch: request.source_epoch,
        source_hash: request.source_hash,
        result: Err(RebuildFailure {
            error: "rebuild cancelled".to_string(),
            diagnostics: Vec::new(),
            partial_source_index: None,
        }),
        elapsed_ms: start.elapsed().as_secs_f32() * 1000.0,
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
    fn submit_restarts_worker_when_channel_is_missing() {
        let mut worker = RebuildWorker::start();
        // Simulate the worker channel being closed (old API state).
        worker.request_tx = None;

        let doc =
            DocumentSession::from_source(std::path::PathBuf::from("test.amx"), "#0s\n".to_string())
                .expect("create session");
        let editor = EditorBuffer::new(&doc.file_path, doc.source_text.clone());
        let source = SourceStore::new(doc, editor);

        let token = worker.submit(&source).expect("submit should restart worker");
        assert!(token.0 > 0, "submit should return a token after restart");
    }

    #[test]
    fn cancelled_request_receives_cancelled_response() {
        let (req_tx, req_rx) = crossbeam_channel::unbounded::<RebuildRequest>();
        let (res_tx, res_rx) = crossbeam_channel::bounded::<RebuildResponse>(4);
        let handle = std::thread::spawn(move || RebuildWorker::worker_loop(req_rx, res_tx));

        let cancel_source = CancellationSource::new();
        let token = cancel_source.token();
        cancel_source.cancel(1);
        req_tx
            .send(RebuildRequest {
                token: RebuildToken(7),
                source_epoch: SourceEpoch(2),
                source_hash: SourceHash(99),
                file_path: std::path::PathBuf::from("test.amx"),
                source_text: "#0s\n".to_string(),
                asset_cache: None,
                cancellation: token,
            })
            .expect("send request");

        let response = res_rx.recv().expect("cancelled request should produce a response");
        assert_eq!(response.token, RebuildToken(7));
        match response.result {
            Err(failure) => {
                assert_eq!(failure.error, "rebuild cancelled");
            },
            Ok(_) => panic!("cancelled request should report cancellation"),
        }

        drop(req_tx);
        handle.join().expect("worker loop should exit");
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
