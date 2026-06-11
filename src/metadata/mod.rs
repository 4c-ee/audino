pub mod ffmpeg;

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::fs;
use anyhow::Result;
use serde::{Serialize, Deserialize};
pub use ffmpeg::{TrackMetadata, extract_metadata, extract_album_art};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MetadataCache {
    pub tracks: HashMap<String, TrackMetadata>,
}

pub struct MetadataProvider {
    cache_path: PathBuf,
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
        let cache_path = cache_dir.join("metadata.json");

        let cache = if cache_path.exists() {
            fs::read_to_string(&cache_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            MetadataCache::default()
        };

        Self {
            cache_path,
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

    pub fn get_metadata(&mut self, path: &Path) -> Result<TrackMetadata> {
        let path_str = path.to_string_lossy().to_string();
        if let Some(meta) = self.cache.tracks.get(&path_str) {
            return Ok(meta.clone());
        }

        let meta = extract_metadata(path)?;
        self.cache.tracks.insert(path_str, meta.clone());
        self.dirty = true;
        self.unsaved_changes += 1;
        if self.should_save() {
            self.save_cache()?;
        }
        Ok(meta)
    }

    fn save_cache(&mut self) -> Result<()> {
        let s = serde_json::to_string(&self.cache)?;
        fs::write(&self.cache_path, s)?;
        self.dirty = false;
        self.unsaved_changes = 0;
        self.last_save = std::time::Instant::now();
        Ok(())
    }

    pub fn get_album_art_path(&self, audio_path: &Path) -> PathBuf {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        audio_path.hash(&mut hasher);
        let hash = hasher.finish();
        self.cache_path.parent().unwrap().join(format!("{:x}.img", hash))
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
