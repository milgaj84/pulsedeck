//! Remote control: media keys and desktop media widgets.
//!
//! The desktop (through MPRIS on Linux and the BSDs) can ask PulseDeck to play,
//! pause, stop, skip and change the volume, and shows what is playing. The
//! transport is behind [`RemoteControl`] so the app logic here runs, and is
//! tested, without any desktop.
//!
//! Commands act on the app directly instead of going through `App::update`:
//! `update` hands actions to whichever overlay is open, and a media key should
//! work no matter what is on screen.

use super::*;

/// What the desktop asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Built by the platform transport (MPRIS); builds without one only use them in tests.
#[cfg_attr(not(test), allow(dead_code))]
pub enum RemoteCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    /// Volume in percent, 0 to 100.
    SetVolume(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemoteStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

/// What the desktop is shown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NowPlaying {
    pub status: RemoteStatus,
    pub station: Option<String>,
    /// The song, when the station reports one.
    pub title: Option<String>,
    /// Volume in percent; 0 while muted.
    pub volume: u8,
}

/// A way for the desktop to control the app.
pub trait RemoteControl: Send {
    /// Commands that arrived since the last call.
    fn poll(&mut self) -> Vec<RemoteCommand>;
    /// Tell the desktop what is playing. Called only when it changed.
    fn publish(&mut self, now: &NowPlaying);
}

/// No remote control (other platforms, tests, or switched off).
pub struct NullRemote;

impl RemoteControl for NullRemote {
    fn poll(&mut self) -> Vec<RemoteCommand> {
        Vec::new()
    }

    fn publish(&mut self, _now: &NowPlaying) {}
}

/// The remote control for this platform, or [`NullRemote`] when `enabled` is
/// false or the platform has none.
#[cfg(all(
    feature = "mpris",
    any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )
))]
pub fn system_remote(enabled: bool) -> Box<dyn RemoteControl> {
    if enabled {
        Box::new(crate::mpris::MprisRemote::start())
    } else {
        Box::new(NullRemote)
    }
}

#[cfg(not(all(
    feature = "mpris",
    any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )
)))]
pub fn system_remote(_enabled: bool) -> Box<dyn RemoteControl> {
    Box::new(NullRemote)
}

impl App {
    /// Handle the desktop's commands and tell it what changed. Runs every
    /// iteration of the main loop.
    pub fn process_remote(&mut self) {
        for command in self.remote.poll() {
            self.apply_remote_command(command);
        }
        let now = self.now_playing_snapshot();
        if self.remote_published.as_ref() != Some(&now) {
            self.remote.publish(&now);
            self.remote_published = Some(now);
        }
    }

    pub(super) fn apply_remote_command(&mut self, command: RemoteCommand) {
        use PlaybackState::{Connecting, FadingOut, Playing};
        let state = self.playback.view.state.clone();
        match command {
            RemoteCommand::Play => {
                if !matches!(state, Playing | Connecting | FadingOut { .. }) {
                    self.toggle_pause();
                }
            }
            RemoteCommand::Pause => {
                if state == Playing {
                    self.toggle_pause();
                }
            }
            RemoteCommand::PlayPause => self.toggle_pause(),
            RemoteCommand::Stop => {
                if state != PlaybackState::Stopped {
                    self.stop_playback();
                }
            }
            RemoteCommand::Next => self.play_adjacent_station(1),
            RemoteCommand::Previous => self.play_adjacent_station(-1),
            RemoteCommand::SetVolume(percent) => {
                self.playback.volume = percent.min(100);
                self.playback.muted = false;
                self.sync_volume();
                self.mark_ui_state_dirty();
            }
        }
    }

    /// Play the station after (or, with a negative `step`, before) the one
    /// that is playing in the list on screen, wrapping around. Without a
    /// playing station the selection is the starting point.
    pub(super) fn play_adjacent_station(&mut self, step: isize) {
        let stations = self.visible_stations();
        if stations.is_empty() {
            return;
        }
        let from = self
            .playback
            .view
            .playing_url
            .as_deref()
            .and_then(|url| {
                stations
                    .iter()
                    .position(|station| crate::radio::station_url_matches(&station.url, url))
            })
            .unwrap_or(self.ui.nav.selected.min(stations.len() - 1));
        let count = stations.len() as isize;
        let next = (from as isize + step).rem_euclid(count) as usize;
        drop(stations);

        self.ui.nav.selected = next;
        self.play_selected();
    }

    pub(super) fn now_playing_snapshot(&self) -> NowPlaying {
        let view = &self.playback.view;
        let status = match &view.state {
            PlaybackState::Playing | PlaybackState::Connecting => RemoteStatus::Playing,
            PlaybackState::Paused => RemoteStatus::Paused,
            PlaybackState::FadingOut { .. } if !view.intentional_stop => RemoteStatus::Playing,
            PlaybackState::FadingOut { .. } | PlaybackState::Stopped | PlaybackState::Error(_) => {
                RemoteStatus::Stopped
            }
        };
        let (station, title) = if status == RemoteStatus::Stopped {
            (None, None)
        } else {
            (
                self.now_playing().map(|station| station.name.clone()),
                view.current_track
                    .as_ref()
                    .map(|title| title.trim().to_string())
                    .filter(|title| !title.is_empty()),
            )
        };
        NowPlaying {
            status,
            station,
            title,
            volume: if self.playback.muted {
                0
            } else {
                self.playback.volume
            },
        }
    }
}

/// A remote control for tests: commands are queued by the test and what was
/// published is recorded.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct FakeRemote {
    pub inbox: std::sync::Arc<std::sync::Mutex<Vec<RemoteCommand>>>,
    pub published: std::sync::Arc<std::sync::Mutex<Vec<NowPlaying>>>,
}

#[cfg(test)]
impl RemoteControl for FakeRemote {
    fn poll(&mut self) -> Vec<RemoteCommand> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }

    fn publish(&mut self, now: &NowPlaying) {
        self.published.lock().unwrap().push(now.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn app_with_remote() -> (App, FakeRemote) {
        let mut app = App::new(Library::in_memory(vec![
            station("A", "http://a"),
            station("B", "http://b"),
            station("C", "http://c"),
        ]));
        let remote = FakeRemote::default();
        app.remote = Box::new(remote.clone());
        (app, remote)
    }

    fn send(app: &mut App, remote: &FakeRemote, command: RemoteCommand) {
        remote.inbox.lock().unwrap().push(command);
        app.process_remote();
    }

    fn playing_station(app: &mut App, index: usize) {
        app.ui.nav.selected = index;
        app.play_selected();
        app.playback.view.state = PlaybackState::Playing;
    }

    #[test]
    fn play_pause_starts_the_selected_station_when_stopped() {
        let (mut app, remote) = app_with_remote();
        app.ui.nav.selected = 2;

        send(&mut app, &remote, RemoteCommand::PlayPause);

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://c"));
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
    }

    #[test]
    fn play_pause_pauses_the_elapsed_clock_of_a_playing_stream() {
        let (mut app, remote) = app_with_remote();
        playing_station(&mut app, 0);
        app.playback.elapsed_timer.start();

        send(&mut app, &remote, RemoteCommand::PlayPause);

        assert!(!app.playback.elapsed_timer.is_running());
    }

    #[test]
    fn play_starts_the_selected_station_when_stopped() {
        let (mut app, remote) = app_with_remote();
        app.ui.nav.selected = 1;

        send(&mut app, &remote, RemoteCommand::Play);

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://b"));
    }

    #[test]
    fn play_does_nothing_while_playing() {
        let (mut app, remote) = app_with_remote();
        playing_station(&mut app, 0);

        send(&mut app, &remote, RemoteCommand::Play);

        assert_eq!(app.playback.view.state, PlaybackState::Playing);
    }

    #[test]
    fn pause_only_acts_on_a_playing_stream() {
        let (mut app, remote) = app_with_remote();
        send(&mut app, &remote, RemoteCommand::Pause);
        assert_eq!(app.playback.view.state, PlaybackState::Stopped);
        assert!(app.playback.view.playing_url.is_none());
    }

    #[test]
    fn stop_stops_and_is_idempotent() {
        let (mut app, remote) = app_with_remote();
        playing_station(&mut app, 0);

        send(&mut app, &remote, RemoteCommand::Stop);
        assert!(matches!(
            app.playback.view.state,
            PlaybackState::FadingOut { .. }
        ));
        assert!(app.playback.view.intentional_stop);

        app.playback.view.state = PlaybackState::Stopped;
        send(&mut app, &remote, RemoteCommand::Stop);
        assert_eq!(app.playback.view.state, PlaybackState::Stopped);
    }

    #[test]
    fn next_and_previous_move_through_the_list_and_wrap() {
        let (mut app, remote) = app_with_remote();
        playing_station(&mut app, 2);

        send(&mut app, &remote, RemoteCommand::Next);
        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://a"));

        send(&mut app, &remote, RemoteCommand::Previous);
        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://c"));
        send(&mut app, &remote, RemoteCommand::Previous);
        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://b"));
        assert_eq!(app.ui.nav.selected, 1, "the cursor follows");
    }

    #[test]
    fn next_follows_the_playing_station_not_the_cursor() {
        let (mut app, remote) = app_with_remote();
        playing_station(&mut app, 0);
        app.ui.nav.selected = 2;

        send(&mut app, &remote, RemoteCommand::Next);

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://b"));
    }

    #[test]
    fn next_with_nothing_playing_starts_from_the_cursor() {
        let (mut app, remote) = app_with_remote();
        app.ui.nav.selected = 0;

        send(&mut app, &remote, RemoteCommand::Next);

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://b"));
    }

    #[test]
    fn next_in_an_empty_list_does_nothing() {
        let mut app = App::new(Library::in_memory(vec![]));
        let remote = FakeRemote::default();
        app.remote = Box::new(remote.clone());

        send(&mut app, &remote, RemoteCommand::Next);

        assert!(app.playback.view.playing_url.is_none());
    }

    #[test]
    fn set_volume_sets_the_level_and_unmutes() {
        let (mut app, remote) = app_with_remote();
        app.playback.muted = true;

        send(&mut app, &remote, RemoteCommand::SetVolume(35));
        assert_eq!(app.playback.volume, 35);
        assert!(!app.playback.muted);

        send(&mut app, &remote, RemoteCommand::SetVolume(250));
        assert_eq!(app.playback.volume, 100);
    }

    #[test]
    fn commands_work_while_an_overlay_is_open() {
        let (mut app, remote) = app_with_remote();
        app.ui.overlays.active = ActiveOverlay::Settings;
        app.ui.nav.selected = 1;

        send(&mut app, &remote, RemoteCommand::Play);

        assert_eq!(app.playback.view.playing_url.as_deref(), Some("http://b"));
    }

    #[test]
    fn the_snapshot_reports_status_station_title_and_volume() {
        let (mut app, _) = app_with_remote();
        assert_eq!(app.now_playing_snapshot().status, RemoteStatus::Stopped);
        assert_eq!(app.now_playing_snapshot().station, None);

        playing_station(&mut app, 1);
        app.playback.view.current_track = Some("  Artist - Song ".to_string());
        app.playback.volume = 55;
        let now = app.now_playing_snapshot();
        assert_eq!(now.status, RemoteStatus::Playing);
        assert_eq!(now.station.as_deref(), Some("B"));
        assert_eq!(now.title.as_deref(), Some("Artist - Song"));
        assert_eq!(now.volume, 55);

        app.playback.muted = true;
        assert_eq!(app.now_playing_snapshot().volume, 0);

        app.playback.view.state = PlaybackState::Paused;
        assert_eq!(app.now_playing_snapshot().status, RemoteStatus::Paused);

        app.playback.view.current_track = Some("   ".to_string());
        assert_eq!(app.now_playing_snapshot().title, None);
    }

    #[test]
    fn an_error_or_intentional_stop_reads_as_stopped() {
        let (mut app, _) = app_with_remote();
        playing_station(&mut app, 0);

        app.playback.view.state = PlaybackState::Error("boom".to_string());
        assert_eq!(app.now_playing_snapshot().status, RemoteStatus::Stopped);

        app.playback.view.state = PlaybackState::FadingOut {
            current_volume: 0.5,
        };
        app.playback.view.intentional_stop = false;
        assert_eq!(app.now_playing_snapshot().status, RemoteStatus::Playing);
        app.playback.view.intentional_stop = true;
        assert_eq!(app.now_playing_snapshot().status, RemoteStatus::Stopped);
    }

    #[test]
    fn only_changes_are_published() {
        let (mut app, remote) = app_with_remote();

        app.process_remote();
        app.process_remote();
        assert_eq!(
            remote.published.lock().unwrap().len(),
            1,
            "first state once"
        );

        playing_station(&mut app, 0);
        app.process_remote();
        app.process_remote();
        assert_eq!(remote.published.lock().unwrap().len(), 2);

        app.playback.view.current_track = Some("Song".to_string());
        app.process_remote();
        let published = remote.published.lock().unwrap();
        assert_eq!(published.len(), 3);
        assert_eq!(published[2].title.as_deref(), Some("Song"));
    }

    #[test]
    fn the_null_remote_is_inert() {
        let mut remote = NullRemote;
        assert!(remote.poll().is_empty());
        remote.publish(&NowPlaying::default());
    }
}
