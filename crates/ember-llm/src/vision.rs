//! Vision and Multimodal Support for LLM Providers
//!
//! This module provides support for image inputs and multimodal content
//! across different LLM providers that support vision capabilities.

use serde::{Deserialize, Serialize};
use std::path::Path;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

/// Image input for vision-capable models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInput {
    /// The image data or URL
    pub source: ImageSource,
    /// Optional detail level for the image
    pub detail: Option<ImageDetail>,
    /// Alt text description
    pub alt_text: Option<String>,
}

/// Source of the image data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageSource {
    /// Base64-encoded image data
    #[serde(rename = "base64")]
    Base64 {
        data: String,
        media_type: MediaType,
    },
    /// URL to the image
    #[serde(rename = "url")]
    Url { url: String },
}

/// Supported image media types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MediaType {
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    WebP,
}

impl MediaType {
    /// Get the media type from a file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "gif" => Some(Self::Gif),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

    /// Get the media type string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
        }
    }
}

/// Detail level for image processing
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    /// Low resolution - faster, cheaper
    Low,
    /// High resolution - more detail
    High,
    /// Auto - let the model decide
    #[default]
    Auto,
}

/// Multimodal content that can contain text and images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalContent {
    /// Content parts
    pub parts: Vec<ContentPart>,
}

/// A single part of multimodal content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content
    #[serde(rename = "image")]
    Image(ImageInput),
}

impl MultimodalContent {
    /// Create new multimodal content with just text
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            parts: vec![ContentPart::Text { text: text.into() }],
        }
    }

    /// Create new multimodal content with text and an image
    pub fn text_and_image(text: impl Into<String>, image: ImageInput) -> Self {
        Self {
            parts: vec![
                ContentPart::Text { text: text.into() },
                ContentPart::Image(image),
            ],
        }
    }

    /// Add text to the content
    pub fn add_text(mut self, text: impl Into<String>) -> Self {
        self.parts.push(ContentPart::Text { text: text.into() });
        self
    }

    /// Add an image to the content
    pub fn add_image(mut self, image: ImageInput) -> Self {
        self.parts.push(ContentPart::Image(image));
        self
    }

    /// Check if the content has any images
    pub fn has_images(&self) -> bool {
        self.parts.iter().any(|p| matches!(p, ContentPart::Image(_)))
    }

    /// Get all text parts concatenated
    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl ImageInput {
    /// Create an image input from a URL
    pub fn from_url(url: impl Into<String>) -> Self {
        Self {
            source: ImageSource::Url { url: url.into() },
            detail: None,
            alt_text: None,
        }
    }

    /// Create an image input from base64 data
    pub fn from_base64(data: impl Into<String>, media_type: MediaType) -> Self {
        Self {
            source: ImageSource::Base64 {
                data: data.into(),
                media_type,
            },
            detail: None,
            alt_text: None,
        }
    }

    /// Create an image input from file bytes
    pub fn from_bytes(bytes: &[u8], media_type: MediaType) -> Self {
        let data = BASE64.encode(bytes);
        Self::from_base64(data, media_type)
    }

    /// Load an image from a file path
    pub fn from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        
        let media_type = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(MediaType::from_extension)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Unsupported image format",
                )
            })?;

        Ok(Self::from_bytes(&bytes, media_type))
    }

    /// Set the detail level
    pub fn with_detail(mut self, detail: ImageDetail) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Set the alt text
    pub fn with_alt_text(mut self, alt_text: impl Into<String>) -> Self {
        self.alt_text = Some(alt_text.into());
        self
    }

    /// Get the data URL for the image (for base64 images)
    pub fn data_url(&self) -> Option<String> {
        match &self.source {
            ImageSource::Base64 { data, media_type } => {
                Some(format!("data:{};base64,{}", media_type.as_str(), data))
            }
            ImageSource::Url { url } => Some(url.clone()),
        }
    }
}

/// Trait for providers that support vision/multimodal inputs
pub trait VisionCapable {
    /// Check if the provider supports vision for a specific model
    fn supports_vision(&self, model: &str) -> bool;
    
    /// Get the maximum image size in bytes for a model
    fn max_image_size(&self, model: &str) -> Option<usize>;
    
    /// Get the maximum number of images per request
    fn max_images_per_request(&self, model: &str) -> Option<usize>;
}

/// Models that support vision capabilities
pub struct VisionModels;

impl VisionModels {
    /// Check if a model supports vision
    pub fn is_vision_model(provider: &str, model: &str) -> bool {
        match provider.to_lowercase().as_str() {
            "openai" => {
                model.contains("gpt-4") && (model.contains("vision") || model.contains("turbo") || model.contains("o"))
                    || model.starts_with("gpt-4o")
            }
            "anthropic" => {
                model.contains("claude-3") || model.contains("claude-4")
            }
            "google" | "gemini" => {
                model.contains("gemini") && (model.contains("pro") || model.contains("ultra") || model.contains("flash"))
            }
            "ollama" => {
                model.contains("llava") || model.contains("bakllava") || model.contains("vision")
            }
            "openrouter" => {
                // OpenRouter supports many vision models
                model.contains("vision") 
                    || model.contains("gpt-4o")
                    || model.contains("claude-3")
                    || model.contains("gemini")
            }
            _ => false,
        }
    }

    /// Get recommended models for vision tasks by provider
    pub fn recommended_vision_models(provider: &str) -> Vec<&'static str> {
        match provider.to_lowercase().as_str() {
            "openai" => vec!["gpt-4o", "gpt-4o-mini", "gpt-4-turbo"],
            "anthropic" => vec!["claude-3-5-sonnet-20241022", "claude-3-opus-20240229", "claude-3-haiku-20240307"],
            "google" | "gemini" => vec!["gemini-1.5-pro", "gemini-1.5-flash", "gemini-pro-vision"],
            "ollama" => vec!["llava", "llava:13b", "bakllava"],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_type_from_extension() {
        assert_eq!(MediaType::from_extension("jpg"), Some(MediaType::Jpeg));
        assert_eq!(MediaType::from_extension("PNG"), Some(MediaType::Png));
        assert_eq!(MediaType::from_extension("gif"), Some(MediaType::Gif));
        assert_eq!(MediaType::from_extension("webp"), Some(MediaType::WebP));
        assert_eq!(MediaType::from_extension("bmp"), None);
    }

    #[test]
    fn test_image_input_from_url() {
        let img = ImageInput::from_url("https://example.com/image.jpg");
        assert!(matches!(img.source, ImageSource::Url { .. }));
    }

    #[test]
    fn test_multimodal_content() {
        let content = MultimodalContent::text("Hello")
            .add_image(ImageInput::from_url("https://example.com/img.png"));
        
        assert!(content.has_images());
        assert_eq!(content.text_content(), "Hello");
        assert_eq!(content.parts.len(), 2);
    }

    #[test]
    fn test_vision_models_detection() {
        assert!(VisionModels::is_vision_model("openai", "gpt-4o"));
        assert!(VisionModels::is_vision_model("anthropic", "claude-3-5-sonnet"));
        assert!(VisionModels::is_vision_model("google", "gemini-1.5-pro"));
        assert!(!VisionModels::is_vision_model("openai", "gpt-3.5-turbo"));
    }
}