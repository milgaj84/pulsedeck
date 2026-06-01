use super::buffer::BufferQueue;
use super::buffer_meter::BufferStatusMeter;
use super::metadata::parse_stream_title;
use super::recording::{inject_id3_tags, sanitize_filename};
use super::{AudioStatus, RecordStateShared};

use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

/// StreamReader consuming from thread-safe ring-buffer and stripping metadata boundaries
pub(super) struct StreamReader {
    url: String,
    queue: Arc<BufferQueue>,
    buffer_meter: Arc<BufferStatusMeter>,
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

pub(super) struct StreamReaderConfig {
    pub(super) url: String,
    pub(super) queue: Arc<BufferQueue>,
    pub(super) buffer_meter: Arc<BufferStatusMeter>,
    pub(super) status_tx: mpsc::Sender<AudioStatus>,
    pub(super) conn_id: u64,
    pub(super) active_conn_id: Arc<AtomicU64>,
    pub(super) record_state: Arc<RecordStateShared>,
    pub(super) metaint: Option<usize>,
}

impl StreamReader {
    pub(super) fn new(config: StreamReaderConfig) -> Self {
        let bytes_until_meta = config.metaint.unwrap_or(0);

        Self {
            url: config.url,
            queue: config.queue,
            buffer_meter: config.buffer_meter,
            pos: 0,
            metaint: config.metaint,
            bytes_until_meta,
            status_tx: config.status_tx,
            conn_id: config.conn_id,
            active_conn_id: config.active_conn_id,
            record_state: config.record_state,
            active_writer: None,
            active_file_path: None,
            active_track_title: None,
            active_track_start_time: None,
        }
    }

    fn read_metadata_block(&mut self) -> std::io::Result<()> {
        let mut length_byte = [0u8; 1];
        let bytes_read = self.queue.pop(&mut length_byte)?;
        self.record_buffer_consumption(bytes_read);
        let length = length_byte[0] as usize * 16;

        if length > 0 {
            let mut meta_buf = vec![0u8; length];
            let bytes_read = self.queue.pop(&mut meta_buf)?;
            self.record_buffer_consumption(bytes_read);
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

    fn record_buffer_consumption(&self, bytes_read: usize) {
        self.buffer_meter.record_consumed(
            bytes_read,
            self.queue.len(),
            self.queue.capacity,
            &self.status_tx,
        );
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
            let _ = self.status_tx.send(AudioStatus::Error(format!(
                "Failed to create folders: {}",
                e
            )));
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
                let _ = self.status_tx.send(AudioStatus::Error(format!(
                    "Failed to create segment file: {}",
                    e
                )));
            }
        }
    }

    fn close_recording_file(&mut self) {
        if let Some(mut file) = self.active_writer.take() {
            let _ = file.flush();
            drop(file); // Closes file handle

            let duration = self
                .active_track_start_time
                .take()
                .map(|t| t.elapsed())
                .unwrap_or(std::time::Duration::ZERO);

            let keep_snippets = self.record_state.keep_snippets.load(Ordering::SeqCst);
            let min_secs = self
                .record_state
                .min_song_duration_secs
                .load(Ordering::SeqCst);

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
                        let _ = self.status_tx.send(AudioStatus::Error(format!(
                            "Failed to delete partial file: {}",
                            e
                        )));
                    } else {
                        let title = self
                            .active_track_title
                            .as_deref()
                            .unwrap_or("Unknown Track");
                        let reason = if is_ad_or_speech {
                            "Speech/Ad Filter"
                        } else {
                            "Short Snippet"
                        };
                        let _ = self.status_tx.send(AudioStatus::Error(format!(
                            "🗑️ Discarded {} - {} ({:.1}s)",
                            reason,
                            title,
                            duration.as_secs_f32()
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
            self.record_buffer_consumption(n);

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
        self.record_buffer_consumption(n);

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
