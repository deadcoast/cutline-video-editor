use super::probe::Media;
use crate::model::Edit;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf, time::UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    source: PathBuf,
    bytes: u64,
    modified: u64,
    edit: Edit,
}
fn location(media: &Media) -> Option<PathBuf> {
    let root = directories::ProjectDirs::from("dev", "MIVIM", "Cutline")?;
    let mut hash = 0xcbf29ce484222325u64;
    for byte in media.path.as_os_str().as_encoded_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Some(
        root.data_local_dir()
            .join("edits")
            .join(format!("{hash:016x}.json")),
    )
}
fn fingerprint(media: &Media) -> Option<(u64, u64)> {
    let m = fs::metadata(&media.path).ok()?;
    Some((
        m.len(),
        m.modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs(),
    ))
}
pub fn restore(media: &Media) -> Option<Edit> {
    let bytes = fs::read(location(media)?).ok()?;
    let saved: Saved = serde_json::from_slice(&bytes).ok()?;
    let (size, modified) = fingerprint(media)?;
    (saved.version == 1
        && saved.source == media.path
        && size == saved.bytes
        && modified == saved.modified
        && saved.edit.valid(media.duration, media.width, media.height))
    .then_some(saved.edit)
}
pub fn save(media: &Media, edit: &Edit) -> Result<()> {
    let path = location(media).context("Windows application data folder is unavailable")?;
    let parent = path.parent().context("Missing edit directory")?;
    fs::create_dir_all(parent)?;
    let (bytes, modified) = fingerprint(media).context("Source file is no longer available")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(
        &mut temp,
        &Saved {
            version: 1,
            source: media.path.clone(),
            bytes,
            modified,
            edit: edit.clone(),
        },
    )?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
