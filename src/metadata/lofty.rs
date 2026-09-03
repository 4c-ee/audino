use std::path::Path;
use anyhow::Result;
use lofty::file::AudioFile;
use lofty::file::TaggedFileExt;
use lofty::picture::Picture;
use lofty::tag::Accessor;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<String>,
    pub duration: Option<f64>,
}

pub fn extract_metadata(path: &Path) -> Result<TrackMetadata> {
    crate::log!("Extracting metadata for {:?}", path);
    let tagged = lofty::read_from_path(path)?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let (title, artist, album, track_number) = if let Some(tag) = tag {
        let track = tag.track().map(|n| n.to_string());
        (
            tag.title().map(|s| s.into_owned()),
            tag.artist().map(|s| s.into_owned()),
            tag.album().map(|s| s.into_owned()),
            track,
        )
    } else {
        (None, None, None, None)
    };

    let duration = Some(tagged.properties().duration().as_secs_f64());

    Ok(TrackMetadata {
        title,
        artist,
        album,
        track_number,
        duration,
    })
}

pub fn extract_album_art(path: &Path) -> Result<Option<Vec<u8>>> {
    crate::log!("Extracting album art for {:?}", path);
    let tagged = lofty::read_from_path(path)?;
    let tag = match tagged.primary_tag().or_else(|| tagged.first_tag()) {
        Some(t) => t,
        None => return Ok(None),
    };

    let pic: Option<&Picture> = tag
        .pictures()
        .iter()
        .find(|p| matches!(p.pic_type(), lofty::picture::PictureType::CoverFront))
        .or(tag.pictures().first());

    Ok(pic.map(|p| p.data().to_vec()))
}
