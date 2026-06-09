use std::path::PathBuf;
use crate::player::Player;
use crate::metadata::MetadataProvider;
use crate::lyrics::{self, LyricLine};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use ratatui::widgets::ListState;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Focus {
    Tree,
    Queue,
    QueueControls,
    Player,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SortMethod {
    Track,
    Artist,
    Album,
    Title,
    Added,
}

#[derive(Debug, PartialEq, Clone)]
pub struct QueueItem {
    pub path: PathBuf,
    pub id: usize, // Original addition order
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
    pub queue: Vec<QueueItem>,
    pub next_id: usize,
    pub sort_method: SortMethod,
    pub current_index: Option<usize>,
    pub folder_items: Vec<PathBuf>,
    pub selected_folder_index: usize,
    pub folder_tree_state: ListState,
    pub focus: Focus,
    pub selected_queue_index: usize,
    pub is_moving_track: bool,
    pub selected_control_index: usize,
    pub queue_state: ListState,
    pub show_hidden: bool,
    pub is_searching: bool,
    pub search_query: String,
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
            next_id: 0,
            sort_method: SortMethod::Added,
            current_index: None,
            folder_items: Vec::new(),
            selected_folder_index: 0,
            folder_tree_state: ListState::default(),
            focus: Focus::Tree,
            selected_queue_index: 0,
            is_moving_track: false,
            selected_control_index: 0,
            queue_state: ListState::default(),
            show_hidden: false,
            is_searching: false,
            search_query: String::new(),
            placeholder_img,
        };
        app.update_folder_items();
        app.folder_tree_state.select(Some(0));
        app.queue_state.select(Some(0));
        app
    }

    pub fn add_to_queue(&mut self, path: PathBuf) {
        if path.is_file() {
            self.queue.push(QueueItem { path, id: self.next_id });
            self.next_id += 1;
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
                    std::cmp::Ordering::Equal => {
                        let a_name = a.1.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                        let b_name = b.1.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                        a_name.cmp(&b_name)
                    },
                    other => other,
                }
            });

            for (_, p) in meta_items {
                self.queue.push(QueueItem { path: p, id: self.next_id });
                self.next_id += 1;
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
                        if !self.show_hidden {
                            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                                if name.starts_with('.') {
                                    return None;
                                }
                            }
                        }
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
        self.folder_items.sort_by(|a, b| {
            let a_str = a.to_string_lossy().to_lowercase();
            let b_str = b.to_string_lossy().to_lowercase();
            a_str.cmp(&b_str)
        });
    }

    pub fn play_index(&mut self, index: usize) {
        crate::log(&format!("App: Attempting to play index {}", index));
        if let Some(item) = self.queue.get(index) {
            let path = item.path.clone();
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
            let old_len = self.queue.len();
            self.add_to_queue(path);
            if self.current_index.is_none() && self.queue.len() > old_len {
                self.play_index(old_len);
            }
        }
    }

    pub fn move_up(&mut self) {
        match self.focus {
            Focus::Tree => {
                if self.selected_folder_index > 0 {
                    self.selected_folder_index -= 1;
                    self.folder_tree_state.select(Some(self.selected_folder_index));
                }
            }
            Focus::Queue => {
                if self.selected_queue_index > 0 {
                    if self.is_moving_track {
                        self.queue.swap(self.selected_queue_index, self.selected_queue_index - 1);
                        // Update current_index if it was swapped
                        if Some(self.selected_queue_index) == self.current_index {
                            self.current_index = Some(self.selected_queue_index - 1);
                        } else if Some(self.selected_queue_index - 1) == self.current_index {
                            self.current_index = Some(self.selected_queue_index);
                        }
                    }
                    self.selected_queue_index -= 1;
                    self.queue_state.select(Some(self.selected_queue_index));
                }
            }
            Focus::QueueControls => {
                if self.selected_control_index > 0 {
                    self.selected_control_index -= 1;
                }
            }
            _ => {}
        }
    }

    pub fn move_down(&mut self) {
        match self.focus {
            Focus::Tree => {
                if self.selected_folder_index < self.folder_items.len().saturating_sub(1) {
                    self.selected_folder_index += 1;
                    self.folder_tree_state.select(Some(self.selected_folder_index));
                }
            }
            Focus::Queue => {
                if self.selected_queue_index < self.queue.len().saturating_sub(1) {
                    if self.is_moving_track {
                        self.queue.swap(self.selected_queue_index, self.selected_queue_index + 1);
                        // Update current_index if it was swapped
                        if Some(self.selected_queue_index) == self.current_index {
                            self.current_index = Some(self.selected_queue_index + 1);
                        } else if Some(self.selected_queue_index + 1) == self.current_index {
                            self.current_index = Some(self.selected_queue_index);
                        }
                    }
                    self.selected_queue_index += 1;
                    self.queue_state.select(Some(self.selected_queue_index));
                }
            }
            Focus::QueueControls => {
                if self.selected_control_index < 2 { // 0: Shuffle, 1: Clear, 2: Sort
                    self.selected_control_index += 1;
                }
            }
            _ => {}
        }
    }

    pub fn remove_selected_queue_track(&mut self) {
        if !self.queue.is_empty() && self.selected_queue_index < self.queue.len() {
            self.queue.remove(self.selected_queue_index);
            
            // Adjust current_index
            if let Some(curr) = self.current_index {
                if curr == self.selected_queue_index {
                    self.current_index = None;
                    // Optionally stop playback or skip to next? 
                    // Let's just leave it for now.
                } else if curr > self.selected_queue_index {
                    self.current_index = Some(curr - 1);
                }
            }

            if self.selected_queue_index >= self.queue.len() && !self.queue.is_empty() {
                self.selected_queue_index = self.queue.len() - 1;
            }
            self.queue_state.select(Some(self.selected_queue_index));
        }
    }

    pub fn shuffle_queue(&mut self) {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        
        if let Some(curr) = self.current_index {
            if curr + 1 < self.queue.len() {
                let (_, next_up) = self.queue.split_at_mut(curr + 1);
                next_up.shuffle(&mut rng);
            }
        } else {
            self.queue.shuffle(&mut rng);
        }
    }

    pub fn clear_queue(&mut self) {
        if let Some(curr) = self.current_index {
            if let Some(item) = self.queue.get(curr).cloned() {
                self.queue = vec![item];
                self.current_index = Some(0);
                self.selected_queue_index = 0;
            }
        } else {
            self.queue.clear();
            self.current_index = None;
            self.selected_queue_index = 0;
        }
        self.queue_state.select(Some(self.selected_queue_index));
    }

    pub fn cycle_sort_method(&mut self) {
        self.sort_method = match self.sort_method {
            SortMethod::Added => SortMethod::Track,
            SortMethod::Track => SortMethod::Artist,
            SortMethod::Artist => SortMethod::Album,
            SortMethod::Album => SortMethod::Title,
            SortMethod::Title => SortMethod::Added,
        };
        self.apply_sort();
    }

    pub fn apply_sort(&mut self) {
        let current_item = self.current_index.and_then(|i| self.queue.get(i).cloned());

        self.queue.sort_by(|a, b| {
            if self.sort_method == SortMethod::Added {
                return a.id.cmp(&b.id);
            }

            let meta_a = self.metadata.get_metadata(&a.path).ok();
            let meta_b = self.metadata.get_metadata(&b.path).ok();

            let (val_a, val_b) = match self.sort_method {
                SortMethod::Track => (
                    meta_a.as_ref().and_then(|m| m.track_number.clone()).unwrap_or_default(),
                    meta_b.as_ref().and_then(|m| m.track_number.clone()).unwrap_or_default()
                ),
                SortMethod::Artist => (
                    meta_a.as_ref().and_then(|m| m.artist.clone()).unwrap_or_default().to_lowercase(),
                    meta_b.as_ref().and_then(|m| m.artist.clone()).unwrap_or_default().to_lowercase()
                ),
                SortMethod::Album => (
                    meta_a.as_ref().and_then(|m| m.album.clone()).unwrap_or_default().to_lowercase(),
                    meta_b.as_ref().and_then(|m| m.album.clone()).unwrap_or_default().to_lowercase()
                ),
                SortMethod::Title => (
                    meta_a.as_ref().and_then(|m| m.title.clone()).unwrap_or_default().to_lowercase(),
                    meta_b.as_ref().and_then(|m| m.title.clone()).unwrap_or_default().to_lowercase()
                ),
                SortMethod::Added => unreachable!(),
            };

            match val_a.cmp(&val_b) {
                std::cmp::Ordering::Equal => {
                    // Fallback: track # > name
                    let track_a = meta_a.as_ref().and_then(|m| m.track_number.as_ref())
                        .and_then(|t| t.split('/').next()).and_then(|t| t.parse::<u32>().ok()).unwrap_or(u32::MAX);
                    let track_b = meta_b.as_ref().and_then(|m| m.track_number.as_ref())
                        .and_then(|t| t.split('/').next()).and_then(|t| t.parse::<u32>().ok()).unwrap_or(u32::MAX);
                    
                    match track_a.cmp(&track_b) {
                        std::cmp::Ordering::Equal => {
                            let name_a = a.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                            let name_b = b.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                            name_a.cmp(&name_b)
                        }
                        other => other,
                    }
                }
                other => other,
            }
        });

        if let Some(item) = current_item {
            self.current_index = self.queue.iter().position(|it| it == &item);
        }
    }

    pub fn execute_selected_control(&mut self) {
        match self.selected_control_index {
            0 => self.shuffle_queue(),
            1 => self.clear_queue(),
            2 => self.cycle_sort_method(),
            _ => {}
        }
    }

    pub fn enter_directory(&mut self) {
        if let Some(path) = self.folder_items.get(self.selected_folder_index).cloned() {
            if path.is_dir() {
                self.library_path = path;
                self.update_folder_items();
                self.selected_folder_index = 0;
                self.folder_tree_state.select(Some(0));
            }
        }
    }

    pub fn go_back(&mut self) {
        let old_path = self.library_path.clone();
        if let Some(parent) = self.library_path.parent() {
            self.library_path = parent.to_path_buf();
            self.update_folder_items();
            
            if let Some(index) = self.folder_items.iter().position(|p| p == &old_path) {
                self.selected_folder_index = index;
            } else {
                self.selected_folder_index = 0;
            }
            self.folder_tree_state.select(Some(self.selected_folder_index));
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn next_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tree => Focus::Queue,
            Focus::Queue => Focus::QueueControls,
            Focus::QueueControls => Focus::Player,
            Focus::Player => Focus::Tree,
        };
    }

    pub fn search(&mut self) {
        if self.search_query.is_empty() {
            return;
        }

        let query = self.search_query.to_lowercase();
        if let Some(index) = self.folder_items.iter().position(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .contains(&query)
        }) {
            self.selected_folder_index = index;
            self.folder_tree_state.select(Some(index));
        }
    }
}
