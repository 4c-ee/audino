mod app;
mod player;
mod metadata;
mod ui;
mod lyrics;
mod theme;

use std::{io, thread, time::{Duration, Instant}, path::PathBuf};
use anyhow::Result;
use theme::Theme;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use app::{App, Focus};

#[cfg(debug_assertions)]
use std::fs::OpenOptions;
#[cfg(debug_assertions)]
use std::io::{BufWriter, Write};
#[cfg(debug_assertions)]
use std::sync::Mutex;

#[cfg(debug_assertions)]
static LOG_FILE: Mutex<Option<BufWriter<std::fs::File>>> = Mutex::new(None);

#[cfg(debug_assertions)]
pub fn log(msg: &str) {
    let mut guard = LOG_FILE.lock().expect("LOG_FILE poisoned");
    if guard.is_none() {
        if let Ok(file) = OpenOptions::new().create(true).append(true).open("audino.log") {
            *guard = Some(BufWriter::new(file));
        }
    }
    if let Some(writer) = guard.as_mut() {
        let _ = writeln!(
            writer,
            "{} - {}",
            Instant::now().elapsed().as_millis(),
            msg
        );
        let _ = writer.flush();
    }
}

#[cfg(not(debug_assertions))]
pub fn log(_msg: &str) {}

fn expand_tilde(path: &str) -> PathBuf {
    if let (Some(rest), Ok(home)) = (path.strip_prefix("~/"), std::env::var("HOME")) {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

fn main() -> Result<()> {
    log("Starting audino");

    let theme = Theme::load().unwrap_or_default();
    theme::set_global_theme(theme);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut library_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-p" {
            if let Some(p) = args.next() {
                library_path = Some(PathBuf::from(p));
            }
        }
    }

    let library_path = library_path
        .or_else(|| theme::config_value("library_path").map(|v| expand_tilde(&v)))
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join("Music")
        });
    let mut app = App::new(library_path);

    let tick_rate = Duration::from_millis(100);
    let res = run_app(&mut terminal, &mut app, tick_rate);

    // Restore terminal before any blocking I/O so the shell is usable immediately
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    ).ok();
    terminal.show_cursor().ok();

    // Flush metadata cache in a background thread so it doesn't delay the restore
    thread::spawn(move || {
        let _ = app.metadata.flush();
    });

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    tick_rate: Duration,
) -> Result<()>
where B::Error: std::error::Error + Send + Sync + 'static
{
    let mut last_tick = Instant::now();
    while app.running {
        app.handle_mpris_events();
        terminal.draw(|f| {
            ui::render(f, app);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if app.is_searching {
                    match key.code {
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                            app.search();
                        }
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            app.search();
                        }
                        KeyCode::Enter | KeyCode::Esc => {
                            app.is_searching = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => app.quit(),
                    KeyCode::Char('/') => {
                        if app.focus == Focus::Tree {
                            app.is_searching = true;
                            app.search_query.clear();
                        }
                    }
                    KeyCode::Char('.') => {
                        app.show_hidden = !app.show_hidden;
                        app.update_folder_items();
                        app.selected_folder_index = 0;
                        app.folder_tree_state.select(Some(0));
                    }
                    KeyCode::Tab => app.next_focus(),
                    KeyCode::Char(' ') => {
                        match app.focus {
                            Focus::Player => {
                                let paused = app.player.get_paused().unwrap_or(false);
                                app.player.pause(!paused).ok();
                            }
                            Focus::Queue => {
                                app.is_moving_track = !app.is_moving_track;
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Backspace | KeyCode::Char('d') => {
                        if app.focus == Focus::Queue {
                            app.remove_selected_queue_track();
                        }
                    }
                    KeyCode::Up => match app.focus {
                        Focus::Tree | Focus::Queue | Focus::QueueControls => app.move_up(),
                        Focus::Player => {
                            let vol = app.player.get_volume().unwrap_or(100.0);
                            app.player.set_volume((vol + 5.0).min(200.0)).ok();
                        }
                    },
                    KeyCode::Down => match app.focus {
                        Focus::Tree | Focus::Queue | Focus::QueueControls => app.move_down(),
                        Focus::Player => {
                            let vol = app.player.get_volume().unwrap_or(100.0);
                            app.player.set_volume((vol - 5.0).max(0.0)).ok();
                        }
                    },
                    KeyCode::Left => match app.focus {
                        Focus::Tree => app.go_back(),
                        Focus::Player => {
                            app.player.seek(-5.0).ok();
                        }
                        Focus::QueueControls => app.move_up(),
                        _ => {}
                    },
                    KeyCode::Right => match app.focus {
                        Focus::Tree => app.enter_directory(),
                        Focus::Player => {
                            app.player.seek(5.0).ok();
                        }
                        Focus::QueueControls => app.move_down(),
                        _ => {}
                    },
                    KeyCode::Enter => match app.focus {
                        Focus::Tree => app.play_selected_tree(),
                        Focus::Queue => app.play_index(app.selected_queue_index),
                        Focus::QueueControls => app.execute_selected_control(),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick();
            last_tick = Instant::now();
        }
    }
    Ok(())
}