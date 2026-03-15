//! Browser tool implementation for Ember agent.
//!
//! Provides a ToolHandler trait implementation for browser automation.

use crate::browser::{BrowserActionResult, BrowserConfig2, BrowserController, ScrollDirection};
use crate::error::{BrowserError, Result};
use async_trait::async_trait;
use ember_tools::{ToolDefinition, ToolHandler, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Browser tool for AI agent use.
///
/// This tool exposes browser automation capabilities to the agent.
pub struct BrowserTool {
    controller: Arc<RwLock<BrowserController>>,
    config: BrowserConfig2,
}

impl BrowserTool {
    /// Create a new browser tool with default configuration.
    pub fn new() -> Self {
        Self::with_config(BrowserConfig2::default())
    }

    /// Create a browser tool with custom configuration.
    pub fn with_config(config: BrowserConfig2) -> Self {
        Self {
            controller: Arc::new(RwLock::new(BrowserController::with_config(config.clone()))),
            config,
        }
    }

    /// Execute a browser action based on the action type.
    async fn execute_action(&self, action: BrowserAction) -> Result<BrowserActionResult> {
        match action {
            BrowserAction::Launch => {
                let mut ctrl = self.controller.write().await;
                *ctrl = BrowserController::with_config(self.config.clone());
                ctrl.launch().await?;
                Ok(BrowserActionResult::success("Browser launched"))
            }
            BrowserAction::Close => {
                let ctrl = self.controller.write().await;
                ctrl.close().await?;
                Ok(BrowserActionResult::success("Browser closed"))
            }
            BrowserAction::Navigate { url } => {
                let controller = self.controller.read().await;
                controller.navigate(&url).await
            }
            BrowserAction::Click { selector } => {
                let controller = self.controller.read().await;
                controller.click(&selector).await
            }
            BrowserAction::Type { selector, text } => {
                let controller = self.controller.read().await;
                controller.type_text(&selector, &text).await
            }
            BrowserAction::GetText { selector } => {
                let controller = self.controller.read().await;
                controller.get_text(&selector).await
            }
            BrowserAction::Screenshot => {
                let controller = self.controller.read().await;
                controller.screenshot().await
            }
            BrowserAction::Evaluate { script } => {
                let controller = self.controller.read().await;
                controller.evaluate(&script).await
            }
            BrowserAction::WaitForSelector { selector, timeout } => {
                let controller = self.controller.read().await;
                controller.wait_for_selector(&selector, timeout).await
            }
            BrowserAction::Scroll { direction, amount } => {
                let dir = match direction.to_lowercase().as_str() {
                    "up" => ScrollDirection::Up,
                    "down" => ScrollDirection::Down,
                    "left" => ScrollDirection::Left,
                    "right" => ScrollDirection::Right,
                    _ => {
                        return Err(BrowserError::UnsupportedAction(format!(
                            "Unknown scroll direction: {}",
                            direction
                        )))
                    }
                };
                let controller = self.controller.read().await;
                controller.scroll(dir, amount).await
            }
            BrowserAction::GetUrl => {
                let controller = self.controller.read().await;
                let url = controller.get_url().await?;
                Ok(BrowserActionResult::success("Retrieved URL").with_url(url))
            }
            BrowserAction::GetTitle => {
                let controller = self.controller.read().await;
                let title = controller.get_title().await?;
                Ok(BrowserActionResult::success("Retrieved title").with_title(title))
            }
        }
    }
}

impl Default for BrowserTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Browser actions that can be performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BrowserAction {
    /// Launch the browser.
    Launch,

    /// Close the browser.
    Close,

    /// Navigate to a URL.
    Navigate {
        /// The URL to navigate to.
        url: String,
    },

    /// Click an element by CSS selector.
    Click {
        /// CSS selector for the element to click.
        selector: String,
    },

    /// Type text into an element.
    Type {
        /// CSS selector for the input element.
        selector: String,
        /// Text to type into the element.
        text: String,
    },

    /// Get text content of an element.
    GetText {
        /// CSS selector for the element.
        selector: String,
    },

    /// Take a screenshot.
    Screenshot,

    /// Execute JavaScript.
    Evaluate {
        /// JavaScript code to execute.
        script: String,
    },

    /// Wait for a selector to appear.
    WaitForSelector {
        /// CSS selector to wait for.
        selector: String,
        /// Timeout in milliseconds (optional).
        #[serde(default)]
        timeout: Option<u64>,
    },

    /// Scroll the page.
    Scroll {
        /// Scroll direction: up, down, left, right.
        direction: String,
        /// Scroll amount in pixels.
        amount: u32,
    },

    /// Get current page URL.
    GetUrl,

    /// Get current page title.
    GetTitle,
}

#[async_trait]
impl ToolHandler for BrowserTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "browser",
            "Control a headless browser for web automation tasks. \
             Can navigate pages, click elements, type text, take screenshots, and execute JavaScript.",
        )
        .with_parameters(serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The browser action to perform",
                    "enum": [
                        "launch", "close", "navigate", "click", "type",
                        "get_text", "screenshot", "evaluate", "wait_for_selector",
                        "scroll", "get_url", "get_title"
                    ]
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (for 'navigate' action)"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for element (for 'click', 'type', 'get_text', 'wait_for_selector')"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type (for 'type' action)"
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript to execute (for 'evaluate' action)"
                },
                "direction": {
                    "type": "string",
                    "description": "Scroll direction: up, down, left, right (for 'scroll' action)",
                    "enum": ["up", "down", "left", "right"]
                },
                "amount": {
                    "type": "integer",
                    "description": "Scroll amount in pixels (for 'scroll' action)"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (for 'wait_for_selector')"
                }
            },
            "required": ["action"]
        }))
    }

    async fn execute(&self, arguments: Value) -> ember_tools::Result<ToolOutput> {
        debug!("Browser tool executing with args: {:?}", arguments);

        // Parse the action from arguments
        let action: BrowserAction = match serde_json::from_value(arguments.clone()) {
            Ok(a) => a,
            Err(e) => {
                return Ok(ToolOutput::error(format!("Invalid browser action: {}", e)));
            }
        };

        info!("Executing browser action: {:?}", action);

        match self.execute_action(action).await {
            Ok(result) => {
                let output_json = serde_json::to_string_pretty(&result).unwrap_or_else(|_| {
                    format!("success: {}, message: {}", result.success, result.message)
                });

                if result.success {
                    if let Some(data) = result.data {
                        Ok(ToolOutput::success_with_data(output_json, data))
                    } else {
                        Ok(ToolOutput::success(output_json))
                    }
                } else {
                    Ok(ToolOutput::error(result.message))
                }
            }
            Err(e) => Ok(ToolOutput::error(format!("Browser action failed: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_tool_definition() {
        let tool = BrowserTool::new();
        let def = tool.definition();

        assert_eq!(def.name, "browser");
        assert!(def.parameters["properties"]["action"].is_object());
    }

    #[test]
    fn test_parse_browser_action() {
        let json = serde_json::json!({
            "action": "navigate",
            "url": "https://example.com"
        });

        let action: BrowserAction = serde_json::from_value(json).unwrap();
        match action {
            BrowserAction::Navigate { url } => assert_eq!(url, "https://example.com"),
            _ => panic!("Expected Navigate action"),
        }
    }

    #[test]
    fn test_parse_click_action() {
        let json = serde_json::json!({
            "action": "click",
            "selector": "#submit-button"
        });

        let action: BrowserAction = serde_json::from_value(json).unwrap();
        match action {
            BrowserAction::Click { selector } => assert_eq!(selector, "#submit-button"),
            _ => panic!("Expected Click action"),
        }
    }
}
