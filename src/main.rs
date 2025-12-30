use std::io;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

mod app;
mod config;
mod models;
mod services;
mod ui;

use crate::app::{App, LoadingState, run_app};
use crate::config::load_config_or_prompt;
use crate::services::{get_backlog_ids, get_items};
use crate::ui::draw_status_screen;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (cfg, config_ok) = load_config_or_prompt();
    let mut app = App::new(cfg);
    let mut res = Ok(());

    if config_ok {
        while !matches!(app.loading_state, LoadingState::Error(_)) {
            if matches!(app.loading_state, LoadingState::Loading) {
                let board = app.current_board();
                let board_title = format!("{} Backlog", board.team);
                terminal.draw(|f| draw_status_screen(f, &format!("Loading {}...", board_title)))?;

                let fetch_result = async {
                    let ids =
                        get_backlog_ids(&board.organization, &board.project, &board.team).await?;
                    let items = get_items(&board.organization, &board.project, ids).await?;
                    Ok::<_, anyhow::Error>(items)
                }
                .await;

                match fetch_result {
                    Ok(items) => app.load_data(items),
                    Err(e) => {
                        let error_msg = format!("Failed to fetch data: {e:?}");
                        eprintln!("\n--- FATAL FETCH ERROR ---\n{}", error_msg);
                        app.loading_state = LoadingState::Error(error_msg);
                    }
                }
                continue;
            }

            res = run_app(&mut terminal, &mut app).await;
            if res.is_err() {
                break;
            }

            if matches!(app.loading_state, LoadingState::Loading) {
                continue;
            }
            break;
        }
    }

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
