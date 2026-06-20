mod engine_loop;
mod metadata;
mod output;
mod session;
mod stream_reader;
mod visualizer;

use std::collections::VecDeque;

pub use output::{
    list_output_device_names, output_device_display_name, DEFAULT_OUTPUT_DEVICE_LABEL,
};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

pub(super) const HARDWARE_OUTPUT_ERROR_PREFIX: &str = "Hardware output error:";
const MAX_HARDWARE_RECOVERY_RETRIES: u8 = 1;

pub(super) fn hardware_output_error(message: impl Into<String>) -> String {
    format!("{HARDWARE_OUTPUT_ERROR_PREFIX} {}", message.into())
}

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
    FadingOut { current_volume: f32 },
    TrackChanged { url: String, title: String },
}

/// Handle to communicate with the audio engine running on a background thread.
pub struct AudioEngine {
    cmd_tx: mpsc::Sender<AudioCommand>,
    pub status_rx: mpsc::Receiver<AudioStatus>,
}

impl AudioEngine {
    /// Spawn the audio engine on a dedicated OS thread.
    pub fn spawn(sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>();
        let (status_tx, status_rx) = mpsc::channel::<AudioStatus>();

        let sample_buffer_clone = sample_buffer.clone();
        std::thread::spawn(move || {
            engine_loop::audio_loop(cmd_rx, status_tx, sample_buffer_clone);
        });

        Self { cmd_tx, status_rx }
    }

    pub fn send(&self, cmd: AudioCommand) -> bool {
        self.cmd_tx.send(cmd).is_ok()
    }
}

#[cfg(test)]
impl AudioEngine {
    pub fn disconnected_for_test() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>();
        drop(cmd_rx);
        let (_status_tx, status_rx) = mpsc::channel::<AudioStatus>();

        Self { cmd_tx, status_rx }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_engine_send_returns_false_when_command_channel_is_closed() {
        let engine = AudioEngine::disconnected_for_test();

        assert!(!engine.send(AudioCommand::Stop));
    }
}
