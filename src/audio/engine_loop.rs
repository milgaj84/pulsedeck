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

    // Premium non-blocking volume crossfade/ramping parameters
    let mut target_volume: f32 = 0.8;
    let mut current_fade_volume: Option<f32> = None;
    let mut pending_action: Option<AudioCommand> = None;

    loop {
        // Non-blocking check for commands (10ms poll)
        match cmd_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(cmd) => {
                if cfg!(test) {
                    handle_test_audio_command(cmd, &status_tx);
                    continue;
                }

                match cmd {
                    AudioCommand::Play(url) => {
                        if current_sink.is_some() {
                            pending_action = Some(AudioCommand::Play(url));
                        } else {
                            start_connection(
                                url,
                                true,
                                &mut current_url,
                                &mut hardware_recovery_retries,
                                &mut SpawnConnectionState {
                                    conn_id_ref: &mut current_conn_id,
                                    active_ref: &active_conn_id,
                                    connect_ref: &mut connect_thread,
                                    output_stream: &mut output_stream,
                                    output_handle: &mut output_handle,
                                    status_tx: &status_tx,
                                    sample_buffer: &sample_buffer,
                                    preferred_output_device_name: &preferred_output_device_name,
                                    reopen_output_on_next_connection:
                                        &mut reopen_output_on_next_connection,
                                },
                            );
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
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }

        // Process pending action / non-blocking fade-out
        if let Some(cmd) = pending_action.take() {
            if let Some(ref sink) = current_sink {
                let current_vol = sink.volume();
                if fade_out_complete(current_vol) {
                    // Fade out completed! Execute pending command
                    sink.set_volume(0.0);
                    execute_pending_action(
                        cmd,
                        &mut current_sink,
                        &mut current_url,
                        &mut hardware_recovery_retries,
                        &mut SpawnConnectionState {
                            conn_id_ref: &mut current_conn_id,
                            active_ref: &active_conn_id,
                            connect_ref: &mut connect_thread,
                            output_stream: &mut output_stream,
                            output_handle: &mut output_handle,
                            status_tx: &status_tx,
                            sample_buffer: &sample_buffer,
                            preferred_output_device_name: &preferred_output_device_name,
                            reopen_output_on_next_connection: &mut reopen_output_on_next_connection,
                        },
                    );
                } else {
                    pending_action = Some(cmd);
                    // Exponential step-down for natural dimming.
                    let next_vol = fade_out_next_volume(current_vol);
                    sink.set_volume(next_vol);
                    let _ = status_tx.send(AudioStatus::FadingOut {
                        current_volume: clamp_status_volume(sink.volume()),
                    });
                }
            } else {
                // No active sink, just execute pending immediately
                execute_pending_action(
                    cmd,
                    &mut current_sink,
                    &mut current_url,
                    &mut hardware_recovery_retries,
                    &mut SpawnConnectionState {
                        conn_id_ref: &mut current_conn_id,
                        active_ref: &active_conn_id,
                        connect_ref: &mut connect_thread,
                        output_stream: &mut output_stream,
                        output_handle: &mut output_handle,
                        status_tx: &status_tx,
                        sample_buffer: &sample_buffer,
                        preferred_output_device_name: &preferred_output_device_name,
                        reopen_output_on_next_connection: &mut reopen_output_on_next_connection,
                    },
                );
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
        if connect_thread
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
        {
            if let Some(finished) = connect_thread.take() {
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
                                    start_connection(
                                        url,
                                        false,
                                        &mut current_url,
                                        &mut hardware_recovery_retries,
                                        &mut SpawnConnectionState {
                                            conn_id_ref: &mut current_conn_id,
                                            active_ref: &active_conn_id,
                                            connect_ref: &mut connect_thread,
                                            output_stream: &mut output_stream,
                                            output_handle: &mut output_handle,
                                            status_tx: &status_tx,
                                            sample_buffer: &sample_buffer,
                                            preferred_output_device_name:
                                                &preferred_output_device_name,
                                            reopen_output_on_next_connection:
                                                &mut reopen_output_on_next_connection,
                                        },
                                    );
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
        AudioCommand::SetVolume(_) | AudioCommand::SetOutputDevice(_) => {}
    }
}

fn execute_pending_action(
    cmd: AudioCommand,
    current_sink: &mut Option<Sink>,
    current_url: &mut Option<String>,
    hardware_recovery_retries: &mut u8,
    state: &mut SpawnConnectionState<'_>,
) {
    match cmd {
        AudioCommand::Play(url) => {
            // Stop current sink before spawning new connection
            if let Some(old_sink) = current_sink.take() {
                old_sink.stop();
            }
            start_connection(url, true, current_url, hardware_recovery_retries, state);
        }
        AudioCommand::Stop => {
            state.active_ref.store(0, Ordering::SeqCst); // abandon in-flight
            *state.connect_ref = None;
            if let Some(old_sink) = current_sink.take() {
                old_sink.stop();
            }
            let _ = state.status_tx.send(AudioStatus::Stopped);
        }
        _ => {}
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

struct SpawnConnectionState<'a> {
    conn_id_ref: &'a mut u64,
    active_ref: &'a Arc<AtomicU64>,
    connect_ref: &'a mut Option<std::thread::JoinHandle<Result<Sink, String>>>,
    output_stream: &'a mut Option<OutputStream>,
    output_handle: &'a mut Option<rodio::OutputStreamHandle>,
    status_tx: &'a mpsc::Sender<AudioStatus>,
    sample_buffer: &'a Arc<Mutex<VecDeque<f32>>>,
    preferred_output_device_name: &'a Option<String>,
    reopen_output_on_next_connection: &'a mut bool,
}

fn start_connection(
    url: String,
    reset_hardware_retries: bool,
    current_url: &mut Option<String>,
    hardware_recovery_retries: &mut u8,
    state: &mut SpawnConnectionState<'_>,
) {
    *current_url = Some(url.clone());
    if reset_hardware_retries {
        *hardware_recovery_retries = 0;
    }
    spawn_connection(url, state);
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
