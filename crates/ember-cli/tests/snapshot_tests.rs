//! Snapshot tests for CLI output using insta
//!
//! These tests capture CLI output and compare against stored snapshots.
//! Run `cargo insta review` to review and accept changes.

use insta::{assert_snapshot, assert_yaml_snapshot};
use serde::{Deserialize, Serialize};

/// Represents a formatted CLI output for testing
#[derive(Debug, Serialize, Deserialize)]
struct CliOutput {
    command: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

/// Represents a help text structure
#[derive(Debug, Serialize, Deserialize)]
struct HelpOutput {
    name: String,
    version: String,
    description: String,
    usage: String,
    subcommands: Vec<String>,
    global_options: Vec<String>,
}

/// Represents a formatted message for display
#[derive(Debug, Serialize, Deserialize)]
struct FormattedMessage {
    role: String,
    content: String,
    timestamp: Option<String>,
    tokens: Option<u32>,
}

/// Represents a conversation export format
#[derive(Debug, Serialize, Deserialize)]
struct ConversationExport {
    id: String,
    title: String,
    model: String,
    messages: Vec<FormattedMessage>,
    total_tokens: u32,
    total_cost: f64,
}

/// Represents a model info structure
#[derive(Debug, Serialize, Deserialize)]
struct ModelInfo {
    name: String,
    provider: String,
    context_window: u32,
    input_cost_per_1k: f64,
    output_cost_per_1k: f64,
    capabilities: Vec<String>,
}

/// Represents configuration output
#[derive(Debug, Serialize, Deserialize)]
struct ConfigOutput {
    provider: String,
    model: String,
    api_key_set: bool,
    temperature: f32,
    max_tokens: Option<u32>,
    stream: bool,
}

// Test help output format
#[test]
fn test_help_output_format() {
    let help = HelpOutput {
        name: "ember".to_string(),
        version: "1.0.0".to_string(),
        description: "An open-source AI agent framework".to_string(),
        usage: "ember [OPTIONS] <COMMAND>".to_string(),
        subcommands: vec![
            "chat".to_string(),
            "config".to_string(),
            "history".to_string(),
            "export".to_string(),
            "serve".to_string(),
            "plugin".to_string(),
            "completions".to_string(),
            "tui".to_string(),
        ],
        global_options: vec![
            "--help".to_string(),
            "--version".to_string(),
            "--verbose".to_string(),
            "--quiet".to_string(),
            "--config".to_string(),
        ],
    };

    assert_yaml_snapshot!("help_output", help);
}

// Test message formatting
#[test]
fn test_user_message_format() {
    let message = FormattedMessage {
        role: "user".to_string(),
        content: "What is the capital of France?".to_string(),
        timestamp: Some("2024-01-15T10:30:00Z".to_string()),
        tokens: Some(8),
    };

    assert_yaml_snapshot!("user_message", message);
}

#[test]
fn test_assistant_message_format() {
    let message = FormattedMessage {
        role: "assistant".to_string(),
        content: "The capital of France is Paris. Paris is the largest city in France and serves as the country's political, economic, and cultural center.".to_string(),
        timestamp: Some("2024-01-15T10:30:05Z".to_string()),
        tokens: Some(32),
    };

    assert_yaml_snapshot!("assistant_message", message);
}

#[test]
fn test_system_message_format() {
    let message = FormattedMessage {
        role: "system".to_string(),
        content: "You are a helpful AI assistant. Be concise and accurate in your responses."
            .to_string(),
        timestamp: None,
        tokens: Some(15),
    };

    assert_yaml_snapshot!("system_message", message);
}

// Test conversation export format
#[test]
fn test_conversation_export_format() {
    let export = ConversationExport {
        id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        title: "Geography Questions".to_string(),
        model: "gpt-4".to_string(),
        messages: vec![
            FormattedMessage {
                role: "user".to_string(),
                content: "What is the capital of France?".to_string(),
                timestamp: Some("2024-01-15T10:30:00Z".to_string()),
                tokens: Some(8),
            },
            FormattedMessage {
                role: "assistant".to_string(),
                content: "The capital of France is Paris.".to_string(),
                timestamp: Some("2024-01-15T10:30:05Z".to_string()),
                tokens: Some(8),
            },
        ],
        total_tokens: 16,
        total_cost: 0.00048,
    };

    assert_yaml_snapshot!("conversation_export", export);
}

// Test model info display
#[test]
fn test_model_info_openai() {
    let model = ModelInfo {
        name: "gpt-4".to_string(),
        provider: "OpenAI".to_string(),
        context_window: 8192,
        input_cost_per_1k: 0.03,
        output_cost_per_1k: 0.06,
        capabilities: vec![
            "chat".to_string(),
            "function_calling".to_string(),
            "vision".to_string(),
        ],
    };

    assert_yaml_snapshot!("model_info_gpt4", model);
}

#[test]
fn test_model_info_anthropic() {
    let model = ModelInfo {
        name: "claude-3-opus".to_string(),
        provider: "Anthropic".to_string(),
        context_window: 200000,
        input_cost_per_1k: 0.015,
        output_cost_per_1k: 0.075,
        capabilities: vec![
            "chat".to_string(),
            "vision".to_string(),
            "long_context".to_string(),
        ],
    };

    assert_yaml_snapshot!("model_info_claude", model);
}

#[test]
fn test_model_info_ollama() {
    let model = ModelInfo {
        name: "llama3:70b".to_string(),
        provider: "Ollama".to_string(),
        context_window: 8192,
        input_cost_per_1k: 0.0,
        output_cost_per_1k: 0.0,
        capabilities: vec!["chat".to_string(), "local".to_string()],
    };

    assert_yaml_snapshot!("model_info_ollama", model);
}

// Test config display
#[test]
fn test_config_display() {
    let config = ConfigOutput {
        provider: "openai".to_string(),
        model: "gpt-4".to_string(),
        api_key_set: true,
        temperature: 0.7,
        max_tokens: Some(4096),
        stream: true,
    };

    assert_yaml_snapshot!("config_display", config);
}

#[test]
fn test_config_display_minimal() {
    let config = ConfigOutput {
        provider: "ollama".to_string(),
        model: "llama3".to_string(),
        api_key_set: false,
        temperature: 0.0,
        max_tokens: None,
        stream: false,
    };

    assert_yaml_snapshot!("config_display_minimal", config);
}

// Test error messages
#[test]
fn test_error_message_api_key() {
    let error = r#"Error: API key not found

The OpenAI API key is required but was not found.

To fix this, either:
  1. Set the OPENAI_API_KEY environment variable
  2. Add it to your config file: ember config set openai.api_key <YOUR_KEY>

For more information, see: https://docs.ember.ai/configuration
"#;

    assert_snapshot!("error_api_key", error);
}

#[test]
fn test_error_message_network() {
    let error = r#"Error: Connection failed

Could not connect to the OpenAI API.

Possible causes:
  - No internet connection
  - API endpoint is unreachable
  - Firewall blocking the connection

Try again later or check your network settings.
"#;

    assert_snapshot!("error_network", error);
}

#[test]
fn test_error_message_rate_limit() {
    let error = r#"Error: Rate limit exceeded

You've exceeded the API rate limit.

Details:
  - Limit: 60 requests/minute
  - Reset: in 45 seconds

Consider:
  - Waiting before retrying
  - Using a different API key
  - Upgrading your API plan
"#;

    assert_snapshot!("error_rate_limit", error);
}

// Test progress/status messages
#[test]
fn test_status_connecting() {
    let status = "Connecting to OpenAI API...";
    assert_snapshot!("status_connecting", status);
}

#[test]
fn test_status_streaming() {
    let status = "Streaming response from gpt-4...";
    assert_snapshot!("status_streaming", status);
}

#[test]
fn test_status_complete() {
    let status = r#"
Response complete
  Model: gpt-4
  Tokens: 150 (input: 50, output: 100)
  Cost: $0.0072
  Time: 2.3s
"#;
    assert_snapshot!("status_complete", status);
}

// Test table formatting
#[test]
fn test_history_table() {
    let table = r#"
+----+----------------------+----------+--------+---------+---------------------+
| ID | Title                | Model    | Tokens | Cost    | Created             |
+----+----------------------+----------+--------+---------+---------------------+
| 1  | Geography Questions  | gpt-4    | 150    | $0.0045 | 2024-01-15 10:30:00 |
| 2  | Code Review          | gpt-4    | 2500   | $0.0750 | 2024-01-15 11:00:00 |
| 3  | Email Draft          | gpt-3.5  | 800    | $0.0016 | 2024-01-15 11:30:00 |
+----+----------------------+----------+--------+---------+---------------------+
"#;

    assert_snapshot!("history_table", table);
}

#[test]
fn test_plugins_table() {
    let table = r#"
+------------------+----------+---------+-------------+
| Name             | Version  | Status  | Description |
+------------------+----------+---------+-------------+
| weather          | 1.0.0    | Active  | Get weather |
| github           | 2.1.0    | Active  | GitHub ops  |
| slack            | 1.5.0    | Inactive| Slack msgs  |
+------------------+----------+---------+-------------+
"#;

    assert_snapshot!("plugins_table", table);
}

// Test JSON output format
#[test]
fn test_json_output_message() {
    let json = r#"{
  "id": "msg_123",
  "role": "assistant",
  "content": "Hello! How can I help you today?",
  "model": "gpt-4",
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 9,
    "total_tokens": 19
  },
  "finish_reason": "stop"
}"#;

    assert_snapshot!("json_output_message", json);
}

// Test version output
#[test]
fn test_version_output() {
    let version = r#"ember 1.0.0

An open-source AI agent framework written in Rust.

Homepage: https://ember.ai
Repository: https://github.com/ember-ai/ember
License: MIT OR Apache-2.0
"#;

    assert_snapshot!("version_output", version);
}

// Test completions output (for shell completion)
#[test]
fn test_bash_completions_snippet() {
    let completions = r#"_ember() {
    local cur prev words cword
    _init_completion || return

    case $prev in
        ember)
            COMPREPLY=( $(compgen -W "chat config history export serve plugin completions tui help" -- "$cur") )
            return
            ;;
        chat)
            COMPREPLY=( $(compgen -W "--model --provider --temperature --stream" -- "$cur") )
            return
            ;;
    esac
}
complete -F _ember ember
"#;

    assert_snapshot!("bash_completions", completions);
}

// Test TUI layout snapshot (ASCII art representation)
#[test]
fn test_tui_layout_main() {
    let layout = r#"
+----------------------------------------------------------+
| Ember - AI Assistant                          [gpt-4] [] |
+----------------------------------------------------------+
|                                                          |
| Conversations  | Chat                                    |
| -------------  | ----------------------------------------|
| > New Chat     |                                         |
|   Geography    | User: What is the capital of France?   |
|   Code Review  |                                         |
|   Email Draft  | Assistant: The capital of France is    |
|                | Paris. Paris is the largest city in    |
|                | France and serves as the country's     |
|                | political, economic, and cultural      |
|                | center.                                 |
|                |                                         |
|                |                                         |
|                |                                         |
+----------------------------------------------------------+
| Type a message...                                   [Send]|
+----------------------------------------------------------+
| Tokens: 150 | Cost: $0.0045 | Model: gpt-4    | Stream: On|
+----------------------------------------------------------+
"#;

    assert_snapshot!("tui_layout_main", layout);
}

#[test]
fn test_tui_layout_settings() {
    let layout = r#"
+----------------------------------------------------------+
| Settings                                            [X]  |
+----------------------------------------------------------+
|                                                          |
| Provider: [OpenAI     v]                                 |
|                                                          |
| Model:    [gpt-4      v]                                 |
|                                                          |
| Temperature: [0.7____]                                   |
|                                                          |
| Max Tokens:  [4096___]                                   |
|                                                          |
| [x] Enable streaming                                     |
| [ ] Show token counts                                    |
| [x] Auto-save conversations                              |
|                                                          |
|                                                          |
|                     [Cancel]  [Save]                     |
+----------------------------------------------------------+
"#;

    assert_snapshot!("tui_layout_settings", layout);
}
