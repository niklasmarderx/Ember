//! # API Tool
//!
//! Tool for making HTTP API requests with authentication and response handling.
//!
//! Features:
//! - RESTful HTTP methods (GET, POST, PUT, PATCH, DELETE)
//! - Multiple authentication schemes (API key, Bearer, Basic, OAuth)
//! - Request/Response transformation
//! - Rate limiting support
//! - Retry with backoff

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, warn};

use crate::{Error, Result, ToolDefinition, ToolHandler, ToolOutput};

/// API request tool.
#[derive(Debug, Clone)]
pub struct ApiTool {
    config: ApiConfig,
    #[cfg(feature = "web")]
    client: reqwest::Client,
}

/// Configuration for API tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Default timeout in seconds
    pub timeout_secs: u64,
    /// Maximum response size in bytes
    pub max_response_size: usize,
    /// Allowed hosts (empty = all allowed)
    pub allowed_hosts: Vec<String>,
    /// Denied hosts
    pub denied_hosts: Vec<String>,
    /// Default headers
    pub default_headers: HashMap<String, String>,
    /// Maximum retries
    pub max_retries: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_response_size: 10 * 1024 * 1024, // 10MB
            allowed_hosts: vec![],
            denied_hosts: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "::1".to_string(),
            ],
            default_headers: HashMap::new(),
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// GET request
    Get,
    /// POST request
    Post,
    /// PUT request
    Put,
    /// PATCH request
    Patch,
    /// DELETE request
    Delete,
    /// HEAD request
    Head,
    /// OPTIONS request
    Options,
}

impl HttpMethod {
    /// Check if method typically has a body.
    pub fn has_body(&self) -> bool {
        matches!(self, HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch)
    }
}

/// Authentication scheme.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthScheme {
    /// No authentication
    #[default]
    None,
    /// API key in header
    ApiKey {
        /// Header name
        header: String,
        /// API key value
        key: String,
    },
    /// Bearer token
    Bearer {
        /// Token value
        token: String,
    },
    /// Basic authentication
    Basic {
        /// Username
        username: String,
        /// Password
        password: String,
    },
    /// Custom header
    Custom {
        /// Headers to add
        headers: HashMap<String, String>,
    },
}

/// API request configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    /// HTTP method
    pub method: HttpMethod,
    /// URL
    pub url: String,
    /// Headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Query parameters
    #[serde(default)]
    pub query: HashMap<String, String>,
    /// Request body (for POST, PUT, PATCH)
    pub body: Option<Value>,
    /// Authentication
    #[serde(default)]
    pub auth: AuthScheme,
    /// Timeout override in seconds
    pub timeout: Option<u64>,
    /// Follow redirects
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
}

fn default_true() -> bool {
    true
}

/// API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    /// HTTP status code
    pub status: u16,
    /// Status text
    pub status_text: String,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body
    pub body: Value,
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Content type
    pub content_type: Option<String>,
    /// Content length
    pub content_length: Option<usize>,
}

impl ApiTool {
    /// Create a new API tool with default configuration.
    #[cfg(feature = "web")]
    pub fn new() -> Self {
        Self::with_config(ApiConfig::default())
    }

    /// Create with custom configuration.
    #[cfg(feature = "web")]
    pub fn with_config(config: ApiConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Create a new API tool (without web feature).
    #[cfg(not(feature = "web"))]
    pub fn new() -> Self {
        Self {
            config: ApiConfig::default(),
        }
    }

    /// Check if host is allowed.
    fn is_host_allowed(&self, url: &str) -> Result<()> {
        let parsed = url::Url::parse(url)
            .map_err(|e| Error::invalid_arguments("api", format!("Invalid URL: {}", e)))?;

        let host = parsed.host_str().unwrap_or("");

        // Check denied hosts
        for denied in &self.config.denied_hosts {
            if host == denied || host.ends_with(&format!(".{}", denied)) {
                return Err(Error::invalid_arguments(
                    "api",
                    format!("Host '{}' is not allowed", host),
                ));
            }
        }

        // Check allowed hosts (if not empty)
        if !self.config.allowed_hosts.is_empty() {
            let allowed = self
                .config
                .allowed_hosts
                .iter()
                .any(|allowed| host == allowed || host.ends_with(&format!(".{}", allowed)));

            if !allowed {
                return Err(Error::invalid_arguments(
                    "api",
                    format!("Host '{}' is not in allowed list", host),
                ));
            }
        }

        Ok(())
    }

    /// Execute API request.
    #[cfg(feature = "web")]
    pub async fn execute_request(&self, request: ApiRequest) -> Result<ApiResponse> {
        self.is_host_allowed(&request.url)?;

        let start = std::time::Instant::now();
        let mut retries = 0;

        loop {
            match self.do_request(&request).await {
                Ok(response) => {
                    let elapsed = start.elapsed().as_millis() as u64;
                    return Ok(ApiResponse {
                        response_time_ms: elapsed,
                        ..response
                    });
                }
                Err(e) => {
                    retries += 1;
                    if retries >= self.config.max_retries {
                        return Err(e);
                    }
                    warn!(
                        "API request failed, retrying ({}/{}): {}",
                        retries, self.config.max_retries, e
                    );
                    tokio::time::sleep(Duration::from_millis(
                        self.config.retry_delay_ms * u64::from(retries),
                    ))
                    .await;
                }
            }
        }
    }

    #[cfg(feature = "web")]
    async fn do_request(&self, request: &ApiRequest) -> Result<ApiResponse> {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

        // Build URL with query parameters
        let mut url = reqwest::Url::parse(&request.url)
            .map_err(|e| Error::invalid_arguments("api", format!("Invalid URL: {}", e)))?;

        for (key, value) in &request.query {
            url.query_pairs_mut().append_pair(key, value);
        }

        // Build request
        let mut req_builder = match request.method {
            HttpMethod::Get => self.client.get(url),
            HttpMethod::Post => self.client.post(url),
            HttpMethod::Put => self.client.put(url),
            HttpMethod::Patch => self.client.patch(url),
            HttpMethod::Delete => self.client.delete(url),
            HttpMethod::Head => self.client.head(url),
            HttpMethod::Options => self.client.request(reqwest::Method::OPTIONS, url),
        };

        // Apply timeout
        if let Some(timeout) = request.timeout {
            req_builder = req_builder.timeout(Duration::from_secs(timeout));
        }

        // Apply headers
        let mut headers = HeaderMap::new();

        // Default headers
        for (key, value) in &self.config.default_headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(key.as_bytes()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(name, val);
            }
        }

        // Request headers
        for (key, value) in &request.headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(key.as_bytes()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(name, val);
            }
        }

        // Authentication
        match &request.auth {
            AuthScheme::None => {}
            AuthScheme::ApiKey { header, key } => {
                if let (Ok(name), Ok(val)) = (
                    HeaderName::from_bytes(header.as_bytes()),
                    HeaderValue::from_str(key),
                ) {
                    headers.insert(name, val);
                }
            }
            AuthScheme::Bearer { token } => {
                if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", token)) {
                    headers.insert(reqwest::header::AUTHORIZATION, val);
                }
            }
            AuthScheme::Basic { username, password } => {
                let credentials = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    format!("{}:{}", username, password),
                );
                if let Ok(val) = HeaderValue::from_str(&format!("Basic {}", credentials)) {
                    headers.insert(reqwest::header::AUTHORIZATION, val);
                }
            }
            AuthScheme::Custom { headers: custom } => {
                for (key, value) in custom {
                    if let (Ok(name), Ok(val)) = (
                        HeaderName::from_bytes(key.as_bytes()),
                        HeaderValue::from_str(value),
                    ) {
                        headers.insert(name, val);
                    }
                }
            }
        }

        req_builder = req_builder.headers(headers);

        // Apply body
        if let Some(body) = &request.body {
            req_builder = req_builder.json(body);
        }

        // Execute request
        let response = req_builder
            .send()
            .await
            .map_err(|e| Error::execution_failed("api", format!("Request failed: {}", e)))?;

        // Extract response info
        let status = response.status().as_u16();
        let status_text = response
            .status()
            .canonical_reason()
            .unwrap_or("Unknown")
            .to_string();

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let _content_length: Option<usize> = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        // Collect headers
        let mut resp_headers = HashMap::new();
        for (name, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                resp_headers.insert(name.to_string(), v.to_string());
            }
        }

        // Read body with size limit
        let bytes = response.bytes().await.map_err(|e| {
            Error::execution_failed("api", format!("Failed to read response: {}", e))
        })?;

        if bytes.len() > self.config.max_response_size {
            return Err(Error::execution_failed(
                "api",
                format!(
                    "Response too large: {} bytes (max: {} bytes)",
                    bytes.len(),
                    self.config.max_response_size
                ),
            ));
        }

        // Parse body
        let body = if let Some(ct) = &content_type {
            if ct.contains("application/json") {
                serde_json::from_slice(&bytes)
                    .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).to_string()))
            } else {
                Value::String(String::from_utf8_lossy(&bytes).to_string())
            }
        } else {
            Value::String(String::from_utf8_lossy(&bytes).to_string())
        };

        Ok(ApiResponse {
            status,
            status_text,
            headers: resp_headers,
            body,
            response_time_ms: 0, // Will be set by caller
            content_type,
            content_length: Some(bytes.len()),
        })
    }

    #[cfg(not(feature = "web"))]
    pub async fn execute_request(&self, _request: ApiRequest) -> Result<ApiResponse> {
        Err(Error::execution_failed(
            "api",
            "API tool requires the 'web' feature to be enabled",
        ))
    }
}

#[cfg(feature = "web")]
impl Default for ApiTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for ApiTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "api".to_string(),
            description: "Make HTTP API requests with authentication and response handling"
                .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["method", "url"],
                "properties": {
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
                        "description": "HTTP method"
                    },
                    "url": {
                        "type": "string",
                        "description": "Request URL"
                    },
                    "headers": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Request headers"
                    },
                    "query": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Query parameters"
                    },
                    "body": {
                        "description": "Request body (JSON)"
                    },
                    "auth": {
                        "type": "object",
                        "description": "Authentication configuration",
                        "properties": {
                            "type": {
                                "type": "string",
                                "enum": ["none", "api_key", "bearer", "basic", "custom"],
                                "description": "Authentication type"
                            },
                            "header": {
                                "type": "string",
                                "description": "Header name for API key"
                            },
                            "key": {
                                "type": "string",
                                "description": "API key value"
                            },
                            "token": {
                                "type": "string",
                                "description": "Bearer token"
                            },
                            "username": {
                                "type": "string",
                                "description": "Username for basic auth"
                            },
                            "password": {
                                "type": "string",
                                "description": "Password for basic auth"
                            },
                            "headers": {
                                "type": "object",
                                "additionalProperties": { "type": "string" },
                                "description": "Custom auth headers"
                            }
                        }
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Request timeout in seconds"
                    },
                    "follow_redirects": {
                        "type": "boolean",
                        "default": true,
                        "description": "Follow HTTP redirects"
                    }
                }
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        debug!("API tool called with: {:?}", arguments);

        let request: ApiRequest = serde_json::from_value(arguments)
            .map_err(|e| Error::invalid_arguments("api", format!("Invalid arguments: {}", e)))?;

        let response = self.execute_request(request).await?;

        Ok(ToolOutput::success(
            serde_json::to_string(&response).unwrap_or_default(),
        ))
    }
}

/// Builder for API requests.
#[derive(Debug, Default)]
pub struct ApiRequestBuilder {
    method: Option<HttpMethod>,
    url: Option<String>,
    headers: HashMap<String, String>,
    query: HashMap<String, String>,
    body: Option<Value>,
    auth: AuthScheme,
    timeout: Option<u64>,
    follow_redirects: bool,
}

impl ApiRequestBuilder {
    /// Create a new request builder.
    pub fn new() -> Self {
        Self {
            follow_redirects: true,
            ..Default::default()
        }
    }

    /// Set HTTP method.
    pub fn method(mut self, method: HttpMethod) -> Self {
        self.method = Some(method);
        self
    }

    /// Set URL.
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Add header.
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Add query parameter.
    pub fn query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.insert(key.into(), value.into());
        self
    }

    /// Set JSON body.
    pub fn json(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    /// Set authentication.
    pub fn auth(mut self, auth: AuthScheme) -> Self {
        self.auth = auth;
        self
    }

    /// Set bearer token.
    pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = AuthScheme::Bearer {
            token: token.into(),
        };
        self
    }

    /// Set API key.
    pub fn api_key(mut self, header: impl Into<String>, key: impl Into<String>) -> Self {
        self.auth = AuthScheme::ApiKey {
            header: header.into(),
            key: key.into(),
        };
        self
    }

    /// Set timeout.
    pub fn timeout(mut self, seconds: u64) -> Self {
        self.timeout = Some(seconds);
        self
    }

    /// Build the request.
    pub fn build(self) -> Result<ApiRequest> {
        let method = self
            .method
            .ok_or_else(|| Error::invalid_arguments("api", "Method is required"))?;

        let url = self
            .url
            .ok_or_else(|| Error::invalid_arguments("api", "URL is required"))?;

        Ok(ApiRequest {
            method,
            url,
            headers: self.headers,
            query: self.query,
            body: self.body,
            auth: self.auth,
            timeout: self.timeout,
            follow_redirects: self.follow_redirects,
        })
    }
}

// Convenience methods
impl ApiRequestBuilder {
    /// Create GET request.
    pub fn get(url: impl Into<String>) -> Self {
        Self::new().method(HttpMethod::Get).url(url)
    }

    /// Create POST request.
    pub fn post(url: impl Into<String>) -> Self {
        Self::new().method(HttpMethod::Post).url(url)
    }

    /// Create PUT request.
    pub fn put(url: impl Into<String>) -> Self {
        Self::new().method(HttpMethod::Put).url(url)
    }

    /// Create PATCH request.
    pub fn patch(url: impl Into<String>) -> Self {
        Self::new().method(HttpMethod::Patch).url(url)
    }

    /// Create DELETE request.
    pub fn delete(url: impl Into<String>) -> Self {
        Self::new().method(HttpMethod::Delete).url(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_has_body() {
        assert!(!HttpMethod::Get.has_body());
        assert!(HttpMethod::Post.has_body());
        assert!(HttpMethod::Put.has_body());
        assert!(HttpMethod::Patch.has_body());
        assert!(!HttpMethod::Delete.has_body());
    }

    #[test]
    fn test_request_builder() {
        let request = ApiRequestBuilder::get("https://api.example.com/users")
            .header("Accept", "application/json")
            .query("page", "1")
            .bearer_token("secret-token")
            .timeout(10)
            .build()
            .unwrap();

        assert_eq!(request.method, HttpMethod::Get);
        assert_eq!(request.url, "https://api.example.com/users");
        assert_eq!(
            request.headers.get("Accept"),
            Some(&"application/json".to_string())
        );
        assert_eq!(request.query.get("page"), Some(&"1".to_string()));
        assert!(matches!(request.auth, AuthScheme::Bearer { .. }));
        assert_eq!(request.timeout, Some(10));
    }

    #[test]
    fn test_builder_post_with_body() {
        let request = ApiRequestBuilder::post("https://api.example.com/users")
            .json(json!({"name": "Test", "email": "test@example.com"}))
            .build()
            .unwrap();

        assert_eq!(request.method, HttpMethod::Post);
        assert!(request.body.is_some());
    }

    #[cfg(feature = "web")]
    #[test]
    fn test_host_validation() {
        let tool = ApiTool::new();

        // Localhost should be denied by default
        assert!(tool.is_host_allowed("http://localhost/api").is_err());
        assert!(tool.is_host_allowed("http://127.0.0.1/api").is_err());

        // External hosts should be allowed
        assert!(tool
            .is_host_allowed("https://api.example.com/users")
            .is_ok());
    }
}
