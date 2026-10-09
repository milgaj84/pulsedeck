//! The HLS fetcher thread: downloads playlists and segments and hands audio
//! bytes to [`HlsSource`](super::source::HlsSource) through a bounded channel.

use super::playlist::{self, HlsError, MediaPlaylist, Playlist};
use super::source::HlsChunk;
use super::ts::AudioExtractor;
use reqwest::Url;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub(crate) const MAX_PLAYLIST_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_SEGMENT_BYTES: usize = 8 * 1024 * 1024;

/// How often waits and blocked sends re-check whether playback was abandoned.
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FetchError(pub String);

/// Blocking HTTP access, abstracted so the fetcher can be tested offline.
pub(crate) trait HlsHttp: Send + 'static {
    /// GET `url`, failing if the body is larger than `max_bytes`.
    fn get(&self, url: &Url, max_bytes: usize) -> Result<Vec<u8>, FetchError>;
}

impl HlsHttp for reqwest::blocking::Client {
    fn get(&self, url: &Url, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
        let response = self
            .get(url.clone())
            .send()
            .map_err(|error| FetchError(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError(format!("HTTP {}", status.as_u16())));
        }
        read_capped(response, max_bytes)
    }
}

/// Read at most `max_bytes`; a longer body is an error, not a truncation.
pub(crate) fn read_capped(reader: impl Read, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
    let mut body = Vec::new();
    reader
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|error| FetchError(error.to_string()))?;
    if body.len() > max_bytes {
        return Err(FetchError(format!(
            "response larger than {max_bytes} bytes"
        )));
    }
    Ok(body)
}

#[derive(Debug, Clone)]
pub(crate) struct FetcherConfig {
    /// Segments to start with on a live playlist (counted from the live edge).
    pub start_segments: usize,
    /// Consecutive segment downloads that may fail (and be skipped).
    pub max_segment_failures: u32,
    /// Consecutive playlist refreshes that may fail.
    pub max_refresh_failures: u32,
    /// Divides every wait; 1 in production, larger only to speed up tests.
    pub wait_divisor: u32,
}

impl Default for FetcherConfig {
    fn default() -> Self {
        Self {
            start_segments: 3,
            max_segment_failures: 3,
            max_refresh_failures: 3,
            wait_divisor: 1,
        }
    }
}

struct Fetcher<H: HlsHttp> {
    http: H,
    playlist_url: Url,
    generation: u64,
    active: Arc<AtomicU64>,
    tx: SyncSender<HlsChunk>,
    cfg: FetcherConfig,
}

/// Run the fetcher until the stream ends, fails or is abandoned. Always
/// terminates by sending `End` or `Fail` unless the receiver is gone.
pub(crate) fn run_fetcher<H: HlsHttp>(
    http: H,
    playlist_url: Url,
    first: MediaPlaylist,
    generation: u64,
    active: Arc<AtomicU64>,
    tx: SyncSender<HlsChunk>,
    cfg: FetcherConfig,
) {
    Fetcher {
        http,
        playlist_url,
        generation,
        active,
        tx,
        cfg,
    }
    .run(first);
}

impl<H: HlsHttp> Fetcher<H> {
    fn is_active(&self) -> bool {
        self.active.load(SeqCst) == self.generation
    }

    /// Wait up to `duration / wait_divisor`; false if playback was abandoned.
    fn wait(&self, duration: Duration) -> bool {
        let mut left = duration / self.cfg.wait_divisor.max(1);
        while !left.is_zero() {
            if !self.is_active() {
                return false;
            }
            let step = left.min(POLL);
            thread::sleep(step);
            left -= step;
        }
        self.is_active()
    }

    /// Send a chunk, waiting while the channel is full. False if the receiver
    /// is gone or playback was abandoned.
    fn send(&self, mut chunk: HlsChunk) -> bool {
        loop {
            if !self.is_active() {
                return false;
            }
            match self.tx.try_send(chunk) {
                Ok(()) => return true,
                Err(TrySendError::Disconnected(_)) => return false,
                Err(TrySendError::Full(returned)) => {
                    chunk = returned;
                    thread::sleep(POLL);
                }
            }
        }
    }

    fn fail(&self, message: impl Into<String>) {
        let _ = self.send(HlsChunk::Fail(message.into()));
    }

    fn run(self, first: MediaPlaylist) {
        let mut playlist = first;
        let mut last_seq: Option<u64> = None;
        let mut extractor = AudioExtractor::default();
        let mut segment_failures = 0u32;

        loop {
            if !self.is_active() {
                return;
            }

            let mut new: Vec<_> = playlist
                .segments
                .iter()
                .filter(|segment| last_seq.is_none_or(|last| segment.seq > last))
                .collect();

            if last_seq.is_none() && !playlist.end_list && new.len() > self.cfg.start_segments {
                new.drain(..new.len() - self.cfg.start_segments);
            }
            if let (Some(last), Some(first_new)) = (last_seq, new.first()) {
                if first_new.seq > last + 1 {
                    // Fell behind the live window: the audio is no longer contiguous.
                    extractor.reset();
                }
            }

            let got_new = !new.is_empty();
            for segment in new {
                if !self.is_active() {
                    return;
                }
                if segment.discontinuity {
                    extractor.reset();
                }

                match self.http.get(&segment.uri, MAX_SEGMENT_BYTES) {
                    Ok(bytes) => {
                        segment_failures = 0;
                        let mut audio = Vec::new();
                        if let Err(error) = extractor.push_segment(&bytes, &mut audio) {
                            self.fail(error.to_string());
                            return;
                        }
                        if !audio.is_empty() && !self.send(HlsChunk::Bytes(audio)) {
                            return;
                        }
                    }
                    Err(FetchError(message)) => {
                        segment_failures += 1;
                        if segment_failures > self.cfg.max_segment_failures {
                            self.fail(format!("HLS: segment download failed: {message}"));
                            return;
                        }
                    }
                }
                last_seq = Some(segment.seq);
            }

            if playlist.end_list {
                let _ = self.send(HlsChunk::End);
                return;
            }

            let wait = if got_new {
                playlist.target_duration
            } else {
                playlist.target_duration / 2
            };
            if !self.wait(wait) {
                return;
            }

            match self.refresh(playlist.target_duration) {
                Some(next) => playlist = next,
                None => return,
            }
        }
    }

    /// Re-fetch the media playlist, retrying a few times. `None` means the
    /// stream was ended (failure already reported) or abandoned.
    fn refresh(&self, target: Duration) -> Option<MediaPlaylist> {
        let mut failures = 0u32;
        loop {
            let error = match self.http.get(&self.playlist_url, MAX_PLAYLIST_BYTES) {
                Ok(body) => {
                    match playlist::parse(&String::from_utf8_lossy(&body), &self.playlist_url) {
                        Ok(Playlist::Media(media)) => return Some(media),
                        Ok(Playlist::Master(_)) => {
                            self.fail("HLS: playlist changed into a master playlist");
                            return None;
                        }
                        Err(error @ (HlsError::Encrypted | HlsError::Fmp4)) => {
                            self.fail(error.to_string());
                            return None;
                        }
                        Err(error) => error.to_string(),
                    }
                }
                Err(FetchError(message)) => message,
            };

            failures += 1;
            if failures > self.cfg.max_refresh_failures {
                self.fail(format!("HLS: playlist refresh failed: {error}"));
                return None;
            }
            if !self.wait(target / 2) {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::*;
    use std::collections::HashMap;
    use std::sync::mpsc::{self, Receiver};
    use std::sync::Mutex;
    use std::time::Instant;

    type Responder = Box<dyn Fn(u32) -> Result<Vec<u8>, FetchError> + Send>;

    /// Serves canned responses by URL path; the closure gets the hit number.
    #[derive(Clone, Default)]
    struct FakeHttp {
        routes: Arc<Mutex<HashMap<String, Responder>>>,
        hits: Arc<Mutex<HashMap<String, u32>>>,
        sizes: Arc<Mutex<Vec<usize>>>,
    }

    impl FakeHttp {
        fn route(
            &self,
            path: &str,
            responder: impl Fn(u32) -> Result<Vec<u8>, FetchError> + Send + 'static,
        ) {
            self.routes
                .lock()
                .unwrap()
                .insert(path.to_string(), Box::new(responder));
        }

        fn fixed(&self, path: &str, body: Vec<u8>) {
            self.route(path, move |_| Ok(body.clone()));
        }

        fn hits(&self, path: &str) -> u32 {
            *self.hits.lock().unwrap().get(path).unwrap_or(&0)
        }
    }

    impl HlsHttp for FakeHttp {
        fn get(&self, url: &Url, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
            let path = url.path().to_string();
            let hit = {
                let mut hits = self.hits.lock().unwrap();
                let entry = hits.entry(path.clone()).or_insert(0);
                *entry += 1;
                *entry
            };
            self.sizes.lock().unwrap().push(max_bytes);
            let routes = self.routes.lock().unwrap();
            let body = match routes.get(&path) {
                Some(responder) => responder(hit)?,
                None => return Err(FetchError("HTTP 404".to_string())),
            };
            if body.len() > max_bytes {
                return Err(FetchError("too large".to_string()));
            }
            Ok(body)
        }
    }

    fn url(path: &str) -> Url {
        Url::parse(&format!("https://radio.example{path}")).unwrap()
    }

    fn fast() -> FetcherConfig {
        FetcherConfig {
            wait_divisor: 500,
            ..FetcherConfig::default()
        }
    }

    fn playlist_text(first_seq: u64, count: usize, end: bool) -> String {
        let mut text =
            format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:{first_seq}\n");
        for seq in first_seq..first_seq + count as u64 {
            text.push_str(&format!("#EXTINF:1,\n/seg{seq}.ts\n"));
        }
        if end {
            text.push_str("#EXT-X-ENDLIST\n");
        }
        text
    }

    fn parse_media(text: &str) -> MediaPlaylist {
        match playlist::parse(text, &url("/live.m3u8")).unwrap() {
            Playlist::Media(media) => media,
            other => panic!("not media: {other:?}"),
        }
    }

    /// A TS segment whose single ADTS frame has a payload of `marker` bytes,
    /// so the audio coming out identifies the segment.
    fn segment(marker: usize) -> Vec<u8> {
        ts_segment(0x0F, &[adts_frame(marker, false, false)])
    }

    fn marker_frame(marker: usize) -> Vec<u8> {
        adts_frame(marker, false, false)
    }

    fn spawn(
        http: &FakeHttp,
        first: &str,
        generation: u64,
        active: &Arc<AtomicU64>,
        cfg: FetcherConfig,
    ) -> (Receiver<HlsChunk>, thread::JoinHandle<()>) {
        let (tx, rx) = mpsc::sync_channel(3);
        let (http, active) = (http.clone(), Arc::clone(active));
        let first = parse_media(first);
        let handle = thread::spawn(move || {
            run_fetcher(http, url("/live.m3u8"), first, generation, active, tx, cfg)
        });
        (rx, handle)
    }

    fn drain(rx: &Receiver<HlsChunk>) -> (Vec<u8>, Option<HlsChunk>) {
        let mut audio = Vec::new();
        loop {
            match rx.recv_timeout(Duration::from_secs(10)) {
                Ok(HlsChunk::Bytes(bytes)) => audio.extend(bytes),
                Ok(terminal) => return (audio, Some(terminal)),
                Err(_) => return (audio, None),
            }
        }
    }

    fn join(handle: thread::JoinHandle<()>) {
        let started = Instant::now();
        while !handle.is_finished() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "fetcher did not stop"
            );
            thread::sleep(Duration::from_millis(5));
        }
        handle.join().unwrap();
    }

    #[test]
    fn vod_playlist_sends_every_segment_then_end() {
        let http = FakeHttp::default();
        for i in 0..3 {
            http.fixed(&format!("/seg{i}.ts"), segment(20 + i));
        }
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 3, true), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(
            audio,
            (0..3)
                .flat_map(|i| marker_frame(20 + i))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn live_playlist_starts_at_the_live_edge() {
        let http = FakeHttp::default();
        for i in 0..6 {
            http.fixed(&format!("/seg{i}.ts"), segment(20 + i));
        }
        // Refreshes reveal the end so the run terminates.
        http.fixed("/live.m3u8", playlist_text(0, 6, true).into_bytes());
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 6, false), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        // Started with segments 3,4,5; the refresh adds nothing new.
        assert_eq!(
            audio,
            (3..6)
                .flat_map(|i| marker_frame(20 + i))
                .collect::<Vec<_>>()
        );
        assert_eq!(http.hits("/seg0.ts"), 0);
    }

    #[test]
    fn rolling_live_playlist_never_repeats_or_skips_segments() {
        let http = FakeHttp::default();
        for i in 0..9 {
            http.fixed(&format!("/seg{i}.ts"), segment(20 + i));
        }
        // Each refresh slides the window by two segments, then ends.
        http.route("/live.m3u8", |hit| {
            Ok(match hit {
                1 => playlist_text(2, 3, false),
                2 => playlist_text(4, 3, false),
                _ => playlist_text(6, 3, true),
            }
            .into_bytes())
        });
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 3, false), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(
            audio,
            (0..9)
                .flat_map(|i| marker_frame(20 + i))
                .collect::<Vec<_>>()
        );
        for i in 0..9 {
            assert_eq!(http.hits(&format!("/seg{i}.ts")), 1, "segment {i}");
        }
    }

    #[test]
    fn falling_behind_the_window_skips_ahead() {
        let http = FakeHttp::default();
        for i in 0..20 {
            http.fixed(&format!("/seg{i}.ts"), segment(20 + i));
        }
        http.fixed("/live.m3u8", playlist_text(10, 3, true).into_bytes());
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 3, false), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        let expected: Vec<u8> = [0, 1, 2, 10, 11, 12]
            .iter()
            .flat_map(|i| marker_frame(20 + i))
            .collect();
        assert_eq!(audio, expected);
    }

    #[test]
    fn failed_segments_are_skipped_up_to_the_limit() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", segment(20));
        http.fixed("/seg2.ts", segment(22));
        // seg1 has no route: 404.
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 3, true), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(audio, [marker_frame(20), marker_frame(22)].concat());
    }

    #[test]
    fn too_many_consecutive_segment_failures_end_the_stream() {
        let http = FakeHttp::default();
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 6, true), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert!(audio.is_empty());
        match terminal {
            Some(HlsChunk::Fail(message)) => {
                assert!(
                    message.starts_with("HLS: segment download failed"),
                    "{message}"
                )
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn oversized_segments_count_as_failures_and_are_skipped() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", vec![0u8; MAX_SEGMENT_BYTES + 1]);
        http.fixed("/seg1.ts", segment(21));
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 2, true), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(audio, marker_frame(21));
    }

    #[test]
    fn repeated_refresh_failures_end_the_stream_with_a_message() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", segment(20));
        // /live.m3u8 has no route: every refresh fails.
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 1, false), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(audio, marker_frame(20));
        match terminal {
            Some(HlsChunk::Fail(message)) => {
                assert!(
                    message.starts_with("HLS: playlist refresh failed"),
                    "{message}"
                )
            }
            other => panic!("expected failure, got {other:?}"),
        }
        assert_eq!(http.hits("/live.m3u8"), 4);
    }

    #[test]
    fn a_refresh_that_recovers_keeps_the_stream_going() {
        let http = FakeHttp::default();
        for i in 0..2 {
            http.fixed(&format!("/seg{i}.ts"), segment(20 + i));
        }
        http.route("/live.m3u8", |hit| match hit {
            1 | 2 => Err(FetchError("HTTP 503".to_string())),
            _ => Ok(playlist_text(0, 2, true).into_bytes()),
        });
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 1, false), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(audio, [marker_frame(20), marker_frame(21)].concat());
    }

    #[test]
    fn a_refresh_that_turns_encrypted_fails_clearly() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", segment(20));
        http.fixed(
            "/live.m3u8",
            b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n#EXTINF:1,\n/seg1.ts\n"
                .to_vec(),
        );
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 1, false), 1, &active, fast());
        let (_, terminal) = drain(&rx);
        join(handle);

        assert_eq!(
            terminal,
            Some(HlsChunk::Fail(HlsError::Encrypted.to_string()))
        );
    }

    #[test]
    fn an_unsupported_segment_fails_the_stream() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", b"<html>not audio</html>".to_vec());
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 1, true), 1, &active, fast());
        let (_, terminal) = drain(&rx);
        join(handle);

        match terminal {
            Some(HlsChunk::Fail(message)) => assert!(message.starts_with("HLS:"), "{message}"),
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn a_mid_stream_codec_change_fails_the_stream() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", segment(20));
        http.fixed("/seg1.ts", ts_segment(0x03, &[mp3_frame(40)]));
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 2, true), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(audio, marker_frame(20));
        match terminal {
            Some(HlsChunk::Fail(message)) => {
                assert!(message.contains("not supported"), "{message}")
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn dropping_the_receiver_stops_the_thread() {
        let http = FakeHttp::default();
        for i in 0..50 {
            http.fixed(&format!("/seg{i}.ts"), segment(20));
        }
        let active = Arc::new(AtomicU64::new(1));
        let (rx, handle) = spawn(&http, &playlist_text(0, 50, true), 1, &active, fast());

        drop(rx);

        join(handle);
        assert!(http.hits("/seg0.ts") + http.hits("/seg49.ts") < 50);
    }

    #[test]
    fn abandoning_the_generation_stops_the_thread_while_waiting() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", segment(20));
        http.fixed("/live.m3u8", playlist_text(0, 1, false).into_bytes());
        let active = Arc::new(AtomicU64::new(1));
        // Real waits: a one-second target keeps the fetcher sleeping.
        let cfg = FetcherConfig {
            wait_divisor: 1,
            ..FetcherConfig::default()
        };
        let (rx, handle) = spawn(&http, &playlist_text(0, 1, false), 1, &active, cfg);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(HlsChunk::Bytes(_))
        ));

        active.store(2, SeqCst);

        join(handle);
    }

    #[test]
    fn a_full_channel_applies_backpressure() {
        let http = FakeHttp::default();
        for i in 0..10 {
            http.fixed(&format!("/seg{i}.ts"), segment(20));
        }
        let active = Arc::new(AtomicU64::new(1));
        let (rx, handle) = spawn(&http, &playlist_text(0, 10, true), 1, &active, fast());

        thread::sleep(Duration::from_millis(300));

        // The channel holds 3 chunks and one more is waiting to be sent, so the
        // fetcher cannot have downloaded much beyond that.
        assert!(
            http.hits("/seg9.ts") == 0,
            "fetcher ran ahead of the reader"
        );
        let (_, terminal) = drain(&rx);
        join(handle);
        assert_eq!(terminal, Some(HlsChunk::End));
    }

    #[test]
    fn read_capped_rejects_bodies_over_the_limit() {
        assert_eq!(read_capped(&[1u8, 2, 3][..], 3).unwrap(), [1, 2, 3]);
        assert!(read_capped(&[1u8, 2, 3, 4][..], 3).is_err());
        assert!(read_capped(&[][..], 0).unwrap().is_empty());
    }

    #[test]
    fn default_config_matches_the_documented_limits() {
        let cfg = FetcherConfig::default();
        assert_eq!(cfg.start_segments, 3);
        assert_eq!(cfg.max_segment_failures, 3);
        assert_eq!(cfg.max_refresh_failures, 3);
        assert_eq!(cfg.wait_divisor, 1);
    }
}
