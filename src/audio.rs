mod buffer;
mod metadata;
mod recording;
mod stream_reader;
mod visualizer;

use buffer::BufferQueue;
use stream_reader::StreamReader;
use visualizer::VisualizerSource;

use rodio::{Decoder, OutputStream, Sink};
use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Commands sent from the UI thread to the audio thread.
#[derive(Debug, Clone)]
pub enum AudioCommand {
    Play(String), // URL
    Pause,
    Resume,
    Stop,
    SetVolume(f32), // 0.0 — 1.0
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
    Paused,
    Stopped,
    Error(String),
    Connecting,
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
    // Keep OutputStream alive for the lifetime of this thread.
    let (_stream, handle) = match OutputStream::try_default() {
        Ok(s) => s,
        Err(e) => {
            let _ = status_tx.send(AudioStatus::Error(format!("Soundcard error: {}", e)));
            return;
        }
    };

    let mut current_sink: Option<Sink> = None;
    let mut connect_thread: Option<std::thread::JoinHandle<Result<Sink, String>>> = None;

    // Concurrency guard to abandon stale threads instantly
    let active_conn_id = Arc::new(AtomicU64::new(0));
    let mut current_conn_id: u64 = 0;

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

    loop {
        // Helper closure to spawn a connection thread (used from 3 dispatch sites)
        let spawn_connection =
            |url: String,
             conn_id_ref: &mut u64,
             active_ref: &Arc<AtomicU64>,
             connect_ref: &mut Option<std::thread::JoinHandle<Result<Sink, String>>>| {
                *conn_id_ref += 1;
                active_ref.store(*conn_id_ref, Ordering::SeqCst);
                let _ = status_tx.send(AudioStatus::Connecting);

                let handle_clone = handle.clone();
                let status_tx_clone = status_tx.clone();
                let conn_id = *conn_id_ref;
                let active_conn_id_clone = active_ref.clone();
                let record_state_clone = record_state.clone();
                let sample_buffer_clone = sample_buffer.clone();

                drop(connect_ref.take());
                *connect_ref = Some(std::thread::spawn(move || {
                    connect_and_decode(
                        &url,
                        &handle_clone,
                        status_tx_clone,
                        conn_id,
                        active_conn_id_clone,
                        record_state_clone,
                        sample_buffer_clone,
                    )
                }));
            };

        // Non-blocking check for commands (10ms poll)
        match cmd_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(cmd) => {
                match cmd {
                    AudioCommand::Play(url) => {
                        if current_sink.is_some() {
                            pending_action = Some(AudioCommand::Play(url));
                        } else {
                            spawn_connection(
                                url,
                                &mut current_conn_id,
                                &active_conn_id,
                                &mut connect_thread,
                            );
                        }
                    }
                    AudioCommand::Pause => {
                        if current_sink.is_some() {
                            pending_action = Some(AudioCommand::Pause);
                        } else {
                            let _ = status_tx.send(AudioStatus::Paused);
                        }
                    }
                    AudioCommand::Resume => {
                        if let Some(ref sink) = current_sink {
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
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }

        // Process pending action / non-blocking fade-out
        if pending_action.is_some() {
            if let Some(ref sink) = current_sink {
                let current_vol = sink.volume();
                if current_vol <= 0.05 {
                    // Fade out completed! Execute pending command
                    sink.set_volume(0.0);
                    let cmd = pending_action.take().unwrap();
                    match cmd {
                        AudioCommand::Play(url) => {
                            // Stop current sink before spawning new connection
                            if let Some(old_sink) = current_sink.take() {
                                old_sink.stop();
                            }
                            spawn_connection(
                                url,
                                &mut current_conn_id,
                                &active_conn_id,
                                &mut connect_thread,
                            );
                        }
                        AudioCommand::Stop => {
                            active_conn_id.store(0, Ordering::SeqCst); // abandon in-flight
                            connect_thread = None;
                            if let Some(old_sink) = current_sink.take() {
                                old_sink.stop();
                            }
                            let _ = status_tx.send(AudioStatus::Stopped);
                        }
                        AudioCommand::Pause => {
                            sink.pause();
                            let _ = status_tx.send(AudioStatus::Paused);
                        }
                        _ => {}
                    }
                } else {
                    // Exponential step-down for beautiful natural dimming
                    let step = current_vol * 0.15; // smooth 15% dimming step
                    sink.set_volume((current_vol - step).max(0.0));
                }
            } else {
                // No active sink, just execute pending immediately
                let cmd = pending_action.take().unwrap();
                match cmd {
                    AudioCommand::Play(url) => {
                        spawn_connection(
                            url,
                            &mut current_conn_id,
                            &active_conn_id,
                            &mut connect_thread,
                        );
                    }
                    AudioCommand::Stop => {
                        active_conn_id.store(0, Ordering::SeqCst);
                        connect_thread = None;
                        let _ = status_tx.send(AudioStatus::Stopped);
                    }
                    AudioCommand::Pause => {
                        let _ = status_tx.send(AudioStatus::Paused);
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
                            let _ = status_tx.send(AudioStatus::Error(e));
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

/// Connect to a stream URL and create a playable Sink, with automatic backoff retries.
fn connect_and_decode(
    url: &str,
    handle: &rodio::OutputStreamHandle,
    status_tx: mpsc::Sender<AudioStatus>,
    conn_id: u64,
    active_conn_id: Arc<AtomicU64>,
    record_state: Arc<RecordStateShared>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
) -> Result<Sink, String> {
    let mut retries = 0;
    let max_retries = 5;
    let mut backoff = Duration::from_secs(1);

    loop {
        // Double check cancellation
        if active_conn_id.load(Ordering::SeqCst) != conn_id {
            return Err("Abandoned".into());
        }

        match try_connect_and_decode_once(
            url,
            handle,
            status_tx.clone(),
            conn_id,
            active_conn_id.clone(),
            record_state.clone(),
            sample_buffer.clone(),
        ) {
            Ok(sink) => return Ok(sink),
            Err(e) => {
                if e == "Abandoned" {
                    return Err("Abandoned".into());
                }

                retries += 1;
                if retries >= max_retries {
                    return Err(format!("Failed after {} retries: {}", max_retries, e));
                }

                // Notify UI about tuning status retry
                let _ = status_tx.send(AudioStatus::Connecting);

                // Sleep with backoff, checking for abandonment every 100ms
                let sleep_step = Duration::from_millis(100);
                let steps = (backoff.as_millis() / sleep_step.as_millis()) as usize;
                for _ in 0..steps {
                    if active_conn_id.load(Ordering::SeqCst) != conn_id {
                        return Err("Abandoned".into());
                    }
                    std::thread::sleep(sleep_step);
                }

                backoff = (backoff * 2).min(Duration::from_secs(8));
            }
        }
    }
}

fn try_connect_and_decode_once(
    url: &str,
    handle: &rodio::OutputStreamHandle,
    status_tx: mpsc::Sender<AudioStatus>,
    conn_id: u64,
    active_conn_id: Arc<AtomicU64>,
    record_state: Arc<RecordStateShared>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
) -> Result<Sink, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .connect_timeout(Duration::from_secs(5))
        .user_agent(format!("DriftFM/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    if active_conn_id.load(Ordering::SeqCst) != conn_id {
        return Err("Abandoned".into());
    }

    let response = client
        .get(url)
        .header("Icy-MetaData", "1")
        .send()
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    if active_conn_id.load(Ordering::SeqCst) != conn_id {
        return Err("Abandoned".into());
    }

    let metaint = response
        .headers()
        .get("icy-metaint")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());

    let bitrate_kbps = response
        .headers()
        .get("icy-br")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(128);
    let bytes_per_sec = (bitrate_kbps * 1000 / 8).max(1) as usize;

    // Decouple download from decoding: Spawn Bounded Producer-Consumer resiliences
    let buffer_capacity = 1024 * 1024; // 1 MB circular byte queue
    let queue = Arc::new(BufferQueue::new(buffer_capacity));

    let queue_clone = queue.clone();
    let active_conn_id_clone = active_conn_id.clone();
    let conn_id_clone = conn_id;
    let status_tx_clone = status_tx.clone();
    let mut response_reader = response;

    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            if active_conn_id_clone.load(Ordering::SeqCst) != conn_id_clone {
                queue_clone.set_disconnected(true);
                break;
            }
            match response_reader.read(&mut buf) {
                Ok(0) => {
                    queue_clone.set_disconnected(true);
                    break;
                }
                Ok(n) => {
                    queue_clone.push(&buf[..n]);

                    // Send circular buffer progress telemetries to UI
                    let len = queue_clone.len();
                    let cap = queue_clone.capacity;
                    let percent = ((len * 100) / cap) as u8;
                    let seconds = (len / bytes_per_sec) as u32;
                    let _ = status_tx_clone.send(AudioStatus::BufferLevel { percent, seconds });
                }
                Err(_) => {
                    queue_clone.set_disconnected(true);
                    break;
                }
            }
        }
    });

    let reader = StreamReader::new(
        url.to_string(),
        queue,
        status_tx,
        conn_id,
        active_conn_id,
        record_state,
        metaint,
    );

    let source = Decoder::new(reader).map_err(|e| format!("Decode error: {}", e))?;

    let wrapped_source = VisualizerSource::new(source, sample_buffer);

    let sink = Sink::try_new(handle).map_err(|e| format!("Sink error: {}", e))?;

    sink.append(wrapped_source);

    Ok(sink)
}
