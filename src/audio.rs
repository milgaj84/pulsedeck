mod buffer;
mod buffer_meter;
mod metadata;
mod output;
mod recording;
mod session;
mod stream_reader;
mod visualizer;

use rodio::{OutputStream, Sink};
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

pub use output::{
    list_output_device_names, output_device_display_name, DEFAULT_OUTPUT_DEVICE_LABEL,
};
use session::{connect_and_decode, ConnectionContext};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) const HARDWARE_OUTPUT_ERROR_PREFIX: &str = "Hardware output error:";
const MAX_HARDWARE_RECOVERY_RETRIES: u8 = 1;

pub(super) fn hardware_output_error(message: impl Into<String>) -> String {
    format!("{HARDWARE_OUTPUT_ERROR_PREFIX} {}", message.into())
}

/// Commands sent from the UI thread to the audio thread.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    Play(String), // URL
    PlayLocalFile(PathBuf),
    Pause,
    Resume,
    Stop,
    SetVolume(f32), // 0.0 — 1.0
    SetOutputDevice(Option<String>),
    StartRecording {
        recording_dir: String,
        category: String,
        keep_snippets: bool,
        min_song_duration_secs: u32,
    },
    StopRecording,
}

/// Status updates sent from the audio thread back to the UI.
#[derive(Debug, Clone)]
pub enum AudioStatus {
    Playing,
    LocalFilePlaying { path: PathBuf, title: String },
    Paused,
    Stopped,
    Error(String),
    Connecting,
    FadingOut { current_volume: f32 },
    TrackChanged { url: String, title: String },
    RecordingStateChanged { state: u8, filepath: Option<String> }, // 0 = Off, 1 = Pending, 2 = Active
    BufferLevel { percent: u8, seconds: u32 },
}

/// Shared thread-safe recording configuration state
pub struct RecordStateShared {
    pub state: AtomicU8, // 0 = Off, 1 = Pending, 2 = Active
    pub recording_dir: Mutex<String>,
    pub category: Mutex<String>,
    pub keep_snippets: AtomicBool,
    pub min_song_duration_secs: std::sync::atomic::AtomicU32,
}

/// Handle to communicate with the audio engine running on a background thread.
pub struct AudioEngine {
    cmd_tx: mpsc::Sender<AudioCommand>,
    pub status_rx: mpsc::Receiver<AudioStatus>,
    #[allow(dead_code)]
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
}

impl AudioEngine {
    /// Spawn the audio engine on a dedicated OS thread.
    pub fn spawn(sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>();
        let (status_tx, status_rx) = mpsc::channel::<AudioStatus>();

        let sample_buffer_clone = sample_buffer.clone();
        std::thread::spawn(move || {
            audio_loop(cmd_rx, status_tx, sample_buffer_clone);
        });

        Self {
            cmd_tx,
            status_rx,
            sample_buffer,
        }
    }

    pub fn send(&self, cmd: AudioCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

/// The main audio loop. Pure blocking I/O on a dedicated OS thread.
fn audio_loop(
    cmd_rx: mpsc::Receiver<AudioCommand>,
    status_tx: mpsc::Sender<AudioStatus>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
) {
    // Lazily opened on first playback. This keeps browsing/search usable on
    // systems without an immediately available output device.
    let mut output_stream: Option<OutputStream> = None;
    let mut output_handle: Option<rodio::OutputStreamHandle> = None;
    let mut preferred_output_device_name: Option<String> = None;
    let mut reopen_output_on_next_connection = false;

    let mut current_sink: Option<Sink> = None;
    let mut connect_thread: Option<std::thread::JoinHandle<Result<Sink, String>>> = None;

    // Concurrency guard to abandon stale threads instantly
    let active_conn_id = Arc::new(AtomicU64::new(0));
    let mut current_conn_id: u64 = 0;
    let mut current_url: Option<String> = None;
    let mut hardware_recovery_retries: u8 = 0;

    // Shared thread-safe recording control state
    let record_state = Arc::new(RecordStateShared {
        state: AtomicU8::new(0), // Default: Off
        recording_dir: Mutex::new(String::new()),
        category: Mutex::new(String::new()),
        keep_snippets: AtomicBool::new(false),
        min_song_duration_secs: std::sync::atomic::AtomicU32::new(90),
    });

    // Premium non-blocking volume crossfade/ramping parameters
    let mut target_volume: f32 = 0.8;
    let mut current_fade_volume: Option<f32> = None;
    let mut pending_action: Option<AudioCommand> = None;

    macro_rules! spawn_connection_for {
        ($url:expr) => {{
            let url = $url;
            current_url = Some(url.clone());
            hardware_recovery_retries = 0;
            spawn_connection(
                url,
                &mut SpawnConnectionState {
                    conn_id_ref: &mut current_conn_id,
                    active_ref: &active_conn_id,
                    connect_ref: &mut connect_thread,
                    output_stream: &mut output_stream,
                    output_handle: &mut output_handle,
                    status_tx: &status_tx,
                    record_state: &record_state,
                    sample_buffer: &sample_buffer,
                    preferred_output_device_name: &preferred_output_device_name,
                    reopen_output_on_next_connection: &mut reopen_output_on_next_connection,
                },
            );
        }};
    }

    macro_rules! retry_connection_for {
        ($url:expr) => {{
            let url = $url;
            current_url = Some(url.clone());
            spawn_connection(
                url,
                &mut SpawnConnectionState {
                    conn_id_ref: &mut current_conn_id,
                    active_ref: &active_conn_id,
                    connect_ref: &mut connect_thread,
                    output_stream: &mut output_stream,
                    output_handle: &mut output_handle,
                    status_tx: &status_tx,
                    record_state: &record_state,
                    sample_buffer: &sample_buffer,
                    preferred_output_device_name: &preferred_output_device_name,
                    reopen_output_on_next_connection: &mut reopen_output_on_next_connection,
                },
            );
        }};
    }

    loop {
        // Non-blocking check for commands (10ms poll)
        match cmd_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(cmd) => match cmd {
                AudioCommand::Play(url) => {
                    if current_sink.is_some() {
                        pending_action = Some(AudioCommand::Play(url));
                    } else {
                        spawn_connection_for!(url);
                    }
                }
                AudioCommand::PlayLocalFile(path) => {
                    pending_action = None;
                    current_fade_volume = None;
                    active_conn_id.store(0, Ordering::SeqCst);
                    connect_thread = None;
                    current_url = None;

                    if let Some(old_sink) = current_sink.take() {
                        old_sink.stop();
                    }

                    match play_local_file(
                        &path,
                        &mut output_stream,
                        &mut output_handle,
                        preferred_output_device_name.as_deref(),
                        &status_tx,
                        target_volume,
                    ) {
                        Ok(sink) => {
                            let title = crate::tape_archive::display_track_title(&path);
                            current_sink = Some(sink);
                            let _ = status_tx.send(AudioStatus::LocalFilePlaying { path, title });
                        }
                        Err(err) => {
                            let _ = status_tx.send(AudioStatus::Error(err));
                        }
                    }
                }
                AudioCommand::Pause => {
                    if let Some(ref sink) = current_sink {
                        pending_action = None;
                        current_fade_volume = None;
                        sink.pause();
                        let _ = status_tx.send(AudioStatus::Paused);
                    }
                }
                AudioCommand::Resume => {
                    if let Some(ref sink) = current_sink {
                        pending_action = None;
                        sink.play();
                        let _ = status_tx.send(AudioStatus::Playing);
                        // Smooth fade-in
                        current_fade_volume = Some(0.0);
                    }
                }
                AudioCommand::Stop => {
                    if current_sink.is_some() {
                        pending_action = Some(AudioCommand::Stop);
                    } else {
                        active_conn_id.store(0, Ordering::SeqCst); // abandon in-flight
                        connect_thread = None;
                        let _ = status_tx.send(AudioStatus::Stopped);
                    }
                }
                AudioCommand::SetVolume(vol) => {
                    target_volume = vol;
                    if current_fade_volume.is_none() && pending_action.is_none() {
                        if let Some(ref sink) = current_sink {
                            sink.set_volume(vol);
                        }
                    }
                }
                AudioCommand::SetOutputDevice(device_name) => {
                    preferred_output_device_name =
                        output::normalize_output_device_name(device_name.as_deref());

                    if current_sink.is_some() {
                        reopen_output_on_next_connection = true;
                    } else {
                        output_stream = None;
                        output_handle = None;
                        reopen_output_on_next_connection = false;
                    }
                }
                AudioCommand::StartRecording {
                    recording_dir,
                    category,
                    keep_snippets,
                    min_song_duration_secs,
                } => {
                    *record_state.recording_dir.lock().unwrap() = recording_dir;
                    *record_state.category.lock().unwrap() = category;
                    record_state
                        .keep_snippets
                        .store(keep_snippets, Ordering::SeqCst);
                    record_state
                        .min_song_duration_secs
                        .store(min_song_duration_secs, Ordering::SeqCst);
                    record_state.state.store(1, Ordering::SeqCst); // Transition to Pending
                    let _ = status_tx.send(AudioStatus::RecordingStateChanged {
                        state: 1,
                        filepath: None,
                    });
                }
                AudioCommand::StopRecording => {
                    record_state.state.store(0, Ordering::SeqCst); // Transition to Off
                    let _ = status_tx.send(AudioStatus::RecordingStateChanged {
                        state: 0,
                        filepath: None,
                    });
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }

        // Process pending action / non-blocking fade-out
        if pending_action.is_some() {
            if let Some(ref sink) = current_sink {
                let current_vol = sink.volume();
                if fade_out_complete(current_vol) {
                    // Fade out completed! Execute pending command
                    sink.set_volume(0.0);
                    let cmd = pending_action.take().unwrap();
                    match cmd {
                        AudioCommand::Play(url) => {
                            // Stop current sink before spawning new connection
                            if let Some(old_sink) = current_sink.take() {
                                old_sink.stop();
                            }
                            spawn_connection_for!(url);
                        }
                        AudioCommand::Stop => {
                            active_conn_id.store(0, Ordering::SeqCst); // abandon in-flight
                            connect_thread = None;
                            if let Some(old_sink) = current_sink.take() {
                                old_sink.stop();
                            }
                            let _ = status_tx.send(AudioStatus::Stopped);
                        }
                        _ => {}
                    }
                } else {
                    // Exponential step-down for natural dimming.
                    let next_vol = fade_out_next_volume(current_vol);
                    sink.set_volume(next_vol);
                    let _ = status_tx.send(AudioStatus::FadingOut {
                        current_volume: clamp_status_volume(sink.volume()),
                    });
                }
            } else {
                // No active sink, just execute pending immediately
                let cmd = pending_action.take().unwrap();
                match cmd {
                    AudioCommand::Play(url) => {
                        spawn_connection_for!(url);
                    }
                    AudioCommand::Stop => {
                        active_conn_id.store(0, Ordering::SeqCst);
                        connect_thread = None;
                        let _ = status_tx.send(AudioStatus::Stopped);
                    }
                    _ => {}
                }
            }
        }

        // Process non-blocking fade-in
        if pending_action.is_none() && current_fade_volume.is_some() {
            if let Some(ref sink) = current_sink {
                let current_vol = sink.volume();
                if (current_vol - target_volume).abs() <= 0.03 {
                    sink.set_volume(target_volume);
                    current_fade_volume = None;
                } else {
                    // Exponential step-up towards target_volume for organic swell
                    let step = (target_volume - current_vol) * 0.15;
                    sink.set_volume(current_vol + step);
                }
            } else {
                current_fade_volume = None;
            }
        }

        // Check if a pending connection has completed
        if let Some(ref handle) = connect_thread {
            if handle.is_finished() {
                let finished = connect_thread.take().unwrap();
                match finished.join() {
                    Ok(Ok(sink)) => {
                        // Start playing at 0.0 volume, trigger exponential swell
                        sink.set_volume(0.0);
                        current_sink = Some(sink);
                        let _ = status_tx.send(AudioStatus::Playing);
                        current_fade_volume = Some(0.0);
                    }
                    Ok(Err(e)) => {
                        // Stale thread errors are ignored (they are "Abandoned" or cancelled)
                        if e != "Abandoned" {
                            if is_hardware_output_error(&e)
                                && hardware_recovery_retries < MAX_HARDWARE_RECOVERY_RETRIES
                            {
                                hardware_recovery_retries += 1;
                                reset_output_handle(&mut output_stream, &mut output_handle);
                                let _ = status_tx.send(AudioStatus::Connecting);

                                if let Some(url) = current_url.clone() {
                                    retry_connection_for!(url);
                                } else {
                                    let _ = status_tx.send(AudioStatus::Error(e));
                                }
                            } else {
                                let _ = status_tx.send(AudioStatus::Error(e));
                            }
                        }
                    }
                    Err(_) => {
                        let _ =
                            status_tx.send(AudioStatus::Error("Connection thread panicked".into()));
                    }
                }
            }
        }

        // Check if current playback ended
        if let Some(ref sink) = current_sink {
            if sink.empty() {
                current_sink = None;
                let _ = status_tx.send(AudioStatus::Stopped);
            }
        }
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

fn play_local_file(
    path: &Path,
    output_stream: &mut Option<OutputStream>,
    output_handle: &mut Option<rodio::OutputStreamHandle>,
    preferred_output_device_name: Option<&str>,
    status_tx: &mpsc::Sender<AudioStatus>,
    target_volume: f32,
) -> Result<Sink, String> {
    let Some(handle) = ensure_output_handle(
        output_stream,
        output_handle,
        preferred_output_device_name,
        status_tx,
    ) else {
        return Err("Soundcard unavailable for local tape playback".to_string());
    };

    let file = File::open(path)
        .map_err(|err| format!("Could not open tape file '{}': {err}", path.display()))?;
    let source = rodio::Decoder::new(BufReader::new(file))
        .map_err(|err| format!("Could not decode tape file '{}': {err}", path.display()))?;
    let sink = Sink::try_new(&handle)
        .map_err(|err| hardware_output_error(format!("Sink error: {err}")))?;

    sink.set_volume(target_volume);
    sink.append(source);
    Ok(sink)
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

struct SpawnConnectionState<'a> {
    conn_id_ref: &'a mut u64,
    active_ref: &'a Arc<AtomicU64>,
    connect_ref: &'a mut Option<std::thread::JoinHandle<Result<Sink, String>>>,
    output_stream: &'a mut Option<OutputStream>,
    output_handle: &'a mut Option<rodio::OutputStreamHandle>,
    status_tx: &'a mpsc::Sender<AudioStatus>,
    record_state: &'a Arc<RecordStateShared>,
    sample_buffer: &'a Arc<Mutex<VecDeque<f32>>>,
    preferred_output_device_name: &'a Option<String>,
    reopen_output_on_next_connection: &'a mut bool,
}

fn spawn_connection(url: String, state: &mut SpawnConnectionState<'_>) {
    if *state.reopen_output_on_next_connection {
        *state.output_stream = None;
        *state.output_handle = None;
        *state.reopen_output_on_next_connection = false;
    }

    let Some(handle) = ensure_output_handle(
        state.output_stream,
        state.output_handle,
        state.preferred_output_device_name.as_deref(),
        state.status_tx,
    ) else {
        return;
    };

    *state.conn_id_ref += 1;
    state.active_ref.store(*state.conn_id_ref, Ordering::SeqCst);
    let _ = state.status_tx.send(AudioStatus::Connecting);

    let conn_id = *state.conn_id_ref;
    let context = ConnectionContext {
        status_tx: state.status_tx.clone(),
        conn_id,
        active_conn_id: state.active_ref.clone(),
        record_state: state.record_state.clone(),
        sample_buffer: state.sample_buffer.clone(),
    };

    drop(state.connect_ref.take());
    *state.connect_ref = Some(std::thread::spawn(move || {
        connect_and_decode(url, handle, context)
    }));
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
        let error = hardware_output_error("Sink error: stale handle");

        assert!(is_hardware_output_error(&error));
        assert!(error.contains("stale handle"));
    }

    #[test]
    fn non_hardware_error_does_not_trigger_recovery() {
        assert!(!is_hardware_output_error("Connection failed: timeout"));
        assert!(!is_hardware_output_error("Decode error: unsupported"));
    }

    #[test]
    fn local_file_title_comes_from_tape_archive_helper() {
        assert_eq!(
            crate::tape_archive::display_track_title(std::path::Path::new(
                "recordings/Synthwave/Lazerhawk - King.mp3"
            )),
            "Lazerhawk - King"
        );
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
