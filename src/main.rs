mod action;
mod app;
mod audio;
mod event;
mod favorites;
mod radio;
mod ui;

use std::time::Duration;
use anyhow::Result;

use app::App;
use favorites::Library;
use radio::fallback_stations;

#[tokio::main]
async fn main() -> Result<()> {
    // Load your station library (seeds with starter stations on first launch)
    let library = Library::load(fallback_stations());

    // Initialize theme from saved settings
    let saved_theme = ui::theme::ThemeName::from_key(&library.settings.theme);
    ui::theme::set_active(saved_theme);

    let mut app = App::new(library);

    // Initialize terminal
    let mut terminal = ratatui::init();

    // ── Channel for API search results ───────────────────────────
    let (search_tx, mut search_rx) =
        tokio::sync::mpsc::unbounded_channel::<Vec<radio::Station>>();

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

        // ── Check for pending API search requests ────────────────
        if let Some(query) = app.pending_api_search.take() {
            let tx = search_tx.clone();
            tokio::spawn(async move {
                if let Ok(results) = radio::search_stations(&query).await {
                    let _ = tx.send(results);
                }
            });
        }

        // ── Check for API search results (non-blocking) ─────────
        if let Ok(results) = search_rx.try_recv() {
            app.set_search_results(results);
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    ratatui::restore();

    Ok(())
}
