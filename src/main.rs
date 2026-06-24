mod action;
mod app;
mod audio;
mod cli;
mod config;
mod config_toml;
pub(crate) mod elapsed_format;
pub(crate) mod elapsed_timer;
mod event;
mod favorites;
mod favorites_set;
mod history;
mod keybindings;
mod library_filter;
mod number_jump;
mod playlist;
mod playlist_export;
mod radio;
mod recent_ring;
mod recommend;

mod runtime;
mod scrobble;
mod text;
mod theme_name;
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

    let saved_theme = theme_name::ThemeName::from_key(&library.settings.theme);
    ui::theme::set_active(saved_theme);

    let mut app = App::new(library);

    let mut terminal = ratatui::init();
    let _terminal_restore = TerminalRestoreGuard;
    let mut driver = runtime::AppDriver::new();

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Some(action) =
            event::poll_action_with_registry(TICK_RATE, app.input_mode(), app.display_mode(), &app.keybinding_registry)
        {
            app.update(action);
        } else {
            app.update(action::Action::Tick);
        }

        driver.tick(&mut app);

        if app.should_quit() {
            break;
        }
    }

    Ok(())
}
