/// Monotonically increasing counter allocated per `Play` command.
/// Used to discard stale worker results on rapid station switching.
pub(super) type Generation = u64;

// ---------------------------------------------------------------------------
// EngineState
// ---------------------------------------------------------------------------

/// The single-owner state machine for the audio engine control loop.
///
/// Exactly one variant is active between loop iterations.  All state
/// transitions go through `EngineLoop::transition`, which emits the
/// corresponding `AudioStatus` exactly once per user-visible change.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) enum EngineState {
    /// No stream loaded; engine is idle.
    Idle,
    /// HTTP connection is being established for the given generation.
    Connecting { generation: Generation, url: String },
    /// Stream is connected; prebuffer is being filled.
    Buffering {
        generation: Generation,
        url: String,
        percent: u8,
    },
    /// Audio is actively playing.
    Playing { generation: Generation, url: String },
    /// Audio is paused mid-stream.
    Paused { generation: Generation, url: String },
    /// Output device was lost; attempting hardware reopen.
    Recovering {
        generation: Generation,
        url: String,
        retries: u8,
    },
    /// Engine has entered an unrecoverable error state.
    Failed {
        url: Option<String>,
        error: EngineError,
    },
}

// ---------------------------------------------------------------------------
// EngineEvent
// ---------------------------------------------------------------------------

/// Internal messages sent from workers and `OutputManager` to `EngineLoop`.
#[allow(dead_code)]
pub(super) enum EngineEvent {
    /// Prebuffer progress update from the connection worker.
    Buffering { generation: Generation, percent: u8 },
    /// Worker successfully connected and produced a decoded source.
    Connected {
        generation: Generation,
        source: DecodedSource,
        format: StreamFormat,
    },
    /// ICY metadata yielded a new track title.
    TrackChanged {
        generation: Generation,
        title: String,
    },
    /// Stream has ended (naturally, due to network error, decode error, or
    /// because the worker's generation became stale).
    StreamEnded {
        generation: Generation,
        reason: EndReason,
    },
    /// The audio output device was lost (reported by `OutputManager`).
    OutputLost,
    /// The worker encountered a fatal error.
    Failed {
        generation: Generation,
        error: EngineError,
    },
}

impl EngineEvent {
    /// Returns the generation associated with this event, if any.
    /// `OutputLost` is not generation-scoped (it comes from the output layer).
    pub(super) fn generation(&self) -> Option<Generation> {
        match self {
            Self::Buffering { generation, .. }
            | Self::Connected { generation, .. }
            | Self::TrackChanged { generation, .. }
            | Self::StreamEnded { generation, .. }
            | Self::Failed { generation, .. } => Some(*generation),
            Self::OutputLost => None,
        }
    }
}

// ---------------------------------------------------------------------------
// EngineError
// ---------------------------------------------------------------------------

/// Classified error type replacing scattered `String` errors.
///
/// The stable `to_status_string()` prefixes are load-bearing: the app's
/// `classify_playback_error` function parses them to decide reconnect policy
/// and UI hints.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) enum EngineError {
    /// DNS / TCP / TLS / connect-timeout failure.
    Connect(String),
    /// Non-success HTTP status code received.
    Http(u16),
    /// Probe / codec / corrupt-data decode failure.
    Decode(String),
    /// Output device open / sink / cpal failure.
    Output(String),
    /// Stale generation — worker should exit silently, never user-visible.
    Abandoned,
}

impl EngineError {
    /// Converts the error to a classifiable `AudioStatus::Error` string.
    ///
    /// The prefixes are contractual (see Requirement 12):
    /// - Connect  → `"Connection failed: ..."`
    /// - Http     → `"HTTP {code}"`
    /// - Decode   → `"Decode error: ..."`
    /// - Output   → `"Hardware output error: ..."`
    /// - Abandoned → `"Abandoned"` (internal; never forwarded to the UI)
    pub(super) fn to_status_string(&self) -> String {
        match self {
            Self::Connect(msg) => format!("Connection failed: {msg}"),
            Self::Http(code) => format!("HTTP {code}"),
            Self::Decode(msg) => format!("Decode error: {msg}"),
            Self::Output(msg) => format!("{}{msg}", super::HARDWARE_OUTPUT_ERROR_PREFIX),
            Self::Abandoned => "Abandoned".to_string(),
        }
    }

    /// Returns `true` for output errors that may be recoverable by reopening
    /// the audio device.  All other error kinds are not hardware-recoverable.
    pub(super) fn is_recoverable_output(&self) -> bool {
        matches!(self, Self::Output(_))
    }
}

// ---------------------------------------------------------------------------
// EndReason
// ---------------------------------------------------------------------------

/// Reason a stream worker terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum EndReason {
    /// Mid-stream network read error.
    Network,
    /// Natural end-of-file (stream finished cleanly).
    Eof,
    /// Decoder encountered a fatal error mid-stream.
    Decode,
    /// Worker's generation became inactive; it self-cancelled.
    Abandoned,
}

// ---------------------------------------------------------------------------
// PrebufferConfig
// ---------------------------------------------------------------------------

/// Configuration for the bounded in-memory prebuffer filled before probing.
///
/// Invariants:
/// - `0 < min_bytes <= max_bytes`
/// - `fill_timeout` is finite and small (e.g. 8 s) — prevents the engine from
///   sitting in `Connecting`/`Buffering` forever.
#[derive(Debug, Clone)]
pub(super) struct PrebufferConfig {
    /// Minimum bytes to collect before probing the codec (e.g. 32 KiB).
    pub(super) min_bytes: usize,
    /// Hard memory cap for the prebuffer (bounds startup latency and RAM).
    pub(super) max_bytes: usize,
    /// Give up and emit `EngineError::Connect("prebuffer timeout")` after
    /// this duration elapses without receiving `min_bytes`.
    pub(super) fill_timeout: std::time::Duration,
}

// ---------------------------------------------------------------------------
// PlaybackOptions
// ---------------------------------------------------------------------------

/// Per-session playback options forwarded to connection workers.
#[derive(Debug, Clone)]
pub(super) struct PlaybackOptions {
    /// Whether to request and parse ICY stream metadata.
    pub(super) metadata_enabled: bool,
    /// Target playback volume in `[0.0, 1.0]`.  Set via `SetVolume`, clamped
    /// at the command boundary.
    pub(super) target_volume: f32,
    /// Preferred output device name, or `None` for the system default.
    pub(super) preferred_device: Option<String>,
}

impl Default for PlaybackOptions {
    fn default() -> Self {
        Self {
            metadata_enabled: true,
            target_volume: 0.8,
            preferred_device: None,
        }
    }
}

// ---------------------------------------------------------------------------
// StreamFormat
// ---------------------------------------------------------------------------

/// Codec/container information discovered during probing.
///
/// Used for diagnostics and for deciding between the MP3 fast-path and the
/// generic Symphonia decoder.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct StreamFormat {
    /// Human-readable codec name (e.g. `"MP3"`, `"AAC"`, `"Vorbis"`).
    pub(super) codec: String,
    /// Sample rate in Hz reported by the probed format.
    pub(super) sample_rate: u32,
    /// Channel count reported by the probed format.
    pub(super) channels: u16,
}

// ---------------------------------------------------------------------------
// DecodedSource
// ---------------------------------------------------------------------------

/// A type-erased, boxed rodio `Source` producing `f32` samples.
///
/// Workers produce this and hand it to `EngineLoop` via
/// `EngineEvent::Connected`.  `OutputManager::attach` appends it to the
/// rodio `Sink`.
pub(super) type DecodedSource = Box<dyn rodio::Source<Item = f32> + Send + 'static>;

// ---------------------------------------------------------------------------
// ConnectRequest
// ---------------------------------------------------------------------------

/// All parameters a connection worker needs to start its work.
#[derive(Debug, Clone)]
pub(super) struct ConnectRequest {
    /// This worker's generation id.
    pub(super) generation: Generation,
    /// Stream URL to connect to.
    pub(super) url: String,
    /// Prebuffer sizing / timeout configuration.
    pub(super) prebuffer: PrebufferConfig,
    /// Snapshot of engine-wide playback options at spawn time.
    pub(super) options: PlaybackOptions,
}

impl ConnectRequest {
    pub(super) fn new(
        generation: Generation,
        url: String,
        prebuffer: PrebufferConfig,
        options: PlaybackOptions,
    ) -> Self {
        Self {
            generation,
            url,
            prebuffer,
            options,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{classify_playback_error, PlaybackErrorKind};

    #[test]
    fn engine_error_to_status_string_stable_prefixes() {
        assert!(EngineError::Connect("timeout".into())
            .to_status_string()
            .starts_with("Connection failed:"));
        assert!(EngineError::Http(404)
            .to_status_string()
            .starts_with("HTTP "));
        assert!(EngineError::Decode("unsupported".into())
            .to_status_string()
            .starts_with("Decode error:"));
        assert!(EngineError::Output("stale handle".into())
            .to_status_string()
            .starts_with("Hardware output error:"));
    }

    #[test]
    fn engine_error_is_recoverable_output_only_for_output_variant() {
        assert!(EngineError::Output("x".into()).is_recoverable_output());
        assert!(!EngineError::Connect("x".into()).is_recoverable_output());
        assert!(!EngineError::Http(500).is_recoverable_output());
        assert!(!EngineError::Decode("x".into()).is_recoverable_output());
        assert!(!EngineError::Abandoned.is_recoverable_output());
    }

    // ---------------------------------------------------------------------------
    // Error classification round-trip tests (Requirement 12)
    // ---------------------------------------------------------------------------

    /// Connect errors must classify as Network (contains "Connection failed:").
    #[test]
    fn connect_error_classifies_as_network() {
        let s = EngineError::Connect("DNS lookup failed".into()).to_status_string();
        assert_eq!(
            classify_playback_error(&s),
            PlaybackErrorKind::Network,
            "Connect error string `{s}` should classify as Network"
        );
    }

    /// HTTP errors must classify as Http.
    #[test]
    fn http_error_classifies_as_http() {
        for code in [400u16, 403, 404, 500, 503] {
            let s = EngineError::Http(code).to_status_string();
            assert_eq!(
                classify_playback_error(&s),
                PlaybackErrorKind::Http,
                "HTTP error string `{s}` should classify as Http"
            );
        }
    }

    /// Decode errors must classify as Decode.
    #[test]
    fn decode_error_classifies_as_decode() {
        let s = EngineError::Decode("unsupported codec".into()).to_status_string();
        assert_eq!(
            classify_playback_error(&s),
            PlaybackErrorKind::Decode,
            "Decode error string `{s}` should classify as Decode"
        );
    }

    /// Output errors must classify as Output.
    #[test]
    fn output_error_classifies_as_output() {
        let s = EngineError::Output("sink error: device lost".into()).to_status_string();
        assert_eq!(
            classify_playback_error(&s),
            PlaybackErrorKind::Output,
            "Output error string `{s}` should classify as Output"
        );
    }

    /// Verify the Output string uses the HARDWARE_OUTPUT_ERROR_PREFIX constant.
    #[test]
    fn output_error_uses_hardware_output_error_prefix() {
        let msg = "no device available";
        let s = EngineError::Output(msg.into()).to_status_string();
        let expected_prefix = super::super::HARDWARE_OUTPUT_ERROR_PREFIX;
        assert!(
                s.starts_with(expected_prefix),
                "Output to_status_string `{s}` must start with HARDWARE_OUTPUT_ERROR_PREFIX `{expected_prefix}`"
            );
    }

    /// All non-Abandoned variants must produce a non-Unknown classification.
    #[test]
    fn all_user_visible_errors_classify_as_non_unknown() {
        let variants = vec![
            EngineError::Connect("timeout".into()),
            EngineError::Http(404),
            EngineError::Decode("bad format".into()),
            EngineError::Output("device lost".into()),
        ];
        for err in &variants {
            let s = err.to_status_string();
            let kind = classify_playback_error(&s);
            assert_ne!(
                kind,
                PlaybackErrorKind::Unknown,
                "EngineError variant produced Unknown classification for string: `{s}`"
            );
        }
    }

    #[test]
    fn engine_event_generation_returns_none_for_output_lost() {
        assert!(EngineEvent::OutputLost.generation().is_none());
    }

    #[test]
    fn engine_event_generation_returns_some_for_worker_events() {
        let gen = EngineEvent::Buffering {
            generation: 7,
            percent: 50,
        }
        .generation();
        assert_eq!(gen, Some(7));
    }

    #[test]
    fn playback_options_default_has_metadata_enabled() {
        let opts = PlaybackOptions::default();
        assert!(opts.metadata_enabled);
        assert_eq!(opts.target_volume, 0.8);
        assert!(opts.preferred_device.is_none());
    }

    #[test]
    fn connect_request_stores_all_fields() {
        let opts = PlaybackOptions::default();
        let pre = PrebufferConfig {
            min_bytes: 1024,
            max_bytes: 65536,
            fill_timeout: std::time::Duration::from_secs(8),
        };
        let req = ConnectRequest::new(1, "http://example.com".into(), pre.clone(), opts.clone());
        assert_eq!(req.generation, 1);
        assert_eq!(req.url, "http://example.com");
        assert_eq!(req.prebuffer.min_bytes, 1024);
    }

    // ---------------------------------------------------------------------------
    // Property-based tests — Task 2.1
    // ---------------------------------------------------------------------------

    #[cfg(test)]
    mod pbt {
        use super::*;
        use crate::app::{classify_playback_error, PlaybackErrorKind};
        use proptest::prelude::*;

        /// Arbitrary strategy for non-Abandoned `EngineError` variants.
        ///
        /// We cover:
        ///   - `Connect` with arbitrary message strings
        ///   - `Http` with arbitrary u16 status codes (full range)
        ///   - `Decode` with arbitrary message strings
        ///   - `Output` with arbitrary message strings
        fn arb_engine_error() -> impl Strategy<Value = EngineError> {
            prop_oneof![
                any::<String>().prop_map(EngineError::Connect),
                any::<u16>().prop_map(EngineError::Http),
                any::<String>().prop_map(EngineError::Decode),
                any::<String>().prop_map(EngineError::Output),
            ]
        }

        proptest! {
            /// **Property 10: Status classifiability**
            ///
            /// For any `EngineError` variant (excluding `Abandoned`),
            /// `classify_playback_error(err.to_status_string())` returns a non-`Unknown` kind.
            ///
            /// **Validates: Requirements 12.1, 12.2, 12.3, 12.4, 12.5, 12.7**
            #[test]
            fn prop_error_string_always_classifiable(err in arb_engine_error()) {
                let s = err.to_status_string();
                let kind = classify_playback_error(&s);
                prop_assert_ne!(
                    kind,
                    PlaybackErrorKind::Unknown,
                    "EngineError produced Unknown classification for status string: `{}`",
                    s
                );
            }
        }
    }
}
