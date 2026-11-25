mod audio;
mod mixes;
#[cfg(test)]
mod tests;

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use redb::{
    Builder, ConcurrencyMode, Database, Key, ReadOnlyDatabase, ReadTransaction, ReadableDatabase,
    ReadableTable, Table, TableDefinition, Value, WriteTransaction,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(super) use self::mixes::Mix;
use super::bindings::Binding;

const VERSION: u64 = 1;
const UNSUPPORTED: &str = "This voiceover history has an unsupported format.";
const IN_USE: &str = "This voiceover is open for editing in another window.";

// The singleton stores the format version, saved source digest, and last take sequence.
const STATE: TableDefinition<(), (u64, &str, u64)> =
    TableDefinition::new("animatix_narration_state");
const SNAPSHOTS: TableDefinition<&str, &[u8]> = TableDefinition::new("source_snapshots");
const TAKES: TableDefinition<&str, &[u8]> = TableDefinition::new("takes");
const STREAMS: TableDefinition<(&str, u8), (u32, u16, u64)> = TableDefinition::new("audio_streams");
const CHUNKS: TableDefinition<(&str, u8, u64), &[u8]> = TableDefinition::new("audio_chunks");
const ALIGNMENTS: TableDefinition<&str, (&[u8], &[u8])> = TableDefinition::new("alignments");
const MIXES: TableDefinition<(&str, &str), &[u8]> = TableDefinition::new("mixes");

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error(transparent)]
    Database(#[from] redb::DatabaseError),
    #[error(transparent)]
    Transaction(#[from] redb::TransactionError),
    #[error(transparent)]
    Table(#[from] redb::TableError),
    #[error(transparent)]
    Storage(#[from] redb::StorageError),
    #[error(transparent)]
    Commit(#[from] redb::CommitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Wave(#[from] hound::Error),
    #[error("{0}")]
    Invalid(&'static str),
}

type Result<T> = std::result::Result<T, Error>;

pub(super) enum History {
    Writable { db: Database, _lease: File },
    ReadOnly(ReadOnlyDatabase),
}

#[derive(Clone)]
pub(super) struct Take {
    pub id: String,
    pub paragraph: String,
    pub scene: Option<String>,
    pub text: String,
    pub kind: &'static str,
}

#[derive(Clone, Copy, PartialEq, Deserialize, Serialize)]
enum Status {
    Recording,
    Complete,
    Interrupted,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Complete => "complete",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Deserialize, Serialize)]
struct TakeRecord {
    paragraph: String,
    scene: Option<String>,
    text: String,
    kind: String,
    status: Status,
    sequence: u64,
    issue: Option<String>,
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
        let source_parent = fs::canonicalize(parent(source))?;
        let name = source.file_name().ok_or(Error::Invalid("Save the scene before recording."))?;
        let path = path_for(&source_parent.join(name));
        if !create && !path.try_exists()? {
            return Ok(None);
        }
        let Some(lease) = Self::lease(&path)? else {
            let history = Self::open_read_only(&path)?.ok_or(Error::Invalid(IN_USE))?;
            return Ok(Some(history));
        };
        if path.try_exists()? {
            // Inspect the application format before opening for writes. When redb
            // reports an interrupted commit, let the writer recover the database.
            match Self::open_read_only(&path) {
                Ok(_) | Err(Error::Database(redb::DatabaseError::RepairAborted)) => {},
                Err(error) => return Err(error),
            }
        } else {
            if !create {
                return Ok(None);
            }
            install(&path, |tx| {
                tx.open_table(STATE)?.insert((), (VERSION, "", 0))?;
                tx.open_table(SNAPSHOTS)?;
                tx.open_table(TAKES)?;
                tx.open_table(STREAMS)?;
                tx.open_table(CHUNKS)?;
                tx.open_table(ALIGNMENTS)?;
                tx.open_table(MIXES)?;
                Ok(())
            })?;
        }
        let history = Self::Writable {
            db: builder().open(path)?,
            _lease: lease,
        };
        history.check_version()?;
        history.recover_recordings()?;
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
        Self::open_read_only(&path_for(source))
    }

    fn open_read_only(path: &Path) -> Result<Option<Self>> {
        if !path.try_exists()? {
            return Ok(None);
        }
        let history = Self::ReadOnly(builder().open_read_only(path)?);
        history.check_version()?;
        Ok(Some(history))
    }

    fn check_version(&self) -> Result<()> {
        let tx = self.read()?;
        let state = match tx.open_table(STATE) {
            Ok(state) => state,
            Err(redb::TableError::TableDoesNotExist(_)) => return Err(Error::Invalid(UNSUPPORTED)),
            Err(error) => return Err(error.into()),
        };
        if state.get(())?.is_some_and(|entry| entry.value().0 == VERSION) {
            Ok(())
        } else {
            Err(Error::Invalid(UNSUPPORTED))
        }
    }

    fn read(&self) -> Result<ReadTransaction> {
        Ok(match self {
            Self::Writable { db, .. } => db.begin_read()?,
            Self::ReadOnly(db) => db.begin_read()?,
        })
    }

    fn write(&self) -> Result<WriteTransaction> {
        match self {
            Self::Writable { db, .. } => Ok(db.begin_write()?),
            Self::ReadOnly(_) => Err(Error::Invalid(IN_USE)),
        }
    }

    pub fn is_writable(&self) -> bool {
        matches!(self, Self::Writable { .. })
    }

    pub fn snapshot(&self, source: &str, rows: &[Binding], saved: bool) -> Result<()> {
        let hash = digest(source.as_bytes());
        let bytes = serde_json::to_vec(rows)?;
        let tx = self.write()?;
        tx.open_table(SNAPSHOTS)?.insert(hash.as_str(), bytes.as_slice())?;
        if saved {
            let mut state = tx.open_table(STATE)?;
            let (_, _, sequence) = state.get(())?.ok_or(Error::Invalid(UNSUPPORTED))?.value();
            state.insert((), (VERSION, hash.as_str(), sequence))?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn bindings(&self, source: &str) -> Result<(bool, Vec<Binding>)> {
        let tx = self.read()?;
        let snapshots = tx.open_table(SNAPSHOTS)?;
        if let Some(exact) = snapshots.get(digest(source.as_bytes()).as_str())? {
            return Ok((true, serde_json::from_slice(exact.value())?));
        }
        let state = tx.open_table(STATE)?;
        let entry = state.get(())?.ok_or(Error::Invalid(UNSUPPORTED))?;
        let (_, saved, _) = entry.value();
        let rows = snapshots
            .get(saved)?
            .map(|rows| serde_json::from_slice(rows.value()))
            .transpose()?
            .unwrap_or_default();
        Ok((false, rows))
    }

    fn add_take(tx: &WriteTransaction, take: &Take, status: Status) -> Result<()> {
        let mut takes = tx.open_table(TAKES)?;
        if takes.get(take.id.as_str())?.is_some() {
            return Err(Error::Invalid("This voiceover take already exists."));
        }
        if !matches!(take.kind, "recorded" | "imported" | "synthesized") {
            return Err(Error::Invalid("Invalid voiceover take kind."));
        }
        let mut state = tx.open_table(STATE)?;
        let entry = state.get(())?.ok_or(Error::Invalid(UNSUPPORTED))?;
        let (_, saved, sequence) = entry.value();
        let saved = saved.to_owned();
        drop(entry);
        let sequence =
            sequence.checked_add(1).ok_or(Error::Invalid("Too many voiceover takes."))?;
        state.insert((), (VERSION, saved.as_str(), sequence))?;
        let row = TakeRecord {
            paragraph: take.paragraph.clone(),
            scene: take.scene.clone(),
            text: take.text.clone(),
            kind: take.kind.into(),
            status,
            sequence,
            issue: None,
        };
        put_take(&mut takes, &take.id, &row)
    }

    pub fn issue(&self, id: &str, issue: Option<&str>) -> Result<()> {
        let tx = self.write()?;
        {
            let mut takes = tx.open_table(TAKES)?;
            if let Some(mut row) = get_take(&takes, id)? {
                row.issue = issue.map(str::to_owned);
                put_take(&mut takes, id, &row)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn takes(&self) -> Result<Vec<TakeSummary>> {
        let tx = self.read()?;
        let takes = tx.open_table(TAKES)?;
        let streams = tx.open_table(STREAMS)?;
        let alignments = tx.open_table(ALIGNMENTS)?;
        let mut summaries = Vec::new();
        for entry in takes.iter()? {
            let (id, row) = entry?;
            let id = id.value();
            let Some(stream) = streams.get((id, audio::RAW))? else {
                continue;
            };
            let (rate, _, frames) = stream.value();
            let row: TakeRecord = serde_json::from_slice(row.value())?;
            if frames > 0 && row.status != Status::Recording {
                summaries.push((
                    row.sequence,
                    TakeSummary {
                        id: id.to_owned(),
                        paragraph: row.paragraph,
                        scene: row.scene,
                        text: row.text,
                        kind: row.kind,
                        status: row.status.as_str().into(),
                        issue: row.issue,
                        aligned: alignments.get(id)?.is_some(),
                        seconds: frames as f64 / f64::from(rate),
                    },
                ));
            }
        }
        summaries.sort_unstable_by_key(|(sequence, _)| std::cmp::Reverse(*sequence));
        Ok(summaries.into_iter().map(|(_, summary)| summary).collect())
    }

    pub fn backup(&self, destination: &Path) -> Result<()> {
        let read = self.read()?;
        install(destination, |write| {
            copy_table(&read, write, STATE)?;
            copy_table(&read, write, SNAPSHOTS)?;
            copy_table(&read, write, TAKES)?;
            copy_table(&read, write, STREAMS)?;
            copy_table(&read, write, CHUNKS)?;
            copy_table(&read, write, ALIGNMENTS)?;
            copy_table(&read, write, MIXES)?;
            Ok(())
        })
    }
}

fn get_take(table: &Table<'_, &str, &[u8]>, id: &str) -> Result<Option<TakeRecord>> {
    Ok(table.get(id)?.map(|row| serde_json::from_slice(row.value())).transpose()?)
}

fn put_take(table: &mut Table<'_, &str, &[u8]>, id: &str, row: &TakeRecord) -> Result<()> {
    table.insert(id, serde_json::to_vec(row)?.as_slice())?;
    Ok(())
}

fn builder() -> Builder {
    let mut builder = Database::builder();
    // The project lease limits editing to one window. Other windows and Save As
    // can still read committed snapshots from the open database.
    builder.set_concurrency_mode(ConcurrencyMode::SingleWriter);
    builder
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn install(path: &Path, populate: impl FnOnce(&WriteTransaction) -> Result<()>) -> Result<()> {
    let file = tempfile::NamedTempFile::new_in(parent(path))?;
    let db = builder().create(file.path())?;
    let tx = db.begin_write()?;
    populate(&tx)?;
    tx.commit()?;
    db.close()?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|error| error.error)?;
    Ok(())
}

fn copy_table<K: Key + 'static, V: Value + 'static>(
    read: &ReadTransaction,
    write: &WriteTransaction,
    table: TableDefinition<'_, K, V>,
) -> Result<()> {
    let source = read.open_table(table)?;
    let mut destination = write.open_table(table)?;
    for entry in source.iter()? {
        let (key, value) = entry?;
        destination.insert(key.value(), value.value())?;
    }
    Ok(())
}
