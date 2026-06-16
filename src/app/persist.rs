use super::*;

#[derive(Default)]
pub(super) struct PersistFlags {
    ui_state_dirty: bool,
    history_dirty: bool,
    library_dirty: bool,
}

impl App {
    pub(super) fn mark_ui_state_dirty(&mut self) {
        self.persist.ui_state_dirty = true;
    }

    pub(super) fn mark_history_dirty(&mut self) {
        self.persist.history_dirty = true;
    }

    pub(super) fn mark_library_dirty(&mut self) {
        self.persist.library_dirty = true;
    }

    pub(super) fn flush_persistence(&mut self) {
        if self.persist.ui_state_dirty {
            let state = super::ui_state::UiState::from_app_values(
                self.volume,
                self.muted,
                self.layout_mode,
                self.visualizer_mode,
            );
            match state.save() {
                Ok(()) => self.persist.ui_state_dirty = false,
                Err(err) => self.set_error_notice(format!("Could not save UI state: {err}")),
            }
        }

        if self.persist.history_dirty {
            match self.history.save() {
                Ok(()) => self.persist.history_dirty = false,
                Err(err) => self.set_error_notice(format!("Could not save history: {err}")),
            }
        }

        if self.persist.library_dirty {
            match self.library.save() {
                Ok(()) => self.persist.library_dirty = false,
                Err(err) => self.set_error_notice(format!("Could not save library: {err}")),
            }
        }
    }
}
