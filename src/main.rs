mod action;
mod app;
mod audio;
mod cli;
mod config;
mod event;
mod favorites;
mod history;
mod playlist;
mod playlist_export;
mod radio;
mod runtime;
mod text;
mod ui;

use anyhow::Result;
use std::time::Duration;

use app::App;
use favorites::Library;
use radio::fallback_stations;

const TICK_RATE: Duration = Duration::from_millis(66);

struct TerminalRestoreGuard;

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    if let cli::CliOutcome::Handled = cli::run(std::env::args())? {
        return Ok(());
    }

    let library = Library::load(fallback_stations());

    let saved_theme = ui::theme::ThemeName::from_key(&library.settings.theme);
    ui::theme::set_active(saved_theme);

    let mut app = App::new(library);

    let mut terminal = ratatui::init();
    let _terminal_restore = TerminalRestoreGuard;
    let mut driver = runtime::AppDriver::new();

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Some(action) = event::poll_action(TICK_RATE, &app.input_mode) {
            app.update(action);
        } else {
            app.update(action::Action::Tick);
        }

        driver.tick(&mut app);

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
