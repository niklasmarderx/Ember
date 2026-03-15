//! Browser automation using Chrome DevTools Protocol.
//!
//! This module provides headless browser control for web automation tasks.

use crate::error::{BrowserError, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::Page;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Configuration for browser automation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig2 {
    /// Run browser in headless mode.
    #[serde(default = "default_headless")]
    pub headless: bool,

    /// Browser window width.
    #[serde(default = "default_width")]
    pub width: u32,

    /// Browser window height.
    #[serde(default = "default_height")]
    pub height: u32,

    /// Default timeout for operations in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// User agent string (optional).
    pub user_agent: Option<String>,

    /// Path to Chrome/Chromium executable (optional, auto-detected if not set).
    pub chrome_path: Option<String>,
}

fn default_headless() -> bool {
    true
}

fn default_width() -> u32 {
    1280
}

fn default_height() -> u32 {
    720
}

fn default_timeout() -> u64 {
    30
}

impl Default for BrowserConfig2 {
    fn default() -> Self {
        Self {
            headless: true,
            width: 1280,
            height: 720,
            timeout_secs: 30,
            user_agent: None,
            chrome_path: None,
        }
    }
}

/// Result of a browser action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserActionResult {
    /// Whether the action succeeded.
    pub success: bool,

    /// Description of what happened.
    pub message: String,

    /// Optional data returned by the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Screenshot as base64 (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,

    /// Current page URL after action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Page title after action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl BrowserActionResult {
    /// Create a success result.
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
            screenshot: None,
            url: None,
            title: None,
        }
    }

    /// Create a failure result.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
            screenshot: None,
            url: None,
            title: None,
        }
    }

    /// Add data to the result.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Add screenshot to the result.
    pub fn with_screenshot(mut self, screenshot: String) -> Self {
        self.screenshot = Some(screenshot);
        self
    }

    /// Add URL to the result.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Add title to the result.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Browser automation controller.
///
/// Provides high-level browser control for AI agent tasks.
pub struct BrowserController {
    config: BrowserConfig2,
    browser: RwLock<Option<Browser>>,
    page: RwLock<Option<Arc<Page>>>,
}

impl BrowserController {
    /// Create a new browser controller with default config.
    pub fn new() -> Self {
        Self::with_config(BrowserConfig2::default())
    }

    /// Create a browser controller with custom config.
    pub fn with_config(config: BrowserConfig2) -> Self {
        Self {
            config,
            browser: RwLock::new(None),
            page: RwLock::new(None),
        }
    }

    /// Launch the browser.
    pub async fn launch(&self) -> Result<()> {
        info!("Launching browser (headless: {})", self.config.headless);

        let mut builder = BrowserConfig::builder();

        if self.config.headless {
            builder = builder.with_head();
        }

        builder = builder
            .viewport(chromiumoxide::handler::viewport::Viewport {
                width: self.config.width,
                height: self.config.height,
                device_scale_factor: None,
                emulating_mobile: false,
                is_landscape: true,
                has_touch: false,
            })
            .request_timeout(Duration::from_secs(self.config.timeout_secs));

        // Note: user_agent is set via Chrome args if needed
        if let Some(ref _ua) = self.config.user_agent {
            // chromiumoxide 0.7 doesn't have user_agent on builder
            // Would need to use Chrome args: --user-agent="..."
        }

        let config = builder
            .build()
            .map_err(|e| BrowserError::LaunchFailed(format!("Invalid browser config: {}", e)))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| BrowserError::LaunchFailed(e.to_string()))?;

        // Spawn handler task
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                debug!("Browser event: {:?}", event);
            }
        });

        *self.browser.write().await = Some(browser);
        info!("Browser launched successfully");

        Ok(())
    }

    /// Close the browser.
    pub async fn close(&self) -> Result<()> {
        info!("Closing browser");

        // Drop page first
        *self.page.write().await = None;

        // Drop browser
        *self.browser.write().await = None;

        Ok(())
    }

    /// Navigate to a URL.
    pub async fn navigate(&self, url: &str) -> Result<BrowserActionResult> {
        info!("Navigating to: {}", url);

        let browser = self.browser.read().await;
        let browser = browser.as_ref().ok_or(BrowserError::NotInitialized)?;

        let page = browser
            .new_page(url)
            .await
            .map_err(|e| BrowserError::NavigationFailed {
                url: url.to_string(),
                reason: e.to_string(),
            })?;

        // Wait for page to load
        page.wait_for_navigation().await.ok();

        let current_url = page.url().await.ok().flatten().unwrap_or_default();
        let title = page.get_title().await.ok().flatten().unwrap_or_default();

        *self.page.write().await = Some(Arc::new(page));

        Ok(
            BrowserActionResult::success(format!("Navigated to {}", url))
                .with_url(current_url)
                .with_title(title),
        )
    }

    /// Click an element by CSS selector.
    pub async fn click(&self, selector: &str) -> Result<BrowserActionResult> {
        info!("Clicking element: {}", selector);

        let page = self.get_page().await?;

        let element =
            page.find_element(selector)
                .await
                .map_err(|_| BrowserError::ElementNotFound {
                    selector: selector.to_string(),
                })?;

        element
            .click()
            .await
            .map_err(|e| BrowserError::CdpError(e.to_string()))?;

        // Small delay for any animations/transitions
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(BrowserActionResult::success(format!(
            "Clicked element: {}",
            selector
        )))
    }

    /// Type text into an element.
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<BrowserActionResult> {
        info!("Typing into element: {}", selector);

        let page = self.get_page().await?;

        let element =
            page.find_element(selector)
                .await
                .map_err(|_| BrowserError::ElementNotFound {
                    selector: selector.to_string(),
                })?;

        element.click().await.ok(); // Focus first
        element
            .type_str(text)
            .await
            .map_err(|e| BrowserError::CdpError(e.to_string()))?;

        Ok(BrowserActionResult::success(format!(
            "Typed {} characters into {}",
            text.len(),
            selector
        )))
    }

    /// Get text content of an element.
    pub async fn get_text(&self, selector: &str) -> Result<BrowserActionResult> {
        let page = self.get_page().await?;

        let element =
            page.find_element(selector)
                .await
                .map_err(|_| BrowserError::ElementNotFound {
                    selector: selector.to_string(),
                })?;

        let text = element
            .inner_text()
            .await
            .map_err(|e| BrowserError::CdpError(e.to_string()))?
            .unwrap_or_default();

        Ok(BrowserActionResult::success("Retrieved text content")
            .with_data(serde_json::json!({ "text": text })))
    }

    /// Take a screenshot of the current page.
    pub async fn screenshot(&self) -> Result<BrowserActionResult> {
        info!("Taking screenshot");

        let page = self.get_page().await?;

        let screenshot_data = page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .full_page(false)
                    .build(),
            )
            .await
            .map_err(|e| BrowserError::ScreenshotFailed(e.to_string()))?;

        let base64_screenshot = BASE64.encode(&screenshot_data);

        Ok(BrowserActionResult::success("Screenshot captured").with_screenshot(base64_screenshot))
    }

    /// Execute JavaScript on the page.
    pub async fn evaluate(&self, script: &str) -> Result<BrowserActionResult> {
        debug!("Executing JavaScript");

        let page = self.get_page().await?;

        let result = page
            .evaluate(script)
            .await
            .map_err(|e| BrowserError::JsExecutionFailed(e.to_string()))?;

        let value: serde_json::Value = result.into_value().unwrap_or(serde_json::Value::Null);

        Ok(BrowserActionResult::success("JavaScript executed").with_data(value))
    }

    /// Wait for an element to appear.
    pub async fn wait_for_selector(
        &self,
        selector: &str,
        timeout_ms: Option<u64>,
    ) -> Result<BrowserActionResult> {
        info!("Waiting for selector: {}", selector);

        let page = self.get_page().await?;
        let timeout = timeout_ms.unwrap_or(self.config.timeout_secs * 1000);

        let start = std::time::Instant::now();
        let timeout_duration = Duration::from_millis(timeout);

        while start.elapsed() < timeout_duration {
            if page.find_element(selector).await.is_ok() {
                return Ok(BrowserActionResult::success(format!(
                    "Element found: {}",
                    selector
                )));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(BrowserError::Timeout {
            what: "selector".to_string(),
            details: format!("{} ({}ms)", selector, timeout),
        })
    }

    /// Scroll the page.
    pub async fn scroll(
        &self,
        direction: ScrollDirection,
        amount: u32,
    ) -> Result<BrowserActionResult> {
        let page = self.get_page().await?;

        let script = match direction {
            ScrollDirection::Down => format!("window.scrollBy(0, {})", amount),
            ScrollDirection::Up => format!("window.scrollBy(0, -{})", amount),
            ScrollDirection::Left => format!("window.scrollBy(-{}, 0)", amount),
            ScrollDirection::Right => format!("window.scrollBy({}, 0)", amount),
        };

        page.evaluate(script.as_str())
            .await
            .map_err(|e| BrowserError::JsExecutionFailed(e.to_string()))?;

        Ok(BrowserActionResult::success(format!(
            "Scrolled {:?} by {} pixels",
            direction, amount
        )))
    }

    /// Get current page URL.
    pub async fn get_url(&self) -> Result<String> {
        let page = self.get_page().await?;
        page.url()
            .await
            .ok()
            .flatten()
            .ok_or(BrowserError::NoActivePage)
    }

    /// Get current page title.
    pub async fn get_title(&self) -> Result<String> {
        let page = self.get_page().await?;
        page.get_title()
            .await
            .ok()
            .flatten()
            .ok_or(BrowserError::NoActivePage)
    }

    /// Get the current page, or return an error if not available.
    async fn get_page(&self) -> Result<Arc<Page>> {
        self.page
            .read()
            .await
            .clone()
            .ok_or(BrowserError::NoActivePage)
    }
}

impl Default for BrowserController {
    fn default() -> Self {
        Self::new()
    }
}

/// Scroll direction for scroll operations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollDirection {
    /// Scroll up.
    Up,
    /// Scroll down.
    Down,
    /// Scroll left.
    Left,
    /// Scroll right.
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_config_default() {
        let config = BrowserConfig2::default();
        assert!(config.headless);
        assert_eq!(config.width, 1280);
        assert_eq!(config.height, 720);
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_action_result_builder() {
        let result = BrowserActionResult::success("Test")
            .with_url("https://example.com")
            .with_title("Example");

        assert!(result.success);
        assert_eq!(result.url, Some("https://example.com".to_string()));
        assert_eq!(result.title, Some("Example".to_string()));
    }
}
