use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Clear};
use crate::app::App;
use super::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // Elegant config popup at 54% width, 52% height to prevent any row crowding
    let popup_area = super::centered_rect(54, 52, area);

    // Clear background
    frame.render_widget(Clear, popup_area);

    // Beautiful rounded block with glowing neon cyan border
    let block = Block::default()
        .title(Span::styled(" ✦ DriftFM Config Console ✦ ", theme::title()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::NEON_CYAN).add_modifier(Modifier::BOLD))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().bg(theme::BG));

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Layout the rows inside the block
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title spacer
            Constraint::Length(2), // Setting 1: Notifications
            Constraint::Length(2), // Setting 2: Autoplay
            Constraint::Length(2), // Setting 3: Recording directory
            Constraint::Length(2), // Setting 4: Keep Snippets
            Constraint::Min(0),    // Footer/help guide
        ])
        .split(inner_area);

    // Get active settings from library
    let notify_enabled = app.library.settings.notifications_enabled;
    let autoplay_enabled = app.library.settings.autoplay_last;
    let rec_dir = &app.library.settings.recording_dir;
    let keep_snippets = app.library.settings.keep_snippets;

    // Define highlight styles
    let active_style = Style::default().fg(theme::HOT_PINK).add_modifier(Modifier::BOLD);
    let normal_style = Style::default().fg(theme::text().fg.unwrap());
    let active_bg = Style::default().bg(theme::DEEP_PURPLE);

    // Row 1: Notifications
    let notify_spans = vec![
        Span::styled(
            if app.selected_setting_idx == 0 { " ▸  " } else { "    " },
            active_style
        ),
        Span::styled(
            if notify_enabled { "[ ▣ ] " } else { "[ ▢ ] " },
            Style::default().fg(if notify_enabled { theme::HOT_PINK } else { theme::dim().fg.unwrap() }).add_modifier(Modifier::BOLD)
        ),
        Span::styled("Desktop Song Notifications", if app.selected_setting_idx == 0 { active_style } else { normal_style }),
    ];
    let mut notify_para = Paragraph::new(Line::from(notify_spans));
    if app.selected_setting_idx == 0 {
        notify_para = notify_para.style(active_bg);
    }
    frame.render_widget(notify_para, chunks[1]);

    // Row 2: Autoplay last played on boot
    let autoplay_spans = vec![
        Span::styled(
            if app.selected_setting_idx == 1 { " ▸  " } else { "    " },
            active_style
        ),
        Span::styled(
            if autoplay_enabled { "[ ▣ ] " } else { "[ ▢ ] " },
            Style::default().fg(if autoplay_enabled { theme::HOT_PINK } else { theme::dim().fg.unwrap() }).add_modifier(Modifier::BOLD)
        ),
        Span::styled("Autoplay Last Played Station on Boot", if app.selected_setting_idx == 1 { active_style } else { normal_style }),
    ];
    let mut autoplay_para = Paragraph::new(Line::from(autoplay_spans));
    if app.selected_setting_idx == 1 {
        autoplay_para = autoplay_para.style(active_bg);
    }
    frame.render_widget(autoplay_para, chunks[2]);

    // Row 3: Recording Directory Preset Selector
    let rec_spans = vec![
        Span::styled(
            if app.selected_setting_idx == 2 { " ▸  " } else { "    " },
            active_style
        ),
        Span::styled(
            "[ 🗁 ] ",
            Style::default().fg(theme::NEON_CYAN).add_modifier(Modifier::BOLD)
        ),
        Span::styled("Tape Capture Folder: ", if app.selected_setting_idx == 2 { active_style } else { normal_style }),
        Span::styled(format!("{} ", rec_dir), Style::default().fg(theme::NEON_CYAN).add_modifier(Modifier::UNDERLINED).add_modifier(Modifier::BOLD)),
        Span::styled("(Press Space to cycle)", theme::dim()),
    ];
    let mut rec_para = Paragraph::new(Line::from(rec_spans));
    if app.selected_setting_idx == 2 {
        rec_para = rec_para.style(active_bg);
    }
    frame.render_widget(rec_para, chunks[3]);

    // Row 4: Keep Partial Snippets / Advertisements
    let snippets_spans = vec![
        Span::styled(
            if app.selected_setting_idx == 3 { " ▸  " } else { "    " },
            active_style
        ),
        Span::styled(
            if keep_snippets { "[ ▣ ] " } else { "[ ▢ ] " },
            Style::default().fg(if keep_snippets { theme::HOT_PINK } else { theme::dim().fg.unwrap() }).add_modifier(Modifier::BOLD)
        ),
        Span::styled("Keep Partial Snippets & Commercial Ads", if app.selected_setting_idx == 3 { active_style } else { normal_style }),
    ];
    let mut snippets_para = Paragraph::new(Line::from(snippets_spans));
    if app.selected_setting_idx == 3 {
        snippets_para = snippets_para.style(active_bg);
    }
    frame.render_widget(snippets_para, chunks[4]);

    // Footer instruction bar
    let footer_line = Line::from(vec![
        Span::styled("  j/k", Style::default().fg(theme::NEON_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled(" Navigate  •  ", theme::dim()),
        Span::styled("Space", Style::default().fg(theme::NEON_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled(" Toggle / Cycle  •  ", theme::dim()),
        Span::styled("Esc/,", Style::default().fg(theme::NEON_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled(" Exit Config", theme::dim()),
    ]);
    let footer = Paragraph::new(vec![Line::from(""), footer_line])
        .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[5]);
}
