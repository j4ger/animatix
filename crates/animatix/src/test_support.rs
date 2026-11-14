//! Shared support for engine unit tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A uniquely named temporary directory removed when the test fixture drops.
pub(crate) struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    /// Create a unique directory beneath the operating system's temp root.
    pub(crate) fn new(label: &str) -> Self {
        Self::new_in(&std::env::temp_dir(), label)
    }

    /// Create a unique directory beneath `root` and remove only that directory
    /// when the fixture drops. Relative roots remain relative for path tests.
    pub(crate) fn new_in(root: &Path, label: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(root)
            .unwrap_or_else(|error| panic!("create test temp root {}: {error}", root.display()));
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();

        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                "animatix_test_{label}_{}_{}_{}",
                std::process::id(),
                timestamp,
                id
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create test directory {}: {error}", path.display()),
            }
        }
    }

    /// Return the fixture directory path.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
