use std::fs;
use std::path::Path;

use redb::{ReadTransaction, ReadableTable, WriteTransaction};

use super::mixes::Mix;
use super::{
    ALIGNMENTS, CHUNKS, Error, History, MIXES, Result, STREAMS, Status, TAKES, Take, TakeRecord,
    get_take, put_take,
};

pub(super) const RAW: u8 = 0;
const ALIGNED: u8 = 1;

impl History {
    pub fn start_recording(&self, take: &Take, rate: u32, channels: u16) -> Result<()> {
        let tx = self.write()?;
        Self::add_take(&tx, take, Status::Recording)?;
        add_stream(&tx, &take.id, RAW, rate, channels)?;
        tx.commit()?;
        Ok(())
    }

    pub fn append_recording(&self, id: &str, samples: &[i16], channels: u16) -> Result<()> {
        let tx = self.write()?;
        append(&tx, id, RAW, samples, channels)?;
        tx.commit()?;
        Ok(())
    }

    pub fn finish_recording(&self, id: &str, complete: bool) -> Result<()> {
        let tx = self.write()?;
        let status = if complete {
            Status::Complete
        } else {
            Status::Interrupted
        };
        finish(&tx, id, status)?;
        tx.commit()?;
        Ok(())
    }

    pub(super) fn recover_recordings(&self) -> Result<()> {
        let tx = self.write()?;
        let mut recordings = Vec::new();
        {
            let takes = tx.open_table(TAKES)?;
            for entry in takes.iter()? {
                let (id, row) = entry?;
                let row: TakeRecord = serde_json::from_slice(row.value())?;
                if row.status == Status::Recording {
                    recordings.push(id.value().to_owned());
                }
            }
        }
        if !recordings.is_empty() {
            for id in recordings {
                finish(&tx, &id, Status::Interrupted)?;
            }
            tx.commit()?;
        }
        Ok(())
    }

    pub fn import_wave(&self, take: &Take, path: &Path) -> Result<()> {
        let tx = self.write()?;
        Self::add_take(&tx, take, Status::Complete)?;
        store_wave(&tx, &take.id, RAW, path)?;
        tx.commit()?;
        Ok(())
    }

    pub fn store_alignment(&self, id: &str, candidate: &Path) -> Result<()> {
        let transcript = fs::read(candidate.with_file_name("take.speech.json"))?;
        let receipt = fs::read(candidate)?;
        let tx = self.write()?;
        {
            let mut takes = tx.open_table(TAKES)?;
            let mut row =
                get_take(&takes, id)?.ok_or(Error::Invalid("Voiceover take is missing."))?;
            remove_stream(&tx, id, ALIGNED)?;
            store_wave(&tx, id, ALIGNED, &candidate.with_file_name("take.wav"))?;
            tx.open_table(ALIGNMENTS)?
                .insert(id, (transcript.as_slice(), receipt.as_slice()))?;
            row.issue = None;
            put_take(&mut takes, id, &row)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn candidate(&self, id: &str, selector: &str, directory: &Path) -> Result<()> {
        let tx = self.read()?;
        let alignments = tx.open_table(ALIGNMENTS)?;
        let alignment =
            alignments.get(id)?.ok_or(Error::Invalid("Voiceover alignment is missing."))?;
        let (transcript, receipt) = alignment.value();
        // The worker receipt's selector describes a source projection, while
        // the UUID and exact script snapshot establish which take it is.
        let mut receipt: serde_json::Value = serde_json::from_slice(receipt)?;
        receipt["paragraph"] = selector.into();
        fs::create_dir_all(directory)?;
        fs::write(directory.join("take.json"), serde_json::to_vec(&receipt)?)?;
        fs::write(directory.join("take.speech.json"), transcript)?;
        write_wave(&tx, id, ALIGNED, &directory.join("take.wav"))
    }

    pub fn raw_wave(&self, id: &str, path: &Path) -> Result<()> {
        write_wave(&self.read()?, id, RAW, path)
    }
}

fn finish(tx: &WriteTransaction, id: &str, status: Status) -> Result<()> {
    let mut takes = tx.open_table(TAKES)?;
    let Some(mut row) = get_take(&takes, id)? else {
        return Ok(());
    };
    let has_audio = {
        let streams = tx.open_table(STREAMS)?;
        let mut has_audio = false;
        for role in [RAW, ALIGNED] {
            has_audio |= streams.get((id, role))?.is_some_and(|stream| stream.value().2 > 0);
        }
        has_audio
    };
    if status == Status::Interrupted && !has_audio {
        for entry in tx.open_table(MIXES)?.iter()? {
            let (_, mix) = entry?;
            let mix: Mix = serde_json::from_slice(mix.value())?;
            if mix.parts.values().any(|take| take == id) {
                return Err(Error::Invalid("Voiceover take is referenced by a mix."));
            }
        }
        remove_stream(tx, id, RAW)?;
        remove_stream(tx, id, ALIGNED)?;
        tx.open_table(ALIGNMENTS)?.remove(id)?;
        takes.remove(id)?;
    } else {
        row.status = status;
        put_take(&mut takes, id, &row)?;
    }
    Ok(())
}

fn add_stream(tx: &WriteTransaction, id: &str, role: u8, rate: u32, channels: u16) -> Result<()> {
    if rate == 0 || channels == 0 {
        return Err(Error::Invalid("Invalid voiceover audio format."));
    }
    tx.open_table(STREAMS)?.insert((id, role), (rate, channels, 0))?;
    Ok(())
}

fn remove_stream(tx: &WriteTransaction, id: &str, role: u8) -> Result<()> {
    tx.open_table(STREAMS)?.remove((id, role))?;
    let mut chunks = tx.open_table(CHUNKS)?;
    for entry in chunks.extract_if((id, role, 0)..=(id, role, u64::MAX), |_, _| true)? {
        entry?;
    }
    Ok(())
}

fn append(tx: &WriteTransaction, id: &str, role: u8, samples: &[i16], channels: u16) -> Result<()> {
    if samples.is_empty() {
        return Ok(());
    }
    let mut streams = tx.open_table(STREAMS)?;
    let (rate, stored_channels, frames) = streams
        .get((id, role))?
        .ok_or(Error::Invalid("Voiceover audio is missing."))?
        .value();
    let samples_per_frame = usize::from(channels);
    if channels == 0
        || channels != stored_channels
        || !samples.len().is_multiple_of(samples_per_frame)
    {
        return Err(Error::Invalid("Invalid audio block."));
    }
    let pcm: Vec<_> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    let end = frames
        .checked_add((samples.len() / samples_per_frame) as u64)
        .ok_or(Error::Invalid("Voiceover audio is too long."))?;
    tx.open_table(CHUNKS)?.insert((id, role, frames), pcm.as_slice())?;
    streams.insert((id, role), (rate, channels, end))?;
    Ok(())
}

fn store_wave(tx: &WriteTransaction, id: &str, role: u8, path: &Path) -> Result<()> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.duration() == 0
    {
        return Err(Error::Invalid("Expected nonempty 16-bit PCM audio."));
    }
    add_stream(tx, id, role, spec.sample_rate, spec.channels)?;
    let block = (32768 / usize::from(spec.channels)).max(1) * usize::from(spec.channels);
    let mut samples = reader.samples::<i16>();
    loop {
        let pcm = samples.by_ref().take(block).collect::<std::result::Result<Vec<_>, _>>()?;
        if pcm.is_empty() {
            break;
        }
        append(tx, id, role, &pcm, spec.channels)?;
    }
    Ok(())
}

fn write_wave(tx: &ReadTransaction, id: &str, role: u8, path: &Path) -> Result<()> {
    let streams = tx.open_table(STREAMS)?;
    let (sample_rate, channels, frames) = streams
        .get((id, role))?
        .ok_or(Error::Invalid("Voiceover audio is missing."))?
        .value();
    if sample_rate == 0 || channels == 0 {
        return Err(Error::Invalid("Invalid voiceover audio format."));
    }
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    let chunks = tx.open_table(CHUNKS)?;
    let bytes_per_frame = 2 * usize::from(channels);
    let mut written = 0;
    for entry in chunks.range((id, role, 0)..=(id, role, u64::MAX))? {
        let (key, pcm) = entry?;
        let start = key.value().2;
        let pcm = pcm.value();
        if start != written || pcm.is_empty() || !pcm.len().is_multiple_of(bytes_per_frame) {
            return Err(Error::Invalid("Invalid audio block in voiceover history."));
        }
        for &pair in pcm.as_chunks::<2>().0 {
            writer.write_sample(i16::from_le_bytes(pair))?;
        }
        written += (pcm.len() / bytes_per_frame) as u64;
    }
    if written != frames {
        return Err(Error::Invalid("Incomplete audio in voiceover history."));
    }
    writer.finalize()?;
    Ok(())
}
