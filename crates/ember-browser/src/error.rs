//! Error types for browser automation.

use thiserror::Error;

/// Errors that can occur during browser automation.
#[derive(Debug, Error)]
pub enum BrowserError {
    /// Browser failed to launch.
    #[error("Failed to launch browser: {0}")]
    LaunchFailed(String),

    /// Browser connection failed.
    #[error("Browser connection failed: {0}")]
    ConnectionFailed(String),

    /// Page navigation failed.
    #[error("Navigation failed: {url} - {reason}")]
    NavigationFailed {
        /// The URL that failed to load.
        url: String,
        /// The reason for the failure.
        reason: String,
    },

    /// Element not found on page.
    #[error("Element not found: {selector}")]
    ElementNotFound {
        /// The CSS selector that was not found.
        selector: String,
    },

    /// Timeout waiting for element or page.
    #[error("Timeout waiting for {what}: {details}")]
    Timeout {
        /// What we were waiting for.
        what: String,
        /// Additional details about the timeout.
        details: String,
    },

    /// Screenshot capture failed.
    #[error("Screenshot failed: {0}")]
    ScreenshotFailed(String),

    /// JavaScript execution failed.
    #[error("JavaScript execution failed: {0}")]
    JsExecutionFailed(String),

    /// Invalid selector format.
    #[error("Invalid selector: {selector} - {reason}")]
    InvalidSelector {
        /// The invalid selector.
        selector: String,
        /// Why the selector is invalid.
        reason: String,
    },

    /// Browser is not initialized.
    #[error("Browser not initialized. Call launch() first.")]
    NotInitialized,

    /// Page is not available.
    #[error("No active page. Navigate to a URL first.")]
    NoActivePage,

    /// Action not supported.
    #[error("Action not supported: {0}")]
    UnsupportedAction(String),

    /// Chrome DevTools Protocol error.
    #[error("CDP error: {0}")]
    CdpError(String),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl From<chromiumoxide::error::CdpError> for BrowserError {
    fn from(err: chromiumoxide::error::CdpError) -> Self {
        BrowserError::CdpError(err.to_string())
    }
}

/// Result type for browser operations.
pub type Result<T> = std::result::Result<T, BrowserError>;
