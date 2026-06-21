use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};
use crate::app::{App, CachedFolderEntry, CachedQueueEntry, Focus};

pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(8),
        ])
        .split(f.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(chunks[0]);

    let queue_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(main_chunks[1]);

    render_folder_tree(f, app, main_chunks[0]);
    render_queue(f, app, queue_chunks[0]);
    render_queue_controls(f, app, queue_chunks[1]);
    render_player_bar(f, app, chunks[1]);

    if app.settings_open {
        render_settings(f, app, f.area());
    }
}

const STYLE_FOLDER_SELECTED: Style = Style::new()
    .fg(Color::Rgb(197, 197, 197))
    .add_modifier(Modifier::BOLD);
const STYLE_FOLDER_NORMAL: Style = Style::new().fg(Color::Rgb(136, 136, 136));
const STYLE_QUEUE_CURRENT: Style = STYLE_FOLDER_SELECTED;
const STYLE_QUEUE_PLAYED: Style = Style::new().fg(Color::Rgb(68, 68, 68));
const STYLE_QUEUE_NORMAL: Style = STYLE_FOLDER_NORMAL;

fn render_folder_tree(f: &mut Frame, app: &mut App, area: Rect) {
    let need_rebuild = app.folder_items_cache.is_none()
        || app.folder_items_dirty
        || app.folder_items_cache_area != Some((area.width, area.height));
    if need_rebuild {
        let mut entries: Vec<CachedFolderEntry> = Vec::with_capacity(app.folder_items.len());
        for path in &app.folder_items {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            entries.push(CachedFolderEntry {
                is_dir: path.is_dir(),
                name,
            });
        }
        app.folder_items_cache = Some(entries);
        app.folder_items_cache_area = Some((area.width, area.height));
        app.folder_items_dirty = false;
    }

    let cache = app.folder_items_cache.as_ref().unwrap();
    let items: Vec<ListItem> = cache
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == app.selected_folder_index {
                STYLE_FOLDER_SELECTED
            } else {
                STYLE_FOLDER_NORMAL
            };
            let prefix = if entry.is_dir { "▸ " } else { "  " };
            let mut line = String::with_capacity(prefix.len() + entry.name.len());
            line.push_str(prefix);
            line.push_str(&entry.name);
            ListItem::new(line).style(style)
        })
        .collect();

    let border_color = if app.focus == Focus::Tree {
        Color::Rgb(197, 197, 197)
    } else {
        Color::Rgb(136, 136, 136)
    };

    let title = if app.is_searching {
        format!(" Folder Tree (Search: {}) ", app.search_query)
    } else {
        " Folder Tree ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border_color));

    let list = List::new(items).block(block);
    f.render_stateful_widget(list, area, &mut app.folder_tree_state);
}

fn render_queue(f: &mut Frame, app: &mut App, area: Rect) {
    let need_rebuild = app.queue_items_cache.is_none()
        || app.queue_items_dirty
        || app.queue_items_cache_area != Some((area.width, area.height));
    if need_rebuild {
        let mut entries: Vec<CachedQueueEntry> = Vec::with_capacity(app.queue.len());
        for item in &app.queue {
            let track_str = item
                .meta
                .track_number
                .clone()
                .unwrap_or_else(|| "??".to_string());
            let artist = item
                .meta
                .artist
                .clone()
                .unwrap_or_else(|| "Unknown Artist".to_string());
            let album = item
                .meta
                .album
                .clone()
                .unwrap_or_else(|| "Unknown Album".to_string());
            let title_owned: String;
            let title: String = if let Some(t) = item.meta.title.clone() {
                t
            } else {
                title_owned = item
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                title_owned
            };
            entries.push(CachedQueueEntry {
                track_str,
                artist,
                album,
                title,
            });
        }
        app.queue_items_cache = Some(entries);
        app.queue_items_cache_area = Some((area.width, area.height));
        app.queue_items_dirty = false;
    }

    let cache = app.queue_items_cache.as_ref().unwrap();
    let items: Vec<ListItem> = cache
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let mut style = if Some(i) == app.current_index {
                STYLE_QUEUE_CURRENT
            } else if app.current_index.map_or(false, |curr| i < curr) {
                STYLE_QUEUE_PLAYED
            } else {
                STYLE_QUEUE_NORMAL
            };

            if i == app.selected_queue_index && app.focus == Focus::Queue {
                style = if app.is_moving_track {
                    Style::new()
                        .bg(Color::Rgb(50, 50, 50))
                        .fg(Color::Rgb(255, 255, 255))
                        .add_modifier(Modifier::BOLD)
                } else {
                    style.add_modifier(Modifier::REVERSED)
                };
            }

            use std::fmt::Write;
            let mut s = String::with_capacity(64);
            let _ = write!(
                s,
                "{:>2} {:20} {:20} {}",
                entry.track_str,
                truncate(&entry.artist, 20),
                truncate(&entry.album, 20),
                entry.title
            );
            ListItem::new(s).style(style)
        })
        .collect();

    let border_color = if app.focus == Focus::Queue {
        Color::Rgb(197, 197, 197)
    } else {
        Color::Rgb(136, 136, 136)
    };

    let title = if app.is_moving_track {
        " Queue (Moving Track...) "
    } else {
        " Queue "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border_color));

    let list = List::new(items).block(block);
    f.render_stateful_widget(list, area, &mut app.queue_state);
}

use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

fn truncate(s: &str, max: usize) -> &str {
    let width = s.width();
    if width <= max {
        return s;
    }
    let mut used = 0;
    for (idx, ch) in s.char_indices() {
        let w = ch.width().unwrap_or(0);
        if used + w > max {
            return &s[..idx];
        }
        used += w;
    }
    s
}

fn render_queue_controls(f: &mut Frame, app: &mut App, area: Rect) {
    let sort_label = format!(" Sort ({:?}) ", app.sort_method);
    let controls = [" Shuffle ", " Clear ", &sort_label];

    let border_color = if app.focus == Focus::QueueControls {
        Color::Rgb(197, 197, 197)
    } else {
        Color::Rgb(136, 136, 136)
    };

    let block = Block::default()
        .title(" Queue Controls ")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border_color));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(inner_area);

    for (i, control) in controls.iter().enumerate() {
        let style = if i == app.selected_control_index && app.focus == Focus::QueueControls {
            Style::new()
                .fg(Color::Rgb(197, 197, 197))
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED)
        } else {
            STYLE_FOLDER_NORMAL
        };

        let p = Paragraph::new(*control).style(style).alignment(Alignment::Center);
        f.render_widget(p, chunks[i]);
    }
}

fn render_player_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let border_color = if app.focus == Focus::Player {
        Color::Rgb(197, 197, 197)
    } else {
        Color::Rgb(136, 136, 136)
    };

    let block = Block::default()
        .title(" Player ")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border_color));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(14),
            Constraint::Min(0),
            Constraint::Percentage(30),
        ])
        .split(inner_area);

    if let Some(img) = &app.current_album_art {
        let mut art_area = chunks[0];
        art_area.x += 1;
        art_area.height = art_area.height.saturating_sub(1);
        let key = (art_area.width, art_area.height);
        let needs_rebuild = app.current_protocol.is_none() || app.last_art_area != Some(key);
        if needs_rebuild {
            let size = ratatui::layout::Size::new(art_area.width, art_area.height);
            if let Ok(proto) = app.picker.new_protocol(
                (**img).clone(),
                size,
                ratatui_image::Resize::Fit(Some(image::imageops::FilterType::CatmullRom)),
            ) {
                app.current_protocol = Some(proto);
                app.last_art_area = Some(key);
            }
        }
        if let Some(art) = &app.current_protocol {
            let image = ratatui_image::Image::new(art);
            f.render_widget(image, art_area);
        }
    } else {
        let mut art_area = chunks[0];
        art_area.x += 1;
        art_area.height = art_area.height.saturating_sub(1);
        let key = (art_area.width, art_area.height);
        let needs_rebuild = app.current_protocol.is_none() || app.last_art_area != Some(key);
        if needs_rebuild {
            let size = ratatui::layout::Size::new(art_area.width, art_area.height);
            if let Ok(proto) = app.picker.new_protocol(
                (*app.placeholder_img).clone(),
                size,
                ratatui_image::Resize::Fit(Some(image::imageops::FilterType::CatmullRom)),
            ) {
                app.current_protocol = Some(proto);
                app.last_art_area = Some(key);
            }
        }
        if let Some(art) = &app.current_protocol {
            let image = ratatui_image::Image::new(art);
            f.render_widget(image, art_area);
        }
    }

    let (pos, dur, vol) = app.cached_player_state();
    let progress = if dur > 0.0 { pos / dur } else { 0.0 };

    let current_track_item = app.current_index.and_then(|i| app.queue.get(i));
    let (title, artist, album) = if let Some(item) = current_track_item {
        let title = item
            .meta
            .title
            .clone()
            .unwrap_or_else(|| {
                item.path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "No track playing".to_string())
            });
        let artist = item.meta.artist.clone().unwrap_or_default();
        let album = item.meta.album.clone().unwrap_or_default();
        (title, artist, album)
    } else {
        ("No track playing".to_string(), "".to_string(), "".to_string())
    };

    let meta_text = vec![
        Line::from(Span::styled(
            title,
            Style::new().fg(Color::Rgb(197, 197, 197)).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(artist, STYLE_FOLDER_NORMAL)),
        Line::from(Span::styled(album, STYLE_FOLDER_NORMAL)),
    ];

    let meta_paragraph = Paragraph::new(meta_text);

    let bar_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(chunks[1]);

    f.render_widget(meta_paragraph, bar_chunks[0]);

    let width = bar_chunks[2].width as usize;
    let filled = (width as f64 * progress) as usize;
    let bar_filled = "=".repeat(filled);
    let bar_empty = "-".repeat(width.saturating_sub(filled));
    let time_info = format!(
        " {}:{:02} / {}:{:02} | Vol: {:.0}% ",
        (pos / 60.0) as u32,
        (pos % 60.0) as u32,
        (dur / 60.0) as u32,
        (dur % 60.0) as u32,
        vol
    );

    let progress_text = vec![
        Line::from(vec![
            Span::styled(bar_filled, STYLE_FOLDER_NORMAL),
            Span::styled(bar_empty, Style::new().fg(Color::Rgb(51, 51, 51))),
        ]),
        Line::from(Span::styled(time_info, STYLE_FOLDER_NORMAL)),
    ];
    f.render_widget(Paragraph::new(progress_text), bar_chunks[2]);

    let lyric_lines = if !app.lyrics.is_empty() {
        let active_index = app
            .lyrics
            .iter()
            .position(|l| l.time > pos)
            .map(|i| i.saturating_sub(1))
            .unwrap_or(app.lyrics.len().saturating_sub(1));

        let mut lines = Vec::new();
        for i in (active_index.saturating_sub(2))..=(active_index + 2) {
            if let Some(line) = app.lyrics.get(i) {
                let style = if i == active_index {
                    Style::new()
                        .fg(Color::Rgb(197, 197, 197))
                        .add_modifier(Modifier::BOLD)
                } else {
                    STYLE_QUEUE_PLAYED
                };
                lines.push(
                    Line::from(Span::styled(line.text.as_str(), style))
                        .alignment(Alignment::Center),
                );
            } else {
                lines.push(Line::from(""));
            }
        }
        lines
    } else {
        vec![Line::from("No lyrics found").alignment(Alignment::Center)]
    };

    let lyric_paragraph = Paragraph::new(lyric_lines)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);
    f.render_widget(lyric_paragraph, chunks[2]);
}

fn render_settings(f: &mut Frame, app: &mut App, area: Rect) {
    let popup_w = 52u16;
    let popup_h = 16u16;
    let x = area.x.saturating_add((area.width.saturating_sub(popup_w)) / 2);
    let y = area.y.saturating_add((area.height.saturating_sub(popup_h)) / 2);
    let popup = Rect::new(x, y, popup_w.min(area.width), popup_h.min(area.height));

    f.render_widget(ratatui::widgets::Clear, popup);

    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .style(Style::new().fg(Color::Rgb(197, 197, 197)))
        .border_style(Style::new().fg(Color::Rgb(197, 197, 197)));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let field_style = |app: &App, idx: usize| -> Style {
        let mut s = if app.settings_editing == Some(idx) {
            Style::new().fg(Color::Rgb(255, 255, 255)).bg(Color::Rgb(50, 50, 50))
        } else if app.settings_focus == idx {
            Style::new().fg(Color::Rgb(197, 197, 197)).add_modifier(Modifier::REVERSED)
        } else {
            Style::new().fg(Color::Rgb(136, 136, 136))
        };
        if app.settings_editing == Some(idx) {
            s = s.add_modifier(Modifier::UNDERLINED);
        }
        s
    };

    let fields = [
        ("API Key    ", 0, &app.settings_api_key_buf),
        ("API Secret ", 1, &app.settings_api_secret_buf),
        ("Auth Token ", 2, &app.settings_auth_token_buf),
    ];

    for (i, row) in rows.iter().take(3).enumerate() {
        let (label, idx, value) = &fields[i];
        let display = if app.settings_editing == Some(*idx) {
            format!("{}: {}▏", label, value)
        } else {
            format!("{}: {}", label, if value.is_empty() { "(empty)" } else { value })
        };
        let paragraph = Paragraph::new(display).style(field_style(app, *idx));
        f.render_widget(paragraph, *row);
    }

    let auth_status = if let Some(ref lastfm) = app.lastfm {
        if lastfm.is_authenticated() { "(authenticated)" } else { "(not authenticated)" }
    } else {
        "(not configured)"
    };

    let auth_label = format!("Status: {}", auth_status);
    let auth_style = if app.settings_focus == 3 {
        Style::new().fg(Color::Rgb(197, 197, 197)).add_modifier(Modifier::REVERSED)
    } else {
        Style::new().fg(Color::Rgb(136, 136, 136))
    };
    f.render_widget(Paragraph::new(auth_label).style(auth_style), rows[3]);

    let btn_style = |app: &App, idx: usize| -> Style {
        if app.settings_focus == idx {
            Style::new().fg(Color::Rgb(197, 197, 197)).add_modifier(Modifier::REVERSED)
        } else {
            Style::new().fg(Color::Rgb(136, 136, 136))
        }
    };

    f.render_widget(
        Paragraph::new("[ Authorize ]").style(btn_style(app, 3)).alignment(Alignment::Center),
        rows[4],
    );
    f.render_widget(
        Paragraph::new("[ Open Auth URL in Browser ]").style(btn_style(app, 4)).alignment(Alignment::Center),
        rows[5],
    );

    if let Some(ref msg) = app.settings_status_msg {
        f.render_widget(
            Paragraph::new(msg.as_str())
                .style(Style::new().fg(Color::Rgb(197, 197, 197)))
                .alignment(Alignment::Center),
            rows[6],
        );
    }

    if app.settings_editing.is_none() && app.settings_focus <= 2 {
        let help = if app.settings_focus <= 1 {
            "Enter to edit, Esc to close"
        } else {
            "Enter to authorize with token, Esc to close"
        };
        f.render_widget(
            Paragraph::new(help)
                .style(Style::new().fg(Color::Rgb(68, 68, 68)))
                .alignment(Alignment::Center),
            rows[6],
        );
    }
}
