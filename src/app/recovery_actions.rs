//! Numbered actionable recovery fixes for the Playback Doctor.
//! Builds selectable actions from diagnostic suggestions and tracks execution status.

use super::doctor_suggestions::suggest_actions;
use super::settings::{
    output_device_choices, output_device_display_name, step_output_device_preference,
};
use super::types::PlaybackDiagnostics;

/// Maximum number of recovery actions displayed (keyed to number keys 1-9).
pub const MAX_RECOVERY_ACTIONS: usize = 9;

/// Maximum length for error messages displayed in the recovery wizard.
const MAX_ERROR_MESSAGE_LEN: usize = 120;

/// The kind of recovery operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryActionKind {
    SwitchOutputDevice,
    RetryConnection,
}

/// Status of a recovery action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionStatus {
    Ready,
    InProgress,
    Success,
    Failed(String),
}

/// A numbered actionable fix offered by the Playback Doctor.
#[derive(Debug, Clone)]
pub struct RecoveryAction {
    pub number: u8,
    pub label: String,
    pub kind: RecoveryActionKind,
    pub status: ActionStatus,
}

/// Build numbered recovery actions from diagnostic suggestions.
///
/// Maps suggestion text to actionable operations. `SwitchOutputDevice` is only
/// included when `switch_target` names the device it would switch to.
pub fn build_recovery_actions(
    suggestions: &[&str],
    switch_target: Option<&str>,
) -> Vec<RecoveryAction> {
    let mut actions = Vec::new();
    let mut number: u8 = 1;

    for suggestion in suggestions {
        if actions.len() >= MAX_RECOVERY_ACTIONS {
            break;
        }

        if suggestion.contains("output device") || suggestion.contains("output") {
            if let Some(target) = switch_target {
                actions.push(RecoveryAction {
                    number,
                    label: format!("Switch to {target}"),
                    kind: RecoveryActionKind::SwitchOutputDevice,
                    status: ActionStatus::Ready,
                });
                number += 1;
            }
        } else if suggestion.contains("Retry")
            || suggestion.contains("retry")
            || suggestion.contains("try again")
        {
            actions.push(RecoveryAction {
                number,
                label: "Retry connection".to_string(),
                kind: RecoveryActionKind::RetryConnection,
                status: ActionStatus::Ready,
            });
            number += 1;
        }
    }

    actions
}

/// Display name of the device a "switch output" fix would select, or `None`
/// when there is nowhere else to switch to.
pub fn next_output_device_name(devices: &[String], current: Option<&str>) -> Option<String> {
    let choices = output_device_choices(devices);
    if choices.len() < 2 {
        return None;
    }
    let next = output_device_display_name(
        step_output_device_preference(current, &choices, true).as_deref(),
    );
    let current = output_device_display_name(current);
    (!next.eq_ignore_ascii_case(&current)).then_some(next)
}

/// Recovery actions for the current diagnostics, carrying the status of the
/// action the user last ran (if it is still offered).
///
/// `current_device` is the configured output device preference (`None` = default).
pub fn recovery_actions_for(
    diagnostics: &PlaybackDiagnostics,
    current_device: Option<&str>,
) -> Vec<RecoveryAction> {
    let suggestions = suggest_actions(diagnostics);
    let target = next_output_device_name(&diagnostics.output_devices, current_device);
    let mut actions = build_recovery_actions(&suggestions, target.as_deref());

    if let Some((kind, status)) = &diagnostics.recovery {
        for action in actions.iter_mut().filter(|action| action.kind == *kind) {
            action.status = status.clone();
        }
    }

    actions
}

/// Truncate an error message to at most `max_len` characters.
/// Appends '…' if truncation occurs.
pub fn truncate_error_message(message: &str, max_len: usize) -> String {
    let chars: Vec<char> = message.chars().collect();
    if chars.len() <= max_len {
        message.to_string()
    } else {
        let limit = if max_len > 0 { max_len - 1 } else { 0 };
        let truncated: String = chars[..limit].iter().collect();
        format!("{}…", truncated)
    }
}

/// Truncate using the default max length for recovery error messages.
pub fn truncate_recovery_error(message: &str) -> String {
    truncate_error_message(message, MAX_ERROR_MESSAGE_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_suggestions_returns_empty() {
        let actions = build_recovery_actions(&[], Some("USB DAC"));
        assert!(actions.is_empty());
    }

    #[test]
    fn test_retry_suggestion_creates_action() {
        let actions = build_recovery_actions(&["Retry connection to station"], Some("USB DAC"));
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].number, 1);
        assert_eq!(actions[0].kind, RecoveryActionKind::RetryConnection);
        assert_eq!(actions[0].status, ActionStatus::Ready);
    }

    #[test]
    fn test_output_device_with_alternatives() {
        let actions = build_recovery_actions(&["Try a different output device"], Some("USB DAC"));
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].number, 1);
        assert_eq!(actions[0].kind, RecoveryActionKind::SwitchOutputDevice);
    }

    #[test]
    fn test_output_device_without_alternatives_excluded() {
        let actions = build_recovery_actions(&["Try a different output device"], None);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_sequential_numbering() {
        let suggestions = vec!["Try a different output device", "Retry connection"];
        let actions = build_recovery_actions(&suggestions, Some("USB DAC"));
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].number, 1);
        assert_eq!(actions[1].number, 2);
    }

    #[test]
    fn test_max_actions_capped() {
        let suggestions: Vec<&str> = (0..15).map(|_| "Retry connection").collect();
        let actions = build_recovery_actions(&suggestions, Some("USB DAC"));
        assert_eq!(actions.len(), MAX_RECOVERY_ACTIONS);
    }

    #[test]
    fn test_truncate_short_message_unchanged() {
        let msg = "Short error";
        assert_eq!(truncate_error_message(msg, 120), msg);
    }

    #[test]
    fn test_truncate_exact_length_unchanged() {
        let msg = "a".repeat(120);
        assert_eq!(truncate_error_message(&msg, 120), msg);
    }

    #[test]
    fn test_truncate_long_message_with_ellipsis() {
        let msg = "a".repeat(121);
        let result = truncate_error_message(&msg, 120);
        assert_eq!(result.chars().count(), 120); // 119 + '…'
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_recovery_error_uses_default_max() {
        let msg = "b".repeat(200);
        let result = truncate_recovery_error(&msg);
        assert_eq!(result.chars().count(), MAX_ERROR_MESSAGE_LEN);
        assert!(result.ends_with('…'));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property 10: For N suggestions (1..=9), actions are numbered 1..N sequentially.
        #[test]
        fn prop_recovery_action_numbering(count in 1..=9usize) {
            let suggestions: Vec<&str> = (0..count).map(|_| "Retry connection").collect();
            let actions = build_recovery_actions(&suggestions, Some("USB DAC"));
            prop_assert_eq!(actions.len(), count);
            for (i, action) in actions.iter().enumerate() {
                prop_assert_eq!(action.number as usize, i + 1);
            }
        }

        /// Property 11: truncate_error_message always returns ≤ max_len chars.
        #[test]
        fn prop_error_message_truncation(message in ".*", max_len in 1..200usize) {
            let result = truncate_error_message(&message, max_len);
            prop_assert!(result.chars().count() <= max_len,
                "result {} chars exceeds max {}", result.chars().count(), max_len);
        }
    }

    #[test]
    fn test_unreachable_stream_suggestion_offers_retry() {
        let actions = build_recovery_actions(
            &["Stream may be unreachable — try again later"],
            Some("USB DAC"),
        );
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, RecoveryActionKind::RetryConnection);
    }

    #[test]
    fn test_recovery_actions_for_carries_running_status() {
        let mut diagnostics = PlaybackDiagnostics::new("Speakers".to_string(), true, 5);
        diagnostics.reconnect_attempts = 2;
        diagnostics.recovery = Some((
            RecoveryActionKind::RetryConnection,
            ActionStatus::InProgress,
        ));

        let actions = recovery_actions_for(&diagnostics, None);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].status, ActionStatus::InProgress);
    }

    fn devices(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn output_error_diagnostics(devices: &[&str]) -> PlaybackDiagnostics {
        let mut diagnostics = PlaybackDiagnostics::new("Default".to_string(), true, 5);
        diagnostics.last_error = Some("audio output device vanished".to_string());
        diagnostics.output_devices = self::devices(devices);
        diagnostics
    }

    #[test]
    fn test_next_output_device_none_without_alternatives() {
        assert_eq!(next_output_device_name(&[], None), None);
    }

    #[test]
    fn test_next_output_device_names_first_device_after_default() {
        assert_eq!(
            next_output_device_name(&devices(&["Speakers", "USB DAC"]), None).as_deref(),
            Some("Speakers")
        );
    }

    #[test]
    fn test_next_output_device_steps_and_wraps_to_default() {
        let all = devices(&["Speakers", "USB DAC"]);
        assert_eq!(
            next_output_device_name(&all, Some("Speakers")).as_deref(),
            Some("USB DAC")
        );
        assert_eq!(
            next_output_device_name(&all, Some("USB DAC")).as_deref(),
            Some("Default")
        );
    }

    #[test]
    fn test_next_output_device_falls_back_to_default_when_current_is_unplugged() {
        assert_eq!(
            next_output_device_name(&devices(&["Speakers"]), Some("Gone DAC")).as_deref(),
            Some("Default")
        );
    }

    #[test]
    fn test_switch_fix_hidden_when_no_device_is_cached() {
        let actions = recovery_actions_for(&output_error_diagnostics(&[]), None);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_switch_fix_label_names_the_target_device() {
        let actions = recovery_actions_for(&output_error_diagnostics(&["USB DAC"]), None);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, RecoveryActionKind::SwitchOutputDevice);
        assert_eq!(actions[0].label, "Switch to USB DAC");
    }

    #[test]
    fn test_switch_fix_numbering_stays_sequential_with_retry() {
        let mut diagnostics = output_error_diagnostics(&["USB DAC"]);
        diagnostics.reconnect_attempts = 2;

        let actions = recovery_actions_for(&diagnostics, None);

        assert_eq!(
            actions.iter().map(|a| a.number).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(actions[1].kind, RecoveryActionKind::RetryConnection);
    }
}
