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
pub(crate) mod input_mode;
mod keybindings;
mod library_filter;
pub mod library_sort;
pub(crate) mod mtime_debounce;
mod number_jump;
mod persistence;
mod playlist;
mod playlist_export;
mod radio;
mod recent_ring;
mod recommend;
pub mod search_history;

mod runtime;
mod text;
mod theme_name;
mod ui;

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    if let cli::CliOutcome::Handled = cli::run(std::env::args())? {
        return Ok(());
    }

    // Custom panic hook: restore a clean terminal experience and point users
    // to where to report the issue. The TerminalRestoreGuard (below) restores
    // the raw/alternate-screen terminal on unwind, so this hook only needs to
    // add the friendly message; the original hook still prints the panic details.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("\nPulseDeck hit an unexpected error and will close safely.");
        eprintln!("Please report this at https://github.com/milgaj84/pulsedeck/issues");
        eprintln!("---");
        default_hook(info);
    }));

    let library = Library::load(fallback_stations());

    ui::theme::set_active(theme_name::ThemeName::Retrowave);

    let mut app = App::new(library);

    let mut terminal = ratatui::init();
    let _terminal_restore = TerminalRestoreGuard;
    let mut driver = runtime::AppDriver::new(radio::RadioBrowserApi);

    // Signal handling: on SIGINT (Ctrl+C) or SIGTERM (`kill <pid>`), trigger a
    // clean shutdown so the terminal is restored, pending state is flushed, and
    // audio is torn down — rather than the process dying mid-raw-mode.
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let shutdown_flag = Arc::clone(&shutdown_requested);
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            let mut terminate =
                match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                    Ok(sig) => sig,
                    Err(_) => return,
                };
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        shutdown_flag.store(true, Ordering::SeqCst);
    });

    loop {
        // Check for a requested shutdown before drawing so the terminal is
        // restored promptly and unsaved state is not lost on SIGINT/SIGTERM.
        if shutdown_requested.load(Ordering::SeqCst) {
            app.shutdown();
            break;
        }

        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Some(action) = event::poll_action_with_registry(
            TICK_RATE,
            app.input_mode(),
            app.display_mode(),
            &app.keybinding_registry,
        ) {
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
