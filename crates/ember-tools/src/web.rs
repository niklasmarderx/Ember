//! Web/HTTP request tool.

use crate::{registry::ToolOutput, Error, Result, ToolDefinition, ToolHandler};
use async_trait::async_trait;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tracing::debug;

/// Configuration for the web tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Request timeout in seconds
    pub timeout_secs: u64,

    /// Maximum response body size in bytes
    pub max_response_bytes: usize,

    /// Default headers to include in all requests
    pub default_headers: HashMap<String, String>,

    /// Allowed URL patterns (empty = all allowed)
    pub allowed_urls: Vec<String>,

    /// Blocked URL patterns
    pub blocked_urls: Vec<String>,

    /// User agent string
    pub user_agent: String,

    /// Whether to follow redirects
    pub follow_redirects: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_response_bytes: 10 * 1024 * 1024, // 10MB
            default_headers: HashMap::new(),
            allowed_urls: Vec::new(),
            blocked_urls: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "169.254".to_string(), // Link-local
                "10.".to_string(),     // Private
                "172.16".to_string(),  // Private
                "192.168".to_string(), // Private
            ],
            user_agent: "Ember-Agent/0.1".to_string(),
            follow_redirects: true,
        }
    }
}

/// Web/HTTP request tool.
pub struct WebTool {
    config: WebConfig,
    client: Client,
    enabled: bool,
}

impl WebTool {
    /// Create a new web tool with default configuration.
    pub fn new() -> Self {
        let config = WebConfig::default();
        let client = Self::build_client(&config);
        Self {
            config,
            client,
            enabled: true,
        }
    }

    /// Create a web tool with custom configuration.
    pub fn with_config(config: WebConfig) -> Self {
        let client = Self::build_client(&config);
        Self {
            config,
            client,
            enabled: true,
        }
    }

    /// Build the HTTP client.
    fn build_client(config: &WebConfig) -> Client {
        Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent(&config.user_agent)
            .redirect(if config.follow_redirects {
                reqwest::redirect::Policy::limited(10)
            } else {
                reqwest::redirect::Policy::none()
            })
            .build()
            .unwrap_or_default()
    }

    /// Allow internal/localhost requests.
    pub fn allow_localhost(mut self) -> Self {
        self.config.blocked_urls.retain(|u| {
            !u.contains("localhost") && !u.contains("127.0.0.1") && !u.contains("0.0.0.0")
        });
        self
    }

    /// Set timeout.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.config.timeout_secs = secs;
        self.client = Self::build_client(&self.config);
        self
    }

    /// Add a default header.
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.default_headers.insert(key.into(), value.into());
        self
    }

    /// Set the enabled state.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Validate that a URL is allowed.
    fn validate_url(&self, url: &str) -> Result<()> {
        // Check blocked patterns
        for blocked in &self.config.blocked_urls {
            if url.contains(blocked) {
                return Err(Error::HttpRequest(format!(
                    "URL contains blocked pattern: {}",
                    blocked
                )));
            }
        }

        // Check allowed patterns if set
        if !self.config.allowed_urls.is_empty() {
            let allowed = self
                .config
                .allowed_urls
                .iter()
                .any(|pattern| url.starts_with(pattern) || url.contains(pattern));
            if !allowed {
                return Err(Error::HttpRequest("URL not in allowed list".to_string()));
            }
        }

        Ok(())
    }

    /// Make an HTTP request.
    async fn request(
        &self,
        method: Method,
        url: &str,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    ) -> Result<WebResponse> {
        self.validate_url(url)?;

        debug!(method = %method, url = url, "Making HTTP request");

        let mut request = self.client.request(method.clone(), url);

        // Add default headers
        for (key, value) in &self.config.default_headers {
            request = request.header(key, value);
        }

        // Add custom headers
        if let Some(hdrs) = headers {
            for (key, value) in hdrs {
                request = request.header(&key, &value);
            }
        }

        // Add body for POST/PUT/PATCH
        if let Some(body_content) = body {
            request = request.body(body_content);
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::HttpRequest(format!("Request failed: {}", e)))?;

        let status = response.status();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Read body with size limit
        let body = response
            .bytes()
            .await
            .map_err(|e| Error::HttpRequest(format!("Failed to read response: {}", e)))?;

        if body.len() > self.config.max_response_bytes {
            return Err(Error::HttpRequest(format!(
                "Response too large: {} bytes (max: {})",
                body.len(),
                self.config.max_response_bytes
            )));
        }

        let body_str = String::from_utf8_lossy(&body).to_string();

        Ok(WebResponse {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("").to_string(),
            headers,
            body: body_str,
            success: status.is_success(),
        })
    }
}

impl Default for WebTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for WebTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "web",
            "Make HTTP requests to web APIs. Supports GET, POST, PUT, PATCH, DELETE methods.",
        )
        .with_parameters(serde_json::json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "description": "HTTP method",
                    "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"]
                },
                "url": {
                    "type": "string",
                    "description": "The URL to request"
                },
                "headers": {
                    "type": "object",
                    "description": "Additional HTTP headers",
                    "additionalProperties": { "type": "string" }
                },
                "body": {
                    "type": "string",
                    "description": "Request body (for POST, PUT, PATCH)"
                }
            },
            "required": ["method", "url"]
        }))
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        let method_str = arguments
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_arguments("web", "Missing 'method' parameter"))?;

        let url = arguments
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_arguments("web", "Missing 'url' parameter"))?;

        let method = match method_str.to_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "PATCH" => Method::PATCH,
            "DELETE" => Method::DELETE,
            "HEAD" => Method::HEAD,
            _ => {
                return Err(Error::invalid_arguments(
                    "web",
                    format!("Invalid HTTP method: {}", method_str),
                ))
            }
        };

        let headers: Option<HashMap<String, String>> = arguments
            .get("headers")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let body = arguments
            .get("body")
            .and_then(|v| v.as_str())
            .map(String::from);

        let response = self.request(method, url, headers, body).await?;

        let output = if response.success {
            format!(
                "HTTP {} {}\n\n{}",
                response.status, response.status_text, response.body
            )
        } else {
            format!(
                "HTTP {} {} (Error)\n\n{}",
                response.status, response.status_text, response.body
            )
        };

        Ok(ToolOutput::success_with_data(
            output,
            serde_json::json!({
                "status": response.status,
                "status_text": response.status_text,
                "headers": response.headers,
                "success": response.success
            }),
        ))
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Response from an HTTP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebResponse {
    /// HTTP status code
    pub status: u16,

    /// Status text
    pub status_text: String,

    /// Response headers
    pub headers: HashMap<String, String>,

    /// Response body
    pub body: String,

    /// Whether the request was successful (2xx status)
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_validation_blocked() {
        let tool = WebTool::new();

        assert!(tool.validate_url("http://localhost/api").is_err());
        assert!(tool.validate_url("http://127.0.0.1/api").is_err());
        assert!(tool.validate_url("http://192.168.1.1/api").is_err());
    }

    #[test]
    fn test_url_validation_allowed() {
        let tool = WebTool::new();

        // External URLs should be allowed
        assert!(tool.validate_url("https://api.example.com/data").is_ok());
        assert!(tool.validate_url("https://httpbin.org/get").is_ok());
    }

    #[test]
    fn test_allow_localhost() {
        let tool = WebTool::new().allow_localhost();

        assert!(tool.validate_url("http://localhost/api").is_ok());
        assert!(tool.validate_url("http://127.0.0.1/api").is_ok());
    }

    #[test]
    fn test_default_config() {
        let config = WebConfig::default();

        assert_eq!(config.timeout_secs, 30);
        assert!(config.follow_redirects);
        assert!(!config.blocked_urls.is_empty());
    }

    // Integration tests would require a mock HTTP server
    // or use httpbin.org for real network tests
}
