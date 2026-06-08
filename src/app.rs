use std::path::PathBuf;
use crate::player::Player;
use crate::metadata::MetadataProvider;
use crate::lyrics::{self, LyricLine};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Focus {
    Tree,
    Queue,
    Player,
}

pub struct App {
    pub player: Player,
    pub metadata: MetadataProvider,
    pub picker: Picker,
    pub current_album_art: Option<image::DynamicImage>,
    pub current_protocol: Option<Protocol>,
    pub last_area: Option<ratatui::layout::Rect>,
    pub lyrics: Vec<LyricLine>,
    pub running: bool,
    pub library_path: PathBuf,
    pub queue: Vec<PathBuf>,
    pub current_index: Option<usize>,
    pub folder_items: Vec<PathBuf>,
    pub selected_folder_index: usize,
    pub focus: Focus,
    pub selected_queue_index: usize,
    placeholder_img: image::DynamicImage,
}

impl App {
    pub fn new(library_path: PathBuf) -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        
        let placeholder_bytes = include_bytes!("../placeholder.png");
        let placeholder_img = image::load_from_memory(placeholder_bytes)
            .expect("Failed to load embedded placeholder.png");

        let mut app = Self {
            player: Player::new(),
            metadata: MetadataProvider::new(),
            picker,
            current_album_art: None,
            current_protocol: None,
            last_area: None,
            lyrics: Vec::new(),
            running: true,
            library_path: library_path.clone(),
            queue: Vec::new(),
            current_index: None,
            folder_items: Vec::new(),
            selected_folder_index: 0,
            focus: Focus::Tree,
            selected_queue_index: 0,
            placeholder_img,
        };
        app.update_folder_items();
        app
    }

    pub fn add_to_queue(&mut self, path: PathBuf) {
        if path.is_file() {
            self.queue.push(path);
        } else if path.is_dir() {
            let mut items = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&path) {
                for entry in rd.filter_map(|e| e.ok()) {
                    let p = entry.path();
                    if p.is_file() && self.is_audio_file(&p) {
                        items.push(p);
                    }
                }
            }

            // Sort by track number then filename
            let mut meta_items = Vec::new();
            for p in items {
                let meta = self.metadata.get_metadata(&p).ok();
                let track = meta.as_ref().and_then(|m| m.track_number.as_ref())
                    .and_then(|t| t.split('/').next()) // Handle "01/12"
                    .and_then(|t| t.parse::<u32>().ok())
                    .unwrap_or(u32::MAX);
                meta_items.push((track, p));
            }
            meta_items.sort_by(|a, b| {
                match a.0.cmp(&b.0) {
                    std::cmp::Ordering::Equal => a.1.file_name().cmp(&b.1.file_name()),
                    other => other,
                }
            });

            for (_, p) in meta_items {
                self.queue.push(p);
            }
        }
    }

    fn is_audio_file(&self, path: &PathBuf) -> bool {
        path.extension().map_or(false, |ext| {
            let ext = ext.to_string_lossy().to_lowercase();
            ext == "mp3" || ext == "flac" || ext == "ogg" || ext == "m4a" || ext == "opus"
        })
    }

    pub fn update_folder_items(&mut self) {
        self.folder_items = std::fs::read_dir(&self.library_path)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| {
                        let p = e.path();
                        if p.is_dir() || self.is_audio_file(&p) {
                            Some(p)
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .collect()
            })
            .unwrap_or_default();
        self.folder_items.sort();
    }

    pub fn play_index(&mut self, index: usize) {
        crate::log(&format!("App: Attempting to play index {}", index));
        if let Some(path) = self.queue.get(index).cloned() {
            if let Err(e) = self.player.play(&path.to_string_lossy()) {
                crate::log(&format!("App: Error playing file: {:?}", e));
            } else {
                self.current_index = Some(index);
                self.load_track_assets(&path);
            }
        } else {
            crate::log("App: Index not found in queue");
        }
    }

    pub fn tick(&mut self) {
        // Handle auto-play next track
        if self.player.is_empty() {
            if let Some(current) = self.current_index {
                let next = current + 1;
                if next < self.queue.len() {
                    crate::log("App: Auto-playing next track");
                    self.play_index(next);
                } else {
                    self.current_index = None;
                }
            }
        }
    }

    fn load_track_assets(&mut self, path: &PathBuf) {
        // Load album art
        let mut art_img = None;

        // 1. Try embedded art
        if let Ok(Some(art_path)) = self.metadata.get_or_extract_album_art(path) {
            art_img = self.load_image(&art_path);
        }

        // 2. Try external cover files in the same folder
        if art_img.is_none() {
            if let Some(parent) = path.parent() {
                for ext in &["png", "jpg", "webp"] {
                    let cover_path = parent.join(format!("cover.{}", ext));
                    if cover_path.exists() {
                        art_img = self.load_image(&cover_path);
                        if art_img.is_some() {
                            break;
                        }
                    }
                }
            }
        }

        // 3. Fallback to embedded placeholder
        self.current_album_art = Some(art_img.unwrap_or_else(|| self.placeholder_img.clone()));
        self.current_protocol = None;

        // Load lyrics
        if let Some(lrc_path) = lyrics::find_lrc_file(path) {
            self.lyrics = lyrics::parse_lrc(&lrc_path);
        } else {
            self.lyrics = Vec::new();
        }
    }

    fn load_image(&self, path: &PathBuf) -> Option<image::DynamicImage> {
        image::ImageReader::open(path)
            .ok()
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.decode().ok())
    }

    pub fn play_selected_tree(&mut self) {
        if let Some(path) = self.folder_items.get(self.selected_folder_index).cloned() {
            self.add_to_queue(path);
            if self.current_index.is_none() && !self.queue.is_empty() {
                self.play_index(0);
            }
        }
    }

    pub fn enter_directory(&mut self) {
        if let Some(path) = self.folder_items.get(self.selected_folder_index).cloned() {
            if path.is_dir() {
                self.library_path = path;
                self.update_folder_items();
                self.selected_folder_index = 0;
            }
        }
    }

    pub fn go_back(&mut self) {
        if let Some(parent) = self.library_path.parent() {
            self.library_path = parent.to_path_buf();
            self.update_folder_items();
            self.selected_folder_index = 0;
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn next_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tree => Focus::Queue,
            Focus::Queue => Focus::Player,
            Focus::Player => Focus::Tree,
        };
    }
}
