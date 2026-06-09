mod app;
mod player;
mod metadata;
mod ui;
mod lyrics;

use std::{io, time::{Duration, Instant}, path::PathBuf};
use anyhow::Result;
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
use std::fs::OpenOptions;
use std::io::Write;

pub fn log(msg: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("audino.log") {
        writeln!(file, "{} - {}", Instant::now().elapsed().as_millis(), msg).ok();
    }
}

fn main() -> Result<()> {
    log("Starting audino");
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

    let library_path = library_path.unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Music")
    });
    let mut app = App::new(library_path);

    let tick_rate = Duration::from_millis(100);
    let res = run_app(&mut terminal, &mut app, tick_rate);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

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
        terminal.draw(|f| {
            ui::render(f, app);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => app.quit(),
                    KeyCode::Char('.') => {
                        app.show_hidden = !app.show_hidden;
                        app.update_folder_items();
                        app.selected_folder_index = 0;
                        app.folder_tree_state.select(Some(0));
                    }
                    KeyCode::Tab => app.next_focus(),
                    KeyCode::Char(' ') => {
                        let paused = app.player.get_paused().unwrap_or(false);
                        app.player.pause(!paused).ok();
                    }
                    KeyCode::Up => match app.focus {
                        Focus::Tree | Focus::Queue => app.move_up(),
                        Focus::Player => {
                            let vol = app.player.get_volume().unwrap_or(100.0);
                            app.player.set_volume((vol + 5.0).min(100.0)).ok();
                        }
                    },
                    KeyCode::Down => match app.focus {
                        Focus::Tree | Focus::Queue => app.move_down(),
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
                        _ => {}
                    },
                    KeyCode::Right => match app.focus {
                        Focus::Tree => app.enter_directory(),
                        Focus::Player => {
                            app.player.seek(5.0).ok();
                        }
                        _ => {}
                    },
                    KeyCode::Enter => match app.focus {
                        Focus::Tree => app.play_selected_tree(),
                        Focus::Queue => app.play_index(app.selected_queue_index),
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
