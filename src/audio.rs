use rodio::{Decoder, OutputStream, Sink, Source as RodioSource, Sample as RodioSample};
use rodio::cpal::Sample as CpalSample;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Condvar};
use std::collections::VecDeque;
use std::time::Duration;
use std::fs::File;
use std::path::PathBuf;

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

/// Bounded Producer-Consumer circular byte queue (Resiliency Buffer)
struct BufferQueue {
    queue: Mutex<VecDeque<u8>>,
    cv_read: Condvar,
    cv_write: Condvar,
    capacity: usize,
    disconnected: AtomicBool,
}

impl BufferQueue {
    fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            cv_read: Condvar::new(),
            cv_write: Condvar::new(),
            capacity,
            disconnected: AtomicBool::new(false),
        }
    }

    fn push(&self, bytes: &[u8]) {
        let mut queue = self.queue.lock().unwrap();
        // Block downloader thread if buffer capacity limit is reached
        while queue.len() + bytes.len() > self.capacity && !self.disconnected.load(Ordering::SeqCst) {
            queue = self.cv_write.wait(queue).unwrap();
        }
        if self.disconnected.load(Ordering::SeqCst) {
            return;
        }
        queue.extend(bytes);
        self.cv_read.notify_all();
    }

    fn pop(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut queue = self.queue.lock().unwrap();
        while queue.is_empty() && !self.disconnected.load(Ordering::SeqCst) {
            queue = self.cv_read.wait(queue).unwrap();
        }
        if queue.is_empty() && self.disconnected.load(Ordering::SeqCst) {
            return Ok(0); // EOF or network disconnection
        }

        let count = std::cmp::min(buf.len(), queue.len());
        for slot in buf.iter_mut().take(count) {
            *slot = queue.pop_front().unwrap();
        }
        self.cv_write.notify_all();
        Ok(count)
    }

    fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    fn set_disconnected(&self, disc: bool) {
        self.disconnected.store(disc, Ordering::SeqCst);
        let _queue = self.queue.lock().unwrap();
        // Wake up both consumer and producer to terminate gracefully
        self.cv_read.notify_all();
        self.cv_write.notify_all();
    }
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

        Self { cmd_tx, status_rx, sample_buffer }
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
        let spawn_connection = |
            url: String,
            conn_id_ref: &mut u64,
            active_ref: &Arc<AtomicU64>,
            connect_ref: &mut Option<std::thread::JoinHandle<Result<Sink, String>>>,
        | {
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
                connect_and_decode(&url, &handle_clone, status_tx_clone, conn_id, active_conn_id_clone, record_state_clone, sample_buffer_clone)
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
                            spawn_connection(url, &mut current_conn_id, &active_conn_id, &mut connect_thread);
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
                    AudioCommand::StartRecording { recording_dir, category, keep_snippets, min_song_duration_secs } => {
                        *record_state.recording_dir.lock().unwrap() = recording_dir;
                        *record_state.category.lock().unwrap() = category;
                        record_state.keep_snippets.store(keep_snippets, Ordering::SeqCst);
                        record_state.min_song_duration_secs.store(min_song_duration_secs, Ordering::SeqCst);
                        record_state.state.store(1, Ordering::SeqCst); // Transition to Pending
                        let _ = status_tx.send(AudioStatus::RecordingStateChanged { state: 1, filepath: None });
                    }
                    AudioCommand::StopRecording => {
                        record_state.state.store(0, Ordering::SeqCst); // Transition to Off
                        let _ = status_tx.send(AudioStatus::RecordingStateChanged { state: 0, filepath: None });
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
                            spawn_connection(url, &mut current_conn_id, &active_conn_id, &mut connect_thread);
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
                        spawn_connection(url, &mut current_conn_id, &active_conn_id, &mut connect_thread);
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
                        let _ = status_tx.send(AudioStatus::Error(
                            "Connection thread panicked".into(),
                        ));
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

        match try_connect_and_decode_once(url, handle, status_tx.clone(), conn_id, active_conn_id.clone(), record_state.clone(), sample_buffer.clone()) {
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
        .danger_accept_invalid_certs(true)
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

    let reader = StreamReader::new(url.to_string(), queue, status_tx, conn_id, active_conn_id, record_state, metaint);

    let source = Decoder::new(reader)
        .map_err(|e| format!("Decode error: {}", e))?;

    let wrapped_source = VisualizerSource::new(source, sample_buffer);

    let sink = Sink::try_new(handle)
        .map_err(|e| format!("Sink error: {}", e))?;

    sink.append(wrapped_source);

    Ok(sink)
}

/// A custom source wrapper that intercepts audio sample frames, buffers them,
/// and writes them to a thread-safe circular sample buffer for rendering.
pub struct VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32>,
{
    inner: S,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    local_buf: Vec<f32>,
}

impl<S> VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32>,
{
    pub fn new(inner: S, sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        Self {
            inner,
            sample_buffer,
            local_buf: Vec::with_capacity(128),
        }
    }
}

impl<S> Iterator for VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32>,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next();
        if let Some(s) = sample {
            let float_sample = s.to_float_sample();
            self.local_buf.push(float_sample);
            if self.local_buf.len() >= 128 {
                if let Ok(mut buffer) = self.sample_buffer.lock() {
                    buffer.extend(self.local_buf.drain(..));
                    while buffer.len() > 4096 {
                        buffer.pop_front();
                    }
                } else {
                    self.local_buf.clear();
                }
            }
        }
        sample
    }
}

impl<S> RodioSource for VisualizerSource<S>
where
    S: RodioSource,
    S::Item: RodioSample + CpalSample<Float = f32>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

/// StreamReader consuming from thread-safe ring-buffer and stripping metadata boundaries
struct StreamReader {
    url: String,
    queue: Arc<BufferQueue>,
    pos: u64,
    metaint: Option<usize>,
    bytes_until_meta: usize,
    status_tx: mpsc::Sender<AudioStatus>,
    conn_id: u64,
    active_conn_id: Arc<AtomicU64>,
    
    // Recording state trackers
    record_state: Arc<RecordStateShared>,
    active_writer: Option<File>,
    active_file_path: Option<String>,
    active_track_title: Option<String>,
    active_track_start_time: Option<std::time::Instant>,
}

impl StreamReader {
    fn new(
        url: String,
        queue: Arc<BufferQueue>,
        status_tx: mpsc::Sender<AudioStatus>,
        conn_id: u64,
        active_conn_id: Arc<AtomicU64>,
        record_state: Arc<RecordStateShared>,
        metaint: Option<usize>,
    ) -> Self {
        Self {
            url,
            queue,
            pos: 0,
            metaint,
            bytes_until_meta: metaint.unwrap_or(0),
            status_tx,
            conn_id,
            active_conn_id,
            record_state,
            active_writer: None,
            active_file_path: None,
            active_track_title: None,
            active_track_start_time: None,
        }
    }

    fn read_metadata_block(&mut self) -> std::io::Result<()> {
        let mut length_byte = [0u8; 1];
        self.queue.pop(&mut length_byte)?;
        let length = length_byte[0] as usize * 16;

        if length > 0 {
            let mut meta_buf = vec![0u8; length];
            self.queue.pop(&mut meta_buf)?;
            if let Ok(meta_str) = String::from_utf8(meta_buf) {
                if let Some(title) = parse_stream_title(&meta_str) {
                    // Send main UI track change signal
                    let _ = self.status_tx.send(AudioStatus::TrackChanged {
                        url: self.url.clone(),
                        title: title.clone(),
                    });

                    // Trigger smart recording track boundary splitting!
                    self.handle_track_change(&title);
                }
            }
        }
        Ok(())
    }

    fn handle_track_change(&mut self, new_title: &str) {
        let record = self.record_state.clone();
        let mut state = record.state.load(Ordering::SeqCst);

        // If Pending next track start -> transition to Active!
        if state == 1 {
            record.state.store(2, Ordering::SeqCst);
            state = 2;
        }

        if state == 2 {
            // Close the old recorded segment cleanly
            self.close_recording_file();
            // Start recording the new segmented track!
            self.open_recording_file(new_title);
        }
    }

    fn open_recording_file(&mut self, title: &str) {
        let recording_dir = self.record_state.recording_dir.lock().unwrap().clone();
        let category = self.record_state.category.lock().unwrap().clone();

        if recording_dir.is_empty() {
            return;
        }

        let clean_category = sanitize_filename(&category);
        let clean_title = sanitize_filename(title);

        let mut path = PathBuf::from(&recording_dir);
        path.push(&clean_category);

        if let Err(e) = std::fs::create_dir_all(&path) {
            let _ = self.status_tx.send(AudioStatus::Error(format!("Failed to create folders: {}", e)));
            return;
        }

        // Auto-detect extension based on stream URL heuristics
        let ext = if self.url.contains("aac") || self.url.contains("m4a") {
            "aac"
        } else {
            "mp3"
        };

        path.push(format!("{}.{}", clean_title, ext));

        match File::create(&path) {
            Ok(file) => {
                self.active_writer = Some(file);
                self.active_file_path = Some(path.to_string_lossy().to_string());
                self.active_track_title = Some(title.to_string());
                self.active_track_start_time = Some(std::time::Instant::now());

                // Notify UI state to render red flashes
                let _ = self.status_tx.send(AudioStatus::RecordingStateChanged {
                    state: 2,
                    filepath: Some(path.to_string_lossy().to_string()),
                });
            }
            Err(e) => {
                let _ = self.status_tx.send(AudioStatus::Error(format!("Failed to create segment file: {}", e)));
            }
        }
    }

    fn close_recording_file(&mut self) {
        if let Some(mut file) = self.active_writer.take() {
            let _ = file.flush();
            drop(file); // Closes file handle

            let duration = self.active_track_start_time.take()
                .map(|t| t.elapsed())
                .unwrap_or(std::time::Duration::ZERO);

            let keep_snippets = self.record_state.keep_snippets.load(Ordering::SeqCst);
            let min_secs = self.record_state.min_song_duration_secs.load(Ordering::SeqCst);

            let mut is_ad_or_speech = false;
            if let Some(ref title) = self.active_track_title {
                let t_upper = title.to_uppercase();
                if t_upper.contains("ADVERT")
                    || t_upper.contains("COMMERCIAL")
                    || t_upper.contains("STATION ID")
                    || t_upper.contains("DJ SPEECH")
                    || t_upper.contains("NEWS CAST")
                    || t_upper.contains("NEWS UPDATE")
                    || t_upper.contains("WEATHER REPORT")
                    || t_upper.contains("TRAFFIC REPORT")
                    || t_upper.trim().is_empty()
                {
                    is_ad_or_speech = true;
                }
            } else {
                is_ad_or_speech = true;
            }

            let is_short = duration.as_secs() < min_secs as u64;

            if let Some(ref filepath) = self.active_file_path {
                if (is_short || is_ad_or_speech) && !keep_snippets {
                    // Purge file from disk!
                    if let Err(e) = std::fs::remove_file(filepath) {
                        let _ = self.status_tx.send(AudioStatus::Error(format!("Failed to delete partial file: {}", e)));
                    } else {
                        let title = self.active_track_title.as_deref().unwrap_or("Unknown Track");
                        let reason = if is_ad_or_speech { "Speech/Ad Filter" } else { "Short Snippet" };
                        let _ = self.status_tx.send(AudioStatus::Error(format!(
                            "🗑️ Discarded {} - {} ({:.1}s)",
                            reason, title, duration.as_secs_f32()
                        )));
                    }
                } else {
                    // Inject high-fidelity metadata tags into completed MP3 tracks
                    if filepath.ends_with(".mp3") {
                        if let Some(ref title) = self.active_track_title {
                            let _ = inject_id3_tags(filepath, title);
                        }
                    }
                }
            }
        }
        self.active_file_path = None;
        self.active_track_title = None;
        self.active_track_start_time = None;
    }
}

impl std::io::Read for StreamReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Exit early if this thread has been abandoned
        if self.active_conn_id.load(Ordering::SeqCst) != self.conn_id {
            self.close_recording_file();
            return Err(std::io::Error::other("Abandoned"));
        }

        // Sync recording file state with global config toggles
        let global_state = self.record_state.state.load(Ordering::SeqCst);
        if global_state == 0 && self.active_writer.is_some() {
            self.close_recording_file();
        }

        let Some(metaint) = self.metaint else {
            let n = self.queue.pop(buf)?;
            self.pos += n as u64;

            // Write read bytes directly to tape recorder if currently Active
            if global_state == 2 {
                if let Some(ref mut file) = self.active_writer {
                    let _ = file.write_all(&buf[..n]);
                }
            }
            return Ok(n);
        };

        if self.bytes_until_meta == 0 {
            self.read_metadata_block()?;
            self.bytes_until_meta = metaint;
        }

        let max_to_read = buf.len().min(self.bytes_until_meta);
        let n = self.queue.pop(&mut buf[..max_to_read])?;
        self.pos += n as u64;
        self.bytes_until_meta -= n;

        // Write read bytes directly to tape recorder if currently Active
        if global_state == 2 {
            if let Some(ref mut file) = self.active_writer {
                let _ = file.write_all(&buf[..n]);
            }
        }

        Ok(n)
    }
}

impl std::io::Seek for StreamReader {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Current(0) | std::io::SeekFrom::Start(_) => Ok(self.pos),
            std::io::SeekFrom::Current(offset) if offset > 0 => {
                let mut remaining = offset as u64;
                let mut discard = [0u8; 8192];
                while remaining > 0 {
                    let to_read = remaining.min(discard.len() as u64) as usize;
                    let n = self.read(&mut discard[..to_read])?;
                    if n == 0 {
                        break;
                    }
                    remaining -= n as u64;
                }
                Ok(self.pos)
            }
            _ => Ok(self.pos),
        }
    }
}

/// Inject ID3 metadata frames into completed local recordings
fn inject_id3_tags(filepath: &str, title: &str) -> Result<(), Box<dyn std::error::Error>> {
    use id3::{Tag, TagLike, Version};

    let mut artist = "Unknown Artist".to_string();
    let mut track_title = title.to_string();

    if let Some(pos) = title.find(" - ") {
        artist = title[..pos].trim().to_string();
        track_title = title[pos + 3..].trim().to_string();
    }

    let mut tag = Tag::new();
    tag.set_artist(artist);
    tag.set_title(track_title);
    tag.set_album("DriftFM live capturing");

    tag.write_to_path(filepath, Version::Id3v24)?;
    Ok(())
}

/// Replace invalid filesystem characters to protect host OS filesystems
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect()
}

/// Parse the `StreamTitle` field from an ICY metadata string.
fn parse_stream_title(meta: &str) -> Option<String> {
    let key = "StreamTitle='";
    if let Some(start_idx) = meta.find(key) {
        let value_start = start_idx + key.len();
        if let Some(end_idx) = meta[value_start..].find("';") {
            let title = &meta[value_start..value_start + end_idx];
            return Some(title.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_file.mp3"), "normal_file.mp3");
        assert_eq!(sanitize_filename("artist/song?.mp3"), "artist-song-.mp3");
        assert_eq!(sanitize_filename("windows\\invalid:name*char\".mp3"), "windows-invalid-name-char-.mp3");
        assert_eq!(sanitize_filename("<tag> | pipe.mp3"), "-tag- - pipe.mp3");
    }

    #[test]
    fn test_parse_stream_title() {
        // Valid metadata
        assert_eq!(
            parse_stream_title("StreamTitle='Lazerhawk - King of The Streets';StreamUrl='';"),
            Some("Lazerhawk - King of The Streets".to_string())
        );

        // Whitespace trim
        assert_eq!(
            parse_stream_title("StreamTitle='  Kavinsky - Nightcall  ';StreamUrl='';"),
            Some("Kavinsky - Nightcall".to_string())
        );

        // Missing StreamTitle
        assert_eq!(
            parse_stream_title("StreamUrl='';"),
            None
        );

        // Malformed or empty title
        assert_eq!(
            parse_stream_title("StreamTitle='';"),
            Some("".to_string())
        );
    }
}
