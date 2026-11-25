use std::collections::HashMap;
use std::path::Path;

use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use super::{Error, History, MIXES, Result, TAKES};

#[derive(Debug, Deserialize, Serialize)]
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
        let tx = self.read()?;
        let mixes = tx.open_table(MIXES)?;
        let audio = relative(audio);
        let timing = relative(timing);
        Ok(mixes
            .get((audio.as_str(), timing.as_str()))?
            .map(|entry| serde_json::from_slice(entry.value()))
            .transpose()?)
    }

    pub fn store_mix(&self, audio: &Path, timing: &Path, mix: &Mix) -> Result<()> {
        let tx = self.write()?;
        {
            let mut mixes = tx.open_table(MIXES)?;
            let audio = relative(audio);
            let timing = relative(timing);
            let key = (audio.as_str(), timing.as_str());
            if mixes.get(key)?.is_some() {
                return Ok(());
            }
            let takes = tx.open_table(TAKES)?;
            for take in mix.parts.values() {
                if takes.get(take.as_str())?.is_none() {
                    return Err(Error::Invalid("Voiceover mix refers to a missing take."));
                }
            }
            mixes.insert(key, serde_json::to_vec(mix)?.as_slice())?;
        }
        tx.commit()?;
        Ok(())
    }
}
