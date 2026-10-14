//! The HLS fetcher thread: downloads playlists and segments and hands audio
//! bytes to [`HlsSource`](super::source::HlsSource) through a bounded channel.

use super::crypto::{self, SampleAesKey};
use super::fmp4::Fmp4Init;
use super::playlist::{
    self, ByteRange, HlsError, KeyMethod, MediaPlaylist, Playlist, Segment, SegmentKey,
};
use super::source::HlsChunk;
use super::ts::AudioExtractor;
use reqwest::Url;
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub(crate) const MAX_PLAYLIST_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_SEGMENT_BYTES: usize = 8 * 1024 * 1024;
/// An init section is a few hundred bytes of boxes; this is generous.
const MAX_INIT_BYTES: usize = 1024 * 1024;
/// An AES-128 key is 16 bytes; anything much larger is not a key.
const MAX_KEY_BYTES: usize = 64;
/// Distinct init sections remembered at once.
const MAX_CACHED_INITS: usize = 4;
/// Distinct keys remembered at once (streams rotate keys occasionally).
const MAX_CACHED_KEYS: usize = 8;

/// How often waits and blocked sends re-check whether playback was abandoned.
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FetchError(pub String);

/// Blocking HTTP access, abstracted so the fetcher can be tested offline.
pub(crate) trait HlsHttp: Send + 'static {
    /// GET `url`, failing if the body is larger than `max_bytes`.
    fn get(&self, url: &Url, max_bytes: usize) -> Result<Vec<u8>, FetchError>;

    /// GET only `range` of `url`, failing if it is longer than `max_bytes`. The
    /// default downloads the whole body and cuts the range out of it.
    fn get_range(
        &self,
        url: &Url,
        range: ByteRange,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FetchError> {
        let offset = usize::try_from(range.offset).unwrap_or(usize::MAX);
        let body = self.get(url, offset.saturating_add(max_bytes))?;
        cut_range(&body, range, max_bytes)
    }
}

/// `range` of `body`, if it is all there and no longer than `max_bytes`.
fn cut_range(body: &[u8], range: ByteRange, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
    let outside = || FetchError("byte range outside the response".to_string());
    let length = usize::try_from(range.length).map_err(|_| outside())?;
    if length > max_bytes {
        return Err(FetchError(format!(
            "byte range larger than {max_bytes} bytes"
        )));
    }
    let start = usize::try_from(range.offset).map_err(|_| outside())?;
    let end = start.checked_add(length).ok_or_else(outside)?;
    body.get(start..end).map(<[u8]>::to_vec).ok_or_else(outside)
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

    fn get_range(
        &self,
        url: &Url,
        range: ByteRange,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FetchError> {
        let last = range.offset.saturating_add(range.length).saturating_sub(1);
        let response = self
            .get(url.clone())
            .header(
                reqwest::header::RANGE,
                format!("bytes={}-{last}", range.offset),
            )
            .send()
            .map_err(|error| FetchError(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError(format!("HTTP {}", status.as_u16())));
        }
        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            return read_capped(response, max_bytes);
        }
        // The server ignored the Range header and sent the whole resource.
        let offset = usize::try_from(range.offset).unwrap_or(usize::MAX);
        let body = read_capped(response, offset.saturating_add(max_bytes))?;
        cut_range(&body, range, max_bytes)
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

/// Why a segment could not be turned into audio input.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SegmentError {
    Download(String),
    Key(String),
    Decrypt(HlsError),
    /// The segment's init section can never be used (unsupported codec,
    /// malformed). Retrying the next segment would not help.
    Unusable(HlsError),
}

impl fmt::Display for SegmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Download(message) => write!(f, "HLS: segment download failed: {message}"),
            Self::Key(message) => write!(f, "HLS: encryption key unavailable: {message}"),
            Self::Decrypt(error) | Self::Unusable(error) => write!(f, "{error}"),
        }
    }
}

/// A downloaded segment, ready for the extractor.
struct Fetched {
    /// Decrypted already for AES-128; still encrypted for SAMPLE-AES.
    bytes: Vec<u8>,
    /// The key to decrypt the audio samples with, for SAMPLE-AES.
    sample_aes: Option<SampleAesKey>,
}

type KeyCache = HashMap<Url, [u8; crypto::KEY_LEN]>;
type InitCache = HashMap<(Url, Option<ByteRange>), Arc<Fmp4Init>>;

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
        let mut last_failure: Option<SegmentError> = None;
        let mut delivered = false;
        let mut keys = KeyCache::new();
        let mut inits = InitCache::new();

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

                let fetched = self.fetch_segment(segment, &mut keys).and_then(|fetched| {
                    let init = self.init_for(segment, &mut keys, &mut inits)?;
                    Ok((fetched, init))
                });
                match fetched {
                    Ok((fetched, init)) => {
                        segment_failures = 0;
                        let mut audio = Vec::new();
                        extractor.set_sample_aes(fetched.sample_aes);
                        if let Err(error) = extractor.push_segment_with_init(
                            &fetched.bytes,
                            init.as_deref(),
                            &mut audio,
                        ) {
                            self.fail(error.to_string());
                            return;
                        }
                        // A title applies to this segment's audio, so it goes first.
                        for title in extractor.take_titles() {
                            if !self.send(HlsChunk::Title(title)) {
                                return;
                            }
                        }
                        if !audio.is_empty() {
                            delivered = true;
                            if !self.send(HlsChunk::Bytes(audio)) {
                                return;
                            }
                        }
                    }
                    Err(SegmentError::Unusable(error)) => {
                        self.fail(error.to_string());
                        return;
                    }
                    Err(error) => {
                        segment_failures += 1;
                        if segment_failures > self.cfg.max_segment_failures {
                            self.fail(error.to_string());
                            return;
                        }
                        last_failure = Some(error);
                    }
                }
                last_seq = Some(segment.seq);
            }

            if playlist.end_list {
                // A short stream whose every segment failed would otherwise end
                // silently; say why nothing played.
                match (&last_failure, delivered) {
                    (Some(error), false) => self.fail(error.to_string()),
                    _ => {
                        let _ = self.send(HlsChunk::End);
                    }
                }
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

    /// GET a resource, or only `range` of it.
    fn get_section(
        &self,
        uri: &Url,
        range: Option<ByteRange>,
        max_bytes: usize,
    ) -> Result<Vec<u8>, SegmentError> {
        match range {
            Some(range) => self.http.get_range(uri, range, max_bytes),
            None => self.http.get(uri, max_bytes),
        }
        .map_err(|FetchError(message)| SegmentError::Download(message))
    }

    /// The AES-128 key for `key_info`, from the cache or downloaded.
    fn key_for(
        &self,
        key_info: &SegmentKey,
        keys: &mut KeyCache,
    ) -> Result<[u8; crypto::KEY_LEN], SegmentError> {
        if let Some(key) = keys.get(&key_info.uri) {
            return Ok(*key);
        }
        let body = self
            .http
            .get(&key_info.uri, MAX_KEY_BYTES)
            .map_err(|FetchError(message)| SegmentError::Key(message))?;
        let key: [u8; crypto::KEY_LEN] = body.as_slice().try_into().map_err(|_| {
            SegmentError::Key(format!(
                "the key is {} bytes, expected {}",
                body.len(),
                crypto::KEY_LEN
            ))
        })?;
        if keys.len() >= MAX_CACHED_KEYS {
            keys.clear();
        }
        keys.insert(key_info.uri.clone(), key);
        Ok(key)
    }

    /// Download a segment and, if it is AES-128 encrypted, decrypt it. A
    /// SAMPLE-AES segment is left as it is: only its audio samples are
    /// encrypted, which the extractor undoes.
    fn fetch_segment(
        &self,
        segment: &Segment,
        keys: &mut KeyCache,
    ) -> Result<Fetched, SegmentError> {
        let bytes = self.get_section(&segment.uri, segment.range, MAX_SEGMENT_BYTES)?;
        let Some(key_info) = &segment.key else {
            return Ok(Fetched {
                bytes,
                sample_aes: None,
            });
        };
        let key = self.key_for(key_info, keys)?;
        let iv = crypto::segment_iv(key_info.iv, segment.seq);
        match key_info.method {
            KeyMethod::Aes128 => Ok(Fetched {
                bytes: crypto::decrypt_segment(&bytes, &key, &iv).map_err(SegmentError::Decrypt)?,
                sample_aes: None,
            }),
            KeyMethod::SampleAes => Ok(Fetched {
                bytes,
                sample_aes: Some(SampleAesKey { key, iv }),
            }),
        }
    }

    /// The parsed init section of a fragmented MP4 segment, downloaded once.
    fn init_for(
        &self,
        segment: &Segment,
        keys: &mut KeyCache,
        inits: &mut InitCache,
    ) -> Result<Option<Arc<Fmp4Init>>, SegmentError> {
        let Some(map) = &segment.map else {
            return Ok(None);
        };
        let id = (map.uri.clone(), map.range);
        if let Some(init) = inits.get(&id) {
            return Ok(Some(Arc::clone(init)));
        }

        let mut bytes = self.get_section(&map.uri, map.range, MAX_INIT_BYTES)?;
        // Only AES-128 encrypts the init section; with SAMPLE-AES it is in the clear.
        if let Some(key_info) = map.key.iter().find(|key| key.method == KeyMethod::Aes128) {
            let key = self.key_for(key_info, keys)?;
            let iv = crypto::segment_iv(key_info.iv, segment.seq);
            bytes = crypto::decrypt_segment(&bytes, &key, &iv).map_err(SegmentError::Decrypt)?;
        }
        let init = Arc::new(Fmp4Init::parse(&bytes).map_err(SegmentError::Unusable)?);
        if inits.len() >= MAX_CACHED_INITS {
            inits.clear();
        }
        inits.insert(id, Arc::clone(&init));
        Ok(Some(init))
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
                        Err(error @ (HlsError::Drm | HlsError::UnsupportedEncryption(_))) => {
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

    const FMP4_INIT: &[u8] = include_bytes!("testdata/fmp4/aac/init.mp4");
    const FMP4_SEGMENTS: [&[u8]; 3] = [
        include_bytes!("testdata/fmp4/aac/seg0.m4s"),
        include_bytes!("testdata/fmp4/aac/seg1.m4s"),
        include_bytes!("testdata/fmp4/aac/seg2.m4s"),
    ];

    /// What the fMP4 segments turn into, worked out without the fetcher.
    fn expected_fmp4_audio() -> Vec<u8> {
        let init = Fmp4Init::parse(FMP4_INIT).unwrap();
        let mut demuxer = super::super::fmp4::Fmp4Demuxer::default();
        let mut audio = Vec::new();
        for segment in FMP4_SEGMENTS {
            demuxer.push(&init, segment, None, &mut audio).unwrap();
        }
        audio
    }

    #[test]
    fn fmp4_init_is_fetched_once_for_every_segment() {
        let http = FakeHttp::default();
        http.fixed("/init.mp4", FMP4_INIT.to_vec());
        for (i, segment) in FMP4_SEGMENTS.iter().enumerate() {
            http.fixed(&format!("/seg{i}.m4s"), segment.to_vec());
        }
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MAP:URI=\"/init.mp4\"\n\
                        #EXTINF:1,\n/seg0.m4s\n#EXTINF:1,\n/seg1.m4s\n#EXTINF:1,\n/seg2.m4s\n\
                        #EXT-X-ENDLIST\n";
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, playlist, 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(audio, expected_fmp4_audio());
        assert_eq!(http.hits("/init.mp4"), 1);
    }

    #[test]
    fn segments_cut_out_of_one_file_by_byte_range_are_played() {
        // The common CMAF layout: init section and fragments in a single file.
        let mut file = FMP4_INIT.to_vec();
        let mut ranges = Vec::new();
        for segment in FMP4_SEGMENTS {
            ranges.push((file.len(), segment.len()));
            file.extend_from_slice(segment);
        }
        let http = FakeHttp::default();
        http.fixed("/main.mp4", file);
        let mut playlist = format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MAP:URI=\"/main.mp4\",BYTERANGE=\"{}@0\"\n",
            FMP4_INIT.len()
        );
        for (offset, length) in &ranges {
            playlist.push_str(&format!(
                "#EXT-X-BYTERANGE:{length}@{offset}\n#EXTINF:1,\n/main.mp4\n"
            ));
        }
        playlist.push_str("#EXT-X-ENDLIST\n");
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist, 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(audio, expected_fmp4_audio());
    }

    #[test]
    fn a_range_outside_the_file_is_a_failed_segment() {
        let http = FakeHttp::default();
        http.fixed("/main.mp4", vec![0; 100]);
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-BYTERANGE:50@90\n\
                        #EXTINF:1,\n/main.mp4\n#EXT-X-ENDLIST\n";
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, playlist, 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert!(audio.is_empty());
        match terminal {
            Some(HlsChunk::Fail(message)) => {
                assert!(
                    message.contains("byte range outside the response"),
                    "{message}"
                )
            }
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn an_init_section_that_cannot_be_read_ends_the_stream() {
        let http = FakeHttp::default();
        http.fixed("/init.mp4", b"not an mp4 at all".to_vec());
        http.fixed("/seg0.m4s", FMP4_SEGMENTS[0].to_vec());
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MAP:URI=\"/init.mp4\"\n\
                        #EXTINF:1,\n/seg0.m4s\n#EXT-X-ENDLIST\n";
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, playlist, 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert!(audio.is_empty());
        assert!(matches!(terminal, Some(HlsChunk::Fail(m)) if m.starts_with("HLS:")));
    }

    #[test]
    fn a_missing_init_section_is_a_failed_segment_download() {
        let http = FakeHttp::default();
        http.fixed("/seg0.m4s", FMP4_SEGMENTS[0].to_vec());
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MAP:URI=\"/init.mp4\"\n\
                        #EXTINF:1,\n/seg0.m4s\n#EXT-X-ENDLIST\n";
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, playlist, 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert!(audio.is_empty());
        assert!(matches!(terminal, Some(HlsChunk::Fail(m)) if m.contains("HTTP 404")));
    }

    #[test]
    fn the_real_http_client_fetches_byte_ranges() {
        use crate::audio::test_http::{fixed, Response, TestServer};
        // 200 bytes: within offset (100) + the size limit (100) used below.
        let body: Vec<u8> = (0..200).collect();
        let (honoring, ignoring) = (body.clone(), body.clone());
        let server = TestServer::start(vec![
            (
                "/ranges",
                fixed(move || Response::ok("video/mp4", honoring.clone())),
            ),
            (
                "/no-ranges",
                fixed(move || Response::ok("video/mp4", ignoring.clone()).ignoring_ranges()),
            ),
            ("/gone", fixed(|| Response::status(404))),
        ]);
        let client = reqwest::blocking::Client::new();
        let range = ByteRange {
            offset: 100,
            length: 20,
        };
        let fetch = |path: &str, max: usize| {
            let url = Url::parse(&server.url(path)).unwrap();
            HlsHttp::get_range(&client, &url, range, max)
        };

        // A server that supports ranges sends just the slice.
        assert_eq!(fetch("/ranges", 100).unwrap(), body[100..120].to_vec());
        // One that does not sends everything; the range is cut out of it.
        assert_eq!(fetch("/no-ranges", 100).unwrap(), body[100..120].to_vec());
        // The size limit applies to the range, not the file...
        assert!(fetch("/ranges", 10).is_err());
        // ...but a server that ignores ranges cannot be trusted with a bigger file.
        assert_eq!(
            fetch("/no-ranges", 50),
            Err(FetchError("response larger than 150 bytes".to_string()))
        );
        assert_eq!(fetch("/gone", 100), Err(FetchError("HTTP 404".to_string())));
    }

    #[test]
    fn cut_range_takes_only_what_is_asked_for() {
        let body: Vec<u8> = (0..20).collect();
        let range = |offset, length| ByteRange { offset, length };

        assert_eq!(cut_range(&body, range(5, 3), 100).unwrap(), vec![5, 6, 7]);
        assert_eq!(cut_range(&body, range(0, 20), 100).unwrap(), body);
        assert!(cut_range(&body, range(18, 5), 100).is_err());
        assert!(cut_range(&body, range(30, 1), 100).is_err());
        assert!(cut_range(&body, range(0, 20), 10).is_err());
        assert!(cut_range(&body, range(u64::MAX - 1, 5), 100).is_err());
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
    fn a_refresh_that_turns_drm_protected_fails_clearly() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", segment(20));
        http.fixed(
            "/live.m3u8",
            b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://k\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n#EXTINF:1,\n/seg1.ts\n"
                .to_vec(),
        );
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 1, false), 1, &active, fast());
        let (_, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::Fail(HlsError::Drm.to_string())));
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

    fn drain_all(rx: &Receiver<HlsChunk>) -> Vec<HlsChunk> {
        let mut chunks = Vec::new();
        while let Ok(chunk) = rx.recv_timeout(Duration::from_secs(10)) {
            let done = matches!(chunk, HlsChunk::End | HlsChunk::Fail(_));
            chunks.push(chunk);
            if done {
                break;
            }
        }
        chunks
    }

    #[test]
    fn titles_precede_their_audio_and_are_sent_only_when_they_change() {
        let http = FakeHttp::default();
        let titled = |title: &str, marker: usize| {
            ts_segment_with_metadata(
                &[marker_frame(marker)],
                &[id3_text_tag(title, Some("Band"))],
            )
        };
        http.fixed("/seg0.ts", titled("Alpha", 20));
        http.fixed("/seg1.ts", titled("Alpha", 21));
        http.fixed("/seg2.ts", titled("Beta", 22));
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 3, true), 1, &active, fast());
        let chunks = drain_all(&rx);
        join(handle);

        assert_eq!(
            chunks,
            vec![
                HlsChunk::Title("Band - Alpha".to_string()),
                HlsChunk::Bytes(marker_frame(20)),
                HlsChunk::Bytes(marker_frame(21)),
                HlsChunk::Title("Band - Beta".to_string()),
                HlsChunk::Bytes(marker_frame(22)),
                HlsChunk::End,
            ]
        );
    }

    #[test]
    fn streams_without_metadata_send_no_title_chunks() {
        let http = FakeHttp::default();
        for i in 0..2 {
            http.fixed(&format!("/seg{i}.ts"), segment(20 + i));
        }
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 2, true), 1, &active, fast());
        let chunks = drain_all(&rx);
        join(handle);

        assert!(chunks
            .iter()
            .all(|chunk| !matches!(chunk, HlsChunk::Title(_))));
    }

    const KEY: [u8; 16] = [
        0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE,
        0xAF,
    ];

    fn encrypt(data: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Vec<u8> {
        use aes::cipher::{block_padding::Pkcs7, BlockModeEncrypt, KeyIvInit};
        cbc::Encryptor::<aes::Aes128>::new(key.into(), iv.into()).encrypt_padded_vec::<Pkcs7>(data)
    }

    /// A VOD playlist with an AES-128 key tag, the given first sequence number
    /// and `count` segments.
    fn encrypted_playlist(key_attrs: &str, first_seq: u64, count: usize) -> String {
        let mut text = format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:{first_seq}\n#EXT-X-KEY:METHOD=AES-128,{key_attrs}\n"
        );
        for seq in first_seq..first_seq + count as u64 {
            text.push_str(&format!("#EXTINF:1,\n/seg{seq}.ts\n"));
        }
        text.push_str("#EXT-X-ENDLIST\n");
        text
    }

    #[test]
    fn encrypted_segments_are_decrypted_and_the_key_is_fetched_once() {
        let http = FakeHttp::default();
        let iv = [7u8; 16];
        http.fixed("/key", KEY.to_vec());
        for seq in 0..3u64 {
            http.fixed(
                &format!("/seg{seq}.ts"),
                encrypt(&segment(20 + seq as usize), &KEY, &iv),
            );
        }
        let playlist = encrypted_playlist(&format!("URI=\"/key\",IV=0x{}", "07".repeat(16)), 0, 3);
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist, 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(
            audio,
            (0..3)
                .flat_map(|i| marker_frame(20 + i))
                .collect::<Vec<_>>()
        );
        assert_eq!(http.hits("/key"), 1, "the key is cached");
    }

    #[test]
    fn without_an_iv_attribute_the_sequence_number_is_the_iv() {
        let http = FakeHttp::default();
        http.fixed("/key", KEY.to_vec());
        for seq in 40..43u64 {
            let mut iv = [0u8; 16];
            iv[8..].copy_from_slice(&seq.to_be_bytes());
            http.fixed(
                &format!("/seg{seq}.ts"),
                encrypt(&segment(20 + (seq - 40) as usize), &KEY, &iv),
            );
        }
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(
            &http,
            &encrypted_playlist("URI=\"/key\"", 40, 3),
            1,
            &active,
            fast(),
        );
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
    fn rotated_keys_are_each_fetched_for_their_segments() {
        let http = FakeHttp::default();
        let key_b = [0xB0u8; 16];
        let iv = [1u8; 16];
        http.fixed("/key-a", KEY.to_vec());
        http.fixed("/key-b", key_b.to_vec());
        http.fixed("/seg0.ts", encrypt(&segment(20), &KEY, &iv));
        http.fixed("/seg1.ts", encrypt(&segment(21), &key_b, &iv));
        let iv_attr = format!("IV=0x{}", "01".repeat(16));
        let playlist = format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-KEY:METHOD=AES-128,URI=\"/key-a\",{iv_attr}\n#EXTINF:1,\n/seg0.ts\n#EXT-X-KEY:METHOD=AES-128,URI=\"/key-b\",{iv_attr}\n#EXTINF:1,\n/seg1.ts\n#EXT-X-ENDLIST\n"
        );
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist, 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(audio, [marker_frame(20), marker_frame(21)].concat());
        assert_eq!((http.hits("/key-a"), http.hits("/key-b")), (1, 1));
    }

    #[test]
    fn unencrypted_segments_after_a_key_tag_cleared_by_none_are_not_decrypted() {
        let http = FakeHttp::default();
        http.fixed("/seg0.ts", segment(20));
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:1,\n/seg0.ts\n#EXT-X-ENDLIST\n";
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, playlist, 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(audio, marker_frame(20));
        assert_eq!(http.hits("/key"), 0);
    }

    fn failure_message(terminal: Option<HlsChunk>) -> String {
        match terminal {
            Some(HlsChunk::Fail(message)) => message,
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn an_unavailable_key_fails_the_stream_with_a_key_message() {
        let http = FakeHttp::default();
        for seq in 0..6u64 {
            http.fixed(
                &format!("/seg{seq}.ts"),
                encrypt(&segment(20), &KEY, &[0; 16]),
            );
        }
        // /key has no route: 404.
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(
            &http,
            &encrypted_playlist("URI=\"/key\"", 0, 6),
            1,
            &active,
            fast(),
        );
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert!(audio.is_empty());
        let message = failure_message(terminal);
        assert!(
            message.starts_with("HLS: encryption key unavailable"),
            "{message}"
        );
        assert!(!message.contains("not supported"), "must stay retryable");
    }

    #[test]
    fn a_key_of_the_wrong_size_is_rejected() {
        let http = FakeHttp::default();
        http.fixed("/key", vec![1u8; 15]);
        for seq in 0..6u64 {
            http.fixed(
                &format!("/seg{seq}.ts"),
                encrypt(&segment(20), &KEY, &[0; 16]),
            );
        }
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(
            &http,
            &encrypted_playlist("URI=\"/key\"", 0, 6),
            1,
            &active,
            fast(),
        );
        let (_, terminal) = drain(&rx);
        join(handle);

        let message = failure_message(terminal);
        assert!(
            message.contains("the key is 15 bytes, expected 16"),
            "{message}"
        );
    }

    #[test]
    fn the_wrong_key_fails_with_a_decrypt_message_after_the_failure_limit() {
        let http = FakeHttp::default();
        http.fixed("/key", [0x55u8; 16].to_vec());
        for seq in 0..6u64 {
            http.fixed(
                &format!("/seg{seq}.ts"),
                encrypt(&segment(20), &KEY, &[0; 16]),
            );
        }
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(
            &http,
            &encrypted_playlist("URI=\"/key\"", 0, 6),
            1,
            &active,
            fast(),
        );
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert!(audio.is_empty());
        let message = failure_message(terminal);
        assert!(
            message.starts_with("HLS: could not decrypt a segment"),
            "{message}"
        );
    }

    #[test]
    fn a_segment_that_is_not_block_aligned_is_a_decrypt_failure() {
        let http = FakeHttp::default();
        http.fixed("/key", KEY.to_vec());
        http.fixed("/seg0.ts", vec![0u8; 100]);
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(
            &http,
            &encrypted_playlist("URI=\"/key\"", 0, 1),
            1,
            &active,
            fast(),
        );
        let (_, terminal) = drain(&rx);
        join(handle);

        // Within the skip limit, but nothing ever played, so the reason is reported.
        assert!(failure_message(terminal).contains("multiple of the AES block size"));
    }

    #[test]
    fn a_stream_that_recovers_after_skipped_segments_still_ends_cleanly() {
        let http = FakeHttp::default();
        http.fixed("/seg1.ts", segment(21));
        // seg0 is a 404 and is skipped; seg1 plays.
        let active = Arc::new(AtomicU64::new(1));

        let (rx, handle) = spawn(&http, &playlist_text(0, 2, true), 1, &active, fast());
        let (audio, terminal) = drain(&rx);
        join(handle);

        assert_eq!(terminal, Some(HlsChunk::End));
        assert_eq!(audio, marker_frame(21));
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
