use crate::ui::model::UiModel;
use crate::ui::theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

pub(crate) fn render_oscilloscope(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    if area.width < 3 || area.height < 3 {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(super::visualizer_title(app), theme::title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    super::render_visualizer_signal(frame, inner, app);
}
