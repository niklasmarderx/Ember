//! Natural language command parsing for voice interface.

use crate::{Result, VoiceError};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Voice command types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceCommand {
    /// Chat with AI.
    Chat { message: String },

    /// Execute shell command.
    Shell { command: String },

    /// File operations.
    File { operation: FileOperation },

    /// Git operations.
    Git { operation: GitOperation },

    /// Code operations.
    Code { operation: CodeOperation },

    /// Navigation commands.
    Navigate { target: NavigationTarget },

    /// Control commands (stop, pause, resume).
    Control { action: ControlAction },

    /// Settings commands.
    Settings { setting: SettingChange },

    /// Help request.
    Help { topic: Option<String> },

    /// Confirmation response.
    Confirm { confirmed: bool },

    /// Unknown command (fallback to chat).
    Unknown { text: String },
}

/// File operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileOperation {
    Open { path: String },
    Create { path: String, content: Option<String> },
    Delete { path: String },
    Read { path: String },
    Write { path: String, content: String },
    Search { pattern: String, path: Option<String> },
    List { path: Option<String> },
}

/// Git operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitOperation {
    Status,
    Commit { message: Option<String> },
    Push { branch: Option<String> },
    Pull { branch: Option<String> },
    Branch { name: Option<String>, create: bool },
    Checkout { target: String },
    Diff { target: Option<String> },
    Log { count: Option<u32> },
}

/// Code operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeOperation {
    Analyze { path: String },
    Refactor { path: String, suggestion: Option<String> },
    Test { path: String },
    Run { command: String },
    Debug { target: String },
    Explain { code: String },
}

/// Navigation targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavigationTarget {
    File { path: String },
    Line { number: u32 },
    Function { name: String },
    Class { name: String },
    Definition { symbol: String },
    Reference { symbol: String },
    Back,
    Forward,
}

/// Control actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlAction {
    Stop,
    Pause,
    Resume,
    Cancel,
    Undo,
    Redo,
    Clear,
    Exit,
}

/// Setting changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingChange {
    SetModel { model: String },
    SetProvider { provider: String },
    SetVoice { voice: String },
    SetLanguage { language: String },
    ToggleStreaming,
    ToggleFeedback,
    SetVolume { level: u8 },
    SetRate { rate: f32 },
}

/// Parsed voice command with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedCommand {
    /// The parsed command.
    pub command: VoiceCommand,

    /// Original transcribed text.
    pub original_text: String,

    /// Confidence in the parsing (0.0 - 1.0).
    pub confidence: f32,

    /// Detected intent.
    pub intent: String,

    /// Extracted entities.
    pub entities: HashMap<String, String>,

    /// Is this a dangerous command?
    pub dangerous: bool,

    /// Suggested confirmation prompt.
    pub confirmation_prompt: Option<String>,
}

impl ParsedCommand {
    /// Check if this command is dangerous.
    pub fn is_dangerous(&self) -> bool {
        self.dangerous
    }
}

/// Command parser for natural language input.
pub struct CommandParser {
    language: String,
    patterns: CommandPatterns,
}

struct CommandPatterns {
    // Control patterns
    stop: Regex,
    cancel: Regex,
    undo: Regex,
    redo: Regex,
    exit: Regex,
    clear: Regex,

    // Confirmation patterns
    yes: Regex,
    no: Regex,

    // File patterns
    open_file: Regex,
    create_file: Regex,
    delete_file: Regex,
    read_file: Regex,
    search_files: Regex,
    list_files: Regex,

    // Git patterns
    git_status: Regex,
    git_commit: Regex,
    git_push: Regex,
    git_pull: Regex,
    git_branch: Regex,
    git_checkout: Regex,
    git_diff: Regex,
    git_log: Regex,

    // Code patterns
    analyze_code: Regex,
    refactor_code: Regex,
    run_tests: Regex,
    run_command: Regex,
    explain_code: Regex,

    // Navigation patterns
    go_to_line: Regex,
    go_to_function: Regex,
    go_to_definition: Regex,
    go_back: Regex,

    // Settings patterns
    set_model: Regex,
    set_voice: Regex,
    set_volume: Regex,

    // Shell pattern
    run_shell: Regex,

    // Help pattern
    help: Regex,
}

impl CommandPatterns {
    fn new() -> Self {
        Self {
            // Control patterns
            stop: Regex::new(r"(?i)^(stop|halt|abort)").unwrap(),
            cancel: Regex::new(r"(?i)^cancel").unwrap(),
            undo: Regex::new(r"(?i)^undo").unwrap(),
            redo: Regex::new(r"(?i)^redo").unwrap(),
            exit: Regex::new(r"(?i)^(exit|quit|bye|goodbye)").unwrap(),
            clear: Regex::new(r"(?i)^clear").unwrap(),

            // Confirmation patterns
            yes: Regex::new(r"(?i)^(yes|yeah|yep|correct|confirm|do it|go ahead|proceed)").unwrap(),
            no: Regex::new(r"(?i)^(no|nope|cancel|don't|stop|abort)").unwrap(),

            // File patterns
            open_file: Regex::new(r"(?i)^open\s+(?:file\s+)?(.+)").unwrap(),
            create_file: Regex::new(r"(?i)^create\s+(?:a\s+)?(?:new\s+)?file\s+(?:called\s+)?(.+)").unwrap(),
            delete_file: Regex::new(r"(?i)^delete\s+(?:file\s+)?(.+)").unwrap(),
            read_file: Regex::new(r"(?i)^read\s+(?:file\s+)?(.+)").unwrap(),
            search_files: Regex::new(r"(?i)^(?:search|find)\s+(?:for\s+)?(.+?)(?:\s+in\s+(.+))?$").unwrap(),
            list_files: Regex::new(r"(?i)^list\s+(?:files(?:\s+in\s+)?)?(.*)$").unwrap(),

            // Git patterns
            git_status: Regex::new(r"(?i)^(?:git\s+)?status").unwrap(),
            git_commit: Regex::new(r"(?i)^(?:git\s+)?commit(?:\s+(?:with\s+)?(?:message\s+)?(.+))?").unwrap(),
            git_push: Regex::new(r"(?i)^(?:git\s+)?push(?:\s+(?:to\s+)?(.+))?").unwrap(),
            git_pull: Regex::new(r"(?i)^(?:git\s+)?pull(?:\s+(?:from\s+)?(.+))?").unwrap(),
            git_branch: Regex::new(r"(?i)^(?:git\s+)?(?:create\s+)?branch(?:\s+(.+))?").unwrap(),
            git_checkout: Regex::new(r"(?i)^(?:git\s+)?checkout\s+(.+)").unwrap(),
            git_diff: Regex::new(r"(?i)^(?:git\s+)?diff(?:\s+(.+))?").unwrap(),
            git_log: Regex::new(r"(?i)^(?:git\s+)?log(?:\s+(\d+))?").unwrap(),

            // Code patterns
            analyze_code: Regex::new(r"(?i)^analyze\s+(?:code\s+)?(?:in\s+)?(.+)").unwrap(),
            refactor_code: Regex::new(r"(?i)^refactor\s+(.+)").unwrap(),
            run_tests: Regex::new(r"(?i)^(?:run\s+)?tests?\s+(?:for\s+)?(.+)").unwrap(),
            run_command: Regex::new(r"(?i)^run\s+(.+)").unwrap(),
            explain_code: Regex::new(r"(?i)^explain\s+(.+)").unwrap(),

            // Navigation patterns
            go_to_line: Regex::new(r"(?i)^go\s+to\s+line\s+(\d+)").unwrap(),
            go_to_function: Regex::new(r"(?i)^go\s+to\s+(?:function\s+)?(.+)").unwrap(),
            go_to_definition: Regex::new(r"(?i)^(?:go\s+to\s+)?definition\s+(?:of\s+)?(.+)").unwrap(),
            go_back: Regex::new(r"(?i)^go\s+back").unwrap(),

            // Settings patterns
            set_model: Regex::new(r"(?i)^(?:use|set)\s+model\s+(.+)").unwrap(),
            set_voice: Regex::new(r"(?i)^(?:use|set)\s+voice\s+(.+)").unwrap(),
            set_volume: Regex::new(r"(?i)^(?:set\s+)?volume\s+(?:to\s+)?(\d+)").unwrap(),

            // Shell pattern
            run_shell: Regex::new(r"(?i)^(?:run\s+)?(?:shell\s+)?command\s+(.+)").unwrap(),

            // Help pattern
            help: Regex::new(r"(?i)^help(?:\s+(?:with\s+)?(.+))?").unwrap(),
        }
    }
}

impl CommandParser {
    /// Create a new command parser.
    pub fn new(language: String) -> Self {
        Self {
            language,
            patterns: CommandPatterns::new(),
        }
    }

    /// Parse text into a command.
    pub fn parse(&self, text: &str) -> Result<ParsedCommand> {
        let text = text.trim();

        if text.is_empty() {
            return Err(VoiceError::CommandParsing("Empty input".to_string()));
        }

        // Try each pattern in order
        let (command, intent, confidence, entities, dangerous) = self.match_patterns(text);

        let confirmation_prompt = if dangerous {
            Some(self.generate_confirmation_prompt(&command))
        } else {
            None
        };

        Ok(ParsedCommand {
            command,
            original_text: text.to_string(),
            confidence,
            intent,
            entities,
            dangerous,
            confirmation_prompt,
        })
    }

    fn match_patterns(
        &self,
        text: &str,
    ) -> (VoiceCommand, String, f32, HashMap<String, String>, bool) {
        let mut entities = HashMap::new();

        // Control commands (highest priority)
        if self.patterns.stop.is_match(text) {
            return (
                VoiceCommand::Control {
                    action: ControlAction::Stop,
                },
                "control.stop".to_string(),
                0.95,
                entities,
                false,
            );
        }

        if self.patterns.cancel.is_match(text) {
            return (
                VoiceCommand::Control {
                    action: ControlAction::Cancel,
                },
                "control.cancel".to_string(),
                0.95,
                entities,
                false,
            );
        }

        if self.patterns.undo.is_match(text) {
            return (
                VoiceCommand::Control {
                    action: ControlAction::Undo,
                },
                "control.undo".to_string(),
                0.95,
                entities,
                false,
            );
        }

        if self.patterns.redo.is_match(text) {
            return (
                VoiceCommand::Control {
                    action: ControlAction::Redo,
                },
                "control.redo".to_string(),
                0.95,
                entities,
                false,
            );
        }

        if self.patterns.exit.is_match(text) {
            return (
                VoiceCommand::Control {
                    action: ControlAction::Exit,
                },
                "control.exit".to_string(),
                0.95,
                entities,
                false,
            );
        }

        if self.patterns.clear.is_match(text) {
            return (
                VoiceCommand::Control {
                    action: ControlAction::Clear,
                },
                "control.clear".to_string(),
                0.95,
                entities,
                false,
            );
        }

        // Confirmation commands
        if self.patterns.yes.is_match(text) {
            return (
                VoiceCommand::Confirm { confirmed: true },
                "confirm.yes".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if self.patterns.no.is_match(text) {
            return (
                VoiceCommand::Confirm { confirmed: false },
                "confirm.no".to_string(),
                0.90,
                entities,
                false,
            );
        }

        // Help command
        if let Some(caps) = self.patterns.help.captures(text) {
            let topic = caps.get(1).map(|m| m.as_str().to_string());
            if let Some(ref t) = topic {
                entities.insert("topic".to_string(), t.clone());
            }
            return (
                VoiceCommand::Help { topic },
                "help".to_string(),
                0.90,
                entities,
                false,
            );
        }

        // File operations
        if let Some(caps) = self.patterns.delete_file.captures(text) {
            let path = caps.get(1).unwrap().as_str().to_string();
            entities.insert("path".to_string(), path.clone());
            return (
                VoiceCommand::File {
                    operation: FileOperation::Delete { path },
                },
                "file.delete".to_string(),
                0.90,
                entities,
                true, // Dangerous
            );
        }

        if let Some(caps) = self.patterns.create_file.captures(text) {
            let path = caps.get(1).unwrap().as_str().to_string();
            entities.insert("path".to_string(), path.clone());
            return (
                VoiceCommand::File {
                    operation: FileOperation::Create {
                        path,
                        content: None,
                    },
                },
                "file.create".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.open_file.captures(text) {
            let path = caps.get(1).unwrap().as_str().to_string();
            entities.insert("path".to_string(), path.clone());
            return (
                VoiceCommand::File {
                    operation: FileOperation::Open { path },
                },
                "file.open".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.read_file.captures(text) {
            let path = caps.get(1).unwrap().as_str().to_string();
            entities.insert("path".to_string(), path.clone());
            return (
                VoiceCommand::File {
                    operation: FileOperation::Read { path },
                },
                "file.read".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.search_files.captures(text) {
            let pattern = caps.get(1).unwrap().as_str().to_string();
            let path = caps.get(2).map(|m| m.as_str().to_string());
            entities.insert("pattern".to_string(), pattern.clone());
            if let Some(ref p) = path {
                entities.insert("path".to_string(), p.clone());
            }
            return (
                VoiceCommand::File {
                    operation: FileOperation::Search { pattern, path },
                },
                "file.search".to_string(),
                0.85,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.list_files.captures(text) {
            let path = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .filter(|s| !s.is_empty());
            if let Some(ref p) = path {
                entities.insert("path".to_string(), p.clone());
            }
            return (
                VoiceCommand::File {
                    operation: FileOperation::List { path },
                },
                "file.list".to_string(),
                0.85,
                entities,
                false,
            );
        }

        // Git operations
        if self.patterns.git_status.is_match(text) {
            return (
                VoiceCommand::Git {
                    operation: GitOperation::Status,
                },
                "git.status".to_string(),
                0.95,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.git_commit.captures(text) {
            let message = caps.get(1).map(|m| m.as_str().to_string());
            if let Some(ref m) = message {
                entities.insert("message".to_string(), m.clone());
            }
            return (
                VoiceCommand::Git {
                    operation: GitOperation::Commit { message },
                },
                "git.commit".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.git_push.captures(text) {
            let branch = caps.get(1).map(|m| m.as_str().to_string());
            if let Some(ref b) = branch {
                entities.insert("branch".to_string(), b.clone());
            }
            return (
                VoiceCommand::Git {
                    operation: GitOperation::Push { branch },
                },
                "git.push".to_string(),
                0.90,
                entities,
                true, // Potentially dangerous
            );
        }

        if let Some(caps) = self.patterns.git_pull.captures(text) {
            let branch = caps.get(1).map(|m| m.as_str().to_string());
            if let Some(ref b) = branch {
                entities.insert("branch".to_string(), b.clone());
            }
            return (
                VoiceCommand::Git {
                    operation: GitOperation::Pull { branch },
                },
                "git.pull".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.git_checkout.captures(text) {
            let target = caps.get(1).unwrap().as_str().to_string();
            entities.insert("target".to_string(), target.clone());
            return (
                VoiceCommand::Git {
                    operation: GitOperation::Checkout { target },
                },
                "git.checkout".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.git_branch.captures(text) {
            let name = caps.get(1).map(|m| m.as_str().to_string());
            let create = text.to_lowercase().contains("create");
            if let Some(ref n) = name {
                entities.insert("name".to_string(), n.clone());
            }
            return (
                VoiceCommand::Git {
                    operation: GitOperation::Branch { name, create },
                },
                "git.branch".to_string(),
                0.85,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.git_diff.captures(text) {
            let target = caps.get(1).map(|m| m.as_str().to_string());
            if let Some(ref t) = target {
                entities.insert("target".to_string(), t.clone());
            }
            return (
                VoiceCommand::Git {
                    operation: GitOperation::Diff { target },
                },
                "git.diff".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.git_log.captures(text) {
            let count = caps.get(1).and_then(|m| m.as_str().parse().ok());
            if let Some(c) = count {
                entities.insert("count".to_string(), c.to_string());
            }
            return (
                VoiceCommand::Git {
                    operation: GitOperation::Log { count },
                },
                "git.log".to_string(),
                0.90,
                entities,
                false,
            );
        }

        // Code operations
        if let Some(caps) = self.patterns.analyze_code.captures(text) {
            let path = caps.get(1).unwrap().as_str().to_string();
            entities.insert("path".to_string(), path.clone());
            return (
                VoiceCommand::Code {
                    operation: CodeOperation::Analyze { path },
                },
                "code.analyze".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.refactor_code.captures(text) {
            let path = caps.get(1).unwrap().as_str().to_string();
            entities.insert("path".to_string(), path.clone());
            return (
                VoiceCommand::Code {
                    operation: CodeOperation::Refactor {
                        path,
                        suggestion: None,
                    },
                },
                "code.refactor".to_string(),
                0.85,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.run_tests.captures(text) {
            let path = caps.get(1).unwrap().as_str().to_string();
            entities.insert("path".to_string(), path.clone());
            return (
                VoiceCommand::Code {
                    operation: CodeOperation::Test { path },
                },
                "code.test".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.explain_code.captures(text) {
            let code = caps.get(1).unwrap().as_str().to_string();
            entities.insert("code".to_string(), code.clone());
            return (
                VoiceCommand::Code {
                    operation: CodeOperation::Explain { code },
                },
                "code.explain".to_string(),
                0.85,
                entities,
                false,
            );
        }

        // Navigation
        if let Some(caps) = self.patterns.go_to_line.captures(text) {
            let number: u32 = caps.get(1).unwrap().as_str().parse().unwrap_or(1);
            entities.insert("line".to_string(), number.to_string());
            return (
                VoiceCommand::Navigate {
                    target: NavigationTarget::Line { number },
                },
                "navigate.line".to_string(),
                0.95,
                entities,
                false,
            );
        }

        if self.patterns.go_back.is_match(text) {
            return (
                VoiceCommand::Navigate {
                    target: NavigationTarget::Back,
                },
                "navigate.back".to_string(),
                0.95,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.go_to_definition.captures(text) {
            let symbol = caps.get(1).unwrap().as_str().to_string();
            entities.insert("symbol".to_string(), symbol.clone());
            return (
                VoiceCommand::Navigate {
                    target: NavigationTarget::Definition { symbol },
                },
                "navigate.definition".to_string(),
                0.85,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.go_to_function.captures(text) {
            let name = caps.get(1).unwrap().as_str().to_string();
            entities.insert("function".to_string(), name.clone());
            return (
                VoiceCommand::Navigate {
                    target: NavigationTarget::Function { name },
                },
                "navigate.function".to_string(),
                0.85,
                entities,
                false,
            );
        }

        // Settings
        if let Some(caps) = self.patterns.set_model.captures(text) {
            let model = caps.get(1).unwrap().as_str().to_string();
            entities.insert("model".to_string(), model.clone());
            return (
                VoiceCommand::Settings {
                    setting: SettingChange::SetModel { model },
                },
                "settings.model".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.set_voice.captures(text) {
            let voice = caps.get(1).unwrap().as_str().to_string();
            entities.insert("voice".to_string(), voice.clone());
            return (
                VoiceCommand::Settings {
                    setting: SettingChange::SetVoice { voice },
                },
                "settings.voice".to_string(),
                0.90,
                entities,
                false,
            );
        }

        if let Some(caps) = self.patterns.set_volume.captures(text) {
            let level: u8 = caps.get(1).unwrap().as_str().parse().unwrap_or(50);
            entities.insert("volume".to_string(), level.to_string());
            return (
                VoiceCommand::Settings {
                    setting: SettingChange::SetVolume { level },
                },
                "settings.volume".to_string(),
                0.90,
                entities,
                false,
            );
        }

        // Shell command
        if let Some(caps) = self.patterns.run_shell.captures(text) {
            let command = caps.get(1).unwrap().as_str().to_string();
            entities.insert("command".to_string(), command.clone());
            return (
                VoiceCommand::Shell { command },
                "shell.run".to_string(),
                0.85,
                entities,
                true, // Dangerous
            );
        }

        // Run command (more general)
        if let Some(caps) = self.patterns.run_command.captures(text) {
            let command = caps.get(1).unwrap().as_str().to_string();
            entities.insert("command".to_string(), command.clone());
            return (
                VoiceCommand::Code {
                    operation: CodeOperation::Run { command },
                },
                "code.run".to_string(),
                0.80,
                entities,
                true, // Potentially dangerous
            );
        }

        // Default: treat as chat message
        (
            VoiceCommand::Chat {
                message: text.to_string(),
            },
            "chat".to_string(),
            0.70,
            entities,
            false,
        )
    }

    fn generate_confirmation_prompt(&self, command: &VoiceCommand) -> String {
        match command {
            VoiceCommand::File {
                operation: FileOperation::Delete { path },
            } => {
                format!("Are you sure you want to delete '{}'?", path)
            }
            VoiceCommand::Shell { command } => {
                format!("Execute shell command: '{}'?", command)
            }
            VoiceCommand::Git {
                operation: GitOperation::Push { branch },
            } => {
                if let Some(b) = branch {
                    format!("Push to branch '{}'?", b)
                } else {
                    "Push to remote?".to_string()
                }
            }
            VoiceCommand::Code {
                operation: CodeOperation::Run { command },
            } => {
                format!("Run command: '{}'?", command)
            }
            _ => "Are you sure?".to_string(),
        }
    }

    /// Get supported command categories.
    pub fn supported_commands(&self) -> Vec<&'static str> {
        vec![
            "chat",
            "file",
            "git",
            "code",
            "navigate",
            "control",
            "settings",
            "help",
        ]
    }

    /// Get the current language.
    pub fn language(&self) -> &str {
        &self.language
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chat() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("Hello, how are you?").unwrap();

        match result.command {
            VoiceCommand::Chat { message } => {
                assert_eq!(message, "Hello, how are you?");
            }
            _ => panic!("Expected Chat command"),
        }
    }

    #[test]
    fn test_parse_stop() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("stop").unwrap();

        match result.command {
            VoiceCommand::Control { action } => {
                assert_eq!(action, ControlAction::Stop);
            }
            _ => panic!("Expected Control command"),
        }
    }

    #[test]
    fn test_parse_open_file() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("open file main.rs").unwrap();

        match result.command {
            VoiceCommand::File {
                operation: FileOperation::Open { path },
            } => {
                assert_eq!(path, "main.rs");
            }
            _ => panic!("Expected File Open command"),
        }
    }

    #[test]
    fn test_parse_delete_file_dangerous() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("delete file temp.txt").unwrap();

        assert!(result.dangerous);
        assert!(result.confirmation_prompt.is_some());
    }

    #[test]
    fn test_parse_git_status() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("git status").unwrap();

        match result.command {
            VoiceCommand::Git {
                operation: GitOperation::Status,
            } => {}
            _ => panic!("Expected Git Status command"),
        }
    }

    #[test]
    fn test_parse_git_commit_with_message() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("commit with message fix bug").unwrap();

        match result.command {
            VoiceCommand::Git {
                operation: GitOperation::Commit { message },
            } => {
                assert_eq!(message, Some("fix bug".to_string()));
            }
            _ => panic!("Expected Git Commit command"),
        }
    }

    #[test]
    fn test_parse_go_to_line() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("go to line 42").unwrap();

        match result.command {
            VoiceCommand::Navigate {
                target: NavigationTarget::Line { number },
            } => {
                assert_eq!(number, 42);
            }
            _ => panic!("Expected Navigate Line command"),
        }
    }

    #[test]
    fn test_parse_confirmation_yes() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("yes").unwrap();

        match result.command {
            VoiceCommand::Confirm { confirmed } => {
                assert!(confirmed);
            }
            _ => panic!("Expected Confirm command"),
        }
    }

    #[test]
    fn test_parse_help() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("help with git").unwrap();

        match result.command {
            VoiceCommand::Help { topic } => {
                assert_eq!(topic, Some("git".to_string()));
            }
            _ => panic!("Expected Help command"),
        }
    }

    #[test]
    fn test_entities_extraction() {
        let parser = CommandParser::new("en".to_string());
        let result = parser.parse("search for TODO in src").unwrap();

        assert!(result.entities.contains_key("pattern"));
        assert_eq!(result.entities.get("pattern"), Some(&"TODO".to_string()));
    }
}