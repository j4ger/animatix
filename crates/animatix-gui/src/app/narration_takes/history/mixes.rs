use std::collections::HashMap;
use std::path::Path;

use rusqlite::{OptionalExtension, params};

use super::{History, Result};

#[derive(Debug)]
pub(in crate::app::narration_takes) struct Mix {
    pub audio_digest: String,
    pub transcript_digest: String,
    pub layout: Vec<u8>,
    pub parts: HashMap<String, String>,
}

pub(in crate::app::narration_takes) fn relative(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

impl History {
    pub fn mix(&self, audio: &Path, timing: &Path) -> Result<Option<Mix>> {
        let db = self.connect()?;
        let audio = relative(audio);
        let timing = relative(timing);
        let matched = db
            .query_row(
                "SELECT audio_digest, transcript_digest, layout FROM mixes
            WHERE audio = ?1 AND transcript = ?2",
                params![audio, timing],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((audio_digest, transcript_digest, layout)) = matched else {
            return Ok(None);
        };
        let mut statement = db.prepare(
            "SELECT selector, take_id FROM mix_parts
            WHERE audio = ?1 AND transcript = ?2",
        )?;
        let parts = statement
            .query_map(params![audio, timing], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(Some(Mix {
            audio_digest,
            transcript_digest,
            layout,
            parts,
        }))
    }

    pub fn store_mix(&self, audio: &Path, timing: &Path, mix: &Mix) -> Result<()> {
        let mut db = self.write_db()?;
        let tx = db.transaction()?;
        let audio = relative(audio);
        let timing = relative(timing);
        let added = tx.execute(
            "INSERT OR IGNORE INTO mixes VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                audio,
                timing,
                mix.audio_digest,
                mix.transcript_digest,
                mix.layout
            ],
        )?;
        if added > 0 {
            for (selector, take) in &mix.parts {
                tx.execute(
                    "INSERT INTO mix_parts VALUES (?1, ?2, ?3, ?4)",
                    params![audio, timing, selector, take],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
