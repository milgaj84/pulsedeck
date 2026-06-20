use super::output;
use super::session::{connect_and_decode, ConnectionContext};
use super::{
    AudioCommand, AudioStatus, HARDWARE_OUTPUT_ERROR_PREFIX, MAX_HARDWARE_RECOVERY_RETRIES,
};
use rodio::{OutputStream, Sink};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The main audio loop. Pure blocking I/O on a dedicated OS thread.
pub(super) fn audio_loop(
    cmd_rx: mpsc::Receiver<AudioCommand>,
    status_tx: mpsc::Sender<AudioStatus>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
) {
    let mut state = AudioLoopState::new();

    loop {
        // Non-blocking check for commands (10ms poll)
        match cmd_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(cmd) => {
                if cfg!(test) {
                    handle_test_audio_command(cmd, &status_tx);
                    continue;
                }

                state.handle_command(cmd, &status_tx, &sample_buffer);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        state.tick_pending_action(&status_tx, &sample_buffer);
        state.tick_fade_in();
        state.tick_connection(&status_tx, &sample_buffer);
        state.tick_sink_end(&status_tx);
    }
}

struct AudioLoopState {
    // Lazily opened on first playback. This keeps browsing/search usable on
    // systems without an immediately available output device.
    output_stream: Option<OutputStream>,
    output_handle: Option<rodio::OutputStreamHandle>,
    preferred_output_device_name: Option<String>,
    stream_metadata_enabled: bool,
    reopen_output_on_next_connection: bool,

    current_sink: Option<Sink>,
    connect_thread: Option<std::thread::JoinHandle<Result<Sink, String>>>,

    // Concurrency guard to abandon stale threads instantly.
    active_conn_id: Arc<AtomicU64>,
    current_conn_id: u64,
    current_url: Option<String>,
    hardware_recovery_retries: u8,

    // Non-blocking volume crossfade/ramping parameters.
    target_volume: f32,
    current_fade_volume: Option<f32>,
    pending_action: Option<AudioCommand>,
}

impl AudioLoopState {
    fn new() -> Self {
        Self {
            output_stream: None,
            output_handle: None,
            preferred_output_device_name: None,
            stream_metadata_enabled: true,
            reopen_output_on_next_connection: false,
            current_sink: None,
            connect_thread: None,
            active_conn_id: Arc::new(AtomicU64::new(0)),
            current_conn_id: 0,
            current_url: None,
            hardware_recovery_retries: 0,
            target_volume: 0.8,
            current_fade_volume: None,
            pending_action: None,
        }
    }

    fn handle_command(
        &mut self,
        cmd: AudioCommand,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        match cmd {
            AudioCommand::Play(url) => {
                if self.current_sink.is_some() {
                    self.pending_action = Some(AudioCommand::Play(url));
                } else {
                    self.start_connection(url, true, status_tx, sample_buffer);
                }
            }
            AudioCommand::Pause => {
                if let Some(ref sink) = self.current_sink {
                    self.pending_action = None;
                    self.current_fade_volume = None;
                    sink.pause();
                    let _ = status_tx.send(AudioStatus::Paused);
                }
            }
            AudioCommand::Resume => {
                if let Some(ref sink) = self.current_sink {
                    self.pending_action = None;
                    sink.play();
                    let _ = status_tx.send(AudioStatus::Playing);
                    // Smooth fade-in.
                    self.current_fade_volume = Some(0.0);
                }
            }
            AudioCommand::Stop => {
                if self.current_sink.is_some() {
                    self.pending_action = Some(AudioCommand::Stop);
                } else {
                    self.active_conn_id.store(0, Ordering::SeqCst); // abandon in-flight
                    self.connect_thread = None;
                    let _ = status_tx.send(AudioStatus::Stopped);
                }
            }
            AudioCommand::SetVolume(vol) => {
                self.target_volume = vol;
                if self.current_fade_volume.is_none() && self.pending_action.is_none() {
                    if let Some(ref sink) = self.current_sink {
                        sink.set_volume(vol);
                    }
                }
            }
            AudioCommand::SetOutputDevice(device_name) => {
                self.preferred_output_device_name =
                    output::normalize_output_device_name(device_name.as_deref());

                if self.current_sink.is_some() {
                    self.reopen_output_on_next_connection = true;
                } else {
                    self.output_stream = None;
                    self.output_handle = None;
                    self.reopen_output_on_next_connection = false;
                }
            }
            AudioCommand::SetStreamMetadata(enabled) => {
                self.stream_metadata_enabled = enabled;
            }
        }
    }

    fn tick_pending_action(
        &mut self,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        let Some(cmd) = self.pending_action.take() else {
            return;
        };

        if let Some(ref sink) = self.current_sink {
            let current_vol = sink.volume();
            if fade_out_complete(current_vol) {
                // Fade out completed. Execute pending command.
                sink.set_volume(0.0);
                self.execute_pending_action(cmd, status_tx, sample_buffer);
            } else {
                self.pending_action = Some(cmd);
                // Exponential step-down for natural dimming.
                let next_vol = fade_out_next_volume(current_vol);
                sink.set_volume(next_vol);
                let _ = status_tx.send(AudioStatus::FadingOut {
                    current_volume: clamp_status_volume(sink.volume()),
                });
            }
        } else {
            // No active sink, just execute pending immediately.
            self.execute_pending_action(cmd, status_tx, sample_buffer);
        }
    }

    fn execute_pending_action(
        &mut self,
        cmd: AudioCommand,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        match cmd {
            AudioCommand::Play(url) => {
                // Stop current sink before spawning new connection.
                if let Some(old_sink) = self.current_sink.take() {
                    old_sink.stop();
                }
                self.start_connection(url, true, status_tx, sample_buffer);
            }
            AudioCommand::Stop => {
                self.active_conn_id.store(0, Ordering::SeqCst); // abandon in-flight
                self.connect_thread = None;
                if let Some(old_sink) = self.current_sink.take() {
                    old_sink.stop();
                }
                let _ = status_tx.send(AudioStatus::Stopped);
            }
            _ => {}
        }
    }

    fn tick_fade_in(&mut self) {
        if self.pending_action.is_some() || self.current_fade_volume.is_none() {
            return;
        }

        if let Some(ref sink) = self.current_sink {
            let current_vol = sink.volume();
            if (current_vol - self.target_volume).abs() <= 0.03 {
                sink.set_volume(self.target_volume);
                self.current_fade_volume = None;
            } else {
                // Exponential step-up towards target_volume for organic swell.
                let step = (self.target_volume - current_vol) * 0.15;
                sink.set_volume(current_vol + step);
            }
        } else {
            self.current_fade_volume = None;
        }
    }

    fn tick_connection(
        &mut self,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        if !self
            .connect_thread
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
        {
            return;
        }

        let Some(finished) = self.connect_thread.take() else {
            return;
        };

        match finished.join() {
            Ok(Ok(sink)) => {
                // Start playing at 0.0 volume, trigger exponential swell.
                sink.set_volume(0.0);
                self.current_sink = Some(sink);
                let _ = status_tx.send(AudioStatus::Playing);
                self.current_fade_volume = Some(0.0);
            }
            Ok(Err(error)) => {
                // Stale thread errors are ignored (they are "Abandoned" or cancelled).
                if error == "Abandoned" {
                    return;
                }

                if is_hardware_output_error(&error)
                    && self.hardware_recovery_retries < MAX_HARDWARE_RECOVERY_RETRIES
                {
                    self.hardware_recovery_retries += 1;
                    reset_output_handle(&mut self.output_stream, &mut self.output_handle);
                    let _ = status_tx.send(AudioStatus::Connecting);

                    if let Some(url) = self.current_url.clone() {
                        self.start_connection(url, false, status_tx, sample_buffer);
                    } else {
                        let _ = status_tx.send(AudioStatus::Error(error));
                    }
                } else {
                    let _ = status_tx.send(AudioStatus::Error(error));
                }
            }
            Err(_) => {
                let _ = status_tx.send(AudioStatus::Error("Connection thread panicked".into()));
            }
        }
    }

    fn tick_sink_end(&mut self, status_tx: &mpsc::Sender<AudioStatus>) {
        if let Some(ref sink) = self.current_sink {
            if sink.empty() {
                self.current_sink = None;
                let _ = status_tx.send(AudioStatus::Stopped);
            }
        }
    }

    fn start_connection(
        &mut self,
        url: String,
        reset_hardware_retries: bool,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        self.current_url = Some(url.clone());
        if reset_hardware_retries {
            self.hardware_recovery_retries = 0;
        }
        self.spawn_connection(url, status_tx, sample_buffer);
    }

    fn spawn_connection(
        &mut self,
        url: String,
        status_tx: &mpsc::Sender<AudioStatus>,
        sample_buffer: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        if self.reopen_output_on_next_connection {
            self.output_stream = None;
            self.output_handle = None;
            self.reopen_output_on_next_connection = false;
        }

        let Some(handle) = ensure_output_handle(
            &mut self.output_stream,
            &mut self.output_handle,
            self.preferred_output_device_name.as_deref(),
            status_tx,
        ) else {
            return;
        };

        self.current_conn_id += 1;
        self.active_conn_id
            .store(self.current_conn_id, Ordering::SeqCst);
        let _ = status_tx.send(AudioStatus::Connecting);

        let conn_id = self.current_conn_id;
        let context = ConnectionContext {
            status_tx: status_tx.clone(),
            conn_id,
            active_conn_id: self.active_conn_id.clone(),
            sample_buffer: sample_buffer.clone(),
            request_stream_metadata: self.stream_metadata_enabled,
        };

        drop(self.connect_thread.take());
        self.connect_thread = Some(std::thread::spawn(move || {
            connect_and_decode(url, handle, context)
        }));
    }
}

fn handle_test_audio_command(cmd: AudioCommand, status_tx: &mpsc::Sender<AudioStatus>) {
    match cmd {
        AudioCommand::Play(_) => {
            let _ = status_tx.send(AudioStatus::Playing);
        }
        AudioCommand::Pause => {
            let _ = status_tx.send(AudioStatus::Paused);
        }
        AudioCommand::Resume => {
            let _ = status_tx.send(AudioStatus::Playing);
        }
        AudioCommand::Stop => {
            let _ = status_tx.send(AudioStatus::Stopped);
        }
        AudioCommand::SetVolume(_)
        | AudioCommand::SetOutputDevice(_)
        | AudioCommand::SetStreamMetadata(_) => {}
    }
}

fn fade_out_complete(current_volume: f32) -> bool {
    current_volume <= 0.05
}

fn fade_out_next_volume(current_volume: f32) -> f32 {
    let step = current_volume * 0.15;
    (current_volume - step).max(0.0)
}

fn clamp_status_volume(current_volume: f32) -> f32 {
    current_volume.clamp(0.0, 1.0)
}

fn reset_output_handle(
    output_stream: &mut Option<OutputStream>,
    output_handle: &mut Option<rodio::OutputStreamHandle>,
) {
    *output_handle = None;
    *output_stream = None;
}

fn is_hardware_output_error(error: &str) -> bool {
    error.starts_with(HARDWARE_OUTPUT_ERROR_PREFIX)
}

fn ensure_output_handle(
    output_stream: &mut Option<OutputStream>,
    output_handle: &mut Option<rodio::OutputStreamHandle>,
    preferred_output_device_name: Option<&str>,
    status_tx: &mpsc::Sender<AudioStatus>,
) -> Option<rodio::OutputStreamHandle> {
    if output_handle.is_none() {
        match output::open_output_stream(preferred_output_device_name) {
            Ok(selection) => {
                *output_stream = Some(selection.stream);
                *output_handle = Some(selection.handle);
            }
            Err(err) => {
                let _ = status_tx.send(AudioStatus::Error(format!("Soundcard error: {err}")));
                return None;
            }
        }
    }

    output_handle.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_out_next_volume_uses_exponential_step() {
        let next = fade_out_next_volume(1.0);

        assert!((next - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn fade_out_complete_triggers_at_low_volume() {
        assert!(!fade_out_complete(0.051));
        assert!(fade_out_complete(0.05));
    }

    #[test]
    fn clamp_status_volume_keeps_ui_payload_normalized() {
        assert_eq!(clamp_status_volume(-0.2), 0.0);
        assert_eq!(clamp_status_volume(0.42), 0.42);
        assert_eq!(clamp_status_volume(1.4), 1.0);
    }

    #[test]
    fn hardware_output_error_uses_recovery_prefix() {
        let error = super::super::hardware_output_error("Sink error: stale handle");

        assert!(is_hardware_output_error(&error));
        assert!(error.contains("stale handle"));
    }

    #[test]
    fn non_hardware_error_does_not_trigger_recovery() {
        assert!(!is_hardware_output_error("Connection failed: timeout"));
        assert!(!is_hardware_output_error("Decode error: unsupported"));
    }

    #[test]
    fn reset_output_handle_accepts_empty_handles() {
        let mut stream: Option<OutputStream> = None;
        let mut handle: Option<rodio::OutputStreamHandle> = None;

        reset_output_handle(&mut stream, &mut handle);

        assert!(stream.is_none());
        assert!(handle.is_none());
    }
}
