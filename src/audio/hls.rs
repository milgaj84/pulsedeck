//! HLS (HTTP Live Streaming) support for audio-only radio streams.
//!
//! Flow: the connection worker (`decode::run_worker`) recognises a playlist,
//! hands the body to [`run`], which resolves it to a media playlist, starts a
//! fetcher thread, prebuffers the audio it produces and builds the same
//! decode pipeline used for plain streams. Segments are demuxed into one
//! continuous AAC/MP3 byte stream, so the decoder does not know it is HLS.

mod fetcher;
mod playlist;
mod source;
#[cfg(test)]
mod test_fixtures;
mod ts;

use super::codec::detect_codec;
use super::decode::{
    fill_prebuffer, send_abandoned, send_failure, DecodePipeline, PrebufferFailure,
};
use super::types::{ConnectRequest, EngineError, EngineEvent, PrebufferConfig};
use fetcher::{run_fetcher, FetcherConfig, HlsHttp, MAX_PLAYLIST_BYTES};
use playlist::{HlsError, MediaPlaylist, Playlist};
use reqwest::Url;
use source::HlsSource;
use std::collections::VecDeque;
use std::io::{BufReader, Cursor, Read};
use std::sync::atomic::AtomicU64;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

pub(super) use fetcher::read_capped;
pub(super) const MAX_PLAYLIST_BODY: usize = MAX_PLAYLIST_BYTES;

/// HLS needs the playlist, a segment and often a second segment before the
/// first audio is ready, so it gets a longer prebuffer window than a plain stream.
const HLS_FILL_TIMEOUT: Duration = Duration::from_secs(20);
const MIN_IDLE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Segments waiting for the decoder (the fetcher blocks when this is full).
const CHANNEL_SEGMENTS: usize = 3;

/// Content types servers use for HLS playlists.
pub(super) fn looks_like_hls(content_type: &str, url: &str) -> bool {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if matches!(
        media_type.as_str(),
        "application/vnd.apple.mpegurl" | "application/x-mpegurl" | "audio/mpegurl"
    ) && !url_path(url).ends_with(".m3u")
    {
        return true;
    }
    url_path(url).ends_with(".m3u8")
}

fn url_path(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// A body that is an HLS playlist even though the server's headers did not say
/// so (misconfigured servers, M3U imports). A plain M3U has no `#EXT-X-` tags
/// and keeps today's behaviour.
pub(super) fn looks_like_playlist(prefix: &[u8]) -> bool {
    let text = String::from_utf8_lossy(&prefix[..prefix.len().min(MAX_PLAYLIST_BYTES)]);
    let text = text.trim_start_matches('\u{feff}').trim_start();
    text.starts_with("#EXTM3U") && text.contains("#EXT-X-")
}

/// Everything `run` needs from the connection worker.
pub(super) struct HlsContext<'a> {
    pub client: reqwest::blocking::Client,
    pub request: &'a ConnectRequest,
    pub event_tx: &'a mpsc::Sender<EngineEvent>,
    pub active_generation: &'a Arc<AtomicU64>,
    pub sample_buffer: Arc<Mutex<VecDeque<f32>>>,
}

fn decode_error(error: HlsError) -> EngineError {
    EngineError::Decode(error.to_string())
}

/// Resolve a fetched playlist (possibly a master) to the media playlist to play.
fn resolve_media_playlist(
    http: &impl HlsHttp,
    playlist_url: Url,
    body: &[u8],
) -> Result<(Url, MediaPlaylist), EngineError> {
    match playlist::parse(&String::from_utf8_lossy(body), &playlist_url).map_err(decode_error)? {
        Playlist::Media(media) => Ok((playlist_url, media)),
        Playlist::Master(master) => {
            let variant = playlist::select_variant(&master).map_err(decode_error)?;
            let body = http
                .get(&variant, MAX_PLAYLIST_BYTES)
                .map_err(|error| EngineError::Connect(format!("HLS: {}", error.0)))?;
            match playlist::parse(&String::from_utf8_lossy(&body), &variant)
                .map_err(decode_error)?
            {
                Playlist::Media(media) => Ok((variant, media)),
                Playlist::Master(_) => Err(EngineError::Decode(
                    "HLS: nested master playlists are not supported".to_string(),
                )),
            }
        }
    }
}

/// How long the decoder may wait for the next segment before giving up.
fn idle_timeout(media: &MediaPlaylist) -> Duration {
    (media.target_duration * 3).clamp(MIN_IDLE_TIMEOUT, MAX_IDLE_TIMEOUT)
}

/// Play the HLS stream whose playlist (`body`, fetched from `playlist_url`)
/// the worker already downloaded. Reports its outcome as engine events.
pub(super) fn run(ctx: HlsContext<'_>, playlist_url: &str, body: Vec<u8>) {
    let HlsContext {
        client,
        request,
        event_tx,
        active_generation,
        sample_buffer,
    } = ctx;
    let generation = request.generation;

    let playlist_url = match Url::parse(playlist_url) {
        Ok(url) => url,
        Err(error) => {
            send_failure(
                event_tx,
                generation,
                EngineError::Connect(format!("HLS: invalid playlist URL: {error}")),
            );
            return;
        }
    };

    let (media_url, media) = match resolve_media_playlist(&client, playlist_url, &body) {
        Ok(resolved) => resolved,
        Err(error) => {
            send_failure(event_tx, generation, error);
            return;
        }
    };

    let (tx, rx) = mpsc::sync_channel(CHANNEL_SEGMENTS);
    let mut source = HlsSource::new(
        rx,
        generation,
        Arc::clone(active_generation),
        idle_timeout(&media),
    );

    let spawned = thread::Builder::new()
        .name(format!("pulsedeck-hls-{generation}"))
        .spawn({
            let active = Arc::clone(active_generation);
            move || {
                run_fetcher(
                    client,
                    media_url,
                    media,
                    generation,
                    active,
                    tx,
                    FetcherConfig::default(),
                )
            }
        });
    if let Err(error) = spawned {
        send_failure(
            event_tx,
            generation,
            EngineError::Connect(format!("HLS: could not start fetcher thread: {error}")),
        );
        return;
    }

    let hls_request = ConnectRequest::new(
        generation,
        request.url.clone(),
        PrebufferConfig {
            fill_timeout: request.prebuffer.fill_timeout.max(HLS_FILL_TIMEOUT),
            ..request.prebuffer.clone()
        },
        request.options.clone(),
    );

    let prebuffer = match fill_prebuffer(&mut source, &hls_request, event_tx, active_generation) {
        Ok(prebuffer) => prebuffer,
        Err(PrebufferFailure::Abandoned) => {
            send_abandoned(event_tx, generation);
            return;
        }
        Err(PrebufferFailure::Timeout) => {
            send_failure(
                event_tx,
                generation,
                EngineError::Connect(format!(
                    "HLS: no audio received after {} seconds",
                    hls_request.prebuffer.fill_timeout.as_secs()
                )),
            );
            return;
        }
        Err(PrebufferFailure::Read(error)) => {
            let message = error.to_string();
            let error = if message.contains("not supported") {
                EngineError::Decode(message)
            } else {
                EngineError::Connect(format!("stream read failed while buffering: {message}"))
            };
            send_failure(event_tx, generation, error);
            return;
        }
    };

    if active_generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
        send_abandoned(event_tx, generation);
        return;
    }

    // The demuxed stream starts with an ADTS or MPEG audio frame, so magic
    // bytes identify it (and select the MP3 fast path when it is MP3).
    let detection = detect_codec(&prebuffer, "", "");
    let chained = Cursor::new(prebuffer).chain(BufReader::with_capacity(64 * 1024, source));

    match DecodePipeline::build(chained, sample_buffer, detection) {
        Ok((decoded, mut format)) => {
            format.codec = format!("HLS {}", format.codec);
            let _ = event_tx.send(EngineEvent::Connected {
                generation,
                source: decoded,
                format,
            });
        }
        Err(error) => send_failure(event_tx, generation, error),
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixtures::*;
    use super::*;
    use crate::audio::test_http::{fixed, Response, TestServer};
    use crate::audio::types::{PlaybackOptions, PrebufferConfig};
    use std::sync::atomic::Ordering::SeqCst;

    const TONE_AAC: &[u8] = include_bytes!("hls/testdata/tone.aac");
    const TONE_MP3: &[u8] = include_bytes!("hls/testdata/tone.mp3");
    const MPEGURL: &str = "application/vnd.apple.mpegurl";

    #[test]
    fn looks_like_hls_checks_content_type_and_extension() {
        assert!(looks_like_hls(MPEGURL, "http://x/stream"));
        assert!(looks_like_hls(
            "application/x-mpegURL; charset=utf-8",
            "http://x/stream"
        ));
        assert!(looks_like_hls("audio/mpegurl", "http://x/stream"));
        assert!(looks_like_hls("text/plain", "http://x/live.m3u8"));
        assert!(looks_like_hls("", "http://x/LIVE.M3U8?token=1#frag"));
    }

    #[test]
    fn looks_like_hls_leaves_plain_m3u_and_audio_alone() {
        assert!(!looks_like_hls("audio/mpeg", "http://x/stream"));
        assert!(!looks_like_hls("audio/x-mpegurl", "http://x/stream"));
        assert!(!looks_like_hls("audio/mpegurl", "http://x/radio.m3u"));
        assert!(!looks_like_hls("", "http://x/stream.mp3"));
        assert!(!looks_like_hls("", "http://x/m3u8/stream"));
    }

    #[test]
    fn looks_like_playlist_needs_hls_tags() {
        assert!(looks_like_playlist(b"#EXTM3U\n#EXT-X-TARGETDURATION:6\n"));
        assert!(looks_like_playlist(
            "\u{feff}  #EXTM3U\r\n#EXT-X-VERSION:3".as_bytes()
        ));
        assert!(!looks_like_playlist(
            b"#EXTM3U\n#EXTINF:-1,Radio\nhttp://x/stream\n"
        ));
        assert!(!looks_like_playlist(b"ID3\x04\x00"));
        assert!(!looks_like_playlist(b""));
    }

    fn media_playlist(count: usize, ext: &str, extra: &str) -> String {
        let mut text =
            format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n{extra}");
        for i in 0..count {
            text.push_str(&format!("#EXTINF:1,\nseg{i}.{ext}\n"));
        }
        text.push_str("#EXT-X-ENDLIST\n");
        text
    }

    fn packed_aac_segment() -> Vec<u8> {
        let mut segment = id3_tag(24);
        segment.extend_from_slice(TONE_AAC);
        segment
    }

    /// Splits the tone into whole ADTS frames so each TS PES holds complete frames.
    fn tone_frames() -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        let mut pos = 0;
        while pos < TONE_AAC.len() {
            let len = (usize::from(TONE_AAC[pos + 3] & 3) << 11)
                | (usize::from(TONE_AAC[pos + 4]) << 3)
                | usize::from(TONE_AAC[pos + 5] >> 5);
            frames.push(TONE_AAC[pos..pos + len].to_vec());
            pos += len;
        }
        frames
    }

    struct Run {
        events: Vec<EngineEvent>,
        _server: TestServer,
    }

    /// Run the HLS worker against a local server and collect its events.
    fn run_against(server: TestServer, entry_path: &str, min_bytes: usize) -> Run {
        let (event_tx, event_rx) = mpsc::channel();
        let active = Arc::new(AtomicU64::new(1));
        let url = server.url(entry_path);
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let body = client.get(&url).send().unwrap().bytes().unwrap().to_vec();
        let request = ConnectRequest::new(
            1,
            url.clone(),
            PrebufferConfig {
                min_bytes,
                max_bytes: 512 * 1024,
                fill_timeout: Duration::from_secs(8),
            },
            PlaybackOptions {
                metadata_enabled: false,
                ..PlaybackOptions::default()
            },
        );

        let ctx = HlsContext {
            client,
            request: &request,
            event_tx: &event_tx,
            active_generation: &active,
            sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
        };
        run(ctx, &url, body);
        // The worker returns after its terminal event; stop the fetcher thread.
        active.store(0, SeqCst);
        drop(event_tx);
        Run {
            events: event_rx.try_iter().collect(),
            _server: server,
        }
    }

    fn terminal(run: &Run) -> &EngineEvent {
        run.events
            .iter()
            .rfind(|event| !matches!(event, EngineEvent::Buffering { .. }))
            .expect("a terminal event")
    }

    fn assert_connected_as(run: &Run, codec: &str) {
        match terminal(run) {
            EngineEvent::Connected { format, .. } => {
                assert_eq!(format.codec, codec);
                assert_eq!(format.sample_rate, 44100);
            }
            other => panic!("expected Connected, got {}", describe(other)),
        }
    }

    fn describe(event: &EngineEvent) -> String {
        match event {
            EngineEvent::Failed { error, .. } => format!("Failed({})", error.to_status_string()),
            EngineEvent::StreamEnded { .. } => "StreamEnded".to_string(),
            EngineEvent::Connected { .. } => "Connected".to_string(),
            EngineEvent::Buffering { percent, .. } => format!("Buffering({percent})"),
            _ => "other event".to_string(),
        }
    }

    fn assert_failed_with(run: &Run, needle: &str) {
        match terminal(run) {
            EngineEvent::Failed { error, .. } => {
                let text = error.to_status_string();
                assert!(text.contains(needle), "{text:?} should contain {needle:?}");
            }
            other => panic!("expected Failed, got {}", describe(other)),
        }
    }

    fn server_with(routes: Vec<(&str, crate::audio::test_http::Response)>) -> TestServer {
        // Responses are built once and cloned per request.
        let routes = routes
            .into_iter()
            .map(|(path, response)| {
                let (status, content_type, body) =
                    (response.status, response.content_type, response.body);
                (
                    path,
                    fixed(move || Response {
                        status,
                        content_type: content_type.clone(),
                        body: body.clone(),
                    }),
                )
            })
            .collect();
        TestServer::start(routes)
    }

    #[test]
    fn packed_aac_stream_connects_and_decodes() {
        let server = server_with(vec![
            (
                "/live.m3u8",
                Response::ok(MPEGURL, media_playlist(3, "aac", "")),
            ),
            ("/seg0.aac", Response::ok("audio/aac", packed_aac_segment())),
            ("/seg1.aac", Response::ok("audio/aac", packed_aac_segment())),
            ("/seg2.aac", Response::ok("audio/aac", packed_aac_segment())),
        ]);

        let run = run_against(server, "/live.m3u8", 4096);

        assert_connected_as(&run, "HLS AAC");
    }

    #[test]
    fn mpeg_ts_stream_connects_and_decodes() {
        let segment = ts_segment(0x0F, &tone_frames());
        let server = server_with(vec![
            (
                "/live.m3u8",
                Response::ok(MPEGURL, media_playlist(3, "ts", "")),
            ),
            ("/seg0.ts", Response::ok("video/mp2t", segment.clone())),
            ("/seg1.ts", Response::ok("video/mp2t", segment.clone())),
            ("/seg2.ts", Response::ok("video/mp2t", segment)),
        ]);

        let run = run_against(server, "/live.m3u8", 4096);

        assert_connected_as(&run, "HLS AAC");
    }

    #[test]
    fn mp3_segments_connect_through_the_mp3_path() {
        let mut segment = id3_tag(16);
        segment.extend_from_slice(TONE_MP3);
        let server = server_with(vec![
            (
                "/live.m3u8",
                Response::ok(MPEGURL, media_playlist(3, "mp3", "")),
            ),
            ("/seg0.mp3", Response::ok("audio/mpeg", segment.clone())),
            ("/seg1.mp3", Response::ok("audio/mpeg", segment.clone())),
            ("/seg2.mp3", Response::ok("audio/mpeg", segment)),
        ]);

        let run = run_against(server, "/live.m3u8", 4096);

        assert_connected_as(&run, "HLS MP3");
    }

    #[test]
    fn master_playlist_is_resolved_to_the_audio_variant() {
        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.4d401f,mp4a.40.2\"\nvideo.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"mp4a.40.2\"\naudio/index.m3u8\n";
        let segments: Vec<_> = (0..3)
            .map(|i| (format!("/audio/seg{i}.aac"), packed_aac_segment()))
            .collect();
        let mut routes = vec![
            ("/master.m3u8", Response::ok(MPEGURL, master)),
            (
                "/audio/index.m3u8",
                Response::ok(MPEGURL, media_playlist(3, "aac", "")),
            ),
        ];
        let paths: Vec<String> = segments.iter().map(|(p, _)| p.clone()).collect();
        let server_routes: Vec<(&str, Response)> = {
            for (path, body) in &segments {
                routes.push((path.as_str(), Response::ok("audio/aac", body.clone())));
            }
            routes
        };
        let server = server_with(server_routes);

        let run = run_against(server, "/master.m3u8", 4096);

        assert_connected_as(&run, "HLS AAC");
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn encrypted_playlist_fails_as_not_supported_before_downloading_anything() {
        let playlist = media_playlist(2, "ts", "#EXT-X-KEY:METHOD=AES-128,URI=\"k.key\"\n");
        let server = server_with(vec![("/live.m3u8", Response::ok(MPEGURL, playlist))]);

        let run = run_against(server, "/live.m3u8", 4096);

        assert_failed_with(&run, "encrypted streams are not supported");
        assert_eq!(run._server.hits("/seg0.ts"), 0);
    }

    #[test]
    fn fmp4_playlist_fails_as_not_supported() {
        let playlist = media_playlist(2, "m4s", "#EXT-X-MAP:URI=\"init.mp4\"\n");
        let server = server_with(vec![("/live.m3u8", Response::ok(MPEGURL, playlist))]);

        let run = run_against(server, "/live.m3u8", 4096);

        assert_failed_with(&run, "fMP4/CMAF segments are not supported");
    }

    #[test]
    fn video_only_master_fails_as_not_supported() {
        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.4d401f\"\nv.m3u8\n";
        let server = server_with(vec![("/master.m3u8", Response::ok(MPEGURL, master))]);

        let run = run_against(server, "/master.m3u8", 4096);

        assert_failed_with(&run, "video-only streams are not supported");
    }

    #[test]
    fn segment_with_unsupported_audio_fails_as_not_supported() {
        let mut pat_pmt = pat_packet(PMT_PID);
        pat_pmt.extend(pmt_packet(PMT_PID, &[(0x81, AUDIO_PID)]));
        let server = server_with(vec![
            (
                "/live.m3u8",
                Response::ok(MPEGURL, media_playlist(1, "ts", "")),
            ),
            ("/seg0.ts", Response::ok("video/mp2t", pat_pmt)),
        ]);

        let run = run_against(server, "/live.m3u8", 4096);

        assert_failed_with(&run, "AC-3 audio is not supported");
    }

    #[test]
    fn unreachable_segments_fail_as_a_network_error_not_unsupported() {
        let server = server_with(vec![(
            "/live.m3u8",
            Response::ok(MPEGURL, media_playlist(5, "aac", "")),
        )]);

        let run = run_against(server, "/live.m3u8", 4096);

        assert_failed_with(&run, "segment download failed");
        match terminal(&run) {
            EngineEvent::Failed { error, .. } => {
                assert!(!error.to_status_string().contains("not supported"));
                assert!(matches!(error, EngineError::Connect(_)));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn garbage_audio_fails_to_decode_instead_of_hanging() {
        let mut segment = id3_tag(8);
        segment.extend((0..3).flat_map(|_| adts_frame(300, false, false)));
        let server = server_with(vec![
            (
                "/live.m3u8",
                Response::ok(MPEGURL, media_playlist(2, "aac", "")),
            ),
            ("/seg0.aac", Response::ok("audio/aac", segment.clone())),
            ("/seg1.aac", Response::ok("audio/aac", segment)),
        ]);

        let run = run_against(server, "/live.m3u8", 1024);

        assert_failed_with(&run, "Decode error");
    }

    #[test]
    fn invalid_playlist_url_is_reported() {
        let (event_tx, event_rx) = mpsc::channel();
        let active = Arc::new(AtomicU64::new(1));
        let request = ConnectRequest::new(
            1,
            "not a url".to_string(),
            PrebufferConfig {
                min_bytes: 1,
                max_bytes: 2,
                fill_timeout: Duration::from_secs(1),
            },
            PlaybackOptions {
                metadata_enabled: false,
                ..PlaybackOptions::default()
            },
        );

        run(
            HlsContext {
                client: reqwest::blocking::Client::new(),
                request: &request,
                event_tx: &event_tx,
                active_generation: &active,
                sample_buffer: Arc::new(Mutex::new(VecDeque::new())),
            },
            "not a url",
            b"#EXTM3U".to_vec(),
        );

        match event_rx.try_recv().unwrap() {
            EngineEvent::Failed { error, .. } => {
                assert!(error.to_status_string().contains("invalid playlist URL"))
            }
            other => panic!("unexpected {}", describe(&other)),
        }
    }

    #[test]
    fn idle_timeout_scales_with_the_target_duration_within_bounds() {
        let media = |secs| MediaPlaylist {
            target_duration: Duration::from_secs(secs),
            end_list: false,
            segments: vec![],
        };

        assert_eq!(idle_timeout(&media(1)), MIN_IDLE_TIMEOUT);
        assert_eq!(idle_timeout(&media(6)), Duration::from_secs(18));
        assert_eq!(idle_timeout(&media(60)), MAX_IDLE_TIMEOUT);
    }
}
