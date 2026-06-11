use std::path::Path;
use std::sync::OnceLock;
use anyhow::Result;
use serde::{Serialize, Deserialize};
use ffmpeg_next as ffmpeg;

static FFMPEG_INIT: OnceLock<()> = OnceLock::new();

fn init_ffmpeg() {
    FFMPEG_INIT.get_or_init(|| {
        ffmpeg::init().expect("Failed to initialize FFmpeg");
    });
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<String>,
    pub duration: Option<f64>,
}

pub fn extract_metadata(path: &Path) -> Result<TrackMetadata> {
    crate::log(&format!("Extracting metadata for {:?}", path));
    init_ffmpeg();
    let ictx = ffmpeg::format::input(&path)?;
    
    let mut title = None;
    let mut artist = None;
    let mut album = None;
    let mut track_number = None;
    let duration = Some(ictx.duration() as f64 / ffmpeg::ffi::AV_TIME_BASE as f64);

    // Extract from global metadata
    for (k, v) in ictx.metadata().iter() {
        match k.to_lowercase().as_str() {
            "title" if title.is_none() => title = Some(v.to_string()),
            "artist" if artist.is_none() => artist = Some(v.to_string()),
            "album" if album.is_none() => album = Some(v.to_string()),
            "track" | "tracknumber" if track_number.is_none() => track_number = Some(v.to_string()),
            _ => {}
        }
    }

    // Extract from stream metadata if missing
    for stream in ictx.streams() {
        for (k, v) in stream.metadata().iter() {
            match k.to_lowercase().as_str() {
                "title" if title.is_none() => title = Some(v.to_string()),
                "artist" if artist.is_none() => artist = Some(v.to_string()),
                "album" if album.is_none() => album = Some(v.to_string()),
                "track" | "tracknumber" if track_number.is_none() => track_number = Some(v.to_string()),
                _ => {}
            }
        }
    }

    Ok(TrackMetadata {
        title,
        artist,
        album,
        track_number,
        duration,
    })
}

pub fn extract_album_art(path: &Path) -> Result<Option<Vec<u8>>> {
    crate::log(&format!("Extracting album art for {:?}", path));
    init_ffmpeg();
    let mut ictx = ffmpeg::format::input(&path)?;

    let stream_index = ictx
        .streams()
        .find(|s| s.disposition().contains(ffmpeg::format::stream::Disposition::ATTACHED_PIC))
        .map(|s| s.index());

    if let Some(index) = stream_index {
        for (stream, packet) in ictx.packets() {
            if stream.index() == index {
                if let Some(data) = packet.data() {
                    return Ok(Some(data.to_vec()));
                }
            }
        }
    }

    Ok(None)
}
