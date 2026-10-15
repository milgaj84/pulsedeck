//! Media keys and desktop media controls through MPRIS (Linux and the BSDs).
//!
//! MPRIS is the D-Bus interface that desktop shells, media-key daemons and
//! tools such as `playerctl` use to control players. This module registers
//! PulseDeck as `org.mpris.MediaPlayer2.pulsedeck` on the session bus, so
//! that the play/pause, next and previous keys and the desktop's media widget
//! work, and the widget shows the station and song.
//!
//! The D-Bus side runs on its own thread and talks to the app over two
//! channels (see [`RemoteControl`]); the app never waits for it. If there is
//! no session bus, or anything goes wrong, media controls are simply off.

use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use async_io::Timer;
use futures_lite::future;
use mpris_server::{Metadata, PlaybackStatus, Player, TrackId};

use crate::app::remote::{NowPlaying, RemoteCommand, RemoteControl, RemoteStatus};

/// The bus name is `org.mpris.MediaPlayer2.` followed by this.
const BUS_NAME: &str = "pulsedeck";
/// How often the D-Bus thread looks for news from the app.
const UPDATE_INTERVAL: Duration = Duration::from_millis(100);

pub struct MprisRemote {
    commands: mpsc::Receiver<RemoteCommand>,
    state: mpsc::Sender<NowPlaying>,
}

impl MprisRemote {
    /// Start serving MPRIS in the background. Never blocks and never fails: if
    /// the bus is unavailable the remote just never sees a command.
    pub fn start() -> Self {
        Self::start_named(BUS_NAME)
    }

    fn start_named(bus_name: &str) -> Self {
        let (command_tx, commands) = mpsc::channel();
        let (state, state_rx) = mpsc::channel();
        let bus_name = bus_name.to_string();
        let spawned = std::thread::Builder::new()
            .name("mpris".to_string())
            .spawn(move || {
                // A panic here must not print over the TUI.
                let _ = crate::audio::run_quiet(|| {
                    let _ = async_io::block_on(serve(&bus_name, command_tx, state_rx));
                });
            });
        drop(spawned);
        Self { commands, state }
    }
}

impl RemoteControl for MprisRemote {
    fn poll(&mut self) -> Vec<RemoteCommand> {
        self.commands.try_iter().collect()
    }

    fn publish(&mut self, now: &NowPlaying) {
        let _ = self.state.send(now.clone());
    }
}

async fn serve(
    bus_name: &str,
    commands: mpsc::Sender<RemoteCommand>,
    state: mpsc::Receiver<NowPlaying>,
) -> mpris_server::zbus::Result<()> {
    let player = match build_player(bus_name).await {
        Ok(player) => player,
        // A second PulseDeck: MPRIS asks for a per-process name.
        Err(_) => build_player(&format!("{bus_name}.instance{}", std::process::id())).await?,
    };

    let send = |command: RemoteCommand| {
        let commands = commands.clone();
        move |_: &Player| {
            let _ = commands.send(command);
        }
    };
    player.connect_play(send(RemoteCommand::Play));
    player.connect_pause(send(RemoteCommand::Pause));
    player.connect_play_pause(send(RemoteCommand::PlayPause));
    player.connect_stop(send(RemoteCommand::Stop));
    player.connect_next(send(RemoteCommand::Next));
    player.connect_previous(send(RemoteCommand::Previous));
    {
        let commands = commands.clone();
        player.connect_set_volume(move |_, volume| {
            let percent = (volume.clamp(0.0, 1.0) * 100.0).round() as u8;
            let _ = commands.send(RemoteCommand::SetVolume(percent));
        });
    }

    // Answer the desktop's calls and watch for news from the app; whichever
    // ends first (the app went away) ends both.
    let updates = async {
        let mut track = Track::default();
        loop {
            let mut latest = None;
            loop {
                match state.try_recv() {
                    Ok(now) => latest = Some(now),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            if let Some(now) = latest {
                let _ = publish(&player, &now, &mut track).await;
            }
            Timer::after(UPDATE_INTERVAL).await;
        }
    };
    future::or(player.run(), updates).await;
    Ok(())
}

async fn build_player(bus_name: &str) -> mpris_server::zbus::Result<Player> {
    Player::builder(bus_name)
        .identity("PulseDeck")
        .can_play(true)
        .can_pause(true)
        .can_go_next(true)
        .can_go_previous(true)
        .can_seek(false)
        .can_control(true)
        .build()
        .await
}

/// Numbers the tracks so that each new station or song gets a new track id.
#[derive(Default)]
struct Track {
    serial: u64,
    last: Option<(Option<String>, Option<String>)>,
}

async fn publish(
    player: &Player,
    now: &NowPlaying,
    track: &mut Track,
) -> mpris_server::zbus::Result<()> {
    let key = (now.station.clone(), now.title.clone());
    if track.last.as_ref() != Some(&key) {
        track.serial += 1;
        track.last = Some(key);
    }
    player.set_metadata(metadata_for(now, track.serial)).await?;
    player.set_playback_status(status_for(now.status)).await?;
    player.set_volume(f64::from(now.volume) / 100.0).await?;
    Ok(())
}

fn status_for(status: RemoteStatus) -> PlaybackStatus {
    match status {
        RemoteStatus::Playing => PlaybackStatus::Playing,
        RemoteStatus::Paused => PlaybackStatus::Paused,
        RemoteStatus::Stopped => PlaybackStatus::Stopped,
    }
}

/// A live stream has no length, album or position: the song (or, without one,
/// the station) is the title and the station is the artist.
fn metadata_for(now: &NowPlaying, serial: u64) -> Metadata {
    if now.status == RemoteStatus::Stopped || (now.station.is_none() && now.title.is_none()) {
        return Metadata::builder().trackid(TrackId::NO_TRACK).build();
    }
    let trackid =
        TrackId::try_from(format!("/org/pulsedeck/track/{serial}")).unwrap_or(TrackId::NO_TRACK);
    let builder = Metadata::builder().trackid(trackid);
    match (&now.title, &now.station) {
        (Some(title), Some(station)) => builder.title(title.clone()).artist([station.clone()]),
        (Some(title), None) => builder.title(title.clone()),
        (None, Some(station)) => builder.title(station.clone()),
        (None, None) => builder,
    }
    .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpris_server::zbus::blocking::{Connection, Proxy};
    use mpris_server::zbus::zvariant::OwnedValue;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;

    fn now(status: RemoteStatus, station: &str, title: Option<&str>) -> NowPlaying {
        NowPlaying {
            status,
            station: Some(station.to_string()),
            title: title.map(str::to_string),
            volume: 50,
        }
    }

    #[test]
    fn a_song_is_the_title_and_the_station_the_artist() {
        let metadata = metadata_for(&now(RemoteStatus::Playing, "KISS", Some("A - B")), 3);

        assert_eq!(metadata.title(), Some("A - B"));
        assert_eq!(metadata.artist(), Some(vec!["KISS".to_string()]));
        assert_eq!(
            metadata.trackid().map(|id| id.to_string()),
            Some("/org/pulsedeck/track/3".to_string())
        );
        assert_eq!(metadata.length(), None, "a live stream has no length");
    }

    #[test]
    fn without_a_song_the_station_is_the_title() {
        let metadata = metadata_for(&now(RemoteStatus::Paused, "KISS", None), 1);

        assert_eq!(metadata.title(), Some("KISS"));
        assert_eq!(metadata.artist(), None);
    }

    #[test]
    fn nothing_playing_has_no_track() {
        let metadata = metadata_for(&NowPlaying::default(), 9);
        assert_eq!(metadata.title(), None);
        assert_eq!(metadata.trackid(), Some(TrackId::NO_TRACK));

        let stopped = metadata_for(&now(RemoteStatus::Stopped, "KISS", Some("x")), 9);
        assert_eq!(stopped.title(), None);
    }

    #[test]
    fn statuses_map_one_to_one() {
        assert_eq!(status_for(RemoteStatus::Playing), PlaybackStatus::Playing);
        assert_eq!(status_for(RemoteStatus::Paused), PlaybackStatus::Paused);
        assert_eq!(status_for(RemoteStatus::Stopped), PlaybackStatus::Stopped);
    }

    /// A private session bus, killed on drop.
    struct PrivateBus {
        child: Child,
        address: String,
    }

    impl PrivateBus {
        fn start() -> Option<Self> {
            let mut child = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let mut address = String::new();
            BufReader::new(child.stdout.take()?)
                .read_line(&mut address)
                .ok()?;
            let address = address.trim().to_string();
            if address.is_empty() {
                let _ = child.kill();
                return None;
            }
            Some(Self { child, address })
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn eventually<T>(what: &str, mut attempt: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(value) = attempt() {
                return value;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The whole thing against a real D-Bus: a client sees what is published
    /// and the app sees what the client asks for.
    #[test]
    fn a_desktop_client_sees_the_player_and_controls_it() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not available");
            return;
        };
        // This is the only test that talks to a bus.
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &bus.address);

        let mut remote = MprisRemote::start_named("pulsedeck_test");
        let connection = Connection::session().expect("client connection");
        let name = "org.mpris.MediaPlayer2.pulsedeck_test";
        let path = "/org/mpris/MediaPlayer2";
        let player = eventually("the player to appear", || {
            Proxy::new(&connection, name, path, "org.mpris.MediaPlayer2.Player")
                .ok()
                .filter(|proxy| proxy.get_property::<String>("PlaybackStatus").is_ok())
        });
        let root = Proxy::new(&connection, name, path, "org.mpris.MediaPlayer2").unwrap();
        assert_eq!(
            root.get_property::<String>("Identity").unwrap(),
            "PulseDeck"
        );
        assert!(!root.get_property::<bool>("CanQuit").unwrap());

        // What the app publishes reaches the client.
        remote.publish(&now(
            RemoteStatus::Playing,
            "KISS FM",
            Some("Artist - Song"),
        ));
        eventually("the status", || {
            (player.get_property::<String>("PlaybackStatus").ok()? == "Playing").then_some(())
        });
        let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").unwrap();
        let text = |key: &str| -> String {
            String::try_from(metadata.get(key).expect(key).try_clone().unwrap()).unwrap()
        };
        assert_eq!(text("xesam:title"), "Artist - Song");
        let artists: Vec<String> =
            Vec::try_from(metadata["xesam:artist"].try_clone().unwrap()).unwrap();
        assert_eq!(artists, vec!["KISS FM".to_string()]);
        assert!((player.get_property::<f64>("Volume").unwrap() - 0.5).abs() < 1e-9);
        assert!(player.get_property::<bool>("CanGoNext").unwrap());
        assert!(!player.get_property::<bool>("CanSeek").unwrap());

        remote.publish(&now(RemoteStatus::Paused, "KISS FM", None));
        eventually("the pause", || {
            (player.get_property::<String>("PlaybackStatus").ok()? == "Paused").then_some(())
        });

        // What the client asks for reaches the app.
        for (method, expected) in [
            ("PlayPause", RemoteCommand::PlayPause),
            ("Next", RemoteCommand::Next),
            ("Previous", RemoteCommand::Previous),
            ("Stop", RemoteCommand::Stop),
            ("Play", RemoteCommand::Play),
            ("Pause", RemoteCommand::Pause),
        ] {
            player.call_method(method, &()).unwrap();
            let received = eventually(method, || remote.poll().into_iter().next());
            assert_eq!(received, expected, "{method}");
        }
        player.set_property("Volume", 0.25_f64).unwrap();
        let received = eventually("the volume", || remote.poll().into_iter().next());
        assert_eq!(received, RemoteCommand::SetVolume(25));
    }

    #[test]
    fn a_remote_whose_thread_is_gone_is_inert() {
        // This is what the app holds when there is no session bus: the thread
        // ended and dropped its ends of the channels.
        let (command_tx, commands) = mpsc::channel();
        let (state, state_rx) = mpsc::channel();
        drop((command_tx, state_rx));
        let mut remote = MprisRemote { commands, state };

        remote.publish(&NowPlaying::default());

        assert!(remote.poll().is_empty());
    }
}
