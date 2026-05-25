mod action;
mod app;
mod audio;
mod event;
mod favorites;
mod radio;
mod ui;

use anyhow::Result;
use std::time::{Duration, Instant};

use app::App;
use favorites::Library;
use radio::fallback_stations;

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

struct TerminalRestoreGuard;

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let library = Library::load(fallback_stations());

    let saved_theme = ui::theme::ThemeName::from_key(&library.settings.theme);
    ui::theme::set_active(saved_theme);

    let mut app = App::new(library);

    let mut terminal = ratatui::init();
    let _terminal_restore = TerminalRestoreGuard;

    let (search_tx, mut search_rx) =
        tokio::sync::mpsc::unbounded_channel::<(String, Result<Vec<radio::Station>, String>)>();

    let tick_rate = Duration::from_millis(66);
    let mut search_debounce: Option<(String, Instant)> = None;

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Some(action) = event::poll_action(tick_rate, &app.input_mode) {
            app.update(action);
        } else {
            app.update(action::Action::Tick);
        }

        if let Some(query) = app.current_debounce_query().map(str::to_string) {
            match &search_debounce {
                Some((pending_query, _deadline)) if pending_query == &query => {}
                _ => {
                    search_debounce = Some((query, Instant::now() + SEARCH_DEBOUNCE));
                }
            }
        } else {
            search_debounce = None;
        }

        if let Some((query, deadline)) = search_debounce.as_ref() {
            if Instant::now() >= *deadline {
                let query = query.clone();
                search_debounce = None;

                if app.mark_search_started(&query) {
                    let tx = search_tx.clone();
                    tokio::spawn(async move {
                        let result = radio::search_stations(&query)
                            .await
                            .map_err(|err| err.to_string());
                        let _ = tx.send((query, result));
                    });
                }
            }
        }

        while let Ok((query, result)) = search_rx.try_recv() {
            app.apply_search_response(query, result);
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
