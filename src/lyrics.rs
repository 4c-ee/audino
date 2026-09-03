use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub time: f64,
    pub text: String,
}

pub fn parse_lrc(path: &Path) -> Vec<LyricLine> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut lines = Vec::new();

    for line in content.lines() {
        if line.starts_with('[') {
            if let Some(end_bracket) = line.find(']') {
                let time_str = &line[1..end_bracket];
                let text = &line[end_bracket + 1..];

                if let Some(time) = parse_time(time_str) {
                    lines.push(LyricLine {
                        time,
                        text: text.trim().to_string(),
                    });
                }
            }
        }
    }

    lines.sort_by(|a, b| a.time.total_cmp(&b.time));
    lines
}

fn parse_time(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 2 {
        let mins = parts[0].parse::<f64>().ok()?;
        let secs = parts[1].parse::<f64>().ok()?;
        Some(mins * 60.0 + secs)
    } else {
        None
    }
}

pub fn find_lrc_file(audio_path: &Path) -> Option<PathBuf> {
    let lrc_path = audio_path.with_extension("lrc");
    if lrc_path.exists() {
        Some(lrc_path)
    } else {
        None
    }
}
