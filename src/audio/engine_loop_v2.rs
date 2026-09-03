//! Single-owner audio engine state machine.
//!
//! The control thread owns output resources, worker generations, playback state,
//! and user-visible status emission. Connection workers never mutate engine state.
#![cfg_attr(test, allow(dead_code))]

use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::output::normalize_output_device_name;
use super::output_manager::OutputManager;
use super::supervisor::ConnectionSupervisor;
use super::types::{
    ConnectRequest, DeviceRecoveryConfig, EndReason, EngineError, EngineEvent, EngineState,
    PlaybackOptions, PrebufferConfig,
};
use super::volume::{clamp_volume, VolumeRamp};
use super::{AudioCommand, AudioStatus};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn default_prebuffer_config() -> PrebufferConfig {
    PrebufferConfig {
        min_bytes: 32 * 1024,
        max_bytes: 512 * 1024,
        fill_timeout: Duration::from_secs(8),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeviceSwitchResume {
    None,
    Playing(String),
    Paused(String),
}

fn device_switch_resume(state: &EngineState) -> DeviceSwitchResume {
    match state {
        EngineState::Connecting { url, .. }
        | EngineState::Buffering { url, .. }
        | EngineState::Playing { url, .. }
        | EngineState::Recovering { url, .. } => DeviceSwitchResume::Playing(url.clone()),
        EngineState::Paused { url, .. } => DeviceSwitchResume::Paused(url.clone()),
        EngineState::Idle | EngineState::Failed { .. } => DeviceSwitchResume::None,
    }
}

pub(super) struct EngineLoop {
    state: EngineState,
    output: OutputManager,
    supervisor: ConnectionSupervisor,
    options: PlaybackOptions,
    volume: VolumeRamp,
    recovery_config: DeviceRecoveryConfig,
    status_tx: mpsc::Sender<AudioStatus>,
    event_rx: mpsc::Receiver<EngineEvent>,
    event_tx: mpsc::Sender<EngineEvent>,
    sample_buffer: Arc<Mutex<VecDeque<f32>>>,
    pause_after_connect: bool,
}

impl EngineLoop {
    fn new(
        status_tx: mpsc::Sender<AudioStatus>,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
        recovery_config: DeviceRecoveryConfig,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            state: EngineState::Idle,
            output: OutputManager::new(),
            supervisor: ConnectionSupervisor::new(),
            options: PlaybackOptions::default(),
            volume: VolumeRamp::new(0.8),
            recovery_config,
            status_tx,
            event_rx,
            event_tx,
            sample_buffer,
            pause_after_connect: false,
        }
    }

    pub(super) fn run(
        cmd_rx: mpsc::Receiver<AudioCommand>,
        status_tx: mpsc::Sender<AudioStatus>,
        sample_buffer: Arc<Mutex<VecDeque<f32>>>,
        recovery_config: DeviceRecoveryConfig,
    ) {
        let mut engine = Self::new(status_tx, sample_buffer, recovery_config);

        loop {
            match cmd_rx.recv_timeout(POLL_INTERVAL) {
                Ok(command) => engine.handle_command(command),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            while let Ok(event) = engine.event_rx.try_recv() {
                engine.handle_event(event);
            }

            engine.tick();

            if matches!(engine.state, EngineState::Playing { .. })
                && engine.output.is_sink_drained()
            {
                engine.output.stop();
                engine.pause_after_connect = false;
                engine.transition_to(EngineState::Idle);
                engine.emit(AudioStatus::Stopped);
            }
        }

        engine.supervisor.abandon();
        engine.output.stop();
    }

    fn handle_command(&mut self, command: AudioCommand) {
        match command {
            AudioCommand::Play(url) => {
                self.output.stop();
                self.start_connection(url, false);
            }
            AudioCommand::Pause => {
                if let EngineState::Playing { generation, url } = self.state.clone() {
                    self.output.pause();
                    self.transition_to(EngineState::Paused { generation, url });
                    self.emit(AudioStatus::Paused);
                }
            }
            AudioCommand::Resume => {
                if let EngineState::Paused { generation, url } = self.state.clone() {
                    self.output.resume();
                    self.volume.begin_fade_in();
                    self.transition_to(EngineState::Playing { generation, url });
                    self.emit(AudioStatus::Playing);
                }
            }
            AudioCommand::Stop => {
                self.pause_after_connect = false;
                self.supervisor.abandon();
                self.output.stop();
                self.transition_to(EngineState::Idle);
                self.emit(AudioStatus::Stopped);
            }
            AudioCommand::SetVolume(value) => {
                let value = clamp_volume(value);
                self.options.target_volume = value;
                self.volume.retarget(value);
            }
            AudioCommand::SetOutputDevice(requested) => {
                self.handle_output_device_change(requested);
            }
            AudioCommand::SetStreamMetadata(enabled) => {
                self.options.metadata_enabled = enabled;
            }
        }
    }

    fn handle_output_device_change(&mut self, requested: Option<String>) {
        let normalized_requested = normalize_output_device_name(requested.as_deref());
        let previous = self.output.preferred_device().map(str::to_string);
        let resume = device_switch_resume(&self.state);

        match self.output.switch_device(requested) {
            Ok(active) => {
                self.options.preferred_device = active.clone();
                self.emit(AudioStatus::OutputDeviceChanged {
                    active: active.clone(),
                });

                if active == previous {
                    return;
                }

                match resume {
                    DeviceSwitchResume::None => {}
                    DeviceSwitchResume::Playing(url) => self.start_connection(url, false),
                    DeviceSwitchResume::Paused(url) => self.start_connection(url, true),
                }
            }
            Err(error) => {
                self.emit(AudioStatus::OutputDeviceChangeFailed {
                    requested: normalized_requested,
                    active: previous,
                    error: error.to_status_string(),
                });
            }
        }
    }

    fn start_connection(&mut self, url: String, pause_after_connect: bool) {
        self.pause_after_connect = pause_after_connect;
        let generation = self.supervisor.next_generation();
        self.transition_to(EngineState::Connecting {
            generation,
            url: url.clone(),
        });
        self.emit(AudioStatus::Connecting);
        self.supervisor.spawn(
            ConnectRequest::new(
                generation,
                url,
                default_prebuffer_config(),
                self.options.clone(),
            ),
            self.event_tx.clone(),
            Arc::clone(&self.sample_buffer),
        );
    }

    fn handle_event(&mut self, event: EngineEvent) {
        if event
            .generation()
            .is_some_and(|generation| !self.supervisor.is_active(generation))
        {
            return;
        }

        match event {
            EngineEvent::Buffering {
                generation,
                percent,
            } => {
                let next = match &self.state {
                    EngineState::Connecting { url, .. } | EngineState::Buffering { url, .. } => {
                        Some(EngineState::Buffering {
                            generation,
                            url: url.clone(),
                            percent,
                        })
                    }
                    _ => None,
                };
                if let Some(next) = next {
                    self.transition_to(next);
                    self.emit(AudioStatus::Buffering { percent });
                }
            }
            EngineEvent::Connected {
                generation,
                source,
                format: _,
            } => {
                let url = self.current_url().unwrap_or_default().to_string();
                match self.output.attach(source) {
                    Ok(()) if self.pause_after_connect => {
                        self.pause_after_connect = false;
                        self.output.pause();
                        self.transition_to(EngineState::Paused { generation, url });
                        self.emit(AudioStatus::Paused);
                    }
                    Ok(()) => {
                        self.volume.begin_fade_in();
                        self.transition_to(EngineState::Playing { generation, url });
                        self.emit(AudioStatus::Playing);
                    }
                    Err(error) => {
                        let paused = self.pause_after_connect;
                        self.fail_or_recover(error, Some(url), paused);
                    }
                }
            }
            EngineEvent::TrackChanged { title, .. } => {
                let url = self.current_url().unwrap_or_default().to_string();
                self.emit(AudioStatus::TrackChanged { url, title });
            }
            EngineEvent::StreamEnded {
                reason: EndReason::Abandoned,
                ..
            } => {}
            EngineEvent::StreamEnded {
                reason: EndReason::Eof,
                ..
            } => {
                self.pause_after_connect = false;
                self.output.stop();
                self.transition_to(EngineState::Idle);
                self.emit(AudioStatus::Stopped);
            }
            EngineEvent::StreamEnded {
                reason: EndReason::Network | EndReason::Decode,
                ..
            } => {
                self.pause_after_connect = false;
                let error = EngineError::Connect("Connection lost".to_string());
                let status = error.to_status_string();
                let url = self.current_url().map(str::to_string);
                self.transition_to(EngineState::Failed { url, error });
                self.emit(AudioStatus::Error(status));
            }
            EngineEvent::OutputLost => self.try_recover_output(),
            EngineEvent::Failed { error, .. } => {
                let url = self.current_url().map(str::to_string);
                let paused = self.pause_after_connect;
                self.fail_or_recover(error, url, paused);
            }
        }
    }

    fn tick(&mut self) {
        self.supervisor.reap_finished();
        self.output.apply_volume_ramp(&mut self.volume);

        if self.volume.is_fading_out() {
            self.emit(AudioStatus::FadingOut {
                current_volume: self.volume.current_volume(),
            });
        }
    }

    fn transition_to(&mut self, next: EngineState) {
        self.state = next;
    }

    fn emit(&self, status: AudioStatus) {
        let _ = self.status_tx.send(status);
    }

    fn current_url(&self) -> Option<&str> {
        match &self.state {
            EngineState::Connecting { url, .. }
            | EngineState::Buffering { url, .. }
            | EngineState::Playing { url, .. }
            | EngineState::Paused { url, .. }
            | EngineState::Recovering { url, .. } => Some(url),
            EngineState::Failed { url, .. } => url.as_deref(),
            EngineState::Idle => None,
        }
    }

    fn fail_or_recover(
        &mut self,
        error: EngineError,
        url: Option<String>,
        pause_after_connect: bool,
    ) {
        if error.is_recoverable_output()
            && self.output.recovery_retries() < self.recovery_config.max_attempts
        {
            if self.output.recovery_retries() > 0 {
                std::thread::sleep(Duration::from_millis(self.recovery_config.delay_ms));
            }

            match self.output.reopen() {
                Ok(()) => {
                    if let Some(url) = url {
                        self.start_connection(url, pause_after_connect);
                        return;
                    }
                }
                Err(reopen_error)
                    if self.output.recovery_retries() < self.recovery_config.max_attempts =>
                {
                    self.fail_or_recover(reopen_error, url, pause_after_connect);
                    return;
                }
                Err(_) => {}
            }
        }

        self.pause_after_connect = false;
        let final_error = if error.is_recoverable_output()
            && self.output.recovery_retries() >= self.recovery_config.max_attempts
        {
            EngineError::Output(format!(
                "device recovery exhausted after {} attempts",
                self.recovery_config.max_attempts
            ))
        } else {
            error
        };
        let status = final_error.to_status_string();
        self.transition_to(EngineState::Failed {
            url,
            error: final_error,
        });
        self.emit(AudioStatus::Error(status));
    }

    fn try_recover_output(&mut self) {
        let resume = device_switch_resume(&self.state);
        let (url, paused) = match resume {
            DeviceSwitchResume::None => (None, false),
            DeviceSwitchResume::Playing(url) => (Some(url), false),
            DeviceSwitchResume::Paused(url) => (Some(url), true),
        };
        self.fail_or_recover(
            EngineError::Output("output device lost".to_string()),
            url,
            paused,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> (EngineLoop, mpsc::Receiver<AudioStatus>) {
        let (status_tx, status_rx) = mpsc::channel();
        let sample_buffer = Arc::new(Mutex::new(VecDeque::new()));
        (
            EngineLoop::new(status_tx, sample_buffer, DeviceRecoveryConfig::default()),
            status_rx,
        )
    }

    fn drain(receiver: &mpsc::Receiver<AudioStatus>) -> Vec<AudioStatus> {
        let mut statuses = Vec::new();
        while let Ok(status) = receiver.try_recv() {
            statuses.push(status);
        }
        statuses
    }

    #[test]
    fn device_switch_resume_preserves_each_active_state() {
        assert_eq!(
            device_switch_resume(&EngineState::Idle),
            DeviceSwitchResume::None
        );
        assert_eq!(
            device_switch_resume(&EngineState::Playing {
                generation: 1,
                url: "play".to_string(),
            }),
            DeviceSwitchResume::Playing("play".to_string())
        );
        assert_eq!(
            device_switch_resume(&EngineState::Paused {
                generation: 1,
                url: "pause".to_string(),
            }),
            DeviceSwitchResume::Paused("pause".to_string())
        );
        assert_eq!(
            device_switch_resume(&EngineState::Connecting {
                generation: 1,
                url: "connect".to_string(),
            }),
            DeviceSwitchResume::Playing("connect".to_string())
        );
        assert_eq!(
            device_switch_resume(&EngineState::Buffering {
                generation: 1,
                url: "buffer".to_string(),
                percent: 50,
            }),
            DeviceSwitchResume::Playing("buffer".to_string())
        );
    }

    #[test]
    #[ignore] // Requires audio hardware; crashes on headless CI (macOS/Windows)
    fn failed_device_switch_keeps_options_and_playback_state() {
        let (mut engine, status_rx) = make_engine();
        engine.state = EngineState::Playing {
            generation: 1,
            url: "http://station".to_string(),
        };

        engine.handle_command(AudioCommand::SetOutputDevice(Some(
            "__pulsedeck_missing_output_device__".to_string(),
        )));

        assert!(engine.options.preferred_device.is_none());
        assert!(matches!(engine.state, EngineState::Playing { .. }));
        assert!(drain(&status_rx).iter().any(|status| matches!(
            status,
            AudioStatus::OutputDeviceChangeFailed { active: None, .. }
        )));
    }

    #[test]
    fn play_sets_connecting_and_clears_pause_restore() {
        let (mut engine, status_rx) = make_engine();
        engine.pause_after_connect = true;

        engine.handle_command(AudioCommand::Play("http://station".to_string()));

        assert!(!engine.pause_after_connect);
        assert!(matches!(engine.state, EngineState::Connecting { .. }));
        assert!(drain(&status_rx)
            .iter()
            .any(|status| matches!(status, AudioStatus::Connecting)));
    }

    #[test]
    fn stop_is_total_and_emits_one_stopped() {
        let (mut engine, status_rx) = make_engine();
        engine.state = EngineState::Paused {
            generation: 1,
            url: "http://station".to_string(),
        };
        engine.pause_after_connect = true;

        engine.handle_command(AudioCommand::Stop);

        assert!(matches!(engine.state, EngineState::Idle));
        assert!(!engine.pause_after_connect);
        assert_eq!(
            drain(&status_rx)
                .iter()
                .filter(|status| matches!(status, AudioStatus::Stopped))
                .count(),
            1
        );
    }

    #[test]
    fn set_volume_sanitizes_non_finite_values() {
        let (mut engine, _) = make_engine();
        engine.handle_command(AudioCommand::SetVolume(f32::NAN));
        assert_eq!(engine.options.target_volume, 0.0);
        engine.handle_command(AudioCommand::SetVolume(f32::INFINITY));
        assert_eq!(engine.options.target_volume, 1.0);
    }

    #[test]
    fn stale_event_is_ignored() {
        let (mut engine, status_rx) = make_engine();
        let _old = engine.supervisor.next_generation();
        let current = engine.supervisor.next_generation();
        engine.state = EngineState::Playing {
            generation: current,
            url: "http://station".to_string(),
        };

        engine.handle_event(EngineEvent::Buffering {
            generation: 1,
            percent: 80,
        });

        assert!(matches!(engine.state, EngineState::Playing { .. }));
        assert!(drain(&status_rx).is_empty());
    }

    #[test]
    fn paused_restore_flag_is_consumed_after_successful_attach_or_failure() {
        let (mut engine, _) = make_engine();
        engine.pause_after_connect = true;
        engine.handle_command(AudioCommand::Stop);
        assert!(!engine.pause_after_connect);
    }

    #[test]
    fn metadata_setting_is_kept_in_worker_options() {
        let (mut engine, _) = make_engine();
        engine.handle_command(AudioCommand::SetStreamMetadata(false));
        assert!(!engine.options.metadata_enabled);
    }
}
