mod action;
mod app;
mod audio;
mod event;
mod favorites;
mod radio;
mod ui;

use anyhow::Result;
use std::time::Duration;

use app::{App, InputMode};
use favorites::Library;
use radio::fallback_stations;

struct TerminalRestoreGuard;

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load your station library (seeds with starter stations on first launch)
    let library = Library::load(fallback_stations());

    // Initialize theme from saved settings
    let saved_theme = ui::theme::ThemeName::from_key(&library.settings.theme);
    ui::theme::set_active(saved_theme);

    let mut app = App::new(library);

    // Initialize terminal. The guard restores the terminal even if the loop exits early with an error.
    let mut terminal = ratatui::init();
    let _terminal_restore = TerminalRestoreGuard;

    // ── Channel for API search results ───────────────────────────
    let (search_tx, mut search_rx) =
        tokio::sync::mpsc::unbounded_channel::<(String, Vec<radio::Station>)>();

    // ── Main Loop ────────────────────────────────────────────────
    let tick_rate = Duration::from_millis(66); // ~15 FPS

    loop {
        // Draw
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // Poll for user input (mode-aware key mapping)
        if let Some(action) = event::poll_action(tick_rate, &app.input_mode) {
            app.update(action);
        } else {
            app.update(action::Action::Tick);
        }

        // Keep short search queries honest: API search starts at 2+ chars, so clear older results below that.
        if matches!(app.input_mode, InputMode::Search) && app.search_query.trim().len() < 2 {
            app.search_results.clear();
            app.searching_api = false;
        }

        // ── Check for pending API search requests ────────────────
        if let Some(query) = app.pending_api_search.take() {
            let tx = search_tx.clone();
            tokio::spawn(async move {
                if let Ok(results) = radio::search_stations(&query).await {
                    let _ = tx.send((query, results));
                }
            });
        }

        // ── Check for API search results (non-blocking) ─────────
        while let Ok((query, results)) = search_rx.try_recv() {
            let current_query = app.search_query.trim();
            if matches!(app.input_mode, InputMode::Search) && query == current_query {
                app.set_search_results(results);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
