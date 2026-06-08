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
}

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

        Self { cache_path, cache }
    }

    pub fn get_metadata(&mut self, path: &Path) -> Result<TrackMetadata> {
        let path_str = path.to_string_lossy().to_string();
        if let Some(meta) = self.cache.tracks.get(&path_str) {
            return Ok(meta.clone());
        }

        let meta = extract_metadata(path)?;
        self.cache.tracks.insert(path_str, meta.clone());
        self.save_cache()?;
        Ok(meta)
    }

    fn save_cache(&self) -> Result<()> {
        let s = serde_json::to_string(&self.cache)?;
        fs::write(&self.cache_path, s)?;
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
