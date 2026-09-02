//! Numbered actionable recovery fixes for the Playback Doctor.
//! Builds selectable actions from diagnostic suggestions and tracks execution status.
#![allow(dead_code)] // Types exercised by tests; action execution wiring pending

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
#[derive(Debug, Clone, PartialEq)]
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
/// Maps suggestion text to actionable operations. Only includes
/// `SwitchOutputDevice` if alternative devices are available.
pub fn build_recovery_actions(
    suggestions: &[&str],
    alternative_devices_available: bool,
) -> Vec<RecoveryAction> {
    let mut actions = Vec::new();
    let mut number: u8 = 1;

    for suggestion in suggestions {
        if actions.len() >= MAX_RECOVERY_ACTIONS {
            break;
        }

        if suggestion.contains("output device") || suggestion.contains("output") {
            if alternative_devices_available {
                actions.push(RecoveryAction {
                    number,
                    label: "Switch to next output device".to_string(),
                    kind: RecoveryActionKind::SwitchOutputDevice,
                    status: ActionStatus::Ready,
                });
                number += 1;
            }
        } else if suggestion.contains("Retry") || suggestion.contains("retry") {
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
        let actions = build_recovery_actions(&[], true);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_retry_suggestion_creates_action() {
        let actions = build_recovery_actions(&["Retry connection to station"], true);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].number, 1);
        assert_eq!(actions[0].kind, RecoveryActionKind::RetryConnection);
        assert_eq!(actions[0].status, ActionStatus::Ready);
    }

    #[test]
    fn test_output_device_with_alternatives() {
        let actions = build_recovery_actions(&["Try a different output device"], true);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].number, 1);
        assert_eq!(actions[0].kind, RecoveryActionKind::SwitchOutputDevice);
    }

    #[test]
    fn test_output_device_without_alternatives_excluded() {
        let actions = build_recovery_actions(&["Try a different output device"], false);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_sequential_numbering() {
        let suggestions = vec!["Try a different output device", "Retry connection"];
        let actions = build_recovery_actions(&suggestions, true);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].number, 1);
        assert_eq!(actions[1].number, 2);
    }

    #[test]
    fn test_max_actions_capped() {
        let suggestions: Vec<&str> = (0..15).map(|_| "Retry connection").collect();
        let actions = build_recovery_actions(&suggestions, true);
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
            let actions = build_recovery_actions(&suggestions, true);
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
}
