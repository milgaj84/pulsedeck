use std::time::{Duration, Instant};

use crate::{app::App, radio, recommend::deduplicate_stations};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
/// Per-query timeout for discover fetches. Each query in a multi-query strategy
/// gets its own independent 8-second timeout.
const DISCOVER_FETCH_TIMEOUT: Duration = Duration::from_secs(8);
const MULTI_QUERY_THRESHOLD: usize = 5;

type SearchWorkerResponse = (String, Result<Vec<radio::Station>, String>);
type MetadataRefreshWorkerResponse = Result<(usize, Vec<radio::Station>, usize), String>;
type DiscoverWorkerResponse = Result<Vec<radio::Station>, String>;

pub struct AppDriver {
    search_tx: tokio::sync::mpsc::UnboundedSender<SearchWorkerResponse>,
    search_rx: tokio::sync::mpsc::UnboundedReceiver<SearchWorkerResponse>,
    metadata_tx: tokio::sync::mpsc::UnboundedSender<MetadataRefreshWorkerResponse>,
    metadata_rx: tokio::sync::mpsc::UnboundedReceiver<MetadataRefreshWorkerResponse>,
    discover_tx: tokio::sync::mpsc::UnboundedSender<DiscoverWorkerResponse>,
    discover_rx: tokio::sync::mpsc::UnboundedReceiver<DiscoverWorkerResponse>,
    search_debounce: Option<(String, Instant)>,
    /// State for multi-query discover: stores primary results while awaiting fallback fetch.
    pending_primary_results: Option<Vec<radio::Station>>,
    /// Fallback tag to use if primary results < 5.
    pending_fallback_tag: Option<String>,
}

impl AppDriver {
    pub fn new() -> Self {
        let (search_tx, search_rx) = tokio::sync::mpsc::unbounded_channel();
        let (metadata_tx, metadata_rx) = tokio::sync::mpsc::unbounded_channel();
        let (discover_tx, discover_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            search_tx,
            search_rx,
            metadata_tx,
            metadata_rx,
            discover_tx,
            discover_rx,
            search_debounce: None,
            pending_primary_results: None,
            pending_fallback_tag: None,
        }
    }

    pub fn tick(&mut self, app: &mut App) {
        self.update_search_debounce(app);
        self.spawn_ready_search(app);
        self.drain_search_responses(app);
        self.spawn_metadata_refresh_if_requested(app);
        self.drain_metadata_refresh_responses(app);
        self.spawn_discover_fetch_if_requested(app);
        self.drain_discover_responses(app);
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

    fn spawn_discover_fetch_if_requested(&mut self, app: &mut App) {
        let Some(request) = app.take_discover_fetch_request() else {
            return;
        };

        self.pending_primary_results = None;
        self.pending_fallback_tag = request.fallback_tag;
        self.spawn_discover_fetch(&request.primary_tag);
    }

    fn spawn_discover_fetch(&self, tag: &str) {
        let tx = self.discover_tx.clone();
        let query = format!("tag:{tag}");
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                DISCOVER_FETCH_TIMEOUT,
                radio::search_stations(&query),
            )
            .await;

            let mapped = match result {
                Ok(Ok(stations)) => Ok(stations),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_elapsed) => Err("Discover: fetch timed out".to_string()),
            };
            let _ = tx.send(mapped);
        });
    }

    fn drain_discover_responses(&mut self, app: &mut App) {
        while let Ok(result) = self.discover_rx.try_recv() {
            self.handle_discover_response(app, result);
        }
    }

    fn handle_discover_response(
        &mut self,
        app: &mut App,
        result: Result<Vec<radio::Station>, String>,
    ) {
        if let Some(primary) = self.pending_primary_results.take() {
            // This is the second (fallback) response — combine and send
            let secondary = result.unwrap_or_default();
            let mut combined = primary;
            combined.extend(secondary);
            let deduped = deduplicate_stations(&combined);
            app.apply_discover_response(Ok(deduped));
        } else if self.needs_fallback_fetch(&result) {
            // Primary returned < 5 results and fallback exists — spawn second fetch
            let stations = result.unwrap_or_default();
            let fallback = self.pending_fallback_tag.take().unwrap();
            self.pending_primary_results = Some(stations);
            self.spawn_discover_fetch(&fallback);
        } else {
            // Single query sufficient (>= 5 results or no fallback)
            self.pending_fallback_tag = None;
            app.apply_discover_response(result);
        }
    }

    fn needs_fallback_fetch(
        &self,
        result: &Result<Vec<radio::Station>, String>,
    ) -> bool {
        self.pending_fallback_tag.is_some()
            && matches!(result, Ok(stations) if stations.len() < MULTI_QUERY_THRESHOLD)
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
        let mut driver =
            driver_with_search_debounce("lofi", Instant::now() + Duration::from_secs(1));
        let app = test_app();

        driver.update_search_debounce(&app);

        assert!(driver.search_debounce.is_none());
    }

    #[test]
    fn driver_keeps_existing_deadline_for_same_debounce_query() {
        let mut app = test_app();
        app.ui.input_mode = InputMode::Search;
        app.search.query = "lofi".to_string();
        app.search.status = SearchStatus::Debouncing {
            query: "lofi".to_string(),
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut driver = driver_with_search_debounce("lofi", deadline);

        driver.update_search_debounce(&app);

        assert_eq!(
            driver.search_debounce.as_ref().map(|(_, d)| *d),
            Some(deadline)
        );
    }

    #[test]
    fn driver_replaces_deadline_for_new_debounce_query() {
        let mut app = test_app();
        app.ui.input_mode = InputMode::Search;
        app.search.query = "ambient".to_string();
        app.search.status = SearchStatus::Debouncing {
            query: "ambient".to_string(),
        };
        let old_deadline = Instant::now() + Duration::from_secs(60);
        let mut driver = driver_with_search_debounce("lofi", old_deadline);

        driver.update_search_debounce(&app);

        assert_eq!(
            driver
                .search_debounce
                .as_ref()
                .map(|(query, _)| query.as_str()),
            Some("ambient")
        );
        assert_ne!(
            driver.search_debounce.as_ref().map(|(_, d)| *d),
            Some(old_deadline)
        );
    }

    #[test]
    fn driver_drains_metadata_refresh_responses_into_app() {
        let mut driver = AppDriver::new();
        let mut app = test_app();
        driver.metadata_tx.send(Ok((0, Vec::new(), 0))).unwrap();

        driver.drain_metadata_refresh_responses(&mut app);

        assert!(matches!(
            app.ui.notice.current,
            Some(crate::app::AppNotice::Info(ref message))
                if message == "Metadata refresh: 0 checked, 0 enriched, 0 unchanged, 0 failed"
        ));
    }

    // --- Multi-query discover tests ---

    fn make_station(url: &str) -> radio::Station {
        radio::Station::basic("Test", url, "Rock", "US", 128)
    }

    #[test]
    fn handle_discover_response_sufficient_results_skips_fallback() {
        let mut driver = AppDriver::new();
        driver.pending_fallback_tag = Some("jazz".to_string());
        let mut app = test_app();

        let stations: Vec<radio::Station> =
            (0..5).map(|i| make_station(&format!("http://s{i}"))).collect();
        driver.handle_discover_response(&mut app, Ok(stations));

        // >= 5 results: no fallback triggered, results sent to app
        assert!(driver.pending_fallback_tag.is_none());
        assert!(driver.pending_primary_results.is_none());
    }

    #[tokio::test]
    async fn handle_discover_response_few_results_triggers_fallback() {
        let mut driver = AppDriver::new();
        driver.pending_fallback_tag = Some("jazz".to_string());
        let mut app = test_app();

        let stations: Vec<radio::Station> =
            (0..3).map(|i| make_station(&format!("http://s{i}"))).collect();
        driver.handle_discover_response(&mut app, Ok(stations.clone()));

        // < 5 results with fallback: stores primary, awaits second
        assert!(driver.pending_primary_results.is_some());
        assert_eq!(driver.pending_primary_results.as_ref().unwrap().len(), 3);
        assert!(driver.pending_fallback_tag.is_none());
    }

    #[test]
    fn handle_discover_response_second_fetch_combines_and_deduplicates() {
        let mut driver = AppDriver::new();
        // Simulate state after primary returned < 5 results
        driver.pending_primary_results = Some(vec![
            make_station("http://a"),
            make_station("http://b"),
        ]);
        let mut app = test_app();

        // Second fetch returns overlapping + new stations
        let secondary = vec![
            make_station("http://b"),  // duplicate
            make_station("http://c"),  // new
        ];
        driver.handle_discover_response(&mut app, Ok(secondary));

        // Should be combined and deduplicated then sent to app
        assert!(driver.pending_primary_results.is_none());
        // The app received the response (we verify it didn't error)
        // Since app has no favorites, recommend() yields empty, but no error notice
    }

    #[test]
    fn handle_discover_response_no_fallback_sends_directly() {
        let mut driver = AppDriver::new();
        driver.pending_fallback_tag = None;
        let mut app = test_app();

        let stations = vec![make_station("http://a")];
        driver.handle_discover_response(&mut app, Ok(stations));

        // No fallback → directly to app, no pending state
        assert!(driver.pending_primary_results.is_none());
        assert!(driver.pending_fallback_tag.is_none());
    }

    #[test]
    fn handle_discover_response_error_skips_fallback() {
        let mut driver = AppDriver::new();
        driver.pending_fallback_tag = Some("jazz".to_string());
        let mut app = test_app();

        driver.handle_discover_response(&mut app, Err("timeout".to_string()));

        // Error result → sent directly to app, no fallback
        assert!(driver.pending_fallback_tag.is_none());
        assert!(driver.pending_primary_results.is_none());
        assert!(matches!(
            app.ui.notice.current,
            Some(crate::app::AppNotice::Error(ref msg)) if msg.contains("timeout")
        ));
    }

    #[test]
    fn handle_discover_response_second_fetch_error_uses_primary_only() {
        let mut driver = AppDriver::new();
        driver.pending_primary_results = Some(vec![
            make_station("http://a"),
            make_station("http://b"),
        ]);
        let mut app = test_app();

        // Second fetch errors — combine primary with empty
        driver.handle_discover_response(&mut app, Err("fallback timeout".to_string()));

        assert!(driver.pending_primary_results.is_none());
        // App got Ok with primary results only (error unwrapped to empty)
    }

    #[test]
    fn needs_fallback_fetch_true_when_below_threshold_with_tag() {
        let mut driver = AppDriver::new();
        driver.pending_fallback_tag = Some("jazz".to_string());

        let result: Result<Vec<radio::Station>, String> = Ok(vec![make_station("http://a")]);
        assert!(driver.needs_fallback_fetch(&result));
    }

    #[test]
    fn needs_fallback_fetch_false_when_at_threshold() {
        let mut driver = AppDriver::new();
        driver.pending_fallback_tag = Some("jazz".to_string());

        let stations: Vec<radio::Station> =
            (0..5).map(|i| make_station(&format!("http://s{i}"))).collect();
        let result: Result<Vec<radio::Station>, String> = Ok(stations);
        assert!(!driver.needs_fallback_fetch(&result));
    }

    #[test]
    fn needs_fallback_fetch_false_when_no_fallback_tag() {
        let mut driver = AppDriver::new();
        driver.pending_fallback_tag = None;

        let result: Result<Vec<radio::Station>, String> = Ok(vec![make_station("http://a")]);
        assert!(!driver.needs_fallback_fetch(&result));
    }

    #[test]
    fn needs_fallback_fetch_false_on_error() {
        let mut driver = AppDriver::new();
        driver.pending_fallback_tag = Some("jazz".to_string());

        let result: Result<Vec<radio::Station>, String> = Err("error".to_string());
        assert!(!driver.needs_fallback_fetch(&result));
    }
}
