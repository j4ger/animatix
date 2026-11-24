use std::fs;
use std::path::Path;

use rusqlite::{Connection, params};

use super::{Error, History, Result, Take};

const RAW: i64 = 0;
const ALIGNED: i64 = 1;

impl History {
    pub fn start_recording(&self, take: &Take, rate: u32, channels: u16) -> Result<()> {
        let mut db = self.write_db()?;
        let tx = db.transaction()?;
        Self::add_take(&tx, take, "recording")?;
        add_stream(&tx, &take.id, RAW, rate, channels)?;
        tx.commit()?;
        Ok(())
    }

    pub fn append_recording(&self, id: &str, samples: &[i16], channels: u16) -> Result<()> {
        let mut db = self.write_db()?;
        let tx = db.transaction()?;
        append(&tx, id, RAW, samples, channels)?;
        tx.commit()?;
        Ok(())
    }

    pub fn finish_recording(&self, id: &str, complete: bool) -> Result<()> {
        let db = self.write_db()?;
        if complete {
            db.execute("UPDATE takes SET status = 'complete' WHERE id = ?1", [id])?;
        } else {
            db.execute(
                "DELETE FROM takes WHERE id = ?1 AND NOT EXISTS
                (SELECT 1 FROM audio_streams WHERE take_id = ?1 AND frames > 0)",
                [id],
            )?;
            db.execute("UPDATE takes SET status = 'interrupted' WHERE id = ?1", [id])?;
        }
        Ok(())
    }

    pub fn import_wave(&self, take: &Take, path: &Path) -> Result<()> {
        let mut db = self.write_db()?;
        let tx = db.transaction()?;
        Self::add_take(&tx, take, "complete")?;
        store_wave(&tx, &take.id, RAW, path)?;
        tx.commit()?;
        Ok(())
    }

    pub fn store_alignment(&self, id: &str, candidate: &Path) -> Result<()> {
        let mut db = self.write_db()?;
        let tx = db.transaction()?;
        tx.execute("DELETE FROM audio_streams WHERE take_id = ?1 AND role = 1", [id])?;
        store_wave(&tx, id, ALIGNED, &candidate.with_file_name("take.wav"))?;
        tx.execute(
            "INSERT OR REPLACE INTO alignments VALUES (?1, ?2, ?3)",
            params![
                id,
                fs::read(candidate.with_file_name("take.speech.json"))?,
                fs::read(candidate)?,
            ],
        )?;
        tx.execute("UPDATE takes SET issue = NULL WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn candidate(&self, id: &str, selector: &str, directory: &Path) -> Result<()> {
        let db = self.connect()?;
        let (transcript, receipt): (Vec<u8>, Vec<u8>) = db.query_row(
            "SELECT transcript, receipt FROM alignments WHERE take_id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        // The worker receipt's selector describes a source projection, while
        // the UUID and exact script snapshot establish which take it is.
        let mut receipt: serde_json::Value = serde_json::from_slice(&receipt)?;
        receipt["paragraph"] = selector.into();
        fs::create_dir_all(directory)?;
        fs::write(directory.join("take.json"), serde_json::to_vec(&receipt)?)?;
        fs::write(directory.join("take.speech.json"), transcript)?;
        write_wave(&db, id, ALIGNED, &directory.join("take.wav"))
    }

    pub fn raw_wave(&self, id: &str, path: &Path) -> Result<()> {
        write_wave(&self.connect()?, id, RAW, path)
    }
}

fn add_stream(db: &Connection, id: &str, role: i64, rate: u32, channels: u16) -> Result<()> {
    db.execute(
        "INSERT INTO audio_streams(take_id, role, rate, channels) VALUES (?1, ?2, ?3, ?4)",
        params![id, role, rate, channels],
    )?;
    Ok(())
}

fn append(db: &Connection, id: &str, role: i64, samples: &[i16], channels: u16) -> Result<()> {
    let channels = usize::from(channels);
    if samples.is_empty() {
        return Ok(());
    }
    if channels == 0 || !samples.len().is_multiple_of(channels) {
        return Err(Error::Invalid("Invalid audio block."));
    }
    let pcm: Vec<_> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    db.execute(
        "INSERT INTO audio_chunks SELECT take_id, role, frames, ?3 FROM audio_streams
        WHERE take_id = ?1 AND role = ?2",
        params![id, role, pcm],
    )?;
    db.execute(
        "UPDATE audio_streams SET frames = frames + ?3 WHERE take_id = ?1 AND role = ?2",
        params![id, role, samples.len() / channels],
    )?;
    Ok(())
}

fn store_wave(db: &Connection, id: &str, role: i64, path: &Path) -> Result<()> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.duration() == 0
    {
        return Err(Error::Invalid("Expected nonempty 16-bit PCM audio."));
    }
    add_stream(db, id, role, spec.sample_rate, spec.channels)?;
    let block = (32768 / usize::from(spec.channels)).max(1) * usize::from(spec.channels);
    let mut samples = reader.samples::<i16>();
    loop {
        let pcm = samples.by_ref().take(block).collect::<std::result::Result<Vec<_>, _>>()?;
        if pcm.is_empty() {
            break;
        }
        append(db, id, role, &pcm, spec.channels)?;
    }
    Ok(())
}

fn write_wave(db: &Connection, id: &str, role: i64, path: &Path) -> Result<()> {
    let (sample_rate, channels, frames): (u32, u16, u64) = db.query_row(
        "SELECT rate, channels, frames FROM audio_streams WHERE take_id = ?1 AND role = ?2",
        params![id, role],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    let mut statement = db.prepare(
        "SELECT start_frame, pcm FROM audio_chunks
        WHERE take_id = ?1 AND role = ?2 ORDER BY start_frame",
    )?;
    let mut rows = statement.query(params![id, role])?;
    let mut written = 0;
    while let Some(row) = rows.next()? {
        let start: u64 = row.get(0)?;
        let pcm = row.get_ref(1)?.as_blob().map_err(rusqlite::Error::from)?;
        if start != written
            || pcm.is_empty()
            || !pcm.len().is_multiple_of(2 * usize::from(channels))
        {
            return Err(Error::Invalid("Invalid audio block in voiceover history."));
        }
        for &pair in pcm.as_chunks::<2>().0 {
            writer.write_sample(i16::from_le_bytes(pair))?;
        }
        written += (pcm.len() / (2 * usize::from(channels))) as u64;
    }
    if written != frames {
        return Err(Error::Invalid("Incomplete audio in voiceover history."));
    }
    writer.finalize()?;
    Ok(())
}
