//! Runs the numbered fixes offered by the Playback Doctor.

use super::recovery_actions::{
    recovery_actions_for, truncate_recovery_error, ActionStatus, RecoveryActionKind,
};
use super::settings::{
    available_output_device_choices, output_device_display_name, step_output_device_preference,
};
use super::*;

impl App {
    /// Handle a digit pressed while the Playback Doctor is open.
    pub(super) fn handle_doctor_digit(&mut self, digit: char) {
        if let Some(number) = digit.to_digit(10).filter(|n| *n > 0) {
            self.run_recovery_action(number as u8);
        }
    }

    pub(super) fn run_recovery_action(&mut self, number: u8) {
        if matches!(
            self.playback.diagnostics.recovery,
            Some((_, ActionStatus::InProgress))
        ) {
            return;
        }

        let actions = recovery_actions_for(&self.playback.diagnostics);
        let Some(action) = actions.into_iter().find(|action| action.number == number) else {
            return;
        };

        self.playback.diagnostics.recovery = Some((action.kind.clone(), ActionStatus::InProgress));

        match action.kind {
            RecoveryActionKind::RetryConnection => {
                if self.playback.view.playing_url.is_none() {
                    self.fail_recovery(RecoveryActionKind::RetryConnection, "No stream to retry");
                } else {
                    self.retry_stream();
                }
            }
            RecoveryActionKind::SwitchOutputDevice => {
                self.switch_to_next_output_device(available_output_device_choices());
            }
        }
    }

    pub(super) fn switch_to_next_output_device(&mut self, choices: Vec<String>) {
        const KIND: RecoveryActionKind = RecoveryActionKind::SwitchOutputDevice;

        if choices.len() < 2 {
            self.fail_recovery(KIND, "No other output device available");
            return;
        }

        let next = step_output_device_preference(
            self.config.audio.output_device.as_deref(),
            &choices,
            true,
        );
        self.config.audio.output_device = next.clone();
        self.playback.diagnostics.output_device = output_device_display_name(next.as_deref());

        if !self.sync_output_device() {
            self.fail_recovery(KIND, "Audio engine unavailable");
        }
    }

    /// Record the outcome of the fix currently in progress, if it is `kind`.
    pub(super) fn finish_recovery(&mut self, kind: RecoveryActionKind, status: ActionStatus) {
        if matches!(
            &self.playback.diagnostics.recovery,
            Some((current, ActionStatus::InProgress)) if *current == kind
        ) {
            self.playback.diagnostics.recovery = Some((kind, status));
        }
    }

    fn fail_recovery(&mut self, kind: RecoveryActionKind, message: &str) {
        self.finish_recovery(kind, ActionStatus::Failed(truncate_recovery_error(message)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::favorites::Library;

    fn doctor_app() -> App {
        let mut app = App::new(Library::in_memory(vec![]));
        app.playback.diagnostics.output_device = "Speakers".to_string();
        app.update(Action::TogglePlaybackDoctor);
        app
    }

    fn retry_app() -> App {
        let mut app = doctor_app();
        app.playback.diagnostics.reconnect_attempts = 2; // offers "manual retry"
        app
    }

    #[test]
    fn doctor_digit_runs_retry_and_marks_it_in_progress() {
        let mut app = retry_app();
        app.playback.view.playing_url = Some("http://a".to_string());

        app.update(Action::NumberJumpDigit('1'));

        assert_eq!(
            app.playback.diagnostics.recovery,
            Some((
                RecoveryActionKind::RetryConnection,
                ActionStatus::InProgress
            ))
        );
        assert_eq!(app.playback.view.state, PlaybackState::Connecting);
        assert!(!app.number_jump.is_active());
    }

    #[test]
    fn doctor_digit_without_a_stream_fails_the_retry() {
        let mut app = retry_app();

        app.update(Action::NumberJumpDigit('1'));

        assert_eq!(
            app.playback.diagnostics.recovery,
            Some((
                RecoveryActionKind::RetryConnection,
                ActionStatus::Failed("No stream to retry".to_string())
            ))
        );
    }

    #[test]
    fn doctor_digit_without_a_matching_action_does_nothing() {
        let mut app = retry_app();

        app.update(Action::NumberJumpDigit('2'));
        app.update(Action::NumberJumpDigit('0'));

        assert_eq!(app.playback.diagnostics.recovery, None);
    }

    #[test]
    fn running_a_fix_is_ignored_while_another_is_in_progress() {
        let mut app = retry_app();
        app.playback.view.playing_url = Some("http://a".to_string());
        app.playback.diagnostics.recovery = Some((
            RecoveryActionKind::SwitchOutputDevice,
            ActionStatus::InProgress,
        ));

        app.update(Action::NumberJumpDigit('1'));

        assert_eq!(
            app.playback.diagnostics.recovery,
            Some((
                RecoveryActionKind::SwitchOutputDevice,
                ActionStatus::InProgress
            ))
        );
        assert_ne!(app.playback.view.state, PlaybackState::Connecting);
    }

    #[test]
    fn finish_recovery_only_updates_the_matching_in_progress_fix() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.playback.diagnostics.recovery = Some((
            RecoveryActionKind::RetryConnection,
            ActionStatus::InProgress,
        ));

        app.finish_recovery(
            RecoveryActionKind::SwitchOutputDevice,
            ActionStatus::Success,
        );
        assert_eq!(
            app.playback.diagnostics.recovery,
            Some((
                RecoveryActionKind::RetryConnection,
                ActionStatus::InProgress
            ))
        );

        app.finish_recovery(RecoveryActionKind::RetryConnection, ActionStatus::Success);
        assert_eq!(
            app.playback.diagnostics.recovery,
            Some((RecoveryActionKind::RetryConnection, ActionStatus::Success))
        );

        app.finish_recovery(
            RecoveryActionKind::RetryConnection,
            ActionStatus::Failed("late".to_string()),
        );
        assert_eq!(
            app.playback.diagnostics.recovery,
            Some((RecoveryActionKind::RetryConnection, ActionStatus::Success))
        );
    }

    #[test]
    fn switching_output_steps_to_the_next_device() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.playback.diagnostics.recovery = Some((
            RecoveryActionKind::SwitchOutputDevice,
            ActionStatus::InProgress,
        ));
        app.config.audio.output_device = None;

        app.switch_to_next_output_device(vec!["Default".to_string(), "USB DAC".to_string()]);

        assert_eq!(app.config.audio.output_device.as_deref(), Some("USB DAC"));
        assert_eq!(app.playback.diagnostics.output_device, "USB DAC");
    }

    #[test]
    fn switching_output_fails_when_there_is_no_other_device() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.playback.diagnostics.recovery = Some((
            RecoveryActionKind::SwitchOutputDevice,
            ActionStatus::InProgress,
        ));

        app.switch_to_next_output_device(vec!["Default".to_string()]);

        assert_eq!(
            app.playback.diagnostics.recovery,
            Some((
                RecoveryActionKind::SwitchOutputDevice,
                ActionStatus::Failed("No other output device available".to_string())
            ))
        );
        assert_eq!(app.config.audio.output_device, None);
    }

    #[test]
    fn opening_the_doctor_clears_the_previous_fix_status() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.playback.diagnostics.recovery =
            Some((RecoveryActionKind::RetryConnection, ActionStatus::Success));

        app.update(Action::TogglePlaybackDoctor);

        assert_eq!(app.playback.diagnostics.recovery, None);
    }
}
