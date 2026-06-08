use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};
use crate::app::{App, Focus};

pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(8), // Player bar
        ])
        .split(f.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // Folder tree
            Constraint::Percentage(70), // Queue
        ])
        .split(chunks[0]);

    render_folder_tree(f, app, main_chunks[0]);
    render_queue(f, app, main_chunks[1]);
    render_player_bar(f, app, chunks[1]);
}

fn render_folder_tree(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app.folder_items.iter().enumerate().map(|(i, path)| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let prefix = if path.is_dir() { "▸ " } else { "  " };
        let style = if i == app.selected_folder_index {
            Style::default().fg(Color::Rgb(197, 197, 197)).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(136, 136, 136))
        };
        ListItem::new(format!("{}{}", prefix, name)).style(style)
    }).collect();

    let border_color = if app.focus == Focus::Tree {
        Color::Rgb(197, 197, 197)
    } else {
        Color::Rgb(136, 136, 136)
    };

    let block = Block::default()
        .title(" Folder Tree ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn render_queue(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app.queue.iter().enumerate().map(|(i, path)| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let mut style = if Some(i) == app.current_index {
            Style::default().fg(Color::Rgb(197, 197, 197)).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(136, 136, 136))
        };

        if i == app.selected_queue_index && app.focus == Focus::Queue {
            style = style.add_modifier(Modifier::REVERSED);
        }

        ListItem::new(name.to_string()).style(style)
    }).collect();

    let border_color = if app.focus == Focus::Queue {
        Color::Rgb(197, 197, 197)
    } else {
        Color::Rgb(136, 136, 136)
    };

    let block = Block::default()
        .title(" Queue ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn render_player_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let border_color = if app.focus == Focus::Player {
        Color::Rgb(197, 197, 197)
    } else {
        Color::Rgb(136, 136, 136)
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border_color));
    
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(20), // Album art area
            Constraint::Min(0),      // Metadata & Progress
            Constraint::Percentage(30), // Lyrics
        ])
        .split(inner_area);

    // Album Art
    if let Some(art) = &app.current_album_art {
        let image = ratatui_image::Image::new(art);
        f.render_widget(image, chunks[0]);
    }

    // Meta & Progress
    let pos = app.player.get_position().unwrap_or(0.0);
    let dur = app.player.get_duration().unwrap_or(0.0);
    let progress = if dur > 0.0 { pos / dur } else { 0.0 };
    let vol = app.player.get_volume().unwrap_or(100.0);

    let current_track_path = app.current_index.and_then(|i| app.queue.get(i));
    let (title, artist, album) = if let Some(path) = current_track_path {
        let meta = app.metadata.get_metadata(path).ok();
        (
            meta.as_ref().and_then(|m| m.title.clone()).unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().to_string()),
            meta.as_ref().and_then(|m| m.artist.clone()).unwrap_or_else(|| "Unknown Artist".to_string()),
            meta.as_ref().and_then(|m| m.album.clone()).unwrap_or_else(|| "Unknown Album".to_string()),
        )
    } else {
        ("No track playing".to_string(), "".to_string(), "".to_string())
    };

    let meta_text = vec![
        Line::from(Span::styled(title, Style::default().fg(Color::Rgb(197, 197, 197)).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(artist, Style::default().fg(Color::Rgb(136, 136, 136)))),
        Line::from(Span::styled(album, Style::default().fg(Color::Rgb(136, 136, 136)))),
    ];

    let meta_paragraph = Paragraph::new(meta_text);
    
    let bar_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Meta
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Progress
        ])
        .split(chunks[1]);

    f.render_widget(meta_paragraph, bar_chunks[0]);

    // Simple text-based progress bar
    let width = bar_chunks[2].width as usize;
    let filled = (width as f64 * progress) as usize;
    let bar_filled = "=".repeat(filled);
    let bar_empty = "-".repeat(width.saturating_sub(filled));
    let time_info = format!(" {:.0}:{:.0} / {:.0}:{:.0} | Vol: {:.0}% ", pos / 60.0, pos % 60.0, dur / 60.0, dur % 60.0, vol);
    
    let progress_text = vec![
        Line::from(vec![
            Span::styled(bar_filled, Style::default().fg(Color::Rgb(136, 136, 136))),
            Span::styled(bar_empty, Style::default().fg(Color::Rgb(51, 51, 51))),
        ]),
        Line::from(Span::styled(time_info, Style::default().fg(Color::Rgb(136, 136, 136)))),
    ];
    f.render_widget(Paragraph::new(progress_text), bar_chunks[2]);

    // Lyrics
    let lyric_lines = if !app.lyrics.is_empty() {
        let current_time = pos;
        let active_index = app.lyrics.iter().position(|l| l.time > current_time).map(|i| i.saturating_sub(1)).unwrap_or(app.lyrics.len().saturating_sub(1));
        
        let mut lines = Vec::new();
        for i in (active_index.saturating_sub(2))..=(active_index + 2) {
            if let Some(line) = app.lyrics.get(i) {
                let style = if i == active_index {
                    Style::default().fg(Color::Rgb(197, 197, 197)).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(68, 68, 68))
                };
                lines.push(Line::from(Span::styled(line.text.clone(), style)).alignment(ratatui::layout::Alignment::Center));
            } else {
                lines.push(Line::from(""));
            }
        }
        lines
    } else {
        vec![Line::from("No lyrics found").alignment(ratatui::layout::Alignment::Center)]
    };

    f.render_widget(Paragraph::new(lyric_lines), chunks[2]);
}
