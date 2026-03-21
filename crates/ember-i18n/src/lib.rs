//! # Ember i18n
//!
//! Internationalization (i18n) support for Ember.
//!
//! This crate provides localization capabilities for Ember, supporting multiple
//! languages including English, German, French, Spanish, Chinese, and Japanese.
//!
//! ## Features
//!
//! - Automatic locale detection from system settings
//! - Manual locale override
//! - Fallback to English for missing translations
//! - Type-safe translation keys
//! - Pluralization support
//! - Interpolation support
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ember_i18n::{t, set_locale, get_locale, Locale};
//!
//! // Set locale manually
//! set_locale(Locale::German);
//!
//! // Get translated string
//! println!("{}", t!("welcome"));
//!
//! // With interpolation
//! println!("{}", t!("greeting", name = "World"));
//! ```

use once_cell::sync::Lazy;
use std::sync::RwLock;
use thiserror::Error;

// Re-export the t! macro from rust-i18n
pub use rust_i18n::t;

// Initialize rust-i18n with our locales
rust_i18n::i18n!("locales", fallback = "en");

/// Supported locales in Ember
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale {
    /// English (default)
    #[default]
    English,
    /// German (Deutsch)
    German,
    /// French (Français)
    French,
    /// Spanish (Español)
    Spanish,
    /// Chinese Simplified (简体中文)
    ChineseSimplified,
    /// Japanese (日本語)
    Japanese,
    /// Portuguese Brazilian (Português Brasileiro)
    PortugueseBrazilian,
    /// Korean (한국어)
    Korean,
    /// Italian (Italiano)
    Italian,
    /// Russian (Русский)
    Russian,
    /// Arabic (العربية) - RTL
    Arabic,
}

impl Locale {
    /// Get the locale code (BCP 47)
    pub fn code(&self) -> &'static str {
        match self {
            Locale::English => "en",
            Locale::German => "de",
            Locale::French => "fr",
            Locale::Spanish => "es",
            Locale::ChineseSimplified => "zh-CN",
            Locale::Japanese => "ja",
            Locale::PortugueseBrazilian => "pt-BR",
            Locale::Korean => "ko",
            Locale::Italian => "it",
            Locale::Russian => "ru",
            Locale::Arabic => "ar",
        }
    }

    /// Get the native name of the locale
    pub fn native_name(&self) -> &'static str {
        match self {
            Locale::English => "English",
            Locale::German => "Deutsch",
            Locale::French => "Français",
            Locale::Spanish => "Español",
            Locale::ChineseSimplified => "简体中文",
            Locale::Japanese => "日本語",
            Locale::PortugueseBrazilian => "Português (Brasil)",
            Locale::Korean => "한국어",
            Locale::Italian => "Italiano",
            Locale::Russian => "Русский",
            Locale::Arabic => "العربية",
        }
    }

    /// Get the English name of the locale
    pub fn english_name(&self) -> &'static str {
        match self {
            Locale::English => "English",
            Locale::German => "German",
            Locale::French => "French",
            Locale::Spanish => "Spanish",
            Locale::ChineseSimplified => "Chinese (Simplified)",
            Locale::Japanese => "Japanese",
            Locale::PortugueseBrazilian => "Portuguese (Brazil)",
            Locale::Korean => "Korean",
            Locale::Italian => "Italian",
            Locale::Russian => "Russian",
            Locale::Arabic => "Arabic",
        }
    }

    /// Check if this locale uses right-to-left text direction
    pub fn is_rtl(&self) -> bool {
        matches!(self, Locale::Arabic)
    }

    /// Get the text direction for this locale
    pub fn direction(&self) -> &'static str {
        if self.is_rtl() {
            "rtl"
        } else {
            "ltr"
        }
    }

    /// Get all supported locales
    pub fn all() -> &'static [Locale] {
        &[
            Locale::English,
            Locale::German,
            Locale::French,
            Locale::Spanish,
            Locale::ChineseSimplified,
            Locale::Japanese,
            Locale::PortugueseBrazilian,
            Locale::Korean,
            Locale::Italian,
            Locale::Russian,
            Locale::Arabic,
        ]
    }

    /// Get all RTL locales
    pub fn rtl_locales() -> &'static [Locale] {
        &[Locale::Arabic]
    }

    /// Parse a locale from a string (BCP 47 code)
    pub fn from_code(code: &str) -> Option<Locale> {
        let code_lower = code.to_lowercase();
        let code_part = code_lower.split('-').next().unwrap_or(&code_lower);

        match code_part {
            "en" => Some(Locale::English),
            "de" => Some(Locale::German),
            "fr" => Some(Locale::French),
            "es" => Some(Locale::Spanish),
            "zh" => Some(Locale::ChineseSimplified),
            "ja" => Some(Locale::Japanese),
            "pt" => Some(Locale::PortugueseBrazilian),
            "ko" => Some(Locale::Korean),
            "it" => Some(Locale::Italian),
            "ru" => Some(Locale::Russian),
            "ar" => Some(Locale::Arabic),
            _ => None,
        }
    }
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code())
    }
}

impl std::str::FromStr for Locale {
    type Err = I18nError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Locale::from_code(s).ok_or_else(|| I18nError::UnsupportedLocale(s.to_string()))
    }
}

/// Errors that can occur during i18n operations
#[derive(Error, Debug)]
pub enum I18nError {
    /// The specified locale is not supported
    #[error("Unsupported locale: {0}")]
    UnsupportedLocale(String),

    /// Failed to detect system locale
    #[error("Failed to detect system locale")]
    LocaleDetectionFailed,

    /// Translation key not found
    #[error("Translation key not found: {0}")]
    KeyNotFound(String),
}

/// Result type for i18n operations
pub type Result<T> = std::result::Result<T, I18nError>;

/// Global locale state
static CURRENT_LOCALE: Lazy<RwLock<Locale>> =
    Lazy::new(|| RwLock::new(detect_system_locale().unwrap_or_default()));

/// Set the current locale
pub fn set_locale(locale: Locale) {
    if let Ok(mut current) = CURRENT_LOCALE.write() {
        *current = locale;
        rust_i18n::set_locale(locale.code());
    }
}

/// Get the current locale
pub fn get_locale() -> Locale {
    CURRENT_LOCALE.read().map(|l| *l).unwrap_or_default()
}

/// Detect the system locale
pub fn detect_system_locale() -> Option<Locale> {
    sys_locale::get_locale().and_then(|locale_str| Locale::from_code(&locale_str))
}

/// Initialize i18n with automatic locale detection
pub fn init() {
    if let Some(locale) = detect_system_locale() {
        set_locale(locale);
    } else {
        set_locale(Locale::English);
    }
    log::debug!("Initialized i18n with locale: {}", get_locale().code());
}

/// Initialize i18n with a specific locale
pub fn init_with_locale(locale: Locale) {
    set_locale(locale);
    log::debug!("Initialized i18n with locale: {}", locale.code());
}

/// Get all available locales
pub fn available_locales() -> &'static [Locale] {
    Locale::all()
}

/// Check if a locale is supported
pub fn is_locale_supported(code: &str) -> bool {
    Locale::from_code(code).is_some()
}

// ============================================================================
// Translation Categories
// ============================================================================

/// CLI-related translations
pub mod cli {
    use super::t;

    /// Get the welcome message
    pub fn welcome() -> String {
        t!("cli.welcome").to_string()
    }

    /// Get the help text
    pub fn help() -> String {
        t!("cli.help").to_string()
    }

    /// Get the version text
    pub fn version(version: &str) -> String {
        t!("cli.version", version = version).to_string()
    }

    /// Get chat command description
    pub fn chat_description() -> String {
        t!("cli.commands.chat").to_string()
    }

    /// Get serve command description
    pub fn serve_description() -> String {
        t!("cli.commands.serve").to_string()
    }

    /// Get config command description
    pub fn config_description() -> String {
        t!("cli.commands.config").to_string()
    }

    /// Get TUI command description
    pub fn tui_description() -> String {
        t!("cli.commands.tui").to_string()
    }
}

/// Error message translations
pub mod errors {
    use super::t;

    /// Get API key missing error
    pub fn api_key_missing(provider: &str) -> String {
        t!("errors.api_key_missing", provider = provider).to_string()
    }

    /// Get network error message
    pub fn network_error() -> String {
        t!("errors.network").to_string()
    }

    /// Get rate limit error message
    pub fn rate_limit(retry_after: u64) -> String {
        t!("errors.rate_limit", seconds = retry_after).to_string()
    }

    /// Get model not found error
    pub fn model_not_found(model: &str) -> String {
        t!("errors.model_not_found", model = model).to_string()
    }

    /// Get invalid configuration error
    pub fn invalid_config(field: &str) -> String {
        t!("errors.invalid_config", field = field).to_string()
    }

    /// Get file not found error
    pub fn file_not_found(path: &str) -> String {
        t!("errors.file_not_found", path = path).to_string()
    }

    /// Get permission denied error
    pub fn permission_denied(path: &str) -> String {
        t!("errors.permission_denied", path = path).to_string()
    }

    /// Get timeout error
    pub fn timeout() -> String {
        t!("errors.timeout").to_string()
    }

    /// Get authentication error
    pub fn authentication() -> String {
        t!("errors.authentication").to_string()
    }

    /// Get server error
    pub fn server_error() -> String {
        t!("errors.server").to_string()
    }
}

/// Status and progress messages
pub mod status {
    use super::t;

    /// Get connecting message
    pub fn connecting(provider: &str) -> String {
        t!("status.connecting", provider = provider).to_string()
    }

    /// Get connected message
    pub fn connected(provider: &str) -> String {
        t!("status.connected", provider = provider).to_string()
    }

    /// Get loading message
    pub fn loading() -> String {
        t!("status.loading").to_string()
    }

    /// Get processing message
    pub fn processing() -> String {
        t!("status.processing").to_string()
    }

    /// Get completed message
    pub fn completed() -> String {
        t!("status.completed").to_string()
    }

    /// Get failed message
    pub fn failed() -> String {
        t!("status.failed").to_string()
    }

    /// Get retrying message
    pub fn retrying(attempt: u32, max: u32) -> String {
        t!("status.retrying", attempt = attempt, max = max).to_string()
    }
}

/// Tool-related translations
pub mod tools {
    use super::t;

    /// Get tool execution started message
    pub fn execution_started(tool: &str) -> String {
        t!("tools.execution_started", tool = tool).to_string()
    }

    /// Get tool execution completed message
    pub fn execution_completed(tool: &str) -> String {
        t!("tools.execution_completed", tool = tool).to_string()
    }

    /// Get tool execution failed message
    pub fn execution_failed(tool: &str) -> String {
        t!("tools.execution_failed", tool = tool).to_string()
    }

    /// Get shell tool description
    pub fn shell_description() -> String {
        t!("tools.descriptions.shell").to_string()
    }

    /// Get filesystem tool description
    pub fn filesystem_description() -> String {
        t!("tools.descriptions.filesystem").to_string()
    }

    /// Get web tool description
    pub fn web_description() -> String {
        t!("tools.descriptions.web").to_string()
    }

    /// Get browser tool description
    pub fn browser_description() -> String {
        t!("tools.descriptions.browser").to_string()
    }
}

/// UI-related translations
pub mod ui {
    use super::t;

    /// Get send button text
    pub fn send() -> String {
        t!("ui.send").to_string()
    }

    /// Get cancel button text
    pub fn cancel() -> String {
        t!("ui.cancel").to_string()
    }

    /// Get save button text
    pub fn save() -> String {
        t!("ui.save").to_string()
    }

    /// Get delete button text
    pub fn delete() -> String {
        t!("ui.delete").to_string()
    }

    /// Get settings title
    pub fn settings() -> String {
        t!("ui.settings").to_string()
    }

    /// Get conversation title
    pub fn conversation() -> String {
        t!("ui.conversation").to_string()
    }

    /// Get new conversation text
    pub fn new_conversation() -> String {
        t!("ui.new_conversation").to_string()
    }

    /// Get export text
    pub fn export() -> String {
        t!("ui.export").to_string()
    }

    /// Get import text
    pub fn import() -> String {
        t!("ui.import").to_string()
    }

    /// Get placeholder text for chat input
    pub fn chat_placeholder() -> String {
        t!("ui.chat_placeholder").to_string()
    }

    /// Get dark mode text
    pub fn dark_mode() -> String {
        t!("ui.dark_mode").to_string()
    }

    /// Get light mode text
    pub fn light_mode() -> String {
        t!("ui.light_mode").to_string()
    }
}

/// Provider-related translations
pub mod providers {
    use super::t;

    /// Get provider description
    pub fn description(provider: &str) -> String {
        let key = format!("providers.{}.description", provider.to_lowercase());
        t!(&key).to_string()
    }

    /// Get provider name
    pub fn name(provider: &str) -> String {
        let key = format!("providers.{}.name", provider.to_lowercase());
        t!(&key).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_codes() {
        assert_eq!(Locale::English.code(), "en");
        assert_eq!(Locale::German.code(), "de");
        assert_eq!(Locale::French.code(), "fr");
        assert_eq!(Locale::Spanish.code(), "es");
        assert_eq!(Locale::ChineseSimplified.code(), "zh-CN");
        assert_eq!(Locale::Japanese.code(), "ja");
        assert_eq!(Locale::PortugueseBrazilian.code(), "pt-BR");
        assert_eq!(Locale::Korean.code(), "ko");
        assert_eq!(Locale::Italian.code(), "it");
        assert_eq!(Locale::Russian.code(), "ru");
        assert_eq!(Locale::Arabic.code(), "ar");
    }

    #[test]
    fn test_locale_from_code() {
        assert_eq!(Locale::from_code("en"), Some(Locale::English));
        assert_eq!(Locale::from_code("de"), Some(Locale::German));
        assert_eq!(Locale::from_code("de-DE"), Some(Locale::German));
        assert_eq!(Locale::from_code("en-US"), Some(Locale::English));
        assert_eq!(Locale::from_code("zh-CN"), Some(Locale::ChineseSimplified));
        assert_eq!(
            Locale::from_code("pt-BR"),
            Some(Locale::PortugueseBrazilian)
        );
        assert_eq!(Locale::from_code("ko"), Some(Locale::Korean));
        assert_eq!(Locale::from_code("it"), Some(Locale::Italian));
        assert_eq!(Locale::from_code("ru"), Some(Locale::Russian));
        assert_eq!(Locale::from_code("ar"), Some(Locale::Arabic));
        assert_eq!(Locale::from_code("xx"), None);
    }

    #[test]
    fn test_locale_native_names() {
        assert_eq!(Locale::German.native_name(), "Deutsch");
        assert_eq!(Locale::French.native_name(), "Français");
        assert_eq!(Locale::Japanese.native_name(), "日本語");
        assert_eq!(Locale::Korean.native_name(), "한국어");
        assert_eq!(Locale::Russian.native_name(), "Русский");
        assert_eq!(Locale::Arabic.native_name(), "العربية");
    }

    #[test]
    fn test_all_locales() {
        let all = Locale::all();
        assert_eq!(all.len(), 11);
        assert!(all.contains(&Locale::English));
        assert!(all.contains(&Locale::German));
        assert!(all.contains(&Locale::Japanese));
        assert!(all.contains(&Locale::PortugueseBrazilian));
        assert!(all.contains(&Locale::Korean));
        assert!(all.contains(&Locale::Italian));
        assert!(all.contains(&Locale::Russian));
        assert!(all.contains(&Locale::Arabic));
    }

    #[test]
    fn test_rtl_support() {
        assert!(!Locale::English.is_rtl());
        assert!(!Locale::German.is_rtl());
        assert!(Locale::Arabic.is_rtl());
        assert_eq!(Locale::English.direction(), "ltr");
        assert_eq!(Locale::Arabic.direction(), "rtl");
    }

    #[test]
    fn test_set_and_get_locale() {
        set_locale(Locale::German);
        assert_eq!(get_locale(), Locale::German);

        set_locale(Locale::English);
        assert_eq!(get_locale(), Locale::English);
    }

    #[test]
    fn test_is_locale_supported() {
        assert!(is_locale_supported("en"));
        assert!(is_locale_supported("de"));
        assert!(is_locale_supported("ja"));
        assert!(!is_locale_supported("xx"));
    }
}
