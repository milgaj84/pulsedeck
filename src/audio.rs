mod capability;
pub(super) mod decode;
pub(super) mod engine_loop_v2;
mod metadata;
mod output;
mod output_manager;
pub(super) mod stream_source;
mod supervisor;
pub(super) mod types;
mod visualizer;
pub(super) mod volume;

pub use capability::{codec_capability, PlaybackCapability};

use std::collections::VecDeque;

pub use output::{
    list_output_device_names, output_device_display_name, DEFAULT_OUTPUT_DEVICE_LABEL,
};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

pub(super) const HARDWARE_OUTPUT_ERROR_PREFIX: &str = "Hardware output error:";

pub use types::DeviceRecoveryConfig;

/// Commands sent from the UI thread to the audio thread.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    Play(String),
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
    SetOutputDevice(Option<String>),
    SetStreamMetadata(bool),
}

/// Status updates sent from the audio thread back to the UI.
#[derive(Debug, Clone)]
pub enum AudioStatus {
    Playing,
    Paused,
    Stopped,
    Error(String),
    Connecting,
    Buffering { percent: u8 },
    FadingOut { current_volume: f32 },
    TrackChanged { url: String, title: String },
}

/// Abstraction for sending commands to and receiving status from an audio backend.
pub trait AudioSink: Send {
    /// Send a command to the audio engine. Returns true if sent successfully.
    fn send(&self, command: AudioCommand) -> bool;

    /// Non-blocking poll for the next audio status message.
    fn try_recv_status(&self) -> Option<AudioStatus>;
}

/// Handle to communicate with the audio engine running on a background thread.
pub struct AudioEngine {
    cmd_tx: mpsc::Sender<AudioCommand>,
    pub status_rx: mpsc::Receiver<AudioStatus>,
}

impl AudioEngine {
    /// Spawn the audio engine on a dedicated OS thread.
    pub fn spawn(
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
        recovery_config: DeviceRecoveryConfig,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>();
        let (status_tx, status_rx) = mpsc::channel::<AudioStatus>();

        let sample_buffer_clone = sample_buffer.clone();
        std::thread::spawn(move || {
            engine_loop_v2::EngineLoop::run(
                cmd_rx,
                status_tx,
                sample_buffer_clone,
                recovery_config,
            );
        });

        Self { cmd_tx, status_rx }
    }
}

impl AudioSink for AudioEngine {
    fn send(&self, command: AudioCommand) -> bool {
        self.cmd_tx.send(command).is_ok()
    }

    fn try_recv_status(&self) -> Option<AudioStatus> {
        self.status_rx.try_recv().ok()
    }
}

/// Test mock that captures sent commands and returns queued statuses.
#[cfg(test)]
pub(crate) struct MockAudioSink {
    pub commands: std::cell::RefCell<Vec<AudioCommand>>,
    pub statuses: std::cell::RefCell<VecDeque<AudioStatus>>,
    pub send_succeeds: bool,
}

#[cfg(test)]
impl MockAudioSink {
    /// Create a mock where send always succeeds.
    pub fn new() -> Self {
        Self {
            commands: std::cell::RefCell::new(Vec::new()),
            statuses: std::cell::RefCell::new(VecDeque::new()),
            send_succeeds: true,
        }
    }

    /// Create a mock simulating a disconnected engine (send always fails).
    pub fn disconnected() -> Self {
        Self {
            commands: std::cell::RefCell::new(Vec::new()),
            statuses: std::cell::RefCell::new(VecDeque::new()),
            send_succeeds: false,
        }
    }
}

#[cfg(test)]
impl AudioSink for MockAudioSink {
    fn send(&self, command: AudioCommand) -> bool {
        self.commands.borrow_mut().push(command);
        self.send_succeeds
    }

    fn try_recv_status(&self) -> Option<AudioStatus> {
        self.statuses.borrow_mut().pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_audio_sink_disconnected_returns_false_on_send() {
        let mock = MockAudioSink::disconnected();

        assert!(!mock.send(AudioCommand::Stop));
        assert_eq!(mock.commands.borrow().len(), 1);
    }

    #[test]
    fn mock_audio_sink_new_returns_true_on_send() {
        let mock = MockAudioSink::new();

        assert!(mock.send(AudioCommand::Pause));
        assert_eq!(mock.commands.borrow().len(), 1);
    }

    #[test]
    fn mock_audio_sink_returns_queued_statuses() {
        let mock = MockAudioSink::new();
        mock.statuses.borrow_mut().push_back(AudioStatus::Playing);
        mock.statuses.borrow_mut().push_back(AudioStatus::Stopped);

        assert!(matches!(mock.try_recv_status(), Some(AudioStatus::Playing)));
        assert!(matches!(mock.try_recv_status(), Some(AudioStatus::Stopped)));
        assert!(mock.try_recv_status().is_none());
    }
}
