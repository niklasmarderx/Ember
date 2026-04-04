//! UI rendering for the TUI using ratatui.
//!
//! Professional, modern design with:
//! - Clean branded header with box-drawing separators
//! - Chat messages with role indicators and timestamps
//! - Tool call/result rendering with distinct styling
//! - Animated Braille spinner during AI processing
//! - Styled input with placeholder text and block cursor
//! - Rich status bar with session metrics
//! - Polished help overlay with agent mode info

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};

use super::app::{App, AppState};

// ─────────────────────────────────────────────────────────────────────────────
// Color palette — Ember brand colors
// ─────────────────────────────────────────────────────────────────────────────

/// Warm orange (Ember brand primary)
const EMBER_ORANGE: Color = Color::Rgb(255, 140, 50);
/// Softer amber accent
const EMBER_AMBER: Color = Color::Rgb(255, 183, 77);
/// Deep background for panels
const PANEL_BG: Color = Color::Rgb(22, 22, 30);
/// Slightly lighter panel surface
const SURFACE_BG: Color = Color::Rgb(30, 30, 42);
/// Muted border color
const BORDER_DIM: Color = Color::Rgb(60, 60, 80);
/// Active / focused border color
const BORDER_ACTIVE: Color = Color::Rgb(100, 100, 140);
/// User message accent
const USER_COLOR: Color = Color::Rgb(100, 180, 255);
/// Assistant message accent
const ASSISTANT_COLOR: Color = Color::Rgb(120, 220, 140);
/// Tool call accent (magenta/purple)
const TOOL_COLOR: Color = Color::Rgb(200, 140, 255);
/// Tool result success
const TOOL_SUCCESS: Color = Color::Rgb(80, 220, 120);
/// Tool result failure
const TOOL_FAIL: Color = Color::Rgb(255, 100, 100);
/// Muted text
const MUTED: Color = Color::Rgb(100, 100, 120);
/// Error red
const ERROR_RED: Color = Color::Rgb(255, 80, 80);
/// Success green
const SUCCESS_GREEN: Color = Color::Rgb(80, 220, 120);
/// Warning yellow
const THINKING_YELLOW: Color = Color::Rgb(255, 210, 70);
/// Subtle separator
const SEPARATOR: Color = Color::Rgb(45, 45, 60);

/// Animated spinner frames (Braille dots — smooth rotation)
const SPINNER_FRAMES: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

/// Pulsing bar animation for thinking state
const PULSE_FRAMES: &[&str] = &[
    "━         ",
    " ━        ",
    "  ━━      ",
    "   ━━━    ",
    "    ━━━━  ",
    "     ━━━━━",
    "      ━━━━",
    "       ━━━",
    "        ━━",
    "         ━",
    "        ━━",
    "       ━━━",
    "      ━━━━",
    "     ━━━━━",
    "    ━━━━  ",
    "   ━━━    ",
    "  ━━      ",
    " ━        ",
];

// ─────────────────────────────────────────────────────────────────────────────
// Main render
// ─────────────────────────────────────────────────────────────────────────────

/// Main render function — orchestrates all panels.
pub fn render(frame: &mut Frame, app: &App) {
    let size = frame.size();

    // Fill entire background
    frame.render_widget(Block::default().style(Style::default().bg(PANEL_BG)), size);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header bar
            Constraint::Min(6),    // Chat history
            Constraint::Length(4), // Input area
            Constraint::Length(1), // Status bar
        ])
        .split(size);

    render_header(frame, app, chunks[0]);
    render_chat(frame, app, chunks[1]);
    render_input(frame, app, chunks[2]);
    render_status_bar(frame, app, chunks[3]);

    // Show help overlay if in help state
    if matches!(app.state, AppState::Help) {
        render_help(frame, app);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Header
// ─────────────────────────────────────────────────────────────────────────────

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(SEPARATOR))
        .style(Style::default().bg(SURFACE_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build header: Logo + mode + model + session info
    let mode_label = if app.agent_mode { "AGENT" } else { "CHAT" };
    let mode_color = if app.agent_mode {
        TOOL_COLOR
    } else {
        EMBER_AMBER
    };

    let mut all_spans = vec![
        Span::styled(
            " EMBER",
            Style::default()
                .fg(EMBER_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " AI ",
            Style::default()
                .fg(EMBER_AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{}]", mode_label),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(SEPARATOR)),
        Span::styled(
            app.model.as_str(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(SEPARATOR)),
        Span::styled(
            format!("{} msgs", app.messages.len()),
            Style::default().fg(MUTED),
        ),
    ];

    // Show tool count in agent mode
    if app.agent_mode {
        all_spans.push(Span::styled(" │ ", Style::default().fg(SEPARATOR)));
        all_spans.push(Span::styled(
            format!("{} tools", app.tool_names.len()),
            Style::default().fg(TOOL_COLOR),
        ));
        if app.tool_call_count > 0 {
            all_spans.push(Span::styled(
                format!(" ({} used)", app.tool_call_count),
                Style::default().fg(MUTED),
            ));
        }
    }

    all_spans.push(Span::styled(" │ ", Style::default().fg(SEPARATOR)));
    all_spans.push(Span::styled(app.uptime(), Style::default().fg(MUTED)));

    let header_line = Paragraph::new(Line::from(all_spans))
        .style(Style::default().bg(SURFACE_BG))
        .alignment(Alignment::Left);

    frame.render_widget(header_line, inner);
}

// ─────────────────────────────────────────────────────────────────────────────
// Chat history
// ─────────────────────────────────────────────────────────────────────────────

fn render_chat(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(PANEL_BG));

    let inner = block.inner(area).inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    frame.render_widget(block, area);

    if app.messages.is_empty() {
        // Empty state — show welcome message
        render_welcome(frame, app, inner);
        return;
    }

    // Build chat lines
    let mut lines: Vec<Line> = Vec::new();
    let content_width = inner.width.saturating_sub(6) as usize;

    for msg in &app.messages {
        match msg.role.as_str() {
            "user" => {
                render_user_message(&mut lines, msg, content_width);
            }
            "assistant" => {
                render_assistant_message(&mut lines, msg, content_width);
            }
            "tool_call" => {
                render_tool_call_message(&mut lines, msg);
            }
            "tool_result" => {
                render_tool_result_message(&mut lines, msg, content_width);
            }
            _ => {}
        }

        // Separator between messages
        lines.push(Line::from(""));
    }

    // If currently waiting or executing tools, show animated indicator
    if matches!(app.state, AppState::Waiting) {
        let spinner_idx = (app.tick as usize / 3) % SPINNER_FRAMES.len();
        let pulse_idx = (app.tick as usize / 2) % PULSE_FRAMES.len();
        let spinner = SPINNER_FRAMES[spinner_idx];
        let pulse = PULSE_FRAMES[pulse_idx];

        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", spinner),
                Style::default().fg(THINKING_YELLOW),
            ),
            Span::styled(
                "thinking ",
                Style::default()
                    .fg(THINKING_YELLOW)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(pulse, Style::default().fg(EMBER_AMBER)),
        ]));
        lines.push(Line::from(""));
    } else if matches!(app.state, AppState::ExecutingTool) {
        let spinner_idx = (app.tick as usize / 2) % SPINNER_FRAMES.len();
        let spinner = SPINNER_FRAMES[spinner_idx];
        let tool_name = app.current_tool.as_deref().unwrap_or("tool");

        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", spinner), Style::default().fg(TOOL_COLOR)),
            Span::styled(
                format!("executing {} ", tool_name),
                Style::default()
                    .fg(TOOL_COLOR)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(
                "...",
                Style::default().fg(TOOL_COLOR).add_modifier(Modifier::DIM),
            ),
        ]));
        lines.push(Line::from(""));
    }

    let text = Text::from(lines);
    let total_lines = text.lines.len() as u16;

    // Auto-scroll to bottom
    let visible_height = inner.height;
    let scroll_offset = total_lines.saturating_sub(visible_height);

    let chat = Paragraph::new(text)
        .style(Style::default().bg(PANEL_BG).fg(Color::White))
        .scroll((scroll_offset, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(chat, inner);

    // Scrollbar
    if total_lines > visible_height {
        let mut scrollbar_state = ScrollbarState::new(total_lines as usize)
            .position(scroll_offset as usize)
            .viewport_content_length(visible_height as usize);

        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .style(Style::default().fg(BORDER_DIM)),
            area,
            &mut scrollbar_state,
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Message renderers
// ─────────────────────────────────────────────────────────────────────────────

fn render_user_message(lines: &mut Vec<Line>, msg: &super::app::ChatMessage, content_width: usize) {
    let elapsed = msg.timestamp.elapsed();
    let time_str = format_elapsed(elapsed);

    // Role header line
    lines.push(Line::from(vec![
        Span::styled(
            " >> ",
            Style::default().fg(USER_COLOR).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "you",
            Style::default().fg(USER_COLOR).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", time_str),
            Style::default().fg(MUTED).add_modifier(Modifier::DIM),
        ),
    ]));

    // Message content with vertical gutter
    let wrapped = textwrap::wrap(&msg.content, content_width);
    for line in &wrapped {
        lines.push(Line::from(vec![
            Span::styled(
                "    │ ",
                Style::default().fg(USER_COLOR).add_modifier(Modifier::DIM),
            ),
            Span::raw(line.to_string()),
        ]));
    }
}

fn render_assistant_message(
    lines: &mut Vec<Line>,
    msg: &super::app::ChatMessage,
    content_width: usize,
) {
    let elapsed = msg.timestamp.elapsed();
    let time_str = format_elapsed(elapsed);

    // Role header line
    lines.push(Line::from(vec![
        Span::styled(
            " << ",
            Style::default()
                .fg(ASSISTANT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "ember",
            Style::default()
                .fg(ASSISTANT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", time_str),
            Style::default().fg(MUTED).add_modifier(Modifier::DIM),
        ),
    ]));

    // Message content with vertical gutter
    let wrapped = textwrap::wrap(&msg.content, content_width);
    for line in &wrapped {
        lines.push(Line::from(vec![
            Span::styled(
                "    │ ",
                Style::default()
                    .fg(ASSISTANT_COLOR)
                    .add_modifier(Modifier::DIM),
            ),
            Span::raw(line.to_string()),
        ]));
    }

    // Token count badge
    if let Some(tokens) = msg.tokens {
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(
                format!("[{} tokens]", tokens),
                Style::default().fg(MUTED).add_modifier(Modifier::DIM),
            ),
        ]));
    }
}

fn render_tool_call_message(lines: &mut Vec<Line>, msg: &super::app::ChatMessage) {
    let tool_name = msg.tool_name.as_deref().unwrap_or("unknown");

    // Tool call header with distinctive marker
    lines.push(Line::from(vec![
        Span::styled(
            " -> ",
            Style::default().fg(TOOL_COLOR).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "tool",
            Style::default().fg(TOOL_COLOR).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", tool_name),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Show truncated args (one line, dimmed)
    if !msg.content.is_empty() {
        let display_args = if msg.content.len() > 80 {
            format!("{}...", &msg.content[..80])
        } else {
            msg.content.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                "    │ ",
                Style::default().fg(TOOL_COLOR).add_modifier(Modifier::DIM),
            ),
            Span::styled(
                display_args,
                Style::default().fg(MUTED).add_modifier(Modifier::DIM),
            ),
        ]));
    }
}

fn render_tool_result_message(
    lines: &mut Vec<Line>,
    msg: &super::app::ChatMessage,
    content_width: usize,
) {
    let tool_name = msg.tool_name.as_deref().unwrap_or("unknown");
    let success = msg.tool_success.unwrap_or(true);
    let result_color = if success { TOOL_SUCCESS } else { TOOL_FAIL };
    let status_label = if success { "ok" } else { "err" };

    // Result header
    lines.push(Line::from(vec![
        Span::styled(
            " <- ",
            Style::default()
                .fg(result_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "result",
            Style::default()
                .fg(result_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {} [{}]", tool_name, status_label),
            Style::default().fg(MUTED),
        ),
    ]));

    // Tool output — show first few lines with gutter
    if !msg.content.is_empty() {
        let output_lines: Vec<&str> = msg.content.lines().take(8).collect();
        let has_more = msg.content.lines().count() > 8;

        for line in &output_lines {
            let wrapped = textwrap::wrap(line, content_width);
            for wline in &wrapped {
                lines.push(Line::from(vec![
                    Span::styled(
                        "    │ ",
                        Style::default()
                            .fg(result_color)
                            .add_modifier(Modifier::DIM),
                    ),
                    Span::styled(
                        wline.to_string(),
                        Style::default()
                            .fg(if success { Color::White } else { TOOL_FAIL })
                            .add_modifier(Modifier::DIM),
                    ),
                ]));
            }
        }

        if has_more {
            lines.push(Line::from(vec![
                Span::styled(
                    "    │ ",
                    Style::default()
                        .fg(result_color)
                        .add_modifier(Modifier::DIM),
                ),
                Span::styled(
                    format!("... ({} more lines)", msg.content.lines().count() - 8),
                    Style::default().fg(MUTED).add_modifier(Modifier::DIM),
                ),
            ]));
        }
    }
}

/// Format elapsed duration as human-readable string.
fn format_elapsed(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

/// Render a welcome screen when no messages exist.
fn render_welcome(frame: &mut Frame, app: &App, area: Rect) {
    let center_y = area.height / 2;
    let welcome_area = Rect {
        x: area.x,
        y: area.y + center_y.saturating_sub(5),
        width: area.width,
        height: 10.min(area.height),
    };

    let mut welcome_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "EMBER",
                Style::default()
                    .fg(EMBER_ORANGE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " AI",
                Style::default()
                    .fg(EMBER_AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    if app.agent_mode {
        welcome_lines.push(Line::from(Span::styled(
            format!(
                "Agent mode active -- {} tools available",
                app.tool_names.len()
            ),
            Style::default().fg(TOOL_COLOR),
        )));
        // Show tool names
        let tools_str = app.tool_names.join(", ");
        welcome_lines.push(Line::from(Span::styled(
            tools_str,
            Style::default().fg(MUTED).add_modifier(Modifier::DIM),
        )));
        welcome_lines.push(Line::from(""));
        welcome_lines.push(Line::from(Span::styled(
            "Ask Ember to read, write, or edit files, run commands, and more.",
            Style::default().fg(MUTED),
        )));
    } else {
        welcome_lines.push(Line::from(Span::styled(
            "Start typing to begin a conversation.",
            Style::default().fg(MUTED),
        )));
    }

    welcome_lines.push(Line::from(Span::styled(
        "Ctrl+H  help   ·   Ctrl+C  quit",
        Style::default().fg(MUTED).add_modifier(Modifier::DIM),
    )));
    welcome_lines.push(Line::from(""));

    let welcome = Paragraph::new(welcome_lines)
        .style(Style::default().bg(PANEL_BG))
        .alignment(Alignment::Center);

    frame.render_widget(welcome, welcome_area);
}

// ─────────────────────────────────────────────────────────────────────────────
// Input field
// ─────────────────────────────────────────────────────────────────────────────

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let is_busy = matches!(app.state, AppState::Waiting | AppState::ExecutingTool);

    let border_color = if is_busy { BORDER_DIM } else { BORDER_ACTIVE };

    let title = if matches!(app.state, AppState::ExecutingTool) {
        " executing tool... "
    } else if matches!(app.state, AppState::Waiting) {
        " waiting... "
    } else {
        " message "
    };

    let title_color = if is_busy { MUTED } else { EMBER_AMBER };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(SURFACE_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input_style = if is_busy {
        Style::default().fg(MUTED).bg(SURFACE_BG)
    } else {
        Style::default().fg(Color::White).bg(SURFACE_BG)
    };

    let display_text = if app.input.is_empty() && !is_busy {
        Text::from(Line::from(Span::styled(
            "Type your message here... (Enter to send)",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        )))
    } else {
        // Show input with a visible cursor block
        let before = &app.input[..app.cursor];
        let cursor_char = app.input.get(app.cursor..app.cursor + 1).unwrap_or(" ");
        let after = if app.cursor < app.input.len() {
            &app.input[app.cursor + 1..]
        } else {
            ""
        };

        Text::from(Line::from(vec![
            Span::raw(before),
            Span::styled(
                cursor_char,
                Style::default()
                    .bg(if is_busy { MUTED } else { EMBER_AMBER })
                    .fg(PANEL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(after),
        ]))
    };

    let input = Paragraph::new(display_text)
        .style(input_style)
        .wrap(Wrap { trim: false });

    frame.render_widget(input, inner);
}

// ─────────────────────────────────────────────────────────────────────────────
// Status bar
// ─────────────────────────────────────────────────────────────────────────────

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status_icon;
    let status_color;
    let status_text: String;

    if app.status.starts_with("Error") {
        status_icon = "×";
        status_color = ERROR_RED;
        status_text = app.status.clone();
    } else if app.status.starts_with("Executing") {
        let spinner_idx = (app.tick as usize / 2) % SPINNER_FRAMES.len();
        status_icon = SPINNER_FRAMES[spinner_idx];
        status_color = TOOL_COLOR;
        status_text = app.status.clone();
    } else if app.status.starts_with("Tool iteration") {
        let spinner_idx = (app.tick as usize / 3) % SPINNER_FRAMES.len();
        status_icon = SPINNER_FRAMES[spinner_idx];
        status_color = TOOL_COLOR;
        status_text = app.status.clone();
    } else if app.status == "Thinking..." {
        let spinner_idx = (app.tick as usize / 3) % SPINNER_FRAMES.len();
        status_icon = SPINNER_FRAMES[spinner_idx];
        status_color = THINKING_YELLOW;
        status_text = "processing".to_string();
    } else if app.status == "Cleared" {
        status_icon = "~";
        status_color = MUTED;
        status_text = "history cleared".to_string();
    } else {
        status_icon = "●";
        status_color = SUCCESS_GREEN;
        status_text = app.status.clone();
    };

    let left_parts = vec![
        Span::styled(
            format!(" {} ", status_icon),
            Style::default().fg(status_color),
        ),
        Span::styled(
            status_text.as_str(),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    let right_parts = if app.agent_mode {
        format!(
            "tokens: {}  │  tools: {}  │  Ctrl+H help  │  Ctrl+C quit ",
            app.total_tokens, app.tool_call_count,
        )
    } else {
        format!(
            "tokens: {}  │  Ctrl+H help  │  Ctrl+C quit ",
            app.total_tokens,
        )
    };

    // Calculate available space for right-alignment
    let left_len: usize = left_parts.iter().map(|s| s.width()).sum();
    let right_len = right_parts.len();
    let padding = (area.width as usize).saturating_sub(left_len + right_len);

    let mut spans = left_parts;
    spans.push(Span::raw(" ".repeat(padding)));
    spans.push(Span::styled(right_parts, Style::default().fg(MUTED)));

    let status = Paragraph::new(Line::from(spans)).style(Style::default().bg(SURFACE_BG).fg(MUTED));

    frame.render_widget(status, area);
}

// ─────────────────────────────────────────────────────────────────────────────
// Help overlay
// ─────────────────────────────────────────────────────────────────────────────

fn render_help(frame: &mut Frame, app: &App) {
    let area = centered_rect(55, 70, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(EMBER_AMBER))
        .title(Span::styled(
            " Ember -- Help ",
            Style::default()
                .fg(EMBER_ORANGE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(SURFACE_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let divider_width = inner.width.saturating_sub(2) as usize;
    let divider = "─".repeat(divider_width);

    let mut help_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Keyboard Shortcuts",
            Style::default()
                .fg(EMBER_AMBER)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", divider),
            Style::default().fg(SEPARATOR),
        )),
        Line::from(""),
        help_line("  Enter       ", "Send message"),
        help_line("  Esc         ", "Clear input / Quit"),
        help_line("  Ctrl+C      ", "Quit immediately"),
        help_line("  Ctrl+L      ", "Clear chat history"),
        help_line("  Ctrl+H      ", "Toggle this help"),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", divider),
            Style::default().fg(SEPARATOR),
        )),
        Line::from(""),
        help_line("  Up / Down   ", "Scroll chat history"),
        help_line("  Left / Right", "Move cursor in input"),
        help_line("  Home / End  ", "Jump to start/end"),
    ];

    // Agent mode section
    if app.agent_mode {
        help_text.push(Line::from(""));
        help_text.push(Line::from(Span::styled(
            format!("  {}", divider),
            Style::default().fg(SEPARATOR),
        )));
        help_text.push(Line::from(""));
        help_text.push(Line::from(Span::styled(
            "  Agent Mode",
            Style::default().fg(TOOL_COLOR).add_modifier(Modifier::BOLD),
        )));
        help_text.push(Line::from(""));
        help_text.push(Line::from(Span::styled(
            "  Ember can execute tools automatically:",
            Style::default().fg(MUTED),
        )));

        // List available tools
        for name in &app.tool_names {
            help_text.push(Line::from(vec![
                Span::styled("    -> ", Style::default().fg(TOOL_COLOR)),
                Span::styled(name.clone(), Style::default().fg(Color::White)),
            ]));
        }

        help_text.push(Line::from(""));
        help_text.push(Line::from(Span::styled(
            "  Ask Ember to: read files, write code, run",
            Style::default().fg(MUTED),
        )));
        help_text.push(Line::from(Span::styled(
            "  shell commands, search with grep, and more.",
            Style::default().fg(MUTED),
        )));
    }

    help_text.push(Line::from(""));
    help_text.push(Line::from(Span::styled(
        format!("  {}", divider),
        Style::default().fg(SEPARATOR),
    )));
    help_text.push(Line::from(""));
    help_text.push(Line::from(vec![
        Span::styled("  Press ", Style::default().fg(MUTED)),
        Span::styled(
            "any key",
            Style::default()
                .fg(EMBER_AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" to close", Style::default().fg(MUTED)),
    ]));
    help_text.push(Line::from(""));

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::White).bg(SURFACE_BG))
        .wrap(Wrap { trim: false });

    frame.render_widget(help, inner);
}

/// Build a single help line with key + description styling.
fn help_line(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            key.to_string(),
            Style::default()
                .fg(EMBER_AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {}", desc), Style::default().fg(Color::White)),
    ])
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Helper function to create a centered rect.
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
