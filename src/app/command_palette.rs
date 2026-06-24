use super::*;
use crate::action::Action;

#[derive(Debug, Clone, Default)]
pub struct CommandPaletteState {
    pub query: String,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    SearchStations,
    RetryStream,
    StopPlayback,
    OpenSettings,
    CycleTheme,
    ToggleMetadata,
    RefreshMetadata,
    OpenPlaybackDoctor,
    ExportLibrary,
    Discover,
    OpenHelp,
}

const ALWAYS_AVAILABLE_COMMANDS: &[PaletteCommand] = &[
    PaletteCommand::SearchStations,
    PaletteCommand::OpenSettings,
    PaletteCommand::CycleTheme,
    PaletteCommand::ToggleMetadata,
    PaletteCommand::RefreshMetadata,
    PaletteCommand::OpenPlaybackDoctor,
    PaletteCommand::ExportLibrary,
    PaletteCommand::Discover,
    PaletteCommand::OpenHelp,
];

pub fn command_label(command: PaletteCommand) -> &'static str {
    match command {
        PaletteCommand::SearchStations => "Search stations",
        PaletteCommand::RetryStream => "Retry stream",
        PaletteCommand::StopPlayback => "Stop playback",
        PaletteCommand::OpenSettings => "Open settings",
        PaletteCommand::CycleTheme => "Change theme",
        PaletteCommand::ToggleMetadata => "Toggle song info metadata",
        PaletteCommand::RefreshMetadata => "Refresh library metadata",
        PaletteCommand::OpenPlaybackDoctor => "Open playback doctor",
        PaletteCommand::ExportLibrary => "Export library",
        PaletteCommand::Discover => "Discover stations",
        PaletteCommand::OpenHelp => "Open help",
    }
}

pub fn command_action(command: PaletteCommand) -> Action {
    match command {
        PaletteCommand::SearchStations => Action::EnterSearch,
        PaletteCommand::RetryStream => Action::RetryStream,
        PaletteCommand::StopPlayback => Action::Stop,
        PaletteCommand::OpenSettings => Action::ToggleSettings,
        PaletteCommand::CycleTheme => Action::CycleThemeSetting,
        PaletteCommand::ToggleMetadata => Action::ToggleStreamMetadata,
        PaletteCommand::RefreshMetadata => Action::RefreshLibraryMetadata,
        PaletteCommand::OpenPlaybackDoctor => Action::TogglePlaybackDoctor,
        PaletteCommand::ExportLibrary => Action::ExportLibrary,
        PaletteCommand::Discover => Action::Discover,
        PaletteCommand::OpenHelp => Action::ToggleHelp,
    }
}

pub fn filtered_commands(query: &str, app: &App) -> Vec<PaletteCommand> {
    let normalized = normalize_query(query);
    available_commands(app)
        .into_iter()
        .filter(|command| command_matches(command_label(*command), &normalized))
        .collect()
}

fn available_commands(app: &App) -> Vec<PaletteCommand> {
    let mut commands = ALWAYS_AVAILABLE_COMMANDS.to_vec();
    if app.playback.view.playing_url.is_some() {
        commands.insert(1, PaletteCommand::RetryStream);
    }
    if matches!(
        app.playback.view.state,
        PlaybackState::Connecting
            | PlaybackState::Playing
            | PlaybackState::Paused
            | PlaybackState::FadingOut { .. }
            | PlaybackState::Error(_)
    ) {
        commands.insert(2, PaletteCommand::StopPlayback);
    }
    commands
}

fn command_matches(label: &str, normalized_query: &str) -> bool {
    if normalized_query.is_empty() {
        return true;
    }
    let label = normalize_query(label);
    normalized_query
        .split_whitespace()
        .all(|token| label.contains(token))
}

fn normalize_query(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

impl App {
    pub(super) fn open_command_palette(&mut self) {
        if self.ui.input_mode != InputMode::Normal {
            return;
        }
        self.close_any_overlay();
        self.ui.input_mode = InputMode::CommandPalette;
        self.ui.command_palette.query.clear();
        self.ui.command_palette.selected = 0;
    }

    pub(super) fn close_command_palette(&mut self) {
        if self.ui.input_mode == InputMode::CommandPalette {
            self.ui.input_mode = InputMode::Normal;
            self.ui.command_palette.query.clear();
            self.ui.command_palette.selected = 0;
        }
    }

    pub(super) fn handle_command_palette_action(&mut self, action: Action) {
        match action {
            Action::CommandPaletteInput(c) => {
                self.ui.command_palette.query.push(c);
                self.clamp_command_palette_selection();
            }
            Action::CommandPaletteBackspace => {
                self.ui.command_palette.query.pop();
                self.clamp_command_palette_selection();
            }
            Action::CommandPaletteNext => self.step_command_palette_selection(true),
            Action::CommandPalettePrev => self.step_command_palette_selection(false),
            Action::CommandPaletteClose => self.close_command_palette(),
            Action::CommandPaletteConfirm => self.confirm_command_palette(),
            Action::Tick => self.tick(),
            Action::Quit => self.quit(),
            _ => {}
        }
    }

    fn confirm_command_palette(&mut self) {
        let Some(command) = self
            .command_palette_commands()
            .get(self.ui.command_palette.selected)
            .copied()
        else {
            self.set_error_notice("No matching command");
            return;
        };

        self.close_command_palette();
        self.update(command_action(command));
    }

    fn step_command_palette_selection(&mut self, forward: bool) {
        let len = self.command_palette_commands().len();
        if len == 0 {
            self.ui.command_palette.selected = 0;
            return;
        }

        self.ui.command_palette.selected = if forward {
            (self.ui.command_palette.selected + 1) % len
        } else if self.ui.command_palette.selected == 0 {
            len - 1
        } else {
            self.ui.command_palette.selected - 1
        };
    }

    fn clamp_command_palette_selection(&mut self) {
        let len = self.command_palette_commands().len();
        if len == 0 {
            self.ui.command_palette.selected = 0;
        } else {
            self.ui.command_palette.selected = self.ui.command_palette.selected.min(len - 1);
        }
    }

    pub fn command_palette_commands(&self) -> Vec<PaletteCommand> {
        filtered_commands(&self.ui.command_palette.query, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::Library;
    use crate::radio::Station;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn test_app() -> App {
        App::new(Library::in_memory(vec![station("A", "http://a")]))
    }

    #[test]
    fn filters_commands_case_insensitively_by_tokens() {
        let app = test_app();

        let commands = filtered_commands("SONG meta", &app);

        assert_eq!(commands, vec![PaletteCommand::ToggleMetadata]);
    }

    #[test]
    fn retry_is_hidden_when_no_stream_exists() {
        let app = test_app();

        assert!(!filtered_commands("retry", &app).contains(&PaletteCommand::RetryStream));
    }

    #[test]
    fn retry_is_available_when_stream_exists() {
        let mut app = test_app();
        app.playback.view.playing_url = Some("http://a".to_string());

        assert_eq!(
            filtered_commands("retry", &app),
            vec![PaletteCommand::RetryStream]
        );
    }

    #[test]
    fn refresh_metadata_command_is_available() {
        let app = test_app();

        assert_eq!(
            filtered_commands("refresh metadata", &app),
            vec![PaletteCommand::RefreshMetadata]
        );
    }

    #[test]
    fn opens_only_from_normal_mode() {
        let mut app = test_app();
        app.ui.input_mode = InputMode::Search;

        app.open_command_palette();
        assert_eq!(app.ui.input_mode, InputMode::Search);

        app.ui.input_mode = InputMode::Normal;
        app.open_command_palette();
        assert_eq!(app.ui.input_mode, InputMode::CommandPalette);
    }

    #[test]
    fn confirm_executes_selected_command_and_closes() {
        let mut app = test_app();
        app.update(Action::OpenCommandPalette);
        app.update(Action::CommandPaletteInput('s'));
        app.update(Action::CommandPaletteInput('e'));

        app.update(Action::CommandPaletteConfirm);

        assert_eq!(app.ui.input_mode, InputMode::Search);
    }

    #[test]
    fn close_clears_query() {
        let mut app = test_app();
        app.update(Action::OpenCommandPalette);
        app.update(Action::CommandPaletteInput('x'));

        app.update(Action::CommandPaletteClose);

        assert_eq!(app.ui.input_mode, InputMode::Normal);
        assert!(app.ui.command_palette.query.is_empty());
    }

    #[test]
    fn empty_query_returns_all_available_commands() {
        let app = test_app();

        let commands = filtered_commands("", &app);

        assert!(
            commands.len() >= 8,
            "Expected at least 8 always-available commands, got {}",
            commands.len()
        );
        assert_eq!(commands.len(), ALWAYS_AVAILABLE_COMMANDS.len());
    }

    #[test]
    fn non_matching_query_returns_zero_results() {
        let app = test_app();

        let commands = filtered_commands("zzz", &app);

        assert!(commands.is_empty());
    }

    #[test]
    fn multi_token_conjunctive_matching() {
        let app = test_app();

        // "open settings" has both "open" and "settings" tokens
        let commands = filtered_commands("open settings", &app);
        assert_eq!(commands, vec![PaletteCommand::OpenSettings]);

        // "open" alone matches multiple commands (settings, playback doctor, help)
        let open_commands = filtered_commands("open", &app);
        assert!(open_commands.len() > 1);

        // A query where only one token matches but not the other returns nothing
        let commands = filtered_commands("open zzz", &app);
        assert!(commands.is_empty());
    }

    #[test]
    fn leading_trailing_whitespace_is_trimmed() {
        let app = test_app();

        let trimmed = filtered_commands("search", &app);
        let with_whitespace = filtered_commands("  search  ", &app);

        assert_eq!(trimmed, with_whitespace);
        assert!(!trimmed.is_empty());
    }
}
