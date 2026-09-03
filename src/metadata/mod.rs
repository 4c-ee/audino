pub mod lofty;

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use anyhow::Result;
pub use lofty::{TrackMetadata, extract_metadata, extract_album_art};

/// A cached track's metadata along with the file state it was extracted from.
/// If the file's mtime or size no longer match, the entry (and any extracted
/// album art) is considered stale and re-extracted.
#[derive(Debug, Serialize, Deserialize)]
struct CachedTrack {
    meta: TrackMetadata,
    mtime_secs: u64,
    size: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MetadataCache {
    tracks: HashMap<PathBuf, CachedTrack>,
}

use serde::{Serialize, Deserialize};

pub struct MetadataProvider {
    cache_path: PathBuf,
    art_cache_dir: PathBuf,
    cache: MetadataCache,
    dirty: bool,
    unsaved_changes: usize,
    last_save: std::time::Instant,
}

const SAVE_INTERVAL_MS: u64 = 5000;
const MIN_CHANGES_BEFORE_SAVE: usize = 10;

fn file_mtime_size(path: &Path) -> Option<(u64, u64)> {
    let m = fs::metadata(path).ok()?;
    let mtime_secs = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some((mtime_secs, m.len()))
}

impl MetadataProvider {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let cache_dir = PathBuf::from(home).join(".cache").join("audino");
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir).ok();
        }
        let cache_path = cache_dir.join("metadata.bin");
        let art_cache_dir = cache_dir.join("art");
        if !art_cache_dir.exists() {
            fs::create_dir_all(&art_cache_dir).ok();
        }

        let cache = load_cache(&cache_path);

        Self {
            cache_path,
            art_cache_dir,
            cache,
            dirty: false,
            unsaved_changes: 0,
            last_save: std::time::Instant::now(),
        }
    }

    fn should_save(&self) -> bool {
        if !self.dirty {
            return false;
        }
        let time_elapsed = self.last_save.elapsed().as_millis() as u64;
        self.unsaved_changes >= MIN_CHANGES_BEFORE_SAVE || time_elapsed >= SAVE_INTERVAL_MS
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.dirty {
            self.save_cache()?;
        }
        Ok(())
    }

    /// Drop any cached state (metadata + extracted art) for `path` because the
    /// file changed (or was deleted) since it was cached.
    fn invalidate(&mut self, path: &Path) {
        self.cache.tracks.remove(path);
        let art_path = self.get_album_art_path(path);
        if art_path.exists() {
            fs::remove_file(&art_path).ok();
        }
        self.dirty = true;
        self.unsaved_changes += 1;
    }

    pub fn get_metadata(&mut self, path: &Path) -> Result<Arc<TrackMetadata>> {
        if let Some(entry) = self.cache.tracks.get(path) {
            match file_mtime_size(path) {
                Some(st) if st == (entry.mtime_secs, entry.size) => {
                    return Ok(Arc::new(entry.meta.clone()));
                }
                _ => {
                    // File changed or vanished: discard stale metadata and art.
                    self.invalidate(path);
                }
            }
        }

        let meta = Arc::new(extract_metadata(path)?);
        let st = file_mtime_size(path).unwrap_or((0, 0));
        self.cache.tracks.insert(
            path.to_path_buf(),
            CachedTrack {
                meta: (*meta).clone(),
                mtime_secs: st.0,
                size: st.1,
            },
        );
        self.dirty = true;
        self.unsaved_changes += 1;
        if self.should_save() {
            self.save_cache()?;
        }
        Ok(meta)
    }

    fn save_cache(&mut self) -> Result<()> {
        let bytes = bincode::serialize(&self.cache)?;
        let tmp = self.cache_path.with_extension("bin.tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &self.cache_path)?;
        self.dirty = false;
        self.unsaved_changes = 0;
        self.last_save = std::time::Instant::now();
        Ok(())
    }

    fn get_album_art_path(&self, audio_path: &Path) -> PathBuf {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        audio_path.hash(&mut hasher);
        let hash = hasher.finish();
        self.art_cache_dir.join(format!("{:x}.img", hash))
    }

    pub fn get_or_extract_album_art(&mut self, audio_path: &Path) -> Result<Option<PathBuf>> {
        let art_path = self.get_album_art_path(audio_path);
        if art_path.exists() {
            // Drop cached art if the source file changed since it was cached.
            if let Some(entry) = self.cache.tracks.get(audio_path) {
                if let Some(st) = file_mtime_size(audio_path) {
                    if st != (entry.mtime_secs, entry.size) {
                        fs::remove_file(&art_path).ok();
                    }
                }
            }
            if art_path.exists() {
                return Ok(Some(art_path));
            }
        }

        if let Some(data) = extract_album_art(audio_path)? {
            fs::write(&art_path, data)?;
            return Ok(Some(art_path));
        }

        Ok(None)
    }
}

fn load_cache(path: &Path) -> MetadataCache {
    fs::read(path)
        .ok()
        .and_then(|bytes| bincode::deserialize::<MetadataCache>(&bytes).ok())
        .unwrap_or_default()
}
