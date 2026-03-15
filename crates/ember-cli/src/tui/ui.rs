//! UI rendering for the TUI using ratatui.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::{App, AppState};

/// Main render function
pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(5),    // Chat history
            Constraint::Length(3), // Input
            Constraint::Length(1), // Status bar
        ])
        .split(frame.size());

    render_chat(frame, app, chunks[0]);
    render_input(frame, app, chunks[1]);
    render_status_bar(frame, app, chunks[2]);

    // Show help overlay if in help state
    if matches!(app.state, AppState::Help) {
        render_help(frame);
    }
}

/// Render chat history
fn render_chat(frame: &mut Frame, app: &App, area: Rect) {
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|msg| {
            let style = if msg.role == "user" {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Green)
            };

            let role_prefix = if msg.role == "user" { "You" } else { "Ember" };

            // Wrap long messages
            let content = textwrap::wrap(&msg.content, area.width.saturating_sub(10) as usize);
            let mut lines: Vec<Line> = Vec::new();

            // First line with role
            if let Some(first) = content.first() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}: ", role_prefix),
                        style.add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(first.to_string()),
                ]));
            }

            // Remaining lines indented
            for line in content.iter().skip(1) {
                lines.push(Line::from(vec![
                    Span::raw("      "), // Indent
                    Span::raw(line.to_string()),
                ]));
            }

            // Add token count if available
            if let Some(tokens) = msg.tokens {
                lines.push(Line::from(vec![Span::styled(
                    format!("      [{} tokens]", tokens),
                    Style::default().fg(Color::DarkGray),
                )]));
            }

            lines.push(Line::from("")); // Spacing

            ListItem::new(Text::from(lines))
        })
        .collect();

    let chat = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            " Chat ",
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .style(Style::default().fg(Color::White));

    frame.render_widget(chat, area);
}

/// Render input field
fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_style = match app.state {
        AppState::Waiting => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::White),
    };

    let cursor_char = if matches!(app.state, AppState::Input) {
        "_"
    } else {
        ""
    };

    let input_text = if app.input.is_empty() && !matches!(app.state, AppState::Waiting) {
        "Type your message... (Enter to send, Esc to quit)".to_string()
    } else {
        format!("{}{}", app.input, cursor_char)
    };

    let input = Paragraph::new(input_text)
        .style(input_style)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            " Input ",
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .wrap(Wrap { trim: false });

    frame.render_widget(input, area);

    // Set cursor position
    if matches!(app.state, AppState::Input) && !app.input.is_empty() {
        frame.set_cursor(
            area.x + app.cursor as u16 + 1, // +1 for border
            area.y + 1,
        );
    }
}

/// Render status bar
fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status_style = if app.status.starts_with("Error") {
        Style::default().fg(Color::Red)
    } else if app.status == "Thinking..." {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    };

    let status_parts = vec![
        Span::styled(format!(" {} ", app.status), status_style),
        Span::raw(" | "),
        Span::styled(
            format!("Model: {} ", app.model),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("Tokens: {} ", app.total_tokens),
            Style::default().fg(Color::Magenta),
        ),
        Span::raw(" | "),
        Span::styled("Ctrl+H: Help ", Style::default().fg(Color::DarkGray)),
    ];

    let status =
        Paragraph::new(Line::from(status_parts)).style(Style::default().bg(Color::DarkGray));

    frame.render_widget(status, area);
}

/// Render help overlay
fn render_help(frame: &mut Frame) {
    let area = centered_rect(60, 50, frame.size());
    frame.render_widget(Clear, area);

    let help_text = vec![
        Line::from(Span::styled(
            "Ember TUI Help",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" - Send message"),
        ]),
        Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" - Clear input / Quit"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+C", Style::default().fg(Color::Yellow)),
            Span::raw(" - Quit immediately"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+L", Style::default().fg(Color::Yellow)),
            Span::raw(" - Clear chat history"),
        ]),
        Line::from(vec![
            Span::styled("Up/Down", Style::default().fg(Color::Yellow)),
            Span::raw(" - Scroll chat history"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .style(Style::default().bg(Color::DarkGray)),
        )
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));

    frame.render_widget(help, area);
}

/// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
