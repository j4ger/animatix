use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::history::digest;

pub(super) type Pair = (PathBuf, PathBuf);

#[derive(PartialEq, Eq)]
pub(super) struct PairStamp {
    pair: Pair,
    audio: (u64, std::time::SystemTime),
    transcript: (u64, std::time::SystemTime),
}

impl PairStamp {
    pub fn read(pair: &Pair) -> io::Result<Self> {
        let stamp = |path: &Path| {
            let metadata = fs::metadata(path)?;
            Ok::<_, io::Error>((metadata.len(), metadata.modified()?))
        };
        Ok(Self {
            pair: pair.clone(),
            audio: stamp(&pair.0)?,
            transcript: stamp(&pair.1)?,
        })
    }
}

pub(super) fn fingerprint(pair: &Pair) -> io::Result<(String, String)> {
    Ok((hash_file(&pair.0)?, hash_file(&pair.1)?))
}

pub(super) fn publish(
    folder: &Path,
    name: &str,
    pair: &Pair,
) -> io::Result<(Pair, (String, String))> {
    let hashes = fingerprint(pair)?;
    let hash = digest(format!("{}{}", hashes.0, hashes.1).as_bytes());
    let folder = folder.join("narration");
    fs::create_dir_all(&folder)?;
    let name = slug(name);
    for length in 8..=hash.len() {
        let stem = format!("{}-{}", name, &hash[..length]);
        let paths =
            (folder.join(format!("{stem}.wav")), folder.join(format!("{stem}.speech.json")));
        if !can_install(&paths.0, &hashes.0)? || !can_install(&paths.1, &hashes.1)? {
            continue;
        }
        if install(&pair.0, &paths.0, &hashes.0)? && install(&pair.1, &paths.1, &hashes.1)? {
            #[cfg(unix)]
            {
                File::open(&folder)?.sync_all()?;
                if let Some(parent) = folder.parent() {
                    File::open(parent)?.sync_all()?;
                }
            }
            return Ok((paths, hashes));
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "Conflicting narration asset."))
}

pub(super) fn copy_pair(source: &Pair, destination: &Pair) -> io::Result<()> {
    let (audio, timing) = fingerprint(source)?;
    if let Some(parent) = destination.0.parent() {
        fs::create_dir_all(parent)?;
    }
    if !can_install(&destination.0, &audio)?
        || !can_install(&destination.1, &timing)?
        || !install(&source.0, &destination.0, &audio)?
        || !install(&source.1, &destination.1, &timing)?
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A narration file with a different content already exists in the destination.",
        ));
    }
    #[cfg(unix)]
    if let Some(parent) = destination.0.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn can_install(path: &Path, digest: &str) -> io::Result<bool> {
    match hash_file(path) {
        Ok(hash) => Ok(hash == digest),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

fn install(source: &Path, destination: &Path, digest: &str) -> io::Result<bool> {
    if destination.try_exists()? {
        return Ok(hash_file(destination)? == digest);
    }
    let mut file = tempfile::NamedTempFile::new_in(destination.parent().unwrap_or(Path::new(".")))?;
    io::copy(&mut File::open(source)?, &mut file)?;
    file.as_file().sync_all()?;
    let result = file.persist_noclobber(destination);
    match result {
        Ok(_) => Ok(true),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            Ok(hash_file(destination)? == digest)
        },
        Err(error) => Err(error.error),
    }
}

fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 32 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn slug(text: &str) -> String {
    let text: String = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let parts = text.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>();
    let slug = parts.join("-");
    if slug.is_empty() {
        "scene".into()
    } else {
        slug.chars().take(32).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_a_flat_immutable_pair_and_extends_a_collision() {
        let dir = tempfile::tempdir().unwrap();
        let input = (dir.path().join("raw.wav"), dir.path().join("raw.json"));
        fs::write(&input.0, b"audio").unwrap();
        fs::write(&input.1, b"timing").unwrap();
        let (pair, _) = publish(dir.path(), "Scene one", &input).unwrap();
        let name = pair.0.file_name().unwrap().to_str().unwrap();
        assert_eq!(name.len(), "scene-one-12345678.wav".len());
        assert_eq!(publish(dir.path(), "Scene one", &input).unwrap().0, pair);
        fs::write(&pair.1, b"other timing").unwrap();
        let (second, _) = publish(dir.path(), "Scene one", &input).unwrap();
        assert_ne!(second, pair);
        assert_eq!(fs::read(&pair.1).unwrap(), b"other timing");
    }
}
