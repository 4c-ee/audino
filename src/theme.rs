use ratatui::style::Color;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

static CURRENT_THEME: OnceLock<Theme> = OnceLock::new();

pub fn global_theme() -> &'static Theme {
    CURRENT_THEME.get_or_init(|| Theme::load().unwrap_or_default())
}

pub fn set_global_theme(theme: Theme) {
    let _ = CURRENT_THEME.set(theme);
}

pub fn config_value(key: &str) -> Option<String> {
    let config_path = Theme::config_path()?;
    if !config_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&config_path).ok()?;
    parse_kv_config(&content)
        .get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub colors: ThemeColors,
    pub corner_rounding: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            colors: ThemeColors::default(),
            corner_rounding: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub folder_selected: Color,
    pub folder_normal: Color,
    pub queue_current: Color,
    pub queue_played: Color,
    pub queue_normal: Color,
    pub border_focused: Color,
    pub border_unfocused: Color,
    pub title_fg: Color,
    pub progress_filled: Color,
    pub progress_empty: Color,
    pub lyric_active: Color,
    pub lyric_inactive: Color,
    pub moving_track_bg: Color,
    pub moving_track_fg: Color,
    pub controls_selected: Color,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            folder_selected: Color::Rgb(197, 197, 197),
            folder_normal: Color::Rgb(136, 136, 136),
            queue_current: Color::Rgb(197, 197, 197),
            queue_played: Color::Rgb(68, 68, 68),
            queue_normal: Color::Rgb(136, 136, 136),
            border_focused: Color::Rgb(197, 197, 197),
            border_unfocused: Color::Rgb(128, 128, 128),
            title_fg: Color::Rgb(197, 197, 197),
            progress_filled: Color::Rgb(136, 136, 136),
            progress_empty: Color::Rgb(51, 51, 51),
            lyric_active: Color::Rgb(197, 197, 197),
            lyric_inactive: Color::Rgb(68, 68, 68),
            moving_track_bg: Color::Rgb(50, 50, 50),
            moving_track_fg: Color::Rgb(255, 255, 255),
            controls_selected: Color::Rgb(197, 197, 197),
        }
    }
}

impl Theme {
    pub fn load() -> Option<Self> {
        let config_path = Self::config_path()?;
        if !config_path.exists() {
            return None;
        }

        let content = fs::read_to_string(&config_path).ok()?;
        let pairs = parse_kv_config(&content);

        let mut colors = ThemeColors::default();

        if let Some(v) = pairs.get("folder_selected") {
            colors.folder_selected = parse_color(v);
        }
        if let Some(v) = pairs.get("folder_normal") {
            colors.folder_normal = parse_color(v);
        }
        if let Some(v) = pairs.get("queue_current") {
            colors.queue_current = parse_color(v);
        }
        if let Some(v) = pairs.get("queue_played") {
            colors.queue_played = parse_color(v);
        }
        if let Some(v) = pairs.get("queue_normal") {
            colors.queue_normal = parse_color(v);
        }
        if let Some(v) = pairs.get("border_focused") {
            colors.border_focused = parse_color(v);
        }
        if let Some(v) = pairs.get("border_unfocused") {
            colors.border_unfocused = parse_color(v);
        }
        if let Some(v) = pairs.get("title_fg") {
            colors.title_fg = parse_color(v);
        }
        if let Some(v) = pairs.get("progress_filled") {
            colors.progress_filled = parse_color(v);
        }
        if let Some(v) = pairs.get("progress_empty") {
            colors.progress_empty = parse_color(v);
        }
        if let Some(v) = pairs.get("lyric_active") {
            colors.lyric_active = parse_color(v);
        }
        if let Some(v) = pairs.get("lyric_inactive") {
            colors.lyric_inactive = parse_color(v);
        }
        if let Some(v) = pairs.get("moving_track_bg") {
            colors.moving_track_bg = parse_color(v);
        }
        if let Some(v) = pairs.get("moving_track_fg") {
            colors.moving_track_fg = parse_color(v);
        }
        if let Some(v) = pairs.get("controls_selected") {
            colors.controls_selected = parse_color(v);
        }

        let corner_rounding = pairs
            .get("corner_rounding")
            .map(|v| v.trim().to_lowercase())
            .map(|v| v == "1" || v == "true" || v == "yes" || v == "on")
            .unwrap_or(false);

        Some(Self { colors, corner_rounding })
    }

    fn config_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join(".config").join("audino").join("audino.conf"))
    }
}

fn parse_kv_config(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(pos) = line.find('=') {
            let key = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            map.insert(key, value);
        }
    }
    map
}

fn parse_color(s: &str) -> Color {
    let s = s.trim();
    if s.starts_with('#') && s.len() == 7 {
        if let Ok(r) = u8::from_str_radix(&s[1..3], 16) {
            if let Ok(g) = u8::from_str_radix(&s[3..5], 16) {
                if let Ok(b) = u8::from_str_radix(&s[5..7], 16) {
                    return Color::Rgb(r, g, b);
                }
            }
        }
    }
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 3 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            parts[0].trim().parse::<u8>(),
            parts[1].trim().parse::<u8>(),
            parts[2].trim().parse::<u8>(),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    eprintln!("audino: warning: ignoring malformed color {:?}, using default gray", s);
    Color::Rgb(136, 136, 136)
}

impl ThemeColors {
    pub fn folder_selected_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::new()
            .fg(self.folder_selected)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    pub fn folder_normal_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::new().fg(self.folder_normal)
    }

    pub fn queue_current_style(&self) -> ratatui::style::Style {
        self.folder_selected_style()
    }

    pub fn queue_played_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::new().fg(self.queue_played)
    }

    pub fn queue_normal_style(&self) -> ratatui::style::Style {
        self.folder_normal_style()
    }

    pub fn border_style(&self, focused: bool) -> ratatui::style::Style {
        ratatui::style::Style::new().fg(if focused {
            self.border_focused
        } else {
            self.border_unfocused
        })
    }

    pub fn controls_style(&self, selected: bool) -> ratatui::style::Style {
        if selected {
            ratatui::style::Style::new()
                .fg(self.controls_selected)
                .add_modifier(ratatui::style::Modifier::BOLD)
                .add_modifier(ratatui::style::Modifier::REVERSED)
        } else {
            self.folder_normal_style()
        }
    }

    pub fn title_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::new()
            .fg(self.title_fg)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    pub fn lyric_style(&self, active: bool) -> ratatui::style::Style {
        if active {
            self.title_style()
        } else {
            ratatui::style::Style::new().fg(self.lyric_inactive)
        }
    }

    pub fn moving_track_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::new()
            .bg(self.moving_track_bg)
            .fg(self.moving_track_fg)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    pub fn progress_filled_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::new().fg(self.progress_filled)
    }

    pub fn progress_empty_style(&self) -> ratatui::style::Style {
        ratatui::style::Style::new().fg(self.progress_empty)
    }
}