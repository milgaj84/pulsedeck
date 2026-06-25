use super::*;
use std::time::{Duration, Instant};

const PERSIST_RETRY_COOLDOWN: Duration = Duration::from_secs(5);
const PERSIST_NOTICE_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct PersistFlags {
    ui_state_dirty: bool,
    history_dirty: bool,
    library_dirty: bool,
    retry: PersistenceRetry,
}

#[derive(Debug, Clone, Default)]
struct PersistenceRetry {
    next_retry_at: Option<Instant>,
    last_error_key: Option<PersistenceErrorKey>,
    last_error_notice_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PersistenceErrorKey {
    target: PersistTarget,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistTarget {
    UiState,
    History,
    Library,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistenceFlushMode {
    Scheduled,
    Force,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PersistenceError {
    target: PersistTarget,
    message: String,
}

impl PersistenceError {
    fn new(target: PersistTarget, err: impl std::fmt::Display) -> Self {
        Self {
            target,
            message: err.to_string(),
        }
    }

    fn key(&self) -> PersistenceErrorKey {
        PersistenceErrorKey {
            target: self.target,
            message: self.message.clone(),
        }
    }

    fn notice_message(&self) -> String {
        match self.target {
            PersistTarget::UiState => format!("Could not save UI state: {}", self.message),
            PersistTarget::History => format!("Could not save history: {}", self.message),
            PersistTarget::Library => format!("Could not save library: {}", self.message),
        }
    }
}

impl PersistFlags {
    fn mark_ui_state_dirty(&mut self) {
        self.ui_state_dirty = true;
    }

    fn mark_history_dirty(&mut self) {
        self.history_dirty = true;
    }

    fn mark_library_dirty(&mut self) {
        self.library_dirty = true;
    }

    fn has_dirty_work(&self) -> bool {
        self.ui_state_dirty || self.history_dirty || self.library_dirty
    }

    fn retry_due(&self, now: Instant) -> bool {
        self.has_dirty_work()
            && self
                .retry
                .next_retry_at
                .is_none_or(|deadline| now >= deadline)
    }

    fn schedule_retry(&mut self, now: Instant) {
        self.retry.next_retry_at = Some(now + PERSIST_RETRY_COOLDOWN);
    }

    fn clear_retry_state(&mut self) {
        self.retry = PersistenceRetry::default();
    }

    fn should_show_error_notice(&self, key: &PersistenceErrorKey, now: Instant) -> bool {
        self.retry.last_error_key.as_ref() != Some(key)
            || self
                .retry
                .last_error_notice_at
                .is_none_or(|last| now.duration_since(last) >= PERSIST_NOTICE_COOLDOWN)
    }

    fn record_error_notice_state(
        &mut self,
        key: PersistenceErrorKey,
        now: Instant,
        notice_was_shown: bool,
    ) {
        self.retry.last_error_key = Some(key);
        if notice_was_shown {
            self.retry.last_error_notice_at = Some(now);
        }
    }
}

impl App {
    pub(super) fn mark_ui_state_dirty(&mut self) {
        self.persist.mark_ui_state_dirty();
    }

    pub(super) fn mark_history_dirty(&mut self) {
        self.persist.mark_history_dirty();
    }

    pub(super) fn mark_library_dirty(&mut self) {
        self.persist.mark_library_dirty();
    }

    /// Persist the current config to TOML. Shows an error notice on failure.
    pub(super) fn persist_config_change(&mut self) {
        let Some(config_dir) = self.config_dir.as_ref() else {
            return;
        };
        if let Err(msg) =
            crate::config_toml::io::save_config(config_dir, &self.config, &self.config_preserved)
        {
            self.set_error_notice(msg);
        }
    }

    pub(super) fn flush_persistence(&mut self) {
        self.flush_persistence_at(Instant::now(), PersistenceFlushMode::Scheduled);
    }

    pub(super) fn force_flush_persistence(&mut self) {
        self.flush_persistence_at(Instant::now(), PersistenceFlushMode::Force);
    }

    fn flush_persistence_at(&mut self, now: Instant, mode: PersistenceFlushMode) {
        if mode == PersistenceFlushMode::Scheduled && !self.persist.retry_due(now) {
            return;
        }

        match self.try_flush_persistence_once() {
            Ok(()) => self.persist.clear_retry_state(),
            Err(error) => self.handle_persistence_error(error, now),
        }
    }

    fn try_flush_persistence_once(&mut self) -> Result<(), PersistenceError> {
        let mut first_error = None;

        if self.persist.ui_state_dirty {
            let state = super::ui_state::UiState::from_app_values(
                self.playback.volume,
                self.playback.muted,
                self.ui.layout_mode,
                self.ui.visualizer_mode,
                self.ui.display_mode,
                self.stale_dismissed_at,
            );

            match state.save() {
                Ok(()) => self.persist.ui_state_dirty = false,
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(PersistenceError::new(PersistTarget::UiState, err));
                    }
                }
            }
        }

        if self.persist.history_dirty {
            match self.history.save() {
                Ok(()) => self.persist.history_dirty = false,
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(PersistenceError::new(PersistTarget::History, err));
                    }
                }
            }
        }

        if self.persist.library_dirty {
            match self.library.save() {
                Ok(()) => self.persist.library_dirty = false,
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(PersistenceError::new(PersistTarget::Library, err));
                    }
                }
            }
        }

        first_error.map_or(Ok(()), Err)
    }

    fn handle_persistence_error(&mut self, error: PersistenceError, now: Instant) {
        self.persist.schedule_retry(now);

        let key = error.key();
        let should_notice = self.persist.should_show_error_notice(&key, now);
        let message = should_notice.then(|| error.notice_message());
        self.persist
            .record_error_notice_state(key, now, should_notice);

        if let Some(message) = message {
            self.set_error_notice(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn station() -> Station {
        Station::basic(
            "Persist Test",
            "http://persist.test/stream",
            "Radio",
            "US",
            128,
        )
    }

    fn unique_temp_dir(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "pulsedeck-{test_name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp test directory");
        dir
    }

    fn app_with_library_path(path: PathBuf) -> App {
        let mut library = Library::in_memory(vec![station()]);
        library.path = Some(path);
        App::new(library)
    }

    fn app_with_unwritable_library(test_name: &str) -> (App, PathBuf) {
        let dir = unique_temp_dir(test_name);
        let blocker = dir.join("not_a_directory");
        fs::write(&blocker, "blocker").expect("write blocker file");
        let app = app_with_library_path(blocker.join("library.json"));
        (app, dir)
    }

    #[test]
    fn flush_persistence_with_no_dirty_work_does_not_schedule_retry() {
        let mut app = App::new(Library::in_memory(vec![station()]));
        let now = Instant::now();

        app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

        assert!(app.persist.retry.next_retry_at.is_none());
    }

    #[test]
    fn failed_library_save_keeps_dirty_flag_and_schedules_retry() {
        let (mut app, _dir) = app_with_unwritable_library("failed-library-save");
        app.mark_library_dirty();

        let now = Instant::now();
        app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

        assert!(app.persist.library_dirty);
        assert_eq!(
            app.persist.retry.next_retry_at,
            Some(now + PERSIST_RETRY_COOLDOWN)
        );
        assert!(matches!(app.ui.notice.current, Some(AppNotice::Error(_))));
    }

    #[test]
    fn scheduled_flush_before_retry_deadline_skips_disk_work() {
        let (mut app, _dir) = app_with_unwritable_library("scheduled-skip-before-deadline");
        app.mark_library_dirty();

        let now = Instant::now();
        app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);
        let first_notice = app.ui.notice.current.clone();
        let first_retry = app.persist.retry.next_retry_at;

        app.flush_persistence_at(
            now + Duration::from_secs(1),
            PersistenceFlushMode::Scheduled,
        );

        assert!(app.persist.library_dirty);
        assert_eq!(app.ui.notice.current, first_notice);
        assert_eq!(app.persist.retry.next_retry_at, first_retry);
    }

    #[test]
    fn scheduled_flush_at_retry_deadline_retries() {
        let (mut app, _dir) = app_with_unwritable_library("scheduled-retry-at-deadline");
        app.mark_library_dirty();

        let now = Instant::now();
        app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

        let retry_at = now + PERSIST_RETRY_COOLDOWN;
        app.flush_persistence_at(retry_at, PersistenceFlushMode::Scheduled);

        assert!(app.persist.library_dirty);
        assert_eq!(
            app.persist.retry.next_retry_at,
            Some(retry_at + PERSIST_RETRY_COOLDOWN)
        );
    }

    #[test]
    fn force_flush_ignores_retry_deadline() {
        let (mut app, _dir) = app_with_unwritable_library("force-flush-ignores-deadline");
        app.mark_library_dirty();

        let now = Instant::now();
        app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

        let forced_at = now + Duration::from_secs(1);
        app.flush_persistence_at(forced_at, PersistenceFlushMode::Force);

        assert!(app.persist.library_dirty);
        assert_eq!(
            app.persist.retry.next_retry_at,
            Some(forced_at + PERSIST_RETRY_COOLDOWN)
        );
    }

    #[test]
    fn successful_save_clears_retry_state_and_dirty_flag() {
        let dir = unique_temp_dir("successful-save-clears-retry");
        let library_path = dir.join("library.json");
        let mut app = app_with_library_path(library_path.clone());
        app.mark_library_dirty();

        let now = Instant::now();
        app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);

        assert!(!app.persist.library_dirty);
        assert!(app.persist.retry.next_retry_at.is_none());
        assert!(app.persist.retry.last_error_key.is_none());
        assert!(library_path.exists());
    }

    #[test]
    fn repeated_same_persistence_error_does_not_refresh_notice_before_notice_cooldown() {
        let (mut app, _dir) = app_with_unwritable_library("same-error-notice-throttle");
        app.mark_library_dirty();

        let now = Instant::now();
        app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);
        let first_notice = app.ui.notice.current.clone();
        let first_notice_at = app.persist.retry.last_error_notice_at;

        app.flush_persistence_at(
            now + PERSIST_RETRY_COOLDOWN,
            PersistenceFlushMode::Scheduled,
        );

        assert_eq!(app.ui.notice.current, first_notice);
        assert_eq!(app.persist.retry.last_error_notice_at, first_notice_at);
    }

    #[test]
    fn repeated_same_persistence_error_refreshes_notice_after_notice_cooldown() {
        let (mut app, _dir) = app_with_unwritable_library("same-error-notice-refresh");
        app.mark_library_dirty();

        let now = Instant::now();
        app.flush_persistence_at(now, PersistenceFlushMode::Scheduled);
        let first_notice_at = app.persist.retry.last_error_notice_at;

        app.flush_persistence_at(
            now + PERSIST_NOTICE_COOLDOWN + PERSIST_RETRY_COOLDOWN,
            PersistenceFlushMode::Scheduled,
        );

        assert_ne!(app.persist.retry.last_error_notice_at, first_notice_at);
    }

    #[test]
    fn persist_config_change_writes_toml_file() {
        let dir = unique_temp_dir("persist-config-change-writes");
        let mut app = App::new(Library::in_memory(vec![station()]));
        app.config_dir = Some(dir.clone());
        app.config.audio.default_volume = 42;

        app.persist_config_change();

        let written = fs::read_to_string(dir.join("pulsedeck.toml")).unwrap();
        assert!(written.contains("default_volume = 42"));
    }

    #[test]
    fn persist_config_change_shows_error_notice_on_failure() {
        let dir = unique_temp_dir("persist-config-change-error");
        let blocker = dir.join("blocker_file");
        fs::write(&blocker, "blocks dir creation").unwrap();
        let impossible_dir = blocker.join("subdir");

        let mut app = App::new(Library::in_memory(vec![station()]));
        app.config_dir = Some(impossible_dir);

        app.persist_config_change();

        assert!(
            matches!(app.ui.notice.current, Some(AppNotice::Error(ref msg)) if msg.contains("Could not create config directory"))
        );
    }

    #[test]
    fn persist_config_change_does_nothing_when_config_dir_is_none() {
        let mut app = App::new(Library::in_memory(vec![station()]));
        app.config_dir = None;

        app.persist_config_change();

        assert!(
            app.ui.notice.current.is_none()
                || !matches!(app.ui.notice.current, Some(AppNotice::Error(_)))
        );
    }
}
