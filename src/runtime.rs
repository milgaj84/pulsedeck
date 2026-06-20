use std::time::{Duration, Instant};

use crate::{app::App, radio};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

type SearchWorkerResponse = (String, Result<Vec<radio::Station>, String>);
type MetadataRefreshWorkerResponse = Result<(usize, Vec<radio::Station>, usize), String>;

pub struct AppDriver {
    search_tx: tokio::sync::mpsc::UnboundedSender<SearchWorkerResponse>,
    search_rx: tokio::sync::mpsc::UnboundedReceiver<SearchWorkerResponse>,
    metadata_tx: tokio::sync::mpsc::UnboundedSender<MetadataRefreshWorkerResponse>,
    metadata_rx: tokio::sync::mpsc::UnboundedReceiver<MetadataRefreshWorkerResponse>,
    search_debounce: Option<(String, Instant)>,
}

impl AppDriver {
    pub fn new() -> Self {
        let (search_tx, search_rx) = tokio::sync::mpsc::unbounded_channel();
        let (metadata_tx, metadata_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            search_tx,
            search_rx,
            metadata_tx,
            metadata_rx,
            search_debounce: None,
        }
    }

    pub fn tick(&mut self, app: &mut App) {
        self.update_search_debounce(app);
        self.spawn_ready_search(app);
        self.drain_search_responses(app);
        self.spawn_metadata_refresh_if_requested(app);
        self.drain_metadata_refresh_responses(app);
    }

    fn update_search_debounce(&mut self, app: &App) {
        if let Some(query) = app.current_debounce_query().map(str::to_string) {
            match &self.search_debounce {
                Some((pending_query, _deadline)) if pending_query == &query => {}
                _ => {
                    self.search_debounce = Some((query, Instant::now() + SEARCH_DEBOUNCE));
                }
            }
        } else {
            self.search_debounce = None;
        }
    }

    fn spawn_ready_search(&mut self, app: &mut App) {
        let Some((query, deadline)) = self.search_debounce.as_ref() else {
            return;
        };

        if Instant::now() < *deadline {
            return;
        }

        let query = query.clone();
        self.search_debounce = None;

        if app.mark_search_started(&query) {
            let tx = self.search_tx.clone();
            tokio::spawn(async move {
                let result = radio::search_stations(&query)
                    .await
                    .map_err(|err| err.to_string());
                let _ = tx.send((query, result));
            });
        }
    }

    fn drain_search_responses(&mut self, app: &mut App) {
        while let Ok((query, result)) = self.search_rx.try_recv() {
            app.apply_search_response(query, result);
        }
    }

    fn spawn_metadata_refresh_if_requested(&mut self, app: &mut App) {
        let Some(stations) = app.take_metadata_refresh_request() else {
            return;
        };

        let tx = self.metadata_tx.clone();
        tokio::spawn(async move {
            let checked = stations.len();
            let mut matches = Vec::new();
            let mut failed = 0;

            for station in stations {
                match radio::lookup_station_metadata(&station).await {
                    Ok(Some(metadata)) => matches.push(metadata),
                    Ok(None) => {}
                    Err(_) => failed += 1,
                }
            }

            let _ = tx.send(Ok((checked, matches, failed)));
        });
    }

    fn drain_metadata_refresh_responses(&mut self, app: &mut App) {
        while let Ok(result) = self.metadata_rx.try_recv() {
            app.apply_metadata_refresh_response(result);
        }
    }
}

impl Default for AppDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{InputMode, SearchStatus};
    use crate::favorites::Library;

    fn test_app() -> App {
        App::new(Library::in_memory(vec![]))
    }

    fn driver_with_search_debounce(query: impl Into<String>, deadline: Instant) -> AppDriver {
        let mut driver = AppDriver::new();
        driver.search_debounce = Some((query.into(), deadline));
        driver
    }

    #[test]
    fn driver_clears_search_debounce_when_app_is_not_debouncing() {
        let mut driver = driver_with_search_debounce("lofi", Instant::now() + Duration::from_secs(1));
        let app = test_app();

        driver.update_search_debounce(&app);

        assert!(driver.search_debounce.is_none());
    }

    #[test]
    fn driver_keeps_existing_deadline_for_same_debounce_query() {
        let mut app = test_app();
        app.input_mode = InputMode::Search;
        app.search.query = "lofi".to_string();
        app.search.status = SearchStatus::Debouncing {
            query: "lofi".to_string(),
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut driver = driver_with_search_debounce("lofi", deadline);

        driver.update_search_debounce(&app);

        assert_eq!(driver.search_debounce.as_ref().map(|(_, d)| *d), Some(deadline));
    }

    #[test]
    fn driver_replaces_deadline_for_new_debounce_query() {
        let mut app = test_app();
        app.input_mode = InputMode::Search;
        app.search.query = "ambient".to_string();
        app.search.status = SearchStatus::Debouncing {
            query: "ambient".to_string(),
        };
        let old_deadline = Instant::now() + Duration::from_secs(60);
        let mut driver = driver_with_search_debounce("lofi", old_deadline);

        driver.update_search_debounce(&app);

        assert_eq!(
            driver.search_debounce.as_ref().map(|(query, _)| query.as_str()),
            Some("ambient")
        );
        assert_ne!(driver.search_debounce.as_ref().map(|(_, d)| *d), Some(old_deadline));
    }

    #[test]
    fn driver_drains_metadata_refresh_responses_into_app() {
        let mut driver = AppDriver::new();
        let mut app = test_app();
        driver.metadata_tx.send(Ok((0, Vec::new(), 0))).unwrap();

        driver.drain_metadata_refresh_responses(&mut app);

        assert!(matches!(
            app.notice.current,
            Some(crate::app::AppNotice::Info(ref message))
                if message == "Metadata refresh: 0 checked, 0 enriched, 0 unchanged, 0 failed"
        ));
    }
}
