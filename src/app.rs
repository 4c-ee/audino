use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;
use crate::player::Player;
use crate::metadata::MetadataProvider;
use crate::lyrics::{self, LyricLine};
use crate::config::Config;
use crate::lastfm::LastFMClient;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use ratatui::widgets::ListState;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Focus {
    Tree,
    Queue,
    QueueControls,
    Player,
    Settings,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SortMethod {
    Track,
    Artist,
    Album,
    Title,
    Added,
}

use crate::metadata::TrackMetadata;

/// Maximum pixel dimensions for in-memory album art. The picker downscales
/// per cell at render time. 1024×1024 RGBA = 4MB, sharp even on high-DPI
/// terminals.
const ART_MAX_DIM: u32 = 1024;

#[derive(Debug, Clone)]
pub struct QueueItem {
    pub path: PathBuf,
    pub id: usize,
    pub meta: Arc<TrackMetadata>,
}

impl QueueItem {
    pub fn new(path: PathBuf, id: usize, meta: Arc<TrackMetadata>) -> Self {
        Self { path, id, meta }
    }
}

/// Cached folder entry: rendered as a `▸ name` or `  name` line.
#[derive(Clone)]
pub struct CachedFolderEntry {
    pub is_dir: bool,
    pub name: String,
}

/// Cached queue row: the strings we display, pre-formatted once.
#[derive(Clone)]
pub struct CachedQueueEntry {
    pub track_str: String,
    pub artist: String,
    pub album: String,
    pub title: String,
}

pub struct App {
    pub player: Player,
    pub metadata: MetadataProvider,
    pub picker: Picker,
    pub current_album_art: Option<Arc<image::DynamicImage>>,
    pub current_protocol: Option<Protocol>,
    pub last_art_area: Option<(u16, u16)>,
    pub lyrics: Vec<LyricLine>,
    pub lyrics_path: Option<PathBuf>,
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
    pub placeholder_img: Arc<image::DynamicImage>,
    pub controls: Option<MediaControls>,
    pub mpris_rx: Receiver<MediaControlEvent>,
    pub config: Config,
    pub lastfm: Option<LastFMClient>,
    pub lastfm_scrobbled: bool,
    pub lastfm_now_playing_sent: bool,
    pub settings_open: bool,
    pub settings_focus: usize,
    pub settings_editing: Option<usize>,
    pub settings_api_key_buf: String,
    pub settings_api_secret_buf: String,
    pub settings_auth_token_buf: String,
    pub settings_status_msg: Option<String>,
    /// Tracks if folder_items Vec needs rebuild before next draw.
    pub folder_items_dirty: bool,
    /// Tracks if queue Vec needs rebuild before next queue draw.
    pub queue_items_dirty: bool,
    /// Cached folder entry strings. Rebuilt when folder_items_dirty is set
    /// or when the render area size changes.
    pub folder_items_cache: Option<Vec<CachedFolderEntry>>,
    pub folder_items_cache_area: Option<(u16, u16)>,
    /// Cached queue row strings. Rebuilt when queue_items_dirty is set
    /// or when the render area size changes.
    pub queue_items_cache: Option<Vec<CachedQueueEntry>>,
    pub queue_items_cache_area: Option<(u16, u16)>,
    tick_render_cache: TickRenderCache,
    /// Cached sort keys, rebuilt when the queue changes.
    cached_sort_keys: Option<Vec<SortKey>>,
    /// Last playback state pushed to MPRIS, used to skip redundant DBus calls.
    last_mpris_playback: Option<MediaPlayback>,
    /// Path of the track whose metadata was last pushed to MPRIS.
    last_mpris_metadata_path: Option<PathBuf>,
    /// Set when a track just started; metadata is re-pushed once duration is
    /// available from mpv (which loads asynchronously after `loadfile`).
    mpris_metadata_pending: bool,
    /// Millisecond timestamp of last position push to MPRIS, used to throttle
    /// position updates to ~2 Hz so the DBus event channel does not backlog.
    last_mpris_position_ms: u128,
}

pub struct TickRenderCache {
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub last_tick_ms: u128,
}

#[derive(Clone)]
struct SortKey {
    primary: String,
    track_num: u32,
    secondary: String,
    tertiary: String,
    quaternary: String,
    name: String,
    id: usize,
}

impl Ord for SortKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.primary
            .cmp(&other.primary)
            .then(self.track_num.cmp(&other.track_num))
            .then(self.secondary.cmp(&other.secondary))
            .then(self.tertiary.cmp(&other.tertiary))
            .then(self.quaternary.cmp(&other.quaternary))
            .then(self.name.cmp(&other.name))
            .then(self.id.cmp(&other.id))
    }
}

impl PartialOrd for SortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for SortKey {}
impl PartialEq for SortKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl App {
    pub fn new(library_path: PathBuf, config: Config) -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

        let placeholder_bytes = include_bytes!("../placeholder.png");
        let placeholder_full = image::load_from_memory(placeholder_bytes)
            .expect("Failed to load embedded placeholder.png");
        let placeholder_img = Arc::new(pre_resize_for_art(&picker, &placeholder_full));

        let (mpris_tx, mpris_rx) = mpsc::channel();
        let config_mpris = PlatformConfig {
            dbus_name: "audino",
            display_name: "audino",
            hwnd: None,
        };

        let mut controls = MediaControls::new(config_mpris).ok();
        if let Some(ref mut c) = controls {
            c.attach(move |event| {
                mpris_tx.send(event).ok();
            })
            .ok();
        }

        let lastfm = LastFMClient::from_config(&config);
        if lastfm.is_some() {
            crate::log("Last.FM client initialized");
        }

        let mut app = Self {
            player: Player::new(),
            metadata: MetadataProvider::new(),
            picker,
            current_album_art: None,
            current_protocol: None,
            last_art_area: None,
            lyrics: Vec::new(),
            lyrics_path: None,
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
            controls,
            mpris_rx,
            config,
            lastfm,
            lastfm_scrobbled: false,
            lastfm_now_playing_sent: false,
            settings_open: false,
            settings_focus: 0,
            settings_editing: None,
            settings_api_key_buf: String::new(),
            settings_api_secret_buf: String::new(),
            settings_auth_token_buf: String::new(),
            settings_status_msg: None,
            folder_items_dirty: true,
            queue_items_dirty: true,
            folder_items_cache: None,
            folder_items_cache_area: None,
            queue_items_cache: None,
            queue_items_cache_area: None,
            tick_render_cache: TickRenderCache {
                position: 0.0,
                duration: 0.0,
                volume: 100.0,
                last_tick_ms: 0,
            },
            cached_sort_keys: None,
            last_mpris_playback: None,
            last_mpris_metadata_path: None,
            mpris_metadata_pending: false,
            last_mpris_position_ms: 0,
        };
        app.update_folder_items();
        app.folder_tree_state.select(Some(0));
        app.queue_state.select(Some(0));
        app
    }

    pub fn add_to_queue(&mut self, path: PathBuf) {
        if path.is_file() {
            if let Ok(meta) = self.metadata.get_metadata(&path) {
                self.queue.push(QueueItem::new(path, self.next_id, meta));
                self.next_id += 1;
                self.queue_items_dirty = true;
                self.cached_sort_keys = None;
            }
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

            let mut meta_items: Vec<(u32, PathBuf, Arc<TrackMetadata>)> = Vec::new();
            for p in items {
                if let Ok(meta) = self.metadata.get_metadata(&p) {
                    let track = meta
                        .track_number
                        .as_deref()
                        .and_then(|t| t.split('/').next())
                        .and_then(|t| t.parse::<u32>().ok())
                        .unwrap_or(u32::MAX);
                    meta_items.push((track, p, meta));
                }
            }
            meta_items.sort_by(|a, b| match a.0.cmp(&b.0) {
                std::cmp::Ordering::Equal => {
                    let a_name = a.1.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                    let b_name = b.1.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                    a_name.cmp(&b_name)
                }
                other => other,
            });

            for (_, p, meta) in meta_items {
                self.queue.push(QueueItem::new(p, self.next_id, meta));
                self.next_id += 1;
            }
            self.queue_items_dirty = true;
            self.cached_sort_keys = None;
        }
    }

    fn is_audio_file(&self, path: &PathBuf) -> bool {
        path.extension().map_or(false, |ext| {
            let ext = ext.to_string_lossy().to_lowercase();
            ext == "mp3" || ext == "flac" || ext == "ogg" || ext == "m4a" || ext == "opus" || ext == "wav"
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
        self.folder_items_dirty = true;
    }

    pub fn play_index(&mut self, index: usize) {
        crate::log(&format!("App: Attempting to play index {}", index));
        if let Some(item) = self.queue.get(index) {
            let path = item.path.clone();
            if let Err(e) = self.player.play(&path.to_string_lossy()) {
                crate::log(&format!("App: Error playing file: {:?}", e));
            } else {
                self.current_index = Some(index);
                self.lastfm_scrobbled = false;
                self.lastfm_now_playing_sent = false;
                self.load_track_assets(&path);
                // Reset playback state cache so the next tick re-pushes the
                // new status instead of treating it as a no-op.
                self.last_mpris_playback = None;
                self.last_mpris_metadata_path = None;
                self.mpris_metadata_pending = false;
                self.update_mpris_metadata(&path);
            }
        } else {
            crate::log("App: Index not found in queue");
        }
    }

    pub fn tick(&mut self) {
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
        let pos = self.player.get_position().unwrap_or(0.0);
        let dur = self.player.get_duration().unwrap_or(0.0);
        let vol = self.player.get_volume().unwrap_or(100.0);
        self.tick_render_cache.position = pos;
        self.tick_render_cache.duration = dur;
        self.tick_render_cache.volume = vol;
        self.tick_render_cache.last_tick_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        self.update_mpris_playback();

        // mpv loads files asynchronously; duration is unavailable right after
        // a track switch. Re-push metadata once it becomes known.
        if self.mpris_metadata_pending {
            if let Some(idx) = self.current_index {
                if let Some(item) = &self.queue.get(idx) {
                    let path = item.path.clone();
                    if self.player.get_duration().ok().is_some() {
                        self.update_mpris_metadata(&path);
                    }
                }
            }
        }

        self.check_lastfm_scrobble(pos, dur);
    }

    fn check_lastfm_scrobble(&mut self, pos: f64, dur: f64) {
        if dur <= 0.0 || pos <= 0.0 || self.lastfm_scrobbled {
            return;
        }
        if self.player.get_paused().unwrap_or(true) {
            return;
        }
        let Some(idx) = self.current_index else { return };
        let Some(item) = self.queue.get(idx) else { return };
        let Some(artist) = &item.meta.artist else { return };
        let Some(track) = &item.meta.title else { return };

        if !self.lastfm_now_playing_sent {
            if let Some(ref lastfm) = self.lastfm {
                let album = item.meta.album.as_deref();
                if let Err(e) = lastfm.update_now_playing(artist, track, album) {
                    crate::log(&format!("Last.FM now playing error: {}", e));
                }
            }
            self.lastfm_now_playing_sent = true;
        }

        let threshold = (dur / 2.0).min(240.0).max(30.0);
        if pos >= threshold {
            if let Some(ref lastfm) = self.lastfm {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let album = item.meta.album.as_deref();
                if let Err(e) = lastfm.scrobble(artist, track, album, timestamp) {
                    crate::log(&format!("Last.FM scrobble error: {}", e));
                } else {
                    crate::log("Last.FM: track scrobbled");
                }
            }
            self.lastfm_scrobbled = true;
        }
    }

    fn load_track_assets(&mut self, path: &PathBuf) {
        let mut art_path: Option<PathBuf> = None;
        if let Ok(Some(p)) = self.metadata.get_or_extract_album_art(path) {
            art_path = Some(p);
        }
        if art_path.is_none() {
            if let Some(parent) = path.parent() {
                for ext in &["png", "jpg", "jpeg", "webp"] {
                    let cover_path = parent.join(format!("cover.{}", ext));
                    if cover_path.exists() {
                        art_path = Some(cover_path);
                        break;
                    }
                }
            }
        }

        let decoded = art_path
            .as_deref()
            .and_then(|p| image::ImageReader::open(p).ok())
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.decode().ok());

        match decoded {
            Some(img) => {
                let resized = pre_resize_for_art(&self.picker, &img);
                self.current_album_art = Some(Arc::new(resized));
            }
            None => {
                self.current_album_art = None;
            }
        }
        self.current_protocol = None;
        self.last_art_area = None;

        if let Some(lrc_path) = lyrics::find_lrc_file(path) {
            self.lyrics_path = Some(lrc_path.clone());
            self.lyrics = lyrics::parse_lrc(&lrc_path);
        } else {
            self.lyrics_path = None;
            self.lyrics = Vec::new();
        }
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
                        if Some(self.selected_queue_index) == self.current_index {
                            self.current_index = Some(self.selected_queue_index - 1);
                        } else if Some(self.selected_queue_index - 1) == self.current_index {
                            self.current_index = Some(self.selected_queue_index);
                        }
                        self.cached_sort_keys = None;
                        self.queue_items_dirty = true;
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
                        if Some(self.selected_queue_index) == self.current_index {
                            self.current_index = Some(self.selected_queue_index + 1);
                        } else if Some(self.selected_queue_index + 1) == self.current_index {
                            self.current_index = Some(self.selected_queue_index);
                        }
                        self.cached_sort_keys = None;
                        self.queue_items_dirty = true;
                    }
                    self.selected_queue_index += 1;
                    self.queue_state.select(Some(self.selected_queue_index));
                }
            }
            Focus::QueueControls => {
                if self.selected_control_index < 2 {
                    self.selected_control_index += 1;
                }
            }
            _ => {}
        }
    }

    pub fn remove_selected_queue_track(&mut self) {
        if !self.queue.is_empty() && self.selected_queue_index < self.queue.len() {
            self.queue.remove(self.selected_queue_index);

            if let Some(curr) = self.current_index {
                if curr == self.selected_queue_index {
                    self.current_index = None;
                } else if curr > self.selected_queue_index {
                    self.current_index = Some(curr - 1);
                }
            }

            if self.selected_queue_index >= self.queue.len() && !self.queue.is_empty() {
                self.selected_queue_index = self.queue.len() - 1;
            }
            self.queue_state.select(Some(self.selected_queue_index));
            self.queue_items_dirty = true;
            self.cached_sort_keys = None;
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
        self.cached_sort_keys = None;
        self.queue_items_dirty = true;
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
        self.queue_items_dirty = true;
        self.cached_sort_keys = None;
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
        let current_id = self.current_index.and_then(|i| self.queue.get(i).map(|it| it.id));

        let keys = self.build_sort_keys();
        let mut indices: Vec<usize> = (0..self.queue.len()).collect();
        indices.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
        let new_queue: Vec<QueueItem> = indices.iter().map(|&i| self.queue[i].clone()).collect();
        self.queue = new_queue;
        // keys still align by index since both are reordered identically
        self.cached_sort_keys = Some(keys);

        if let Some(id) = current_id {
            self.current_index = self.queue.iter().position(|it| it.id == id);
        }
        self.queue_items_dirty = true;
    }

    fn build_sort_keys(&self) -> Vec<SortKey> {
        if let Some(keys) = &self.cached_sort_keys {
            if keys.len() == self.queue.len() {
                return keys.clone();
            }
        }
        self.queue
            .iter()
            .map(|item| {
                let track_str = item.meta.track_number.clone().unwrap_or_default();
                let track_num = item
                    .meta
                    .track_number
                    .as_deref()
                    .and_then(|t| t.split('/').next())
                    .and_then(|t| t.parse::<u32>().ok())
                    .unwrap_or(u32::MAX);

                let title = item.meta.title.as_deref().unwrap_or("").to_lowercase();
                let artist = item.meta.artist.as_deref().unwrap_or("").to_lowercase();
                let album = item.meta.album.as_deref().unwrap_or("").to_lowercase();
                let name = item
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
                    .to_lowercase();

                let (primary, secondary, tertiary) = match self.sort_method {
                    SortMethod::Added => (String::new(), String::new(), String::new()),
                    SortMethod::Track => (track_str.to_lowercase(), artist.clone(), album.clone()),
                    SortMethod::Artist => (artist, album.clone(), title.clone()),
                    SortMethod::Album => (album, artist.clone(), title.clone()),
                    SortMethod::Title => (title, artist.clone(), album.clone()),
                };
                SortKey {
                    primary,
                    track_num,
                    secondary,
                    tertiary,
                    quaternary: String::new(),
                    name,
                    id: item.id,
                }
            })
            .collect()
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
            Focus::Settings => Focus::Tree,
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

    pub fn update_mpris_metadata(&mut self, path: &PathBuf) {
        if let Some(ref mut controls) = self.controls {
            let meta = self.metadata.get_metadata(path).ok();
            let duration = self.player.get_duration().ok().map(Duration::from_secs_f64);

            let mut art_url = None;
            if let Ok(Some(art_path)) = self.metadata.get_or_extract_album_art(path) {
                if let Some(path_str) = art_path.to_str() {
                    art_url = Some(format!("file://{}", path_str));
                }
            }

            let mpris_meta = MediaMetadata {
                title: meta.as_ref().and_then(|m| m.title.as_deref()),
                artist: meta.as_ref().and_then(|m| m.artist.as_deref()),
                album: meta.as_ref().and_then(|m| m.album.as_deref()),
                duration,
                cover_url: art_url.as_deref(),
                ..Default::default()
            };
            controls.set_metadata(mpris_meta).ok();
            self.last_mpris_metadata_path = Some(path.clone());
            // If duration isn't available yet, mpv is still loading the file;
            // re-push metadata on a later tick once it's known.
            self.mpris_metadata_pending = duration.is_none();
        }
    }

    pub fn update_mpris_playback(&mut self) {
        if let Some(ref mut controls) = self.controls {
            let position = self
                .player
                .get_position()
                .ok()
                .map(|p| MediaPosition(Duration::from_secs_f64(p)));

            let status = if self.player.is_empty() {
                MediaPlayback::Stopped
            } else if self.player.get_paused().unwrap_or(false) {
                MediaPlayback::Paused { progress: position }
            } else {
                MediaPlayback::Playing { progress: position }
            };

            // Only push full PropertiesChanged when the playback variant
            // (Stopped / Playing / Paused) changes.  Per-tick position
            // updates would flood the D-Bus event channel and backlog the
            // service thread (souvlaki's conn.process blocks for up to 1 s
            // when no incoming D-Bus traffic arrives).
            let variant_changed = match (&self.last_mpris_playback, &status) {
                (Some(MediaPlayback::Stopped), MediaPlayback::Stopped) => false,
                (Some(MediaPlayback::Paused { .. }), MediaPlayback::Paused { .. }) => false,
                (Some(MediaPlayback::Playing { .. }), MediaPlayback::Playing { .. }) => false,
                _ => true,
            };

            if variant_changed {
                controls.set_playback(status.clone()).ok();
                self.last_mpris_playback = Some(status);
                self.last_mpris_position_ms = 0;
            } else {
                // Periodically refresh the stored position so that the
                // `Position` D-Bus property stays reasonably current.
                // Rate-limited to ~2 Hz (every 500 ms).
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                if now_ms.saturating_sub(self.last_mpris_position_ms) >= 500 {
                    // Set the playback status with the updated progress
                    // to refresh the internal state. This produces a
                    // PropertiesChanged signal even though the status
                    // string is identical.
                    controls.set_playback(status).ok();
                    self.last_mpris_position_ms = now_ms;
                }
            }
        }
    }

    pub fn handle_mpris_events(&mut self) {
        while let Ok(event) = self.mpris_rx.try_recv() {
            match event {
                MediaControlEvent::Play => {
                    self.player.pause(false).ok();
                }
                MediaControlEvent::Pause => {
                    self.player.pause(true).ok();
                }
                MediaControlEvent::Toggle => {
                    let paused = self.player.get_paused().unwrap_or(false);
                    self.player.pause(!paused).ok();
                }
                MediaControlEvent::Next => {
                    if let Some(current) = self.current_index {
                        if current + 1 < self.queue.len() {
                            self.play_index(current + 1);
                        }
                    }
                }
                MediaControlEvent::Previous => {
                    if let Some(current) = self.current_index {
                        if current > 0 {
                            self.play_index(current - 1);
                        }
                    }
                }
                MediaControlEvent::Stop => {
                    self.player.pause(true).ok();
                }
                MediaControlEvent::Seek(direction) => {
                    let offset = match direction {
                        souvlaki::SeekDirection::Forward => 5.0,
                        souvlaki::SeekDirection::Backward => -5.0,
                    };
                    self.player.seek(offset).ok();
                }
                MediaControlEvent::SeekBy(direction, duration) => {
                    let secs = duration.as_secs_f64();
                    let offset = match direction {
                        souvlaki::SeekDirection::Forward => secs,
                        souvlaki::SeekDirection::Backward => -secs,
                    };
                    self.player.seek(offset).ok();
                }
                _ => {}
            }
        }
    }

    pub fn cached_player_state(&self) -> (f64, f64, f64) {
        (
            self.tick_render_cache.position,
            self.tick_render_cache.duration,
            self.tick_render_cache.volume,
        )
    }

    pub fn toggle_settings(&mut self) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            self.settings_editing = None;
            self.settings_focus = 0;
            self.settings_api_key_buf = self.config.get("lastfm", "api_key").unwrap_or_default();
            self.settings_api_secret_buf = self.config.get("lastfm", "api_secret").unwrap_or_default();
            self.settings_auth_token_buf.clear();
            self.settings_status_msg = None;
        }
    }

    pub fn settings_move_up(&mut self) {
        if self.settings_editing.is_some() {
            return;
        }
        if self.settings_focus > 0 {
            self.settings_focus -= 1;
        }
    }

    pub fn settings_move_down(&mut self) {
        if self.settings_editing.is_some() {
            return;
        }
        if self.settings_focus < 4 {
            self.settings_focus += 1;
        }
    }

    pub fn settings_activate(&mut self) {
        if self.settings_editing.is_some() {
            self.settings_confirm_editing();
            return;
        }
        match self.settings_focus {
            0 | 1 | 2 => {
                self.settings_editing = Some(self.settings_focus);
            }
            3 => {
                self.settings_do_authorize();
            }
            4 => {
                self.settings_open_url();
            }
            _ => {}
        }
    }

    pub fn settings_input_char(&mut self, c: char) {
        match self.settings_editing {
            Some(0) => self.settings_api_key_buf.push(c),
            Some(1) => self.settings_api_secret_buf.push(c),
            Some(2) => self.settings_auth_token_buf.push(c),
            _ => {}
        }
    }

    pub fn settings_backspace(&mut self) {
        match self.settings_editing {
            Some(0) => { self.settings_api_key_buf.pop(); }
            Some(1) => { self.settings_api_secret_buf.pop(); }
            Some(2) => { self.settings_auth_token_buf.pop(); }
            _ => {}
        }
    }

    pub fn settings_cancel_editing(&mut self) {
        if let Some(idx) = self.settings_editing {
            if idx == 0 {
                self.settings_api_key_buf = self.config.get("lastfm", "api_key").unwrap_or_default();
            } else if idx == 1 {
                self.settings_api_secret_buf = self.config.get("lastfm", "api_secret").unwrap_or_default();
            } else if idx == 2 {
                self.settings_auth_token_buf.clear();
            }
        }
        self.settings_editing = None;
    }

    pub fn settings_confirm_editing(&mut self) {
        if let Some(idx) = self.settings_editing {
            match idx {
                0 => {
                    self.config.set("lastfm", "api_key", &self.settings_api_key_buf);
                    self.config.save().ok();
                    self.reinit_lastfm();
                }
                1 => {
                    self.config.set("lastfm", "api_secret", &self.settings_api_secret_buf);
                    self.config.save().ok();
                    self.reinit_lastfm();
                }
                2 => {
                    self.settings_do_authorize();
                }
                _ => {}
            }
        }
        self.settings_editing = None;
    }

    fn reinit_lastfm(&mut self) {
        self.lastfm = LastFMClient::from_config(&self.config);
        if self.lastfm.is_some() {
            crate::log("Last.FM client reinitialized from settings");
        }
    }

    fn settings_do_authorize(&mut self) {
        if self.settings_auth_token_buf.is_empty() {
            self.settings_status_msg = Some("Enter an auth token first".to_string());
            return;
        }
        let token = self.settings_auth_token_buf.clone();
        if let Some(ref mut lastfm) = self.lastfm {
            match lastfm.auth_with_token(&token) {
                Ok(()) => {
                    if let Some(sk) = lastfm.session_key() {
                        self.config.set("lastfm", "session_key", &sk);
                        self.config.save().ok();
                        self.settings_status_msg = Some("Authenticated successfully".to_string());
                        crate::log("Last.FM: session key saved to config");
                    }
                }
                Err(e) => {
                    self.settings_status_msg = Some(format!("Auth failed: {}", e));
                    crate::log(&format!("Last.FM auth error: {}", e));
                }
            }
        } else {
            self.settings_status_msg = Some("Configure API key/secret first".to_string());
        }
    }

    fn settings_open_url(&mut self) {
        let url = if let Some(ref lastfm) = self.lastfm {
            lastfm.get_auth_url()
        } else {
            self.settings_status_msg = Some("Configure API key/secret first".to_string());
            return;
        };
        match std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
        {
            Ok(_) => {
                self.settings_status_msg = Some("Opened auth URL in browser".to_string());
            }
            Err(e) => {
                self.settings_status_msg = Some(format!("Failed to open: {}", e));
            }
        }
    }
}

/// Resize an image so its largest dimension is at most `ART_MAX_DIM`.
/// Caps in-memory album art at ~4MB worst case (1024x1024 RGBA) while still
/// leaving the picker enough source pixels to downscale sharply per cell.
fn pre_resize_for_art(_picker: &Picker, img: &image::DynamicImage) -> image::DynamicImage {
    if img.width() <= ART_MAX_DIM && img.height() <= ART_MAX_DIM {
        return img.clone();
    }
    let scale = (ART_MAX_DIM as f64 / img.width() as f64)
        .min(ART_MAX_DIM as f64 / img.height() as f64);
    let nw = ((img.width() as f64) * scale).round().max(1.0) as u32;
    let nh = ((img.height() as f64) * scale).round().max(1.0) as u32;
    img.resize(nw, nh, image::imageops::FilterType::Triangle)
}
