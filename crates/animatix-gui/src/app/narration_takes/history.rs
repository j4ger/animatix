mod audio;
mod mixes;
#[cfg(test)]
mod tests;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use sha2::{Digest, Sha256};

pub(super) use self::mixes::Mix;
use super::bindings::Binding;

const APPLICATION_ID: i64 = 0x414D584E;
const VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("{0}")]
    Sql(#[from] rusqlite::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Wave(#[from] hound::Error),
    #[error("{0}")]
    Invalid(&'static str),
}

type Result<T> = std::result::Result<T, Error>;

pub(super) struct History {
    path: PathBuf,
    writer: Option<File>,
}

#[derive(Clone)]
pub(super) struct Take {
    pub id: String,
    pub paragraph: String,
    pub scene: Option<String>,
    pub text: String,
    pub kind: &'static str,
}

pub(super) struct TakeSummary {
    pub id: String,
    pub paragraph: String,
    pub scene: Option<String>,
    pub text: String,
    pub kind: String,
    pub status: String,
    pub issue: Option<String>,
    pub aligned: bool,
    pub seconds: f64,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn path_for(source: &Path) -> PathBuf {
    source.with_extension("amxn")
}

impl History {
    pub fn open(source: &Path, create: bool) -> Result<Option<Self>> {
        let source_parent = fs::canonicalize(
            source
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        let name = source.file_name().ok_or(Error::Invalid("Save the scene before recording."))?;
        let path = path_for(&source_parent.join(name));
        if !create && !path.try_exists()? {
            return Ok(None);
        }
        let writer = Self::lease(&path)?;
        if !path.try_exists()? {
            if writer.is_none() {
                return Err(Error::Invalid(
                    "This voiceover is open for editing in another window.",
                ));
            }
            let file = tempfile::NamedTempFile::new_in(&source_parent)?;
            let mut db = Connection::open(file.path())?;
            let transaction = db.transaction()?;
            transaction.execute_batch(include_str!("history/schema.sql"))?;
            transaction.commit()?;
            db.close().map_err(|(_, error)| error)?;
            file.as_file().sync_all()?;
            file.persist_noclobber(&path).map_err(|error| error.error)?;
        }
        let history = Self { path, writer };
        let db = history.connect()?;
        history.check_version(&db)?;
        if history.is_writable() {
            db.execute(
                "DELETE FROM takes WHERE status = 'recording' AND NOT EXISTS
                (SELECT 1 FROM audio_streams WHERE take_id = takes.id AND frames > 0)",
                [],
            )?;
            db.execute("UPDATE takes SET status = 'interrupted' WHERE status = 'recording'", [])?;
        }
        Ok(Some(history))
    }

    pub fn lease(path: &Path) -> Result<Option<File>> {
        let dirs = directories::ProjectDirs::from("dev", "animatix", "animatix")
            .ok_or(Error::Invalid("Could not find the Animatix cache directory."))?;
        let directory = dirs.cache_dir().join("narration-locks");
        fs::create_dir_all(&directory)?;
        let key = digest(path.as_os_str().as_encoded_bytes());
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join(key))?;
        match lock.try_lock() {
            Ok(()) => Ok(Some(lock)),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub fn read_only(source: &Path) -> Result<Option<Self>> {
        let path = path_for(source);
        if !path.try_exists()? {
            return Ok(None);
        }
        let history = Self { path, writer: None };
        history.check_version(&history.connect()?)?;
        Ok(Some(history))
    }

    fn check_version(&self, db: &Connection) -> Result<()> {
        let application: i64 = db.pragma_query_value(None, "application_id", |row| row.get(0))?;
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if application != APPLICATION_ID || version != VERSION {
            return Err(Error::Invalid("This voiceover history has an unsupported format."));
        }
        Ok(())
    }

    fn connect(&self) -> Result<Connection> {
        let flags = if self.is_writable() {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let db = Connection::open_with_flags(&self.path, flags)?;
        db.busy_timeout(Duration::from_millis(500))?;
        db.pragma_update(None, "foreign_keys", true)?;
        db.pragma_update(None, "trusted_schema", false)?;
        if self.is_writable() {
            db.pragma_update(None, "synchronous", "FULL")?;
        }
        Ok(db)
    }

    fn write_db(&self) -> Result<Connection> {
        if !self.is_writable() {
            return Err(Error::Invalid("This voiceover is open for editing in another window."));
        }
        self.connect()
    }

    pub fn is_writable(&self) -> bool {
        self.writer.is_some()
    }

    pub fn snapshot(&self, source: &str, rows: &[Binding], saved: bool) -> Result<()> {
        let digest = digest(source.as_bytes());
        let mut db = self.write_db()?;
        let tx = db.transaction()?;
        if saved {
            tx.execute("UPDATE source_snapshots SET saved = 0 WHERE saved = 1", [])?;
        }
        tx.execute(
            "INSERT INTO source_snapshots VALUES (?1, ?2)
            ON CONFLICT(digest) DO UPDATE SET saved = max(saved, excluded.saved)",
            params![digest, saved],
        )?;
        tx.execute("DELETE FROM paragraph_bindings WHERE snapshot = ?1", [&digest])?;
        {
            let mut statement =
                tx.prepare("INSERT INTO paragraph_bindings VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            for (ordinal, row) in rows.iter().enumerate() {
                statement.execute(params![
                    digest,
                    ordinal,
                    row.scene,
                    row.selector,
                    row.text,
                    row.id
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn bindings(&self, source: &str) -> Result<(bool, Vec<Binding>)> {
        let db = self.connect()?;
        let hash = digest(source.as_bytes());
        let matched = db
            .query_row("SELECT 1 FROM source_snapshots WHERE digest = ?1", [&hash], |_| Ok(()))
            .optional()?
            .is_some();
        let mut statement = db.prepare(
            "SELECT scene, selector, text, paragraph FROM paragraph_bindings
            WHERE snapshot = (SELECT digest FROM source_snapshots WHERE digest = ?1 OR saved = 1
                              ORDER BY digest = ?1 DESC LIMIT 1) ORDER BY ordinal",
        )?;
        let rows = statement
            .query_map([hash], |row| {
                Ok(Binding {
                    scene: row.get(0)?,
                    selector: row.get(1)?,
                    text: row.get(2)?,
                    id: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((matched, rows))
    }

    fn add_take(tx: &Connection, take: &Take, status: &str) -> Result<()> {
        let time =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        tx.execute(
            "INSERT INTO takes VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
            params![
                take.id,
                take.paragraph,
                take.scene,
                take.text,
                take.kind,
                status,
                time
            ],
        )?;
        Ok(())
    }

    pub fn issue(&self, id: &str, issue: Option<&str>) -> Result<()> {
        self.write_db()?
            .execute("UPDATE takes SET issue = ?2 WHERE id = ?1", params![id, issue])?;
        Ok(())
    }

    pub fn takes(&self) -> Result<Vec<TakeSummary>> {
        let db = self.connect()?;
        let mut statement = db.prepare(
            "SELECT takes.id, paragraph, scene, text, kind, status, issue,
                EXISTS(SELECT 1 FROM alignments WHERE take_id = takes.id),
                CAST(audio_streams.frames AS REAL) / audio_streams.rate
            FROM takes JOIN audio_streams ON audio_streams.take_id = takes.id AND role = 0
            WHERE audio_streams.frames > 0 AND status != 'recording'
            ORDER BY created_ms DESC, takes.rowid DESC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(TakeSummary {
                    id: row.get(0)?,
                    paragraph: row.get(1)?,
                    scene: row.get(2)?,
                    text: row.get(3)?,
                    kind: row.get(4)?,
                    status: row.get(5)?,
                    issue: row.get(6)?,
                    aligned: row.get(7)?,
                    seconds: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn backup(&self, destination: &Path) -> Result<()> {
        let from = self.connect()?;
        let mut to = Connection::open(destination)?;
        let backup = rusqlite::backup::Backup::new(&from, &mut to)?;
        backup.run_to_completion(64, Duration::from_millis(10), None)?;
        drop(backup);
        to.close().map_err(|(_, error)| error)?;
        File::open(destination)?.sync_all()?;
        Ok(())
    }
}
