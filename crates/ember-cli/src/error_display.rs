//! User-friendly error display for the CLI
//!
//! This module provides beautiful, helpful error messages for terminal output.
//! Each error includes:
//! - Error code for easy troubleshooting
//! - Clear description of what went wrong
//! - Actionable suggestions to fix the issue
//! - Links to documentation

use colored::Colorize;

/// Display an error message with helpful suggestions
pub fn display_error(error: &anyhow::Error) {
    // Check if this is an LLM error with user message support
    if let Some(llm_err) = error.downcast_ref::<ember_llm::Error>() {
        display_llm_error(llm_err);
        return;
    }

    // Check if this is a core error
    if let Some(core_err) = error.downcast_ref::<ember_core::Error>() {
        display_core_error(core_err);
        return;
    }

    // Generic error display
    display_generic_error(error);
}

/// Display an LLM-specific error with rich formatting
fn display_llm_error(error: &ember_llm::Error) {
    let error_code = error.error_code();
    let title = error.title();
    let user_msg = error.user_message();

    // Print the formatted error header
    eprintln!();
    eprintln!("{}", "━".repeat(60).bright_red());
    eprintln!(
        "{} {} {}",
        format!(" {} ", error_code).bright_white().on_red().bold(),
        title.bright_white().bold(),
        "".bright_red()
    );
    eprintln!("{}", "━".repeat(60).bright_red());
    eprintln!();

    // Print user-friendly message with preserved formatting
    for line in user_msg.lines() {
        if line.starts_with("💡")
            || line.starts_with("📖")
            || line.starts_with("⏱️")
            || line.starts_with("🔑")
            || line.starts_with("🔌")
            || line.starts_with("⚠️")
            || line.starts_with("❌")
            || line.starts_with("📏")
            || line.starts_with("🌐")
            || line.starts_with("⚙️")
            || line.starts_with("📡")
            || line.starts_with("🔧")
            || line.starts_with("📄")
        {
            eprintln!("  {}", line.bright_cyan());
        } else if line.starts_with("  •")
            || line.starts_with("  1.")
            || line.starts_with("  2.")
            || line.starts_with("  3.")
        {
            eprintln!("  {}", line.bright_white());
        } else if line.contains("export ")
            || line.contains("ember ")
            || line.contains("ollama ")
            || line.contains("https://")
        {
            eprintln!("    {}", line.bright_green());
        } else if line.starts_with("Suggestions:")
            || line.starts_with("Options:")
            || line.starts_with("Solutions:")
            || line.starts_with("Troubleshooting")
            || line.starts_with("If this persists")
            || line.starts_with("Popular models")
        {
            eprintln!("  {}", line.bright_yellow());
        } else {
            eprintln!("  {}", line);
        }
    }

    eprintln!();

    // Add recovery suggestions if available
    let suggestions = error.recovery_suggestions();
    if !suggestions.is_empty() {
        eprintln!("{}", " 💡 Quick Actions ".bright_white().on_blue().bold());
        eprintln!();
        for (i, suggestion) in suggestions.iter().enumerate() {
            eprintln!(
                "  {}. {}",
                (i + 1).to_string().bright_blue(),
                suggestion.bright_white()
            );
        }
        eprintln!();
    }

    // Add documentation link
    eprintln!(
        "  {} {}",
        "📚 Documentation:".bright_cyan(),
        error_code.doc_url().bright_blue().underline()
    );
    eprintln!();
    eprintln!("{}", "━".repeat(60).bright_red());
    eprintln!();
}

/// Display a core error
fn display_core_error(error: &ember_core::Error) {
    // If it's a wrapped LLM error, display it with LLM formatting
    if let ember_core::Error::Llm(llm_err) = error {
        display_llm_error(llm_err);
        return;
    }

    eprintln!();
    eprintln!("{}", "━".repeat(60).bright_red());

    let (code, title) = get_core_error_info(error);
    eprintln!(
        "{} {} {}",
        format!(" {} ", code).bright_white().on_red().bold(),
        title.bright_white().bold(),
        "".bright_red()
    );
    eprintln!("{}", "━".repeat(60).bright_red());
    eprintln!();

    match error {
        ember_core::Error::Llm(_) => unreachable!(), // Handled above
        ember_core::Error::ToolExecution { tool, message } => {
            eprintln!(
                "  {} Tool '{}' failed to execute",
                "🔧".bright_red(),
                tool.bright_yellow()
            );
            eprintln!();
            eprintln!("  {}", "Error details:".bright_cyan());
            eprintln!("    {}", message.bright_white());
            eprintln!();
            eprintln!("  {}", "Suggestions:".bright_yellow());
            eprintln!("    1. Check the tool configuration");
            eprintln!("    2. Ensure required permissions are granted");
            eprintln!(
                "    3. Try running without the tool: {}",
                "ember chat --tools \"\"".bright_green()
            );
        }
        ember_core::Error::ContextOverflow { current, max } => {
            eprintln!("  {} Context window exceeded", "📏".bright_red());
            eprintln!();
            eprintln!("  Current: {} tokens", current.to_string().bright_yellow());
            eprintln!("  Maximum: {} tokens", max.to_string().bright_green());
            eprintln!();
            eprintln!("  {}", "Solutions:".bright_yellow());
            eprintln!("    1. Shorten your message");
            eprintln!("    2. Clear conversation: {}", "/clear".bright_green());
            eprintln!("    3. Use a model with larger context window");
        }
        ember_core::Error::Timeout { seconds } => {
            eprintln!(
                "  {} Operation timed out after {} seconds",
                "⏱️".bright_red(),
                seconds.to_string().bright_yellow()
            );
            eprintln!();
            eprintln!("  {}", "Suggestions:".bright_yellow());
            eprintln!("    1. Try a simpler request");
            eprintln!(
                "    2. Use a faster provider (e.g., {})",
                "Groq".bright_green()
            );
            eprintln!("    3. Check your network connection");
        }
        ember_core::Error::ConversationNotFound(id) => {
            eprintln!("  {} Conversation not found", "❌".bright_red());
            eprintln!();
            eprintln!("  ID: {}", id.to_string().bright_yellow());
            eprintln!();
            eprintln!("  The conversation may have been deleted or expired.");
            eprintln!(
                "  Start a new conversation with: {}",
                "ember chat".bright_green()
            );
        }
        ember_core::Error::Config(msg) | ember_core::Error::Configuration(msg) => {
            eprintln!("  {} Configuration error", "⚙️".bright_red());
            eprintln!();
            eprintln!("  {}", msg.bright_white());
            eprintln!();
            eprintln!("  {}", "Actions:".bright_yellow());
            eprintln!("    1. Run: {}", "ember config show".bright_green());
            eprintln!("    2. Run: {}", "ember config init".bright_green());
        }
        ember_core::Error::Memory(msg) => {
            eprintln!("  {} Memory operation failed", "💾".bright_red());
            eprintln!();
            eprintln!("  {}", msg.bright_white());
            eprintln!();
            eprintln!("  This is usually a temporary issue. Please try again.");
        }
        ember_core::Error::NotInitialized(msg) => {
            eprintln!("  {} Agent not initialized", "⚠️".bright_red());
            eprintln!();
            eprintln!("  {}", msg.bright_white());
            eprintln!();
            eprintln!(
                "  Run {} to initialize.",
                "ember config init".bright_green()
            );
        }
        ember_core::Error::LoopLimitExceeded { iterations } => {
            eprintln!(
                "  {} Agent loop limit exceeded ({} iterations)",
                "🔄".bright_red(),
                iterations.to_string().bright_yellow()
            );
            eprintln!();
            eprintln!("  The agent got stuck in a loop and was stopped.");
            eprintln!("  Try rephrasing your request or breaking it into smaller steps.");
        }
        ember_core::Error::Io(e) => {
            eprintln!("  {} I/O error: {}", "📁".bright_red(), e);
            eprintln!();
            eprintln!("  Check file permissions and disk space.");
        }
        _ => {
            eprintln!("  {}", error.to_string().bright_white());
        }
    }

    eprintln!();
    eprintln!(
        "  {} {}",
        "📚 Documentation:".bright_cyan(),
        format!("https://docs.ember.dev/errors/{}", code.to_lowercase())
            .bright_blue()
            .underline()
    );
    eprintln!();
    eprintln!("{}", "━".repeat(60).bright_red());
    eprintln!();
}

/// Get error code and title for core errors
fn get_core_error_info(error: &ember_core::Error) -> (&'static str, &'static str) {
    match error {
        ember_core::Error::Llm(_) => ("E000", "LLM Error"),
        ember_core::Error::Config(_) | ember_core::Error::Configuration(_) => {
            ("E500", "Configuration Error")
        }
        ember_core::Error::NotInitialized(_) => ("E501", "Not Initialized"),
        ember_core::Error::ToolExecution { .. } => ("E304", "Tool Execution Error"),
        ember_core::Error::ContextOverflow { .. } => ("E303", "Context Overflow"),
        ember_core::Error::Memory(_) => ("E600", "Memory Error"),
        ember_core::Error::ConversationNotFound(_) => ("E601", "Conversation Not Found"),
        ember_core::Error::LoopLimitExceeded { .. } => ("E602", "Loop Limit Exceeded"),
        ember_core::Error::Serialization(_) => ("E201", "Serialization Error"),
        ember_core::Error::Io(_) => ("E603", "I/O Error"),
        ember_core::Error::InvalidStateTransition { .. } => ("E604", "Invalid State"),
        ember_core::Error::Timeout { .. } | ember_core::Error::TimeoutMsg(_) => ("E102", "Timeout"),
        ember_core::Error::Cancelled => ("E605", "Operation Cancelled"),
        ember_core::Error::Agent(_) => ("E606", "Agent Error"),
        ember_core::Error::NotImplemented(_) => ("E607", "Not Implemented"),
        ember_core::Error::NotFound(_) => ("E608", "Not Found"),
        ember_core::Error::ResourceExhausted(_) => ("E609", "Resource Exhausted"),
        ember_core::Error::Internal(_) => ("E999", "Internal Error"),
    }
}

/// Display a generic error
fn display_generic_error(error: &anyhow::Error) {
    eprintln!();
    eprintln!("{}", "━".repeat(60).bright_red());
    eprintln!(
        "{} {} {}",
        " E000 ".bright_white().on_red().bold(),
        "Error".bright_white().bold(),
        "".bright_red()
    );
    eprintln!("{}", "━".repeat(60).bright_red());
    eprintln!();

    eprintln!("  {}", error.to_string().bright_white());

    // Print error chain if available
    let mut source = error.source();
    if source.is_some() {
        eprintln!();
        eprintln!("  {}", "Caused by:".bright_yellow());
    }
    while let Some(cause) = source {
        eprintln!("    • {}", cause.to_string().dimmed());
        source = cause.source();
    }

    eprintln!();
    eprintln!(
        "  {} Run {} for more information.",
        "💡".bright_cyan(),
        "ember --help".bright_green()
    );
    eprintln!();
    eprintln!("{}", "━".repeat(60).bright_red());
    eprintln!();
}

/// Display a warning message
#[allow(dead_code)]
pub fn display_warning(message: &str) {
    eprintln!("{} {}", "⚠️  Warning:".bright_yellow().bold(), message);
}

/// Display a success message
#[allow(dead_code)]
pub fn display_success(message: &str) {
    println!("{} {}", "✓".bright_green().bold(), message.bright_white());
}

/// Display an info message
#[allow(dead_code)]
pub fn display_info(message: &str) {
    println!("{} {}", "ℹ".bright_blue().bold(), message);
}

/// Display a retry message
#[allow(dead_code)]
pub fn display_retry(attempt: u32, max_attempts: u32, delay_secs: u64) {
    eprintln!(
        "{} Retrying ({}/{}) in {} seconds...",
        "↻".bright_yellow(),
        attempt,
        max_attempts,
        delay_secs
    );
}

/// Display a progress spinner message
#[allow(dead_code)]
pub fn display_progress(message: &str) {
    print!("\r{} {}", "◐".bright_blue(), message);
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

/// Clear the current line (for progress updates)
#[allow(dead_code)]
pub fn clear_line() {
    print!("\r{}\r", " ".repeat(80));
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

/// Display a hint message
#[allow(dead_code)]
pub fn display_hint(message: &str) {
    eprintln!("  {} {}", "💡".bright_cyan(), message.bright_white());
}

/// Display a command suggestion
#[allow(dead_code)]
pub fn display_command(command: &str) {
    eprintln!("    {}", command.bright_green());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_functions_exist() {
        // Just verify the functions compile
        let msg = "Test message";
        let _ = || display_warning(msg);
        let _ = || display_success(msg);
        let _ = || display_info(msg);
        let _ = || display_retry(1, 3, 5);
        let _ = || display_hint(msg);
        let _ = || display_command(msg);
    }

    #[test]
    fn test_get_core_error_info() {
        let err = ember_core::Error::Config("test".to_string());
        let (code, title) = get_core_error_info(&err);
        assert_eq!(code, "E500");
        assert_eq!(title, "Configuration Error");
    }
}
