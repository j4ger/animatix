use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use crossbeam_channel::{Receiver, Sender};

use super::FileTreeEntry;

pub(super) fn workspace_root_for(file_path: &Path) -> PathBuf {
    let absolute_path = std::path::absolute(file_path);
    let file_path = absolute_path.as_deref().unwrap_or(file_path);
    for ancestor in file_path.ancestors() {
        if ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }
    file_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

pub(super) fn expand_file_ancestors(
    workspace_root: &Path,
    file_path: &Path,
    expanded_dirs: &mut HashSet<PathBuf>,
) {
    expanded_dirs.extend(
        file_path
            .ancestors()
            .skip(1)
            .take_while(|dir| dir.starts_with(workspace_root))
            .map(Path::to_path_buf),
    );
}

type Repaint = Arc<OnceLock<Box<dyn Fn() + Send + Sync>>>;

#[derive(Default)]
pub(crate) struct FileTreeLoader {
    worker: Option<FileTreeWorker>,
    pending: bool,
    repaint: Repaint,
}

impl FileTreeLoader {
    pub(crate) fn set_repaint(&self, callback: impl Fn() + Send + Sync + 'static) {
        let _ = self.repaint.set(Box::new(callback));
    }

    pub(crate) fn request(
        &mut self,
        root: &Path,
        file: &Path,
        expanded_dirs: &HashSet<PathBuf>,
        refresh: bool,
    ) {
        if self.worker.is_none() {
            self.worker = FileTreeWorker::start(Arc::clone(&self.repaint));
        }
        let Some(worker) = &mut self.worker else {
            return;
        };
        let version = worker.version.fetch_add(1, Ordering::Relaxed) + 1;
        if refresh {
            worker.cache_epoch = version;
        }
        self.pending = worker
            .requests
            .send(TreeWork::Build(TreeRequest {
                version,
                cache_epoch: worker.cache_epoch,
                root: root.to_path_buf(),
                file: file.to_path_buf(),
                expanded_dirs: expanded_dirs.clone(),
            }))
            .is_ok();
    }

    pub(crate) fn retire(&self, tree: Vec<FileTreeEntry>) {
        if !tree.is_empty()
            && let Some(worker) = &self.worker
        {
            // Reclaim the old snapshot on the worker too. A large tree can
            // contain hundreds of thousands of owned paths and names.
            let _ = worker.requests.send(TreeWork::Retire(tree));
        }
    }

    pub(crate) fn pending(&self) -> bool {
        self.pending
    }

    pub(crate) fn poll(&mut self) -> Option<Vec<FileTreeEntry>> {
        if !self.pending {
            return None;
        }
        let worker = self.worker.as_ref()?;
        let version = worker.version.load(Ordering::Relaxed);
        loop {
            match worker.responses.try_recv() {
                Ok((received, tree)) if received == version => {
                    self.pending = false;
                    return Some(tree);
                },
                Ok((_, tree)) => self.retire(tree), // Discard a superseded snapshot.
                Err(crossbeam_channel::TryRecvError::Empty) => return None,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.pending = false;
                    return None;
                },
            }
        }
    }
}

struct TreeRequest {
    version: u64,
    cache_epoch: u64,
    root: PathBuf,
    file: PathBuf,
    expanded_dirs: HashSet<PathBuf>,
}

enum TreeWork {
    Build(TreeRequest),
    Retire(Vec<FileTreeEntry>),
}

impl TreeWork {
    fn into_request(self) -> Option<TreeRequest> {
        match self {
            Self::Build(request) => Some(request),
            Self::Retire(tree) => {
                drop(tree);
                None
            },
        }
    }
}

type TreeResponse = (u64, Vec<FileTreeEntry>);

struct FileTreeWorker {
    requests: Sender<TreeWork>,
    responses: Receiver<TreeResponse>,
    version: Arc<AtomicU64>,
    cache_epoch: u64,
}

impl FileTreeWorker {
    fn start(repaint: Repaint) -> Option<Self> {
        let (requests, request_rx) = crossbeam_channel::unbounded();
        let (response_tx, responses) = crossbeam_channel::unbounded();
        let version = Arc::new(AtomicU64::new(0));
        let latest = Arc::clone(&version);
        if let Err(err) = std::thread::Builder::new()
            .name("animatix-file-tree".into())
            .spawn(move || Self::run(request_rx, response_tx, latest, repaint))
        {
            tracing::error!("Failed to start file tree worker: {err}");
            return None;
        }
        Some(Self {
            requests,
            responses,
            version,
            cache_epoch: 0,
        })
    }

    fn run(
        requests: Receiver<TreeWork>,
        responses: Sender<TreeResponse>,
        latest: Arc<AtomicU64>,
        repaint: Repaint,
    ) {
        let mut cache = DirectoryCache::default();
        let mut workspace = None;
        while let Ok(work) = requests.recv() {
            let first = work.into_request();
            let latest_request = requests.try_iter().filter_map(TreeWork::into_request).last();
            let Some(request) = latest_request.or(first) else {
                continue;
            };
            let key = (request.root.clone(), request.cache_epoch);
            if workspace.as_ref() != Some(&key) {
                cache.directories.clear();
                workspace = Some(key);
            }
            let canceled = || latest.load(Ordering::Relaxed) != request.version;
            if let Some(tree) = cache.build(&request, canceled)
                && !canceled()
            {
                if responses.send((request.version, tree)).is_err() {
                    break;
                }
                if let Some(repaint) = repaint.get() {
                    repaint();
                }
            }
        }
    }
}

impl Drop for FileTreeWorker {
    fn drop(&mut self) {
        // Let an in-flight read stop without waiting for the filesystem on the UI thread.
        self.version.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct DirectoryCache {
    directories: HashMap<PathBuf, CachedDirectory>,
}

struct CachedDirectory {
    modified: SystemTime,
    children: Arc<[DirectoryEntry]>,
}

struct DirectoryEntry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

impl DirectoryCache {
    fn build(
        &mut self,
        request: &TreeRequest,
        canceled: impl Fn() -> bool,
    ) -> Option<Vec<FileTreeEntry>> {
        let mut entries = Vec::new();
        let mut levels = vec![(self.children(&request.root, &canceled)?, 0)];
        while let Some(depth) = levels.len().checked_sub(1) {
            if canceled() {
                return None;
            }
            let (children, next) = &mut levels[depth];
            let Some(child) = children.get(*next) else {
                levels.pop();
                continue;
            };
            *next += 1;
            if child.name.starts_with('.') && !request.file.starts_with(&child.path) {
                continue;
            }
            let expanded = child.is_dir && request.expanded_dirs.contains(&child.path);
            entries.push(FileTreeEntry {
                path: child.path.clone(),
                name: child.name.clone(),
                depth,
                is_dir: child.is_dir,
            });
            if expanded {
                let children = self.children(&child.path, &canceled)?;
                levels.push((children, 0));
            }
        }
        Some(entries)
    }

    fn children(
        &mut self,
        dir: &Path,
        canceled: &impl Fn() -> bool,
    ) -> Option<Arc<[DirectoryEntry]>> {
        if canceled() {
            return None;
        }
        let modified = fs::metadata(dir).and_then(|metadata| metadata.modified()).ok();
        if let Some(cached) = self.directories.get(dir)
            && modified == Some(cached.modified)
        {
            return Some(Arc::clone(&cached.children));
        }

        let children = match read_children(dir, canceled) {
            Ok(Some(children)) => children.into(),
            Ok(None) => return None,
            Err(err) => {
                tracing::debug!("failed to read directory '{}': {}", dir.display(), err);
                self.directories.remove(dir);
                return Some(Arc::default());
            },
        };
        if let Some(modified) = modified {
            self.directories.insert(
                dir.to_path_buf(),
                CachedDirectory {
                    modified,
                    children: Arc::clone(&children),
                },
            );
        }
        Some(children)
    }
}

fn read_children(
    dir: &Path,
    canceled: &impl Fn() -> bool,
) -> std::io::Result<Option<Vec<DirectoryEntry>>> {
    let mut children = Vec::new();
    for entry in fs::read_dir(dir)? {
        if canceled() {
            return Ok(None);
        }
        let Ok(entry) = entry else { continue };
        let is_dir = entry
            .file_type()
            .map(|kind| kind.is_dir())
            .unwrap_or_else(|_| entry.path().is_dir());
        children.push((entry.file_name(), is_dir));
    }
    children.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if canceled() {
        return Ok(None);
    }
    let entries = children
        .into_iter()
        .map(|(name, is_dir)| {
            let path = dir.join(&name);
            let name =
                name.into_string().unwrap_or_else(|name| name.to_string_lossy().into_owned());
            DirectoryEntry { path, name, is_dir }
        })
        .collect();
    Ok(Some(entries))
}

#[cfg(test)]
impl FileTreeLoader {
    pub(crate) fn wait(&mut self) -> Vec<FileTreeEntry> {
        let worker = self.worker.as_ref().expect("file tree worker");
        let version = worker.version.load(Ordering::Relaxed);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let (received, tree) =
                worker.responses.recv_deadline(deadline).expect("file tree response");
            if received == version {
                self.pending = false;
                return tree;
            }
            self.retire(tree);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_do_not_hide_files_in_a_large_directory() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..205 {
            fs::create_dir(root.path().join(format!("folder-{index:03}"))).unwrap();
        }
        let scene = root.path().join("main.amx");
        fs::write(&scene, "#0s\n").unwrap();
        fs::write(root.path().join("asset.svg"), "").unwrap();
        fs::write(root.path().join("folder-000/child.amx"), "#0s\n").unwrap();

        let mut loader = FileTreeLoader::default();
        loader.request(root.path(), &scene, &HashSet::new(), false);
        let tree = loader.wait();
        assert_eq!(tree.len(), 207);
        assert!(tree[..205].iter().all(|entry| entry.is_dir && entry.depth == 0));
        assert_eq!(tree[205].name, "asset.svg");
        assert_eq!(tree[206].path, scene);
    }

    #[test]
    fn latest_workspace_wins_when_an_older_snapshot_is_queued() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let old_file = first.path().join("old.amx");
        let new_file = second.path().join("new.amx");
        fs::write(&old_file, "").unwrap();
        fs::write(&new_file, "").unwrap();

        let (tx, ready) = crossbeam_channel::unbounded();
        let mut loader = FileTreeLoader::default();
        loader.set_repaint(move || {
            let _ = tx.send(());
        });
        loader.request(first.path(), &old_file, &HashSet::new(), false);
        ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();

        loader.request(second.path(), &new_file, &HashSet::new(), true);
        ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let tree = loader.poll().unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].path, new_file);
        assert!(!loader.pending());
    }

    #[test]
    fn refreshing_the_workspace_exposes_new_files_and_removes_deleted_ones() {
        let root = tempfile::tempdir().unwrap();
        let old_file = root.path().join("old.amx");
        let new_file = root.path().join("new.amx");
        fs::write(&old_file, "").unwrap();

        let mut loader = FileTreeLoader::default();
        loader.request(root.path(), &old_file, &HashSet::new(), false);
        assert_eq!(loader.wait()[0].path, old_file);

        fs::remove_file(&old_file).unwrap();
        fs::write(&new_file, "").unwrap();
        loader.request(root.path(), &new_file, &HashSet::new(), true);
        let tree = loader.wait();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].path, new_file);
    }
}
