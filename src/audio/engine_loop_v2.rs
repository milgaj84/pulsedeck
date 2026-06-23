//! New `EngineLoop` and `EngineState` state machine (v2).
//!
//! This module implements the redesigned audio engine control loop described in
//! the audio-engine-rewrite spec. It is added alongside the old `engine_loop`
//! module (task 9) and will replace it in task 10.
//!
//! # Design principles
//! - `EngineLoop` is the ONLY place that sends `AudioStatus`.
//! - `handle_command` and `handle_event` are total — no panics for any input.
//! - Stale-generation events are silently dropped before any processing.
//! - `Stop` from any state transitions to `Idle` and emits exactly one `Stopped`.

use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::output_manager::OutputManager;
use super::supervisor::ConnectionSupervisor;
use super::types::{
    ConnectRequest, EndReason, EngineError, EngineEvent, EngineState, PlaybackOptions,
    PrebufferConfig,
};
use super::volume::{clamp_volume, VolumeRamp};
use super::{AudioCommand, AudioStatus, MAX_HARDWARE_RECOVERY_RETRIES};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn default_prebuffer_config() -> PrebufferConfig {
    PrebufferConfig {
        min_bytes: 32 * 1024,
        max_bytes: 512 * 1024,
        fill_timeout: Duration::from_secs(8),
    }
}

// ---------------------------------------------------------------------------
// EngineLoop
// ---------------------------------------------------------------------------

pub(super) struct EngineLoop {
    state: EngineState,
    output: OutputManager,
    supervisor: ConnectionSupervisor,
    options: PlaybackOptions,
    volume: VolumeRamp,
    status_tx: mpsc::Sender<AudioStatus>,
    event_rx: mpsc::Receiver<EngineEvent>,
    event_tx: mpsc::Sender<EngineEvent>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
}

impl EngineLoop {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Creates a new `EngineLoop` in `Idle` state.
    fn new(status_tx: mpsc::Sender<AudioStatus>, sample_buffer: Arc<Mutex<VecDeque<f32>>>) -> Self {
        let (event_tx, event_rx) = mpsc::channel::<EngineEvent>();
        Self {
            state: EngineState::Idle,
            output: OutputManager::new(),
            supervisor: ConnectionSupervisor::new(),
            options: PlaybackOptions::default(),
            volume: VolumeRamp::new(0.8),
            status_tx,
            event_rx,
            event_tx,
            sample_buffer,
        }
    }

    // -----------------------------------------------------------------------
    // Static entry point
    // -----------------------------------------------------------------------

    /// Main entry point called from `AudioEngine::spawn`.
    ///
    /// Runs the control loop on the calling thread until the command channel
    /// disconnects (application exit).
    pub(super) fn run(
        cmd_rx: mpsc::Receiver<AudioCommand>,
        status_tx: mpsc::Sender<AudioStatus>,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    ) {
        let mut engine = EngineLoop::new(status_tx, sample_buffer);
        loop {
            // 1. Drain one command (10 ms timeout).
            match cmd_rx.recv_timeout(POLL_INTERVAL) {
                Ok(cmd) => engine.handle_command(cmd),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // 2. Drain all pending internal events (non-blocking).
            while let Ok(ev) = engine.event_rx.try_recv() {
                engine.handle_event(ev);
            }

            // 3. Advance time-based concerns (volume ramp, etc.).
            engine.tick();

            // 4. Natural stream end: sink drained -> transition to Idle.
            if engine.output.is_sink_drained() {
                engine.output.stop();
                engine.transition_to(EngineState::Idle);
                engine.emit(AudioStatus::Stopped);
            }
        }

        // Clean shutdown: abandon any in-flight worker and stop output.
        engine.supervisor.abandon();
        engine.output.stop();
    }

    // -----------------------------------------------------------------------
    // Command handling (total)
    // -----------------------------------------------------------------------

    fn handle_command(&mut self, cmd: AudioCommand) {
        match cmd {
            AudioCommand::Play(url) => {
                let gen = self.supervisor.next_generation();
                self.output.stop();
                self.transition_to(EngineState::Connecting {
                    generation: gen,
                    url: url.clone(),
                });
                self.emit(AudioStatus::Connecting);
                self.supervisor.spawn(
                    ConnectRequest::new(gen, url, default_prebuffer_config(), self.options.clone()),
                    self.event_tx.clone(),
                    Arc::clone(&self.sample_buffer),
                );
            }
            AudioCommand::Pause => {
                if let EngineState::Playing { generation, url } = &self.state.clone() {
                    let generation = *generation;
                    let url = url.clone();
                    self.output.pause();
                    self.transition_to(EngineState::Paused { generation, url });
                    self.emit(AudioStatus::Paused);
                }
                // No-op in other states.
            }
            AudioCommand::Resume => {
                if let EngineState::Paused { generation, url } = &self.state.clone() {
                    let generation = *generation;
                    let url = url.clone();
                    self.output.resume();
                    self.volume.begin_fade_in();
                    self.transition_to(EngineState::Playing { generation, url });
                    self.emit(AudioStatus::Playing);
                }
                // No-op in other states.
            }
            AudioCommand::Stop => {
                self.supervisor.abandon();
                self.output.stop();
                self.transition_to(EngineState::Idle);
                self.emit(AudioStatus::Stopped);
            }
            AudioCommand::SetVolume(v) => {
                let clamped = clamp_volume(v);
                self.options.target_volume = clamped;
                self.volume.retarget(clamped);
            }
            AudioCommand::SetOutputDevice(d) => {
                self.output.set_preferred_device(d.clone());
                self.options.preferred_device = d;
            }
            AudioCommand::SetStreamMetadata(e) => {
                self.options.metadata_enabled = e;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Event handling (total)
    // -----------------------------------------------------------------------

    fn handle_event(&mut self, ev: EngineEvent) {
        // Stale-generation guard: drop events from non-active generations.
        if let Some(gen) = ev.generation() {
            if !self.supervisor.is_active(gen) {
                return;
            }
        }

        match ev {
            EngineEvent::Buffering {
                percent,
                generation,
            } => {
                // Only update state if we're in Connecting or Buffering.
                let new_state = match &self.state {
                    EngineState::Connecting { url, .. } => Some(EngineState::Buffering {
                        generation,
                        url: url.clone(),
                        percent,
                    }),
                    EngineState::Buffering { url, .. } => Some(EngineState::Buffering {
                        generation,
                        url: url.clone(),
                        percent,
                    }),
                    _ => None,
                };
                if let Some(next) = new_state {
                    self.transition_to(next);
                    self.emit(AudioStatus::Buffering { percent });
                }
            }
            EngineEvent::Connected {
                source,
                generation,
                format: _,
            } => {
                let url = self.current_url().map(str::to_string);
                match self.output.attach(source) {
                    Ok(()) => {
                        self.volume.begin_fade_in();
                        let url = url.unwrap_or_default();
                        self.transition_to(EngineState::Playing { generation, url });
                        self.emit(AudioStatus::Playing);
                    }
                    Err(err) => {
                        self.fail_or_recover(err, url);
                    }
                }
            }
            EngineEvent::TrackChanged { title, .. } => {
                let url = self.current_url().map(str::to_string).unwrap_or_default();
                self.emit(AudioStatus::TrackChanged { url, title });
            }
            EngineEvent::StreamEnded {
                reason: EndReason::Abandoned,
                ..
            } => {
                // Silently ignored — stale worker exited cleanly.
            }
            EngineEvent::StreamEnded {
                reason: EndReason::Eof,
                ..
            } => {
                self.output.stop();
                self.transition_to(EngineState::Idle);
                self.emit(AudioStatus::Stopped);
            }
            EngineEvent::StreamEnded {
                reason: EndReason::Network,
                generation: _,
                ..
            }
            | EngineEvent::StreamEnded {
                reason: EndReason::Decode,
                generation: _,
                ..
            } => {
                let msg = "Connection lost".to_string();
                self.transition_to(EngineState::Failed {
                    url: self.current_url().map(str::to_string),
                    error: EngineError::Connect(msg.clone()),
                });
                self.emit(AudioStatus::Error(format!("Connection failed: {msg}")));
            }
            EngineEvent::OutputLost => {
                self.try_recover_output();
            }
            EngineEvent::Failed { error, .. } => {
                let url = self.current_url().map(str::to_string);
                self.fail_or_recover(error, url);
            }
            // Total match: no other variants exist but catch-all is here for safety.
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Tick — drives time-based concerns each loop iteration
    // -----------------------------------------------------------------------

    fn tick(&mut self) {
        self.output.apply_volume_ramp(&mut self.volume);

        if self.volume.is_fading_out() {
            let current_volume = self.volume.current_volume();
            self.emit(AudioStatus::FadingOut { current_volume });
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Updates `self.state` without emitting any status.
    fn transition_to(&mut self, next: EngineState) {
        self.state = next;
    }

    /// Sends a status message to the UI. Silently drops if the channel is closed.
    fn emit(&self, status: AudioStatus) {
        let _ = self.status_tx.send(status);
    }

    /// Extracts the URL string from whatever state is currently active.
    fn current_url(&self) -> Option<&str> {
        match &self.state {
            EngineState::Connecting { url, .. }
            | EngineState::Buffering { url, .. }
            | EngineState::Playing { url, .. }
            | EngineState::Paused { url, .. }
            | EngineState::Recovering { url, .. } => Some(url.as_str()),
            EngineState::Failed { url, .. } => url.as_deref(),
            EngineState::Idle => None,
        }
    }

    /// Attempts to recover from an error. If the error is a recoverable output
    /// error and recovery retries are not exhausted, reopens the device and
    /// re-spawns the worker. Otherwise, emits `AudioStatus::Error`.
    fn fail_or_recover(&mut self, error: EngineError, url: Option<String>) {
        if error.is_recoverable_output()
            && self.output.recovery_retries() < MAX_HARDWARE_RECOVERY_RETRIES
        {
            match self.output.reopen() {
                Ok(()) => {
                    if let Some(url) = url {
                        let gen = self.supervisor.next_generation();
                        self.transition_to(EngineState::Connecting {
                            generation: gen,
                            url: url.clone(),
                        });
                        self.emit(AudioStatus::Connecting);
                        self.supervisor.spawn(
                            ConnectRequest::new(
                                gen,
                                url,
                                default_prebuffer_config(),
                                self.options.clone(),
                            ),
                            self.event_tx.clone(),
                            Arc::clone(&self.sample_buffer),
                        );
                    } else {
                        let status_str = error.to_status_string();
                        self.transition_to(EngineState::Failed { url: None, error });
                        self.emit(AudioStatus::Error(status_str));
                    }
                }
                Err(reopen_err) => {
                    let status_str = reopen_err.to_status_string();
                    self.transition_to(EngineState::Failed {
                        url,
                        error: reopen_err,
                    });
                    self.emit(AudioStatus::Error(status_str));
                }
            }
        } else {
            let status_str = error.to_status_string();
            self.transition_to(EngineState::Failed { url, error });
            self.emit(AudioStatus::Error(status_str));
        }
    }

    /// Attempts to recover the output device after it was lost.
    ///
    /// If reopen succeeds, re-attaches any in-progress decoded source and
    /// emits `Connecting`. On exhaustion, emits `Error`.
    fn try_recover_output(&mut self) {
        let url = self.current_url().map(str::to_string);
        self.fail_or_recover(EngineError::Output("output device lost".to_string()), url);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Helper: build an `EngineLoop` wired to test channels.
    fn make_engine() -> (EngineLoop, mpsc::Receiver<AudioStatus>) {
        let (status_tx, status_rx) = mpsc::channel::<AudioStatus>();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::<f32>::new()));
        let engine = EngineLoop::new(status_tx, sample_buffer);
        (engine, status_rx)
    }

    /// Drain all pending `AudioStatus` values from the receiver into a Vec.
    fn drain_status(rx: &mpsc::Receiver<AudioStatus>) -> Vec<AudioStatus> {
        let mut out = Vec::new();
        while let Ok(s) = rx.try_recv() {
            out.push(s);
        }
        out
    }

    /// Returns `true` if any status in `statuses` matches the discriminant of `expected`.
    fn has_status(statuses: &[AudioStatus], expected: &AudioStatus) -> bool {
        statuses
            .iter()
            .any(|s| std::mem::discriminant(s) == std::mem::discriminant(expected))
    }

    // -----------------------------------------------------------------------
    // Play from Idle -> Connecting + emits Connecting
    // -----------------------------------------------------------------------

    #[test]
    fn play_from_idle_transitions_to_connecting_and_emits_connecting() {
        let (mut engine, status_rx) = make_engine();

        engine.handle_command(AudioCommand::Play("http://example.com/stream".into()));

        let statuses = drain_status(&status_rx);
        assert!(
            matches!(engine.state, EngineState::Connecting { .. }),
            "expected Connecting state, got {:?}",
            engine.state,
        );
        assert!(
            has_status(&statuses, &AudioStatus::Connecting),
            "expected AudioStatus::Connecting to be emitted, got {:?}",
            statuses,
        );
    }

    // -----------------------------------------------------------------------
    // Play from Playing also transitions to Connecting (any-state rule)
    // -----------------------------------------------------------------------

    #[test]
    fn play_from_playing_transitions_to_connecting() {
        let (mut engine, status_rx) = make_engine();

        // Put engine in Playing state manually.
        engine.state = EngineState::Playing {
            generation: 1,
            url: "http://old.com".into(),
        };
        // Drain any earlier status.
        let _ = drain_status(&status_rx);

        engine.handle_command(AudioCommand::Play("http://new.com/stream".into()));

        assert!(
            matches!(engine.state, EngineState::Connecting { .. }),
            "expected Connecting state, got {:?}",
            engine.state,
        );
        let statuses = drain_status(&status_rx);
        assert!(
            has_status(&statuses, &AudioStatus::Connecting),
            "expected Connecting status, got {:?}",
            statuses,
        );
    }

    // -----------------------------------------------------------------------
    // Stop from any state -> Idle + exactly one Stopped
    // -----------------------------------------------------------------------

    #[test]
    fn stop_from_idle_emits_exactly_one_stopped() {
        let (mut engine, status_rx) = make_engine();
        engine.handle_command(AudioCommand::Stop);

        let statuses = drain_status(&status_rx);
        let stopped_count = statuses
            .iter()
            .filter(|s| matches!(s, AudioStatus::Stopped))
            .count();

        assert!(matches!(engine.state, EngineState::Idle));
        assert_eq!(
            stopped_count, 1,
            "expected exactly one Stopped, got {stopped_count}"
        );
    }

    #[test]
    fn stop_from_connecting_emits_exactly_one_stopped() {
        let (mut engine, status_rx) = make_engine();
        engine.state = EngineState::Connecting {
            generation: 1,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        engine.handle_command(AudioCommand::Stop);

        let statuses = drain_status(&status_rx);
        let stopped_count = statuses
            .iter()
            .filter(|s| matches!(s, AudioStatus::Stopped))
            .count();

        assert!(matches!(engine.state, EngineState::Idle));
        assert_eq!(
            stopped_count, 1,
            "expected exactly one Stopped, got {stopped_count}"
        );
    }

    #[test]
    fn stop_from_playing_emits_exactly_one_stopped() {
        let (mut engine, status_rx) = make_engine();
        engine.state = EngineState::Playing {
            generation: 2,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        engine.handle_command(AudioCommand::Stop);

        let statuses = drain_status(&status_rx);
        let stopped_count = statuses
            .iter()
            .filter(|s| matches!(s, AudioStatus::Stopped))
            .count();

        assert!(matches!(engine.state, EngineState::Idle));
        assert_eq!(
            stopped_count, 1,
            "expected exactly one Stopped, got {stopped_count}"
        );
    }

    // -----------------------------------------------------------------------
    // Pause while Playing -> Paused + emits Paused
    // -----------------------------------------------------------------------

    #[test]
    fn pause_while_playing_transitions_to_paused_and_emits_paused() {
        let (mut engine, status_rx) = make_engine();
        engine.state = EngineState::Playing {
            generation: 1,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        engine.handle_command(AudioCommand::Pause);

        let statuses = drain_status(&status_rx);
        assert!(
            matches!(engine.state, EngineState::Paused { .. }),
            "expected Paused state, got {:?}",
            engine.state,
        );
        assert!(
            has_status(&statuses, &AudioStatus::Paused),
            "expected Paused status, got {:?}",
            statuses,
        );
    }

    // -----------------------------------------------------------------------
    // Resume while Paused -> Playing + emits Playing
    // -----------------------------------------------------------------------

    #[test]
    fn resume_while_paused_transitions_to_playing_and_emits_playing() {
        let (mut engine, status_rx) = make_engine();
        engine.state = EngineState::Paused {
            generation: 1,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        engine.handle_command(AudioCommand::Resume);

        let statuses = drain_status(&status_rx);
        assert!(
            matches!(engine.state, EngineState::Playing { .. }),
            "expected Playing state, got {:?}",
            engine.state,
        );
        assert!(
            has_status(&statuses, &AudioStatus::Playing),
            "expected Playing status, got {:?}",
            statuses,
        );
    }

    // -----------------------------------------------------------------------
    // SetVolume(NaN) does not panic
    // -----------------------------------------------------------------------

    #[test]
    fn set_volume_nan_does_not_panic() {
        let (mut engine, _status_rx) = make_engine();
        engine.handle_command(AudioCommand::SetVolume(f32::NAN));
        // Volume should be clamped to 0.0 for NaN.
        assert_eq!(engine.options.target_volume, 0.0);
    }

    #[test]
    fn set_volume_infinity_does_not_panic() {
        let (mut engine, _status_rx) = make_engine();
        engine.handle_command(AudioCommand::SetVolume(f32::INFINITY));
        assert_eq!(engine.options.target_volume, 1.0);
    }

    #[test]
    fn set_volume_neg_infinity_does_not_panic() {
        let (mut engine, _status_rx) = make_engine();
        engine.handle_command(AudioCommand::SetVolume(f32::NEG_INFINITY));
        assert_eq!(engine.options.target_volume, 0.0);
    }

    // -----------------------------------------------------------------------
    // Stale-generation event does not change state or emit status
    // -----------------------------------------------------------------------

    #[test]
    fn stale_generation_event_is_dropped() {
        let (mut engine, status_rx) = make_engine();

        // Allocate generation 1, then generation 2 (making 1 stale).
        let _gen1 = engine.supervisor.next_generation();
        let _gen2 = engine.supervisor.next_generation();

        engine.state = EngineState::Playing {
            generation: 2,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        // Send a Buffering event from the stale generation 1.
        engine.handle_event(EngineEvent::Buffering {
            generation: 1,
            percent: 50,
        });

        // State must remain Playing, no status emitted.
        assert!(
            matches!(engine.state, EngineState::Playing { .. }),
            "state should not change on stale event, got {:?}",
            engine.state,
        );
        let statuses = drain_status(&status_rx);
        assert!(
            statuses.is_empty(),
            "no status should be emitted for stale event, got {:?}",
            statuses,
        );
    }

    #[test]
    fn stale_generation_connected_event_is_dropped() {
        let (mut engine, status_rx) = make_engine();

        // Allocate generation 1, then generation 2 (making 1 stale).
        let _gen1 = engine.supervisor.next_generation();
        let _gen2 = engine.supervisor.next_generation();

        engine.state = EngineState::Connecting {
            generation: 2,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        // Build a dummy source to use in the event.
        let dummy_source: super::super::types::DecodedSource = Box::new({
            use rodio::Source;
            rodio::source::SineWave::new(440.0).take_duration(Duration::from_millis(1))
        });
        let format = super::super::types::StreamFormat {
            codec: "MP3".into(),
            sample_rate: 44100,
            channels: 2,
        };

        // Send Connected from stale generation 1 — must be dropped.
        engine.handle_event(EngineEvent::Connected {
            generation: 1,
            source: dummy_source,
            format,
        });

        // State must remain Connecting (gen 2), no Playing status emitted.
        assert!(
            matches!(engine.state, EngineState::Connecting { generation: 2, .. }),
            "state should remain Connecting(gen=2) after stale Connected, got {:?}",
            engine.state,
        );
        let statuses = drain_status(&status_rx);
        assert!(
            !has_status(&statuses, &AudioStatus::Playing),
            "Playing should not be emitted for stale Connected, got {:?}",
            statuses,
        );
    }

    // -----------------------------------------------------------------------
    // Connected event transitions to Playing when attach succeeds
    // (On headless/CI machines this may emit Error instead — both are valid)
    // -----------------------------------------------------------------------

    #[test]
    fn connected_event_transitions_to_playing_or_error() {
        let (mut engine, status_rx) = make_engine();

        let gen = engine.supervisor.next_generation();
        engine.state = EngineState::Connecting {
            generation: gen,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        let dummy_source: super::super::types::DecodedSource = Box::new({
            use rodio::Source;
            rodio::source::SineWave::new(440.0).take_duration(Duration::from_millis(1))
        });
        let format = super::super::types::StreamFormat {
            codec: "MP3".into(),
            sample_rate: 44100,
            channels: 2,
        };

        engine.handle_event(EngineEvent::Connected {
            generation: gen,
            source: dummy_source,
            format,
        });

        let statuses = drain_status(&status_rx);
        // Either Playing (real device) or Error (headless/no device) — both valid.
        let transitioned = has_status(&statuses, &AudioStatus::Playing)
            || has_status(&statuses, &AudioStatus::Error(String::new()));
        assert!(
            transitioned,
            "expected Playing or Error status after Connected event, got {:?}",
            statuses,
        );
    }

    // -----------------------------------------------------------------------
    // Pause in non-Playing state is a no-op
    // -----------------------------------------------------------------------

    #[test]
    fn pause_while_idle_is_no_op() {
        let (mut engine, status_rx) = make_engine();
        engine.handle_command(AudioCommand::Pause);
        let statuses = drain_status(&status_rx);
        assert!(matches!(engine.state, EngineState::Idle));
        assert!(
            statuses.is_empty(),
            "no status should be emitted on no-op Pause"
        );
    }

    // -----------------------------------------------------------------------
    // Resume in non-Paused state is a no-op
    // -----------------------------------------------------------------------

    #[test]
    fn resume_while_idle_is_no_op() {
        let (mut engine, status_rx) = make_engine();
        engine.handle_command(AudioCommand::Resume);
        let statuses = drain_status(&status_rx);
        assert!(matches!(engine.state, EngineState::Idle));
        assert!(
            statuses.is_empty(),
            "no status should be emitted on no-op Resume"
        );
    }

    // -----------------------------------------------------------------------
    // StreamEnded Eof -> Idle + Stopped
    // -----------------------------------------------------------------------

    #[test]
    fn stream_ended_eof_transitions_to_idle_and_emits_stopped() {
        let (mut engine, status_rx) = make_engine();
        let gen = engine.supervisor.next_generation();
        engine.state = EngineState::Playing {
            generation: gen,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        engine.handle_event(EngineEvent::StreamEnded {
            generation: gen,
            reason: EndReason::Eof,
        });

        let statuses = drain_status(&status_rx);
        assert!(matches!(engine.state, EngineState::Idle));
        assert!(
            has_status(&statuses, &AudioStatus::Stopped),
            "expected Stopped after Eof, got {:?}",
            statuses,
        );
    }

    // -----------------------------------------------------------------------
    // StreamEnded Abandoned is a no-op
    // -----------------------------------------------------------------------

    #[test]
    fn stream_ended_abandoned_is_no_op() {
        let (mut engine, status_rx) = make_engine();
        let gen = engine.supervisor.next_generation();
        engine.state = EngineState::Playing {
            generation: gen,
            url: "http://example.com".into(),
        };
        let _ = drain_status(&status_rx);

        engine.handle_event(EngineEvent::StreamEnded {
            generation: gen,
            reason: EndReason::Abandoned,
        });

        let statuses = drain_status(&status_rx);
        // State stays Playing, no status emitted.
        assert!(
            matches!(engine.state, EngineState::Playing { .. }),
            "state should not change on Abandoned, got {:?}",
            engine.state,
        );
        assert!(
            statuses.is_empty(),
            "no status should be emitted for Abandoned"
        );
    }

    // -----------------------------------------------------------------------
    // SetOutputDevice stores in options
    // -----------------------------------------------------------------------

    #[test]
    fn set_output_device_updates_options() {
        let (mut engine, _status_rx) = make_engine();
        engine.handle_command(AudioCommand::SetOutputDevice(Some("MyDevice".into())));
        assert_eq!(engine.options.preferred_device.as_deref(), Some("MyDevice"));
    }

    // -----------------------------------------------------------------------
    // SetStreamMetadata stores in options
    // -----------------------------------------------------------------------

    #[test]
    fn set_stream_metadata_updates_options() {
        let (mut engine, _status_rx) = make_engine();
        assert!(engine.options.metadata_enabled); // default is true
        engine.handle_command(AudioCommand::SetStreamMetadata(false));
        assert!(!engine.options.metadata_enabled);
    }
}
