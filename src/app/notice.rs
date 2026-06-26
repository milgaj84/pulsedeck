use super::types::AppNotice;
use super::App;

pub(super) const NOTICE_INFO_TICKS: u16 = 90;
pub(super) const NOTICE_ERROR_TICKS: u16 = 150;

#[derive(Default)]
pub struct NoticeState {
    pub current: Option<AppNotice>,
    pub(super) ticks_remaining: u16,
}

impl App {
    pub(super) fn set_info_notice(&mut self, message: impl Into<String>) {
        self.ui.notice.current = Some(AppNotice::Info(message.into()));
        self.ui.notice.ticks_remaining = NOTICE_INFO_TICKS;
    }

    pub(super) fn set_error_notice(&mut self, message: impl Into<String>) {
        self.ui.notice.current = Some(AppNotice::Error(message.into()));
        self.ui.notice.ticks_remaining = NOTICE_ERROR_TICKS;
    }

    /// Convenience: set an error notice with a context prefix and error details.
    pub(super) fn set_operation_error_notice(
        &mut self,
        context: &str,
        err: &dyn std::fmt::Display,
    ) {
        self.set_error_notice(format!("{context}: {err}"));
    }

    pub(super) fn tick_notice(&mut self) {
        if self.ui.notice.ticks_remaining > 0 {
            self.ui.notice.ticks_remaining -= 1;
        } else {
            self.ui.notice.current = None;
        }
    }
}
