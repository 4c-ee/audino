pub mod lofty;

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use anyhow::Result;
pub use lofty::{TrackMetadata, extract_metadata, extract_album_art};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MetadataCache {
    pub tracks: HashMap<PathBuf, TrackMetadata>,
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

    pub fn get_metadata(&mut self, path: &Path) -> Result<Arc<TrackMetadata>> {
        if let Some(meta) = self.cache.tracks.get(path) {
            return Ok(Arc::new(meta.clone()));
        }

        let meta = Arc::new(extract_metadata(path)?);
        self.cache.tracks.insert(path.to_path_buf(), (*meta).clone());
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

    pub fn get_or_extract_album_art(&self, audio_path: &Path) -> Result<Option<PathBuf>> {
        let art_path = self.get_album_art_path(audio_path);
        if art_path.exists() {
            return Ok(Some(art_path));
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
