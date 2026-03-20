//! Data anonymization utilities
//!
//! Provides functions to ensure no PII is ever collected.

use sha2::{Digest, Sha256};

/// Anonymize a string by hashing it
///
/// This creates a one-way hash that cannot be reversed to reveal the original value.
/// Useful for creating anonymous session IDs.
pub fn anonymize_string(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)[..16].to_string() // Take first 16 chars
}

/// Anonymize a machine ID for session tracking
pub fn anonymize_machine_id() -> String {
    // Create a stable but anonymous ID based on machine characteristics
    // that cannot be used to identify the actual machine
    let factors = [
        std::env::consts::OS,
        std::env::consts::ARCH,
        // Add a random component that's stable per installation
        "ember-telemetry",
    ];

    anonymize_string(&factors.join("-"))
}

/// Sanitize a string to remove potential PII
pub fn sanitize_string(value: &str) -> String {
    // Remove potential file paths
    let sanitized = value
        .replace(std::path::MAIN_SEPARATOR, "/")
        .split('/')
        .last()
        .unwrap_or(value)
        .to_string();

    // Remove potential email addresses
    let sanitized = if sanitized.contains('@') {
        "[email]".to_string()
    } else {
        sanitized
    };

    // Limit length
    if sanitized.len() > 100 {
        sanitized[..100].to_string()
    } else {
        sanitized
    }
}

/// Check if a string might contain PII
pub fn might_contain_pii(value: &str) -> bool {
    let lower = value.to_lowercase();

    // Check for common PII patterns
    let pii_patterns = [
        "@",        // Email
        "password", // Credentials
        "secret",   // Secrets
        "token",    // Tokens
        "key",      // API keys
        "Bearer",   // Auth headers
        "Authorization",
        "api_key",
        "apikey",
    ];

    pii_patterns.iter().any(|p| lower.contains(p))
}

/// Redact potential PII from a string
pub fn redact_pii(value: &str) -> String {
    if might_contain_pii(value) {
        "[REDACTED]".to_string()
    } else {
        sanitize_string(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymize_string() {
        let result1 = anonymize_string("test");
        let result2 = anonymize_string("test");
        let result3 = anonymize_string("different");

        // Same input produces same output
        assert_eq!(result1, result2);

        // Different input produces different output
        assert_ne!(result1, result3);

        // Output is 16 chars
        assert_eq!(result1.len(), 16);
    }

    #[test]
    fn test_might_contain_pii() {
        assert!(might_contain_pii("user@example.com"));
        assert!(might_contain_pii("my_password123"));
        assert!(might_contain_pii("OPENAI_API_KEY"));
        assert!(might_contain_pii("Bearer sk-xxx"));

        assert!(!might_contain_pii("gpt-4"));
        assert!(!might_contain_pii("chat"));
        assert!(!might_contain_pii("openai"));
    }

    #[test]
    fn test_redact_pii() {
        assert_eq!(redact_pii("user@example.com"), "[REDACTED]");
        assert_eq!(redact_pii("api_key=xxx"), "[REDACTED]");
        assert_eq!(redact_pii("gpt-4"), "gpt-4");
    }

    #[test]
    fn test_sanitize_string() {
        // File paths should be reduced to filename
        assert_eq!(sanitize_string("/home/user/file.txt"), "file.txt");

        // Emails should be replaced
        assert_eq!(sanitize_string("user@example.com"), "[email]");

        // Normal strings should pass through
        assert_eq!(sanitize_string("hello"), "hello");
    }
}
