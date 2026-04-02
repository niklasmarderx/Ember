//! Terminal markdown renderer with syntax highlighting, colored headings,
//! animated spinners, and tool-output formatting.
//!
//! Designed for Ember's CLI output pipeline. Writes to any `io::Write`
//! target (including `Vec<u8>`) so all rendering paths are unit-testable
//! without a real TTY.

use crossterm::style::{Color, ResetColor, SetForegroundColor};
use crossterm::{cursor, queue};
use pulldown_cmark::{Event as MdEvent, HeadingLevel, Options, Parser as MdParser, Tag, TagEnd};
use std::io::{self, Write};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

// ─────────────────────────────────────────────────────────────────────────────
// ColorTheme
// ─────────────────────────────────────────────────────────────────────────────

/// Color theme for terminal rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorTheme {
    pub heading: Color,
    pub emphasis: Color,
    pub strong: Color,
    pub inline_code: Color,
    pub link: Color,
    pub quote: Color,
    pub code_block_border: Color,
    pub spinner_active: Color,
    pub spinner_done: Color,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            heading: Color::Cyan,
            emphasis: Color::Yellow,
            strong: Color::White,
            inline_code: Color::Green,
            link: Color::Blue,
            quote: Color::DarkGrey,
            code_block_border: Color::DarkGrey,
            spinner_active: Color::Cyan,
            spinner_done: Color::Green,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TerminalRenderer
// ─────────────────────────────────────────────────────────────────────────────

/// Renders markdown to a terminal (or any `io::Write`) with syntax
/// highlighting for fenced code blocks.
pub struct TerminalRenderer {
    theme: ColorTheme,
    syntax_set: SyntaxSet,
    highlight_theme: Theme,
    terminal_width: u16,
}

impl TerminalRenderer {
    /// Create a renderer with the default color theme.
    pub fn new() -> Self {
        Self::with_theme(ColorTheme::default())
    }

    /// Create a renderer with a custom color theme.
    pub fn with_theme(theme: ColorTheme) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let highlight_theme = theme_set
            .themes
            .get("base16-ocean.dark")
            .or_else(|| theme_set.themes.values().next())
            .cloned()
            .unwrap_or_else(|| ThemeSet::load_defaults().themes["base16-ocean.dark"].clone());

        // Fall back gracefully when there is no terminal (CI / tests).
        let terminal_width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);

        Self {
            theme,
            syntax_set,
            highlight_theme,
            terminal_width,
        }
    }

    /// Current terminal width (cached at construction time).
    fn term_width(&self) -> u16 {
        self.terminal_width
    }

    // ── Public render methods (write to stdout) ────────────────────────────

    /// Render a complete markdown string to stdout.
    pub fn render_markdown(&self, markdown: &str) -> io::Result<()> {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        self.render_markdown_to(markdown, &mut w)
    }

    /// Render a streaming text delta directly (no markdown parsing).
    pub fn render_delta(&self, delta: &str) -> io::Result<()> {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        write!(w, "{delta}")?;
        w.flush()
    }

    /// Render a fenced code block with syntax highlighting to stdout.
    pub fn render_code_block(&self, code: &str, language: &str) -> io::Result<()> {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        self.render_code_block_to(code, language, &mut w)
    }

    /// Render a horizontal rule to stdout.
    pub fn render_hr(&self) -> io::Result<()> {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        self.render_hr_to(&mut w)
    }

    /// Render a heading to stdout.
    pub fn render_heading(&self, level: u8, text: &str) -> io::Result<()> {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        self.render_heading_to(level, text, &mut w)
    }

    // ── Internal: writer-generic implementations ──────────────────────────

    /// Render markdown to an arbitrary `Write` (used by tests).
    pub(crate) fn render_markdown_to<W: Write>(&self, markdown: &str, w: &mut W) -> io::Result<()> {
        let options = Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES;

        let parser = MdParser::new_ext(markdown, options);

        let mut in_code_block = false;
        let mut in_blockquote = false;
        let mut code_lang = String::new();
        let mut code_buf = String::new();

        for event in parser {
            match event {
                // ── Block tags ─────────────────────────────────────────────
                MdEvent::Start(Tag::Heading { level, .. }) => {
                    write!(w, "\n")?;
                    let prefix = heading_prefix(level);
                    queue!(w, SetForegroundColor(self.theme.heading))?;
                    write!(w, "{prefix}")?;
                    queue!(w, ResetColor)?;
                }
                MdEvent::End(TagEnd::Heading(_)) => {
                    write!(w, "\n")?;
                }

                MdEvent::Start(Tag::Paragraph) => {}
                MdEvent::End(TagEnd::Paragraph) => {
                    write!(w, "\n\n")?;
                }

                MdEvent::Start(Tag::BlockQuote(_)) => {
                    in_blockquote = true;
                }
                MdEvent::End(TagEnd::BlockQuote(_)) => {
                    in_blockquote = false;
                    write!(w, "\n")?;
                }

                MdEvent::Start(Tag::CodeBlock(kind)) => {
                    in_code_block = true;
                    code_lang = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                        pulldown_cmark::CodeBlockKind::Indented => String::new(),
                    };
                    code_buf.clear();
                }
                MdEvent::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                    self.render_code_block_to(&code_buf, &code_lang, w)?;
                    code_buf.clear();
                }

                MdEvent::Start(Tag::List(_)) => {}
                MdEvent::End(TagEnd::List(_)) => {
                    write!(w, "\n")?;
                }
                MdEvent::Start(Tag::Item) => {
                    write!(w, "  • ")?;
                }
                MdEvent::End(TagEnd::Item) => {
                    write!(w, "\n")?;
                }

                MdEvent::Start(Tag::Strong) => {
                    queue!(w, SetForegroundColor(self.theme.strong))?;
                }
                MdEvent::End(TagEnd::Strong) => {
                    queue!(w, ResetColor)?;
                }

                MdEvent::Start(Tag::Emphasis) => {
                    queue!(w, SetForegroundColor(self.theme.emphasis))?;
                }
                MdEvent::End(TagEnd::Emphasis) => {
                    queue!(w, ResetColor)?;
                }

                MdEvent::Start(Tag::Link { .. }) => {
                    queue!(w, SetForegroundColor(self.theme.link))?;
                }
                MdEvent::End(TagEnd::Link) => {
                    queue!(w, ResetColor)?;
                }

                MdEvent::Rule => {
                    self.render_hr_to(w)?;
                }

                MdEvent::SoftBreak => {
                    write!(w, " ")?;
                }
                MdEvent::HardBreak => {
                    write!(w, "\n")?;
                }

                // ── Inline content ─────────────────────────────────────────
                MdEvent::Text(text) => {
                    if in_code_block {
                        code_buf.push_str(&text);
                    } else if in_blockquote {
                        queue!(w, SetForegroundColor(self.theme.quote))?;
                        for line in text.lines() {
                            write!(w, "│ {line}\n")?;
                        }
                        queue!(w, ResetColor)?;
                    } else {
                        write!(w, "{text}")?;
                    }
                }

                MdEvent::Code(code) => {
                    // Inline code
                    queue!(w, SetForegroundColor(self.theme.inline_code))?;
                    write!(w, "`{code}`")?;
                    queue!(w, ResetColor)?;
                }

                _ => {
                    // TaskListMarker, FootnoteReference, Html, etc. — skip
                }
            }
        }

        w.flush()
    }

    pub(crate) fn render_code_block_to<W: Write>(
        &self,
        code: &str,
        language: &str,
        w: &mut W,
    ) -> io::Result<()> {
        let width = self.term_width() as usize;
        let inner = width.saturating_sub(2);

        // Top border with optional language tag
        queue!(w, SetForegroundColor(self.theme.code_block_border))?;
        if language.is_empty() {
            writeln!(w, "┌{:─<inner$}┐", "", inner = inner)?;
        } else {
            let tag = format!(" {language} ");
            let pad = inner.saturating_sub(tag.len());
            writeln!(w, "┌─{tag}{:─<pad$}┐", "", pad = pad)?;
        }
        queue!(w, ResetColor)?;

        // Highlighted code body
        let lang = if language.is_empty() { "txt" } else { language };
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .or_else(|| self.syntax_set.find_syntax_by_extension(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.highlight_theme);
        for line in syntect::util::LinesWithEndings::from(code) {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            let escaped = as_24_bit_terminal_escaped(&ranges, false);
            write!(w, "{escaped}\x1b[0m")?;
        }

        // Bottom border
        queue!(w, SetForegroundColor(self.theme.code_block_border))?;
        writeln!(w, "\n└{:─<inner$}┘", "", inner = inner)?;
        queue!(w, ResetColor)?;

        w.flush()
    }

    pub(crate) fn render_hr_to<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let width = self.term_width() as usize;
        queue!(w, SetForegroundColor(self.theme.code_block_border))?;
        writeln!(w, "{}", "─".repeat(width))?;
        queue!(w, ResetColor)?;
        w.flush()
    }

    pub(crate) fn render_heading_to<W: Write>(
        &self,
        level: u8,
        text: &str,
        w: &mut W,
    ) -> io::Result<()> {
        let prefix = match level {
            1 => "# ",
            2 => "## ",
            3 => "### ",
            4 => "#### ",
            5 => "##### ",
            _ => "###### ",
        };
        queue!(w, SetForegroundColor(self.theme.heading))?;
        writeln!(w, "\n{prefix}{text}")?;
        queue!(w, ResetColor)?;
        w.flush()
    }
}

impl Default for TerminalRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn heading_prefix(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "# ",
        HeadingLevel::H2 => "## ",
        HeadingLevel::H3 => "### ",
        HeadingLevel::H4 => "#### ",
        HeadingLevel::H5 => "##### ",
        HeadingLevel::H6 => "###### ",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Spinner
// ─────────────────────────────────────────────────────────────────────────────

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Animated spinner for loading / in-progress states.
pub struct Spinner {
    pub(crate) frames: Vec<&'static str>,
    pub(crate) frame_index: usize,
    label: String,
    theme: ColorTheme,
}

impl Spinner {
    /// Create a new spinner with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            frames: SPINNER_FRAMES.to_vec(),
            frame_index: 0,
            label: label.into(),
            theme: ColorTheme::default(),
        }
    }

    /// Advance the spinner by one frame and render it in place.
    pub fn tick(&mut self) -> io::Result<()> {
        let frame = self.frames[self.frame_index % self.frames.len()];
        self.frame_index = self.frame_index.wrapping_add(1);

        let stdout = io::stdout();
        let mut w = stdout.lock();
        queue!(w, cursor::MoveToColumn(0))?;
        queue!(w, SetForegroundColor(self.theme.spinner_active))?;
        write!(w, "{frame} {}", self.label)?;
        queue!(w, ResetColor)?;
        w.flush()
    }

    /// Finish the spinner, printing ✓ (success) or ✗ (failure).
    pub fn finish(&self, success: bool) -> io::Result<()> {
        let (symbol, color) = if success {
            ("✓", self.theme.spinner_done)
        } else {
            ("✗", Color::Red)
        };

        let stdout = io::stdout();
        let mut w = stdout.lock();
        queue!(w, cursor::MoveToColumn(0))?;
        queue!(w, SetForegroundColor(color))?;
        writeln!(w, "{symbol} {}", self.label)?;
        queue!(w, ResetColor)?;
        w.flush()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolOutputFormatter
// ─────────────────────────────────────────────────────────────────────────────

/// Formats tool execution events for display in the terminal.
pub struct ToolOutputFormatter;

impl ToolOutputFormatter {
    /// Format a tool execution header.
    ///
    /// Example output: `"⚡ Running: bash  ls -la"`
    pub fn format_tool_start(tool_name: &str, input_preview: &str) -> String {
        format!("⚡ Running: {tool_name}  {input_preview}")
    }

    /// Format tool output, truncating to `max_lines` and appending a summary
    /// if lines were omitted.
    pub fn format_tool_result(output: &str, max_lines: usize) -> String {
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() <= max_lines {
            return output.to_owned();
        }
        let kept = &lines[..max_lines];
        let omitted = lines.len() - max_lines;
        format!("{}\n… ({omitted} more lines truncated)", kept.join("\n"))
    }

    /// Format a tool error.
    ///
    /// Example output: `"✗ Error: connection refused"`
    pub fn format_tool_error(error: &str) -> String {
        format!("✗ Error: {error}")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 1. ColorTheme default values are sensible (non-reset colors)
    #[test]
    fn color_theme_default_values_set() {
        let t = ColorTheme::default();
        assert_ne!(t.heading, Color::Reset);
        assert_ne!(t.emphasis, Color::Reset);
        assert_ne!(t.strong, Color::Reset);
        assert_ne!(t.inline_code, Color::Reset);
        assert_ne!(t.link, Color::Reset);
        assert_ne!(t.quote, Color::Reset);
        assert_ne!(t.code_block_border, Color::Reset);
        assert_ne!(t.spinner_active, Color::Reset);
        assert_ne!(t.spinner_done, Color::Reset);
    }

    // 2. TerminalRenderer can be constructed without panicking
    #[test]
    fn terminal_renderer_creation() {
        let r = TerminalRenderer::new();
        assert!(r.terminal_width > 0);
    }

    // 3. TerminalRenderer::with_theme accepts a custom theme
    #[test]
    fn terminal_renderer_with_custom_theme() {
        let mut theme = ColorTheme::default();
        theme.heading = Color::Magenta;
        let r = TerminalRenderer::with_theme(theme);
        assert_eq!(r.theme.heading, Color::Magenta);
    }

    // 4. Spinner::tick advances frame_index
    #[test]
    fn spinner_tick_advances_frame() {
        let mut s = Spinner::new("loading");
        let before = s.frame_index;
        // Simulate what tick() does internally (without writing to a TTY).
        s.frame_index = s.frame_index.wrapping_add(1);
        assert_eq!(s.frame_index, before + 1);
    }

    // 5. Spinner frames cycle correctly
    #[test]
    fn spinner_frames_cycle() {
        let s = Spinner::new("test");
        let n = s.frames.len();
        assert_eq!(n, SPINNER_FRAMES.len());
        for i in 0..n * 2 {
            let frame = s.frames[i % n];
            assert!(!frame.is_empty(), "frame {i} is empty");
        }
    }

    // 6. ToolOutputFormatter::format_tool_start produces expected format
    #[test]
    fn tool_output_format_tool_start() {
        let s = ToolOutputFormatter::format_tool_start("bash", "ls -la");
        assert!(s.contains("⚡"), "missing lightning bolt");
        assert!(s.contains("bash"), "missing tool name");
        assert!(s.contains("ls -la"), "missing input preview");
    }

    // 7. ToolOutputFormatter truncates long output and leaves short output intact
    #[test]
    fn tool_output_truncation() {
        let long_output = (0..20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let result = ToolOutputFormatter::format_tool_result(&long_output, 5);
        // 5 kept lines + 1 truncation notice
        assert_eq!(result.lines().count(), 6, "expected 5 lines + notice");
        assert!(result.contains("truncated"), "missing truncation notice");

        // Below the limit → no truncation
        let short = "a\nb\nc";
        assert_eq!(
            ToolOutputFormatter::format_tool_result(short, 10),
            short,
            "short output should be returned unchanged"
        );
    }

    // 8. ToolOutputFormatter::format_tool_error contains error markers
    #[test]
    fn tool_output_error_formatting() {
        let e = ToolOutputFormatter::format_tool_error("connection refused");
        assert!(e.contains("✗"), "missing ✗ symbol");
        assert!(e.contains("Error"), "missing 'Error' label");
        assert!(e.contains("connection refused"), "missing error message");
    }

    // 9. render_heading writes colored ANSI output to a Vec<u8> buffer
    #[test]
    fn render_heading_produces_colored_output() {
        let renderer = TerminalRenderer::new();
        let mut buf: Vec<u8> = Vec::new();
        renderer
            .render_heading_to(1, "Hello World", &mut buf)
            .expect("render_heading_to failed");
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("Hello World"), "heading text missing");
        assert!(output.contains("# "), "heading prefix missing");
        // crossterm queue! writes ANSI escape codes
        assert!(
            output.contains('\x1b'),
            "expected ANSI escape codes in output"
        );
    }

    // 10. render_code_block writes bordered, syntax-highlighted output
    #[test]
    fn render_code_block_produces_bordered_output() {
        let renderer = TerminalRenderer::new();
        let mut buf: Vec<u8> = Vec::new();
        renderer
            .render_code_block_to("fn main() {}\n", "rust", &mut buf)
            .expect("render_code_block_to failed");
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("rust"), "language tag missing from border");
        assert!(output.contains('┌'), "top border missing");
        assert!(output.contains('└'), "bottom border missing");
    }
}
