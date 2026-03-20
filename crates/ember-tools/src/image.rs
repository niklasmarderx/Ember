//! # Image Tool
//!
//! Tool for image processing operations.
//!
//! Supports:
//! - Image resizing
//! - Format conversion
//! - Metadata extraction
//! - Basic transformations (rotate, flip, crop)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;
use tracing::debug;

use crate::{Error, Result, ToolDefinition, ToolHandler, ToolOutput};

/// Image processing tool.
#[derive(Debug, Clone)]
pub struct ImageTool {
    config: ImageConfig,
}

/// Configuration for image tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    /// Allowed directories for image operations
    pub allowed_dirs: Vec<PathBuf>,
    /// Maximum image dimensions (width, height)
    pub max_dimensions: (u32, u32),
    /// Maximum file size in bytes
    pub max_file_size: u64,
    /// Allowed output formats
    pub allowed_formats: Vec<ImageFormat>,
    /// Use ImageMagick if available
    pub use_imagemagick: bool,
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            allowed_dirs: vec![],
            max_dimensions: (4096, 4096),
            max_file_size: 50 * 1024 * 1024, // 50MB
            allowed_formats: vec![
                ImageFormat::Png,
                ImageFormat::Jpeg,
                ImageFormat::Webp,
                ImageFormat::Gif,
                ImageFormat::Bmp,
            ],
            use_imagemagick: true,
        }
    }
}

/// Supported image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    /// PNG format
    Png,
    /// JPEG format
    Jpeg,
    /// WebP format
    Webp,
    /// GIF format
    Gif,
    /// BMP format
    Bmp,
    /// TIFF format
    Tiff,
    /// ICO format
    Ico,
}

impl ImageFormat {
    /// Get file extension for format.
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
            ImageFormat::Gif => "gif",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Tiff => "tiff",
            ImageFormat::Ico => "ico",
        }
    }

    /// Detect format from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(ImageFormat::Png),
            "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
            "webp" => Some(ImageFormat::Webp),
            "gif" => Some(ImageFormat::Gif),
            "bmp" => Some(ImageFormat::Bmp),
            "tiff" | "tif" => Some(ImageFormat::Tiff),
            "ico" => Some(ImageFormat::Ico),
            _ => None,
        }
    }
}

/// Image metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Format
    pub format: String,
    /// File size in bytes
    pub file_size: u64,
    /// Color depth (bits per pixel)
    pub color_depth: Option<u32>,
    /// Has alpha channel
    pub has_alpha: Option<bool>,
    /// EXIF data (if available)
    pub exif: Option<Value>,
}

/// Image operation type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ImageOperation {
    /// Get image info/metadata
    Info {
        /// Path to image
        path: String,
    },
    /// Resize image
    Resize {
        /// Source path
        source: String,
        /// Output path
        output: String,
        /// Target width
        width: Option<u32>,
        /// Target height
        height: Option<u32>,
        /// Maintain aspect ratio
        #[serde(default = "default_true")]
        maintain_aspect: bool,
    },
    /// Convert image format
    Convert {
        /// Source path
        source: String,
        /// Output path
        output: String,
        /// Target format
        format: ImageFormat,
        /// Quality (0-100, for lossy formats)
        quality: Option<u8>,
    },
    /// Rotate image
    Rotate {
        /// Source path
        source: String,
        /// Output path
        output: String,
        /// Degrees (90, 180, 270)
        degrees: i32,
    },
    /// Flip image
    Flip {
        /// Source path
        source: String,
        /// Output path
        output: String,
        /// Direction
        direction: FlipDirection,
    },
    /// Crop image
    Crop {
        /// Source path
        source: String,
        /// Output path
        output: String,
        /// X offset
        x: u32,
        /// Y offset
        y: u32,
        /// Width
        width: u32,
        /// Height
        height: u32,
    },
    /// Create thumbnail
    Thumbnail {
        /// Source path
        source: String,
        /// Output path
        output: String,
        /// Max dimension (width or height)
        size: u32,
    },
}

fn default_true() -> bool {
    true
}

/// Flip direction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlipDirection {
    /// Horizontal flip
    Horizontal,
    /// Vertical flip
    Vertical,
}

impl ImageTool {
    /// Create a new image tool with default configuration.
    pub fn new() -> Self {
        Self {
            config: ImageConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: ImageConfig) -> Self {
        Self { config }
    }

    /// Check if path is allowed.
    fn is_path_allowed(&self, path: &Path) -> bool {
        if self.config.allowed_dirs.is_empty() {
            return true;
        }

        let Ok(path) = path.canonicalize() else {
            return false;
        };

        self.config.allowed_dirs.iter().any(|allowed| {
            if let Ok(allowed) = allowed.canonicalize() {
                path.starts_with(&allowed)
            } else {
                false
            }
        })
    }

    /// Execute image operation.
    pub async fn execute_operation(&self, operation: ImageOperation) -> Result<ToolOutput> {
        match operation {
            ImageOperation::Info { path } => self.get_info(&path).await,
            ImageOperation::Resize {
                source,
                output,
                width,
                height,
                maintain_aspect,
            } => {
                self.resize(&source, &output, width, height, maintain_aspect)
                    .await
            }
            ImageOperation::Convert {
                source,
                output,
                format,
                quality,
            } => self.convert(&source, &output, format, quality).await,
            ImageOperation::Rotate {
                source,
                output,
                degrees,
            } => self.rotate(&source, &output, degrees).await,
            ImageOperation::Flip {
                source,
                output,
                direction,
            } => self.flip(&source, &output, direction).await,
            ImageOperation::Crop {
                source,
                output,
                x,
                y,
                width,
                height,
            } => self.crop(&source, &output, x, y, width, height).await,
            ImageOperation::Thumbnail {
                source,
                output,
                size,
            } => self.thumbnail(&source, &output, size).await,
        }
    }

    async fn get_info(&self, path: &str) -> Result<ToolOutput> {
        let path = Path::new(path);

        if !self.is_path_allowed(path) {
            return Err(Error::path_not_allowed(path.display().to_string()));
        }

        let metadata = fs::metadata(path)
            .await
            .map_err(|e| Error::filesystem(format!("Cannot read file: {}", e)))?;

        if metadata.len() > self.config.max_file_size {
            return Err(Error::filesystem(format!(
                "File too large: {} bytes (max: {} bytes)",
                metadata.len(),
                self.config.max_file_size
            )));
        }

        // Use ImageMagick identify if available
        if self.config.use_imagemagick {
            if let Ok(info) = self.identify_with_imagemagick(path).await {
                return Ok(ToolOutput::success(serde_json::to_string(&info).unwrap_or_default()));
            }
        }

        // Fallback: basic info from file
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");

        let format = ImageFormat::from_extension(extension)
            .map(|f| f.extension().to_string())
            .unwrap_or_else(|| extension.to_string());

        let info = ImageMetadata {
            width: 0,
            height: 0,
            format,
            file_size: metadata.len(),
            color_depth: None,
            has_alpha: None,
            exif: None,
        };

        Ok(ToolOutput::success(serde_json::to_string(&info).unwrap_or_default()))
    }

    async fn identify_with_imagemagick(&self, path: &Path) -> Result<ImageMetadata> {
        let output = Command::new("identify")
            .arg("-format")
            .arg("%w,%h,%m,%b,%z")
            .arg(path)
            .output()
            .await
            .map_err(|e| Error::execution_failed("image", format!("ImageMagick error: {}", e)))?;

        if !output.status.success() {
            return Err(Error::execution_failed(
                "image",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let info = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = info.trim().split(',').collect();

        if parts.len() >= 4 {
            let metadata = fs::metadata(path).await.map_err(|e| {
                Error::filesystem(format!("Cannot read file metadata: {}", e))
            })?;

            Ok(ImageMetadata {
                width: parts[0].parse().unwrap_or(0),
                height: parts[1].parse().unwrap_or(0),
                format: parts[2].to_string(),
                file_size: metadata.len(),
                color_depth: parts.get(4).and_then(|s| s.parse().ok()),
                has_alpha: None,
                exif: None,
            })
        } else {
            Err(Error::execution_failed(
                "image",
                "Invalid ImageMagick output",
            ))
        }
    }

    async fn resize(
        &self,
        source: &str,
        output: &str,
        width: Option<u32>,
        height: Option<u32>,
        maintain_aspect: bool,
    ) -> Result<ToolOutput> {
        let source_path = Path::new(source);
        let output_path = Path::new(output);

        if !self.is_path_allowed(source_path) || !self.is_path_allowed(output_path) {
            return Err(Error::path_not_allowed("Source or output path not allowed"));
        }

        let (w, h) = match (width, height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, 0),
            (None, Some(h)) => (0, h),
            (None, None) => {
                return Err(Error::invalid_arguments(
                    "image",
                    "Either width or height must be specified",
                ))
            }
        };

        if w > self.config.max_dimensions.0 || h > self.config.max_dimensions.1 {
            return Err(Error::invalid_arguments(
                "image",
                format!(
                    "Dimensions exceed maximum ({}, {})",
                    self.config.max_dimensions.0, self.config.max_dimensions.1
                ),
            ));
        }

        let geometry = if maintain_aspect {
            format!("{}x{}", w, h)
        } else {
            format!("{}x{}!", w, h)
        };

        let output = Command::new("convert")
            .arg(source)
            .arg("-resize")
            .arg(&geometry)
            .arg(output)
            .output()
            .await
            .map_err(|e| Error::execution_failed("image", format!("ImageMagick error: {}", e)))?;

        if output.status.success() {
            Ok(ToolOutput::success(serde_json::to_string(&json!({
                "status": "success",
                "output": output_path.display().to_string(),
                "geometry": geometry
            })).unwrap_or_default()))
        } else {
            Err(Error::execution_failed(
                "image",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    async fn convert(
        &self,
        source: &str,
        output: &str,
        format: ImageFormat,
        quality: Option<u8>,
    ) -> Result<ToolOutput> {
        let source_path = Path::new(source);
        let output_path = Path::new(output);

        if !self.is_path_allowed(source_path) || !self.is_path_allowed(output_path) {
            return Err(Error::path_not_allowed("Source or output path not allowed"));
        }

        if !self.config.allowed_formats.contains(&format) {
            return Err(Error::invalid_arguments(
                "image",
                format!("Format {:?} not allowed", format),
            ));
        }

        let mut cmd = Command::new("convert");
        cmd.arg(source);

        if let Some(q) = quality {
            cmd.arg("-quality").arg(q.to_string());
        }

        cmd.arg(output);

        let output_result = cmd
            .output()
            .await
            .map_err(|e| Error::execution_failed("image", format!("ImageMagick error: {}", e)))?;

        if output_result.status.success() {
            Ok(ToolOutput::success(serde_json::to_string(&json!({
                "status": "success",
                "output": output_path.display().to_string(),
                "format": format.extension()
            })).unwrap_or_default()))
        } else {
            Err(Error::execution_failed(
                "image",
                String::from_utf8_lossy(&output_result.stderr).to_string(),
            ))
        }
    }

    async fn rotate(&self, source: &str, output: &str, degrees: i32) -> Result<ToolOutput> {
        let source_path = Path::new(source);
        let output_path = Path::new(output);

        if !self.is_path_allowed(source_path) || !self.is_path_allowed(output_path) {
            return Err(Error::path_not_allowed("Source or output path not allowed"));
        }

        if ![90, 180, 270, -90, -180, -270].contains(&degrees) {
            return Err(Error::invalid_arguments(
                "image",
                "Degrees must be 90, 180, or 270",
            ));
        }

        let output_result = Command::new("convert")
            .arg(source)
            .arg("-rotate")
            .arg(degrees.to_string())
            .arg(output)
            .output()
            .await
            .map_err(|e| Error::execution_failed("image", format!("ImageMagick error: {}", e)))?;

        if output_result.status.success() {
            Ok(ToolOutput::success(serde_json::to_string(&json!({
                "status": "success",
                "output": output_path.display().to_string(),
                "rotation": degrees
            })).unwrap_or_default()))
        } else {
            Err(Error::execution_failed(
                "image",
                String::from_utf8_lossy(&output_result.stderr).to_string(),
            ))
        }
    }

    async fn flip(&self, source: &str, output: &str, direction: FlipDirection) -> Result<ToolOutput> {
        let source_path = Path::new(source);
        let output_path = Path::new(output);

        if !self.is_path_allowed(source_path) || !self.is_path_allowed(output_path) {
            return Err(Error::path_not_allowed("Source or output path not allowed"));
        }

        let flip_arg = match direction {
            FlipDirection::Horizontal => "-flop",
            FlipDirection::Vertical => "-flip",
        };

        let output_result = Command::new("convert")
            .arg(source)
            .arg(flip_arg)
            .arg(output)
            .output()
            .await
            .map_err(|e| Error::execution_failed("image", format!("ImageMagick error: {}", e)))?;

        if output_result.status.success() {
            Ok(ToolOutput::success(serde_json::to_string(&json!({
                "status": "success",
                "output": output_path.display().to_string(),
                "direction": format!("{:?}", direction)
            })).unwrap_or_default()))
        } else {
            Err(Error::execution_failed(
                "image",
                String::from_utf8_lossy(&output_result.stderr).to_string(),
            ))
        }
    }

    async fn crop(
        &self,
        source: &str,
        output: &str,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<ToolOutput> {
        let source_path = Path::new(source);
        let output_path = Path::new(output);

        if !self.is_path_allowed(source_path) || !self.is_path_allowed(output_path) {
            return Err(Error::path_not_allowed("Source or output path not allowed"));
        }

        if width > self.config.max_dimensions.0 || height > self.config.max_dimensions.1 {
            return Err(Error::invalid_arguments(
                "image",
                format!(
                    "Crop dimensions exceed maximum ({}, {})",
                    self.config.max_dimensions.0, self.config.max_dimensions.1
                ),
            ));
        }

        let geometry = format!("{}x{}+{}+{}", width, height, x, y);

        let output_result = Command::new("convert")
            .arg(source)
            .arg("-crop")
            .arg(&geometry)
            .arg("+repage")
            .arg(output)
            .output()
            .await
            .map_err(|e| Error::execution_failed("image", format!("ImageMagick error: {}", e)))?;

        if output_result.status.success() {
            Ok(ToolOutput::success(serde_json::to_string(&json!({
                "status": "success",
                "output": output_path.display().to_string(),
                "crop": {
                    "x": x,
                    "y": y,
                    "width": width,
                    "height": height
                }
            })).unwrap_or_default()))
        } else {
            Err(Error::execution_failed(
                "image",
                String::from_utf8_lossy(&output_result.stderr).to_string(),
            ))
        }
    }

    async fn thumbnail(&self, source: &str, output: &str, size: u32) -> Result<ToolOutput> {
        let source_path = Path::new(source);
        let output_path = Path::new(output);

        if !self.is_path_allowed(source_path) || !self.is_path_allowed(output_path) {
            return Err(Error::path_not_allowed("Source or output path not allowed"));
        }

        let output_result = Command::new("convert")
            .arg(source)
            .arg("-thumbnail")
            .arg(format!("{}x{}>", size, size))
            .arg(output)
            .output()
            .await
            .map_err(|e| Error::execution_failed("image", format!("ImageMagick error: {}", e)))?;

        if output_result.status.success() {
            Ok(ToolOutput::success(serde_json::to_string(&json!({
                "status": "success",
                "output": output_path.display().to_string(),
                "max_size": size
            })).unwrap_or_default()))
        } else {
            Err(Error::execution_failed(
                "image",
                String::from_utf8_lossy(&output_result.stderr).to_string(),
            ))
        }
    }
}

impl Default for ImageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolHandler for ImageTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "image".to_string(),
            description: "Process images: resize, convert, rotate, flip, crop, and get metadata"
                .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["operation"],
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["info", "resize", "convert", "rotate", "flip", "crop", "thumbnail"],
                        "description": "The operation to perform"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to image file (for info operation)"
                    },
                    "source": {
                        "type": "string",
                        "description": "Source image path"
                    },
                    "output": {
                        "type": "string",
                        "description": "Output image path"
                    },
                    "width": {
                        "type": "integer",
                        "description": "Target width in pixels"
                    },
                    "height": {
                        "type": "integer",
                        "description": "Target height in pixels"
                    },
                    "maintain_aspect": {
                        "type": "boolean",
                        "default": true,
                        "description": "Maintain aspect ratio when resizing"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["png", "jpeg", "webp", "gif", "bmp", "tiff", "ico"],
                        "description": "Target image format"
                    },
                    "quality": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 100,
                        "description": "Output quality (for lossy formats)"
                    },
                    "degrees": {
                        "type": "integer",
                        "enum": [90, 180, 270, -90, -180, -270],
                        "description": "Rotation degrees"
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["horizontal", "vertical"],
                        "description": "Flip direction"
                    },
                    "x": {
                        "type": "integer",
                        "description": "X offset for crop"
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y offset for crop"
                    },
                    "size": {
                        "type": "integer",
                        "description": "Max dimension for thumbnail"
                    }
                }
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<ToolOutput> {
        debug!("Image tool called with: {:?}", arguments);

        let operation: ImageOperation =
            serde_json::from_value(arguments).map_err(|e| {
                Error::invalid_arguments("image", format!("Invalid arguments: {}", e))
            })?;

        self.execute_operation(operation).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_format_extension() {
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
        assert_eq!(ImageFormat::Webp.extension(), "webp");
    }

    #[test]
    fn test_format_from_extension() {
        assert_eq!(
            ImageFormat::from_extension("png"),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            ImageFormat::from_extension("jpg"),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(
            ImageFormat::from_extension("jpeg"),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(ImageFormat::from_extension("unknown"), None);
    }

    #[test]
    fn test_tool_definition() {
        let tool = ImageTool::new();
        let def = tool.definition();
        assert_eq!(def.name, "image");
    }
}