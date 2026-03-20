//! Marketplace Types
//!
//! Defines the data structures for plugin metadata, versions, ratings, and reviews.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin category for marketplace organization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PluginCategory {
    /// Tools that interact with external services
    Integration,
    /// AI and machine learning tools
    Ai,
    /// Developer tools
    Developer,
    /// Productivity tools
    Productivity,
    /// Data processing tools
    Data,
    /// Security tools
    Security,
    /// Communication tools
    Communication,
    /// Utility tools
    Utility,
    /// Other/uncategorized
    Other,
}

impl std::fmt::Display for PluginCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Integration => write!(f, "Integration"),
            Self::Ai => write!(f, "AI"),
            Self::Developer => write!(f, "Developer"),
            Self::Productivity => write!(f, "Productivity"),
            Self::Data => write!(f, "Data"),
            Self::Security => write!(f, "Security"),
            Self::Communication => write!(f, "Communication"),
            Self::Utility => write!(f, "Utility"),
            Self::Other => write!(f, "Other"),
        }
    }
}

/// Semantic version representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
    pub build: Option<String>,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
            build: None,
        }
    }

    pub fn parse(version: &str) -> Result<Self, VersionParseError> {
        let version = version.trim_start_matches('v');
        
        // Split off build metadata
        let (version, build) = match version.split_once('+') {
            Some((v, b)) => (v, Some(b.to_string())),
            None => (version, None),
        };

        // Split off prerelease
        let (version, prerelease) = match version.split_once('-') {
            Some((v, p)) => (v, Some(p.to_string())),
            None => (version, None),
        };

        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err(VersionParseError::InvalidFormat(version.to_string()));
        }

        let major = parts[0].parse().map_err(|_| {
            VersionParseError::InvalidNumber("major".to_string())
        })?;
        let minor = parts[1].parse().map_err(|_| {
            VersionParseError::InvalidNumber("minor".to_string())
        })?;
        let patch = parts[2].parse().map_err(|_| {
            VersionParseError::InvalidNumber("patch".to_string())
        })?;

        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
            build,
        })
    }

    pub fn is_compatible_with(&self, requirement: &VersionRequirement) -> bool {
        match requirement {
            VersionRequirement::Exact(v) => self == v,
            VersionRequirement::Caret(v) => {
                if v.major == 0 {
                    self.major == v.major && self.minor == v.minor && self.patch >= v.patch
                } else {
                    self.major == v.major && (self.minor > v.minor || 
                        (self.minor == v.minor && self.patch >= v.patch))
                }
            }
            VersionRequirement::Tilde(v) => {
                self.major == v.major && self.minor == v.minor && self.patch >= v.patch
            }
            VersionRequirement::GreaterThan(v) => self > v,
            VersionRequirement::GreaterThanOrEqual(v) => self >= v,
            VersionRequirement::LessThan(v) => self < v,
            VersionRequirement::LessThanOrEqual(v) => self <= v,
            VersionRequirement::Any => true,
        }
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.prerelease {
            write!(f, "-{}", pre)?;
        }
        if let Some(ref build) = self.build {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

/// Version parsing error
#[derive(Debug, Clone, thiserror::Error)]
pub enum VersionParseError {
    #[error("Invalid version format: {0}")]
    InvalidFormat(String),
    #[error("Invalid {0} version number")]
    InvalidNumber(String),
}

/// Version requirement specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VersionRequirement {
    /// Exact version: =1.2.3
    Exact(SemVer),
    /// Caret requirement: ^1.2.3 (compatible with 1.x.x)
    Caret(SemVer),
    /// Tilde requirement: ~1.2.3 (compatible with 1.2.x)
    Tilde(SemVer),
    /// Greater than: >1.2.3
    GreaterThan(SemVer),
    /// Greater than or equal: >=1.2.3
    GreaterThanOrEqual(SemVer),
    /// Less than: <1.2.3
    LessThan(SemVer),
    /// Less than or equal: <=1.2.3
    LessThanOrEqual(SemVer),
    /// Any version: *
    Any,
}

impl VersionRequirement {
    pub fn parse(spec: &str) -> Result<Self, VersionParseError> {
        let spec = spec.trim();
        
        if spec == "*" || spec.is_empty() {
            return Ok(Self::Any);
        }

        if let Some(version) = spec.strip_prefix(">=") {
            return Ok(Self::GreaterThanOrEqual(SemVer::parse(version)?));
        }
        if let Some(version) = spec.strip_prefix("<=") {
            return Ok(Self::LessThanOrEqual(SemVer::parse(version)?));
        }
        if let Some(version) = spec.strip_prefix('>') {
            return Ok(Self::GreaterThan(SemVer::parse(version)?));
        }
        if let Some(version) = spec.strip_prefix('<') {
            return Ok(Self::LessThan(SemVer::parse(version)?));
        }
        if let Some(version) = spec.strip_prefix('=') {
            return Ok(Self::Exact(SemVer::parse(version)?));
        }
        if let Some(version) = spec.strip_prefix('^') {
            return Ok(Self::Caret(SemVer::parse(version)?));
        }
        if let Some(version) = spec.strip_prefix('~') {
            return Ok(Self::Tilde(SemVer::parse(version)?));
        }

        // Default to caret
        Ok(Self::Caret(SemVer::parse(spec)?))
    }
}

impl std::fmt::Display for VersionRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact(v) => write!(f, "={}", v),
            Self::Caret(v) => write!(f, "^{}", v),
            Self::Tilde(v) => write!(f, "~{}", v),
            Self::GreaterThan(v) => write!(f, ">{}", v),
            Self::GreaterThanOrEqual(v) => write!(f, ">={}", v),
            Self::LessThan(v) => write!(f, "<{}", v),
            Self::LessThanOrEqual(v) => write!(f, "<={}", v),
            Self::Any => write!(f, "*"),
        }
    }
}

/// Plugin dependency specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// Name of the dependency
    pub name: String,
    /// Version requirement
    pub version: VersionRequirement,
    /// Whether this is an optional dependency
    #[serde(default)]
    pub optional: bool,
}

/// Plugin author information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    /// Author's display name
    pub name: String,
    /// Author's email (optional)
    pub email: Option<String>,
    /// Author's website/profile URL
    pub url: Option<String>,
    /// Whether the author is verified
    #[serde(default)]
    pub verified: bool,
}

/// Plugin license information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum License {
    MIT,
    Apache2,
    #[serde(rename = "GPL-3.0")]
    Gpl3,
    #[serde(rename = "BSD-3-Clause")]
    Bsd3,
    #[serde(rename = "ISC")]
    Isc,
    #[serde(rename = "MPL-2.0")]
    Mpl2,
    Unlicense,
    Proprietary,
    #[serde(other)]
    Other,
}

impl std::fmt::Display for License {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MIT => write!(f, "MIT"),
            Self::Apache2 => write!(f, "Apache-2.0"),
            Self::Gpl3 => write!(f, "GPL-3.0"),
            Self::Bsd3 => write!(f, "BSD-3-Clause"),
            Self::Isc => write!(f, "ISC"),
            Self::Mpl2 => write!(f, "MPL-2.0"),
            Self::Unlicense => write!(f, "Unlicense"),
            Self::Proprietary => write!(f, "Proprietary"),
            Self::Other => write!(f, "Other"),
        }
    }
}

/// Plugin version metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginVersion {
    /// Version number
    pub version: SemVer,
    /// Minimum Ember version required
    pub ember_version: VersionRequirement,
    /// Release date
    pub released_at: DateTime<Utc>,
    /// Download URL
    pub download_url: String,
    /// SHA256 checksum
    pub checksum: String,
    /// File size in bytes
    pub size: u64,
    /// Changelog for this version
    pub changelog: Option<String>,
    /// Whether this version is deprecated
    #[serde(default)]
    pub deprecated: bool,
    /// Deprecation message if deprecated
    pub deprecation_message: Option<String>,
}

/// User review of a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginReview {
    /// Review ID
    pub id: String,
    /// Reviewer's username
    pub username: String,
    /// Rating (1-5 stars)
    pub rating: u8,
    /// Review title
    pub title: Option<String>,
    /// Review body
    pub body: Option<String>,
    /// Review date
    pub created_at: DateTime<Utc>,
    /// Last updated date
    pub updated_at: Option<DateTime<Utc>>,
    /// Number of helpful votes
    #[serde(default)]
    pub helpful_votes: u32,
    /// Plugin version reviewed
    pub version: Option<SemVer>,
}

/// Plugin statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginStats {
    /// Total downloads
    pub downloads: u64,
    /// Downloads in the last 30 days
    pub downloads_last_30_days: u64,
    /// Downloads in the last 7 days
    pub downloads_last_7_days: u64,
    /// Number of stars/favorites
    pub stars: u32,
    /// Number of reviews
    pub review_count: u32,
    /// Average rating (1.0-5.0)
    pub average_rating: f32,
    /// Number of open issues
    pub open_issues: u32,
    /// Last updated timestamp
    pub last_updated: Option<DateTime<Utc>>,
}

/// Complete plugin metadata from marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Unique plugin identifier
    pub id: String,
    /// Plugin name (human-readable)
    pub name: String,
    /// Short description
    pub description: String,
    /// Long description (Markdown)
    pub readme: Option<String>,
    /// Plugin category
    pub category: PluginCategory,
    /// Tags for search
    pub tags: Vec<String>,
    /// Plugin authors
    pub authors: Vec<Author>,
    /// License
    pub license: License,
    /// Repository URL
    pub repository: Option<String>,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Bug tracker URL
    pub bugs: Option<String>,
    /// Documentation URL
    pub documentation: Option<String>,
    /// Icon URL
    pub icon: Option<String>,
    /// Banner image URL
    pub banner: Option<String>,
    /// Screenshots URLs
    #[serde(default)]
    pub screenshots: Vec<String>,
    /// Available versions
    pub versions: Vec<PluginVersion>,
    /// Plugin dependencies
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    /// Plugin statistics
    #[serde(default)]
    pub stats: PluginStats,
    /// Recent reviews
    #[serde(default)]
    pub reviews: Vec<PluginReview>,
    /// Creation date
    pub created_at: DateTime<Utc>,
    /// Last updated date
    pub updated_at: DateTime<Utc>,
    /// Whether the plugin is verified
    #[serde(default)]
    pub verified: bool,
    /// Whether the plugin is featured
    #[serde(default)]
    pub featured: bool,
    /// Keywords for search optimization
    #[serde(default)]
    pub keywords: Vec<String>,
}

impl PluginMetadata {
    /// Get the latest non-deprecated version
    pub fn latest_version(&self) -> Option<&PluginVersion> {
        self.versions
            .iter()
            .filter(|v| !v.deprecated)
            .max_by(|a, b| a.version.cmp(&b.version))
    }

    /// Get a specific version
    pub fn get_version(&self, version: &SemVer) -> Option<&PluginVersion> {
        self.versions.iter().find(|v| &v.version == version)
    }

    /// Find version matching a requirement
    pub fn find_matching_version(&self, req: &VersionRequirement) -> Option<&PluginVersion> {
        self.versions
            .iter()
            .filter(|v| !v.deprecated && v.version.is_compatible_with(req))
            .max_by(|a, b| a.version.cmp(&b.version))
    }
}

/// Search query parameters
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Search text
    pub query: Option<String>,
    /// Filter by category
    pub category: Option<PluginCategory>,
    /// Filter by tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Filter by author
    pub author: Option<String>,
    /// Minimum rating filter
    pub min_rating: Option<f32>,
    /// Sort field
    pub sort_by: Option<SearchSort>,
    /// Sort direction
    pub sort_order: Option<SortOrder>,
    /// Page number (0-indexed)
    pub page: Option<u32>,
    /// Page size
    pub page_size: Option<u32>,
    /// Only show verified plugins
    #[serde(default)]
    pub verified_only: bool,
    /// Only show featured plugins
    #[serde(default)]
    pub featured_only: bool,
}

/// Sort field for search results
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchSort {
    /// Sort by relevance (default)
    #[default]
    Relevance,
    /// Sort by download count
    Downloads,
    /// Sort by rating
    Rating,
    /// Sort by name
    Name,
    /// Sort by recently updated
    RecentlyUpdated,
    /// Sort by creation date
    Newest,
    /// Sort by trending (recent growth)
    Trending,
}

/// Sort order
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Descending,
    Ascending,
}

/// Search results response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// Matching plugins
    pub plugins: Vec<PluginMetadata>,
    /// Total number of matches
    pub total: u64,
    /// Current page
    pub page: u32,
    /// Page size
    pub page_size: u32,
    /// Total pages
    pub total_pages: u32,
}

/// Installation status of a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    /// Plugin metadata
    pub metadata: PluginMetadata,
    /// Installed version
    pub installed_version: SemVer,
    /// Installation path
    pub path: std::path::PathBuf,
    /// Installation date
    pub installed_at: DateTime<Utc>,
    /// Last updated date
    pub updated_at: Option<DateTime<Utc>>,
    /// Whether auto-update is enabled
    #[serde(default)]
    pub auto_update: bool,
    /// Whether the plugin is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// User configuration overrides
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

/// Plugin update information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUpdate {
    /// Plugin ID
    pub plugin_id: String,
    /// Plugin name
    pub name: String,
    /// Current installed version
    pub current_version: SemVer,
    /// Latest available version
    pub latest_version: SemVer,
    /// Changelog
    pub changelog: Option<String>,
    /// Whether this is a security update
    #[serde(default)]
    pub security_update: bool,
    /// Whether this is a breaking change
    #[serde(default)]
    pub breaking_change: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_parse() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.is_none());
        assert!(v.build.is_none());
    }

    #[test]
    fn test_semver_parse_with_prefix() {
        let v = SemVer::parse("v1.2.3").unwrap();
        assert_eq!(v.major, 1);
    }

    #[test]
    fn test_semver_parse_prerelease() {
        let v = SemVer::parse("1.0.0-alpha.1").unwrap();
        assert_eq!(v.prerelease, Some("alpha.1".to_string()));
    }

    #[test]
    fn test_semver_parse_build() {
        let v = SemVer::parse("1.0.0+build.123").unwrap();
        assert_eq!(v.build, Some("build.123".to_string()));
    }

    #[test]
    fn test_version_requirement_caret() {
        let req = VersionRequirement::parse("^1.2.3").unwrap();
        
        let v1 = SemVer::parse("1.2.3").unwrap();
        let v2 = SemVer::parse("1.3.0").unwrap();
        let v3 = SemVer::parse("2.0.0").unwrap();
        let v4 = SemVer::parse("1.2.0").unwrap();
        
        assert!(v1.is_compatible_with(&req));
        assert!(v2.is_compatible_with(&req));
        assert!(!v3.is_compatible_with(&req));
        assert!(!v4.is_compatible_with(&req));
    }

    #[test]
    fn test_version_requirement_tilde() {
        let req = VersionRequirement::parse("~1.2.3").unwrap();
        
        let v1 = SemVer::parse("1.2.3").unwrap();
        let v2 = SemVer::parse("1.2.5").unwrap();
        let v3 = SemVer::parse("1.3.0").unwrap();
        
        assert!(v1.is_compatible_with(&req));
        assert!(v2.is_compatible_with(&req));
        assert!(!v3.is_compatible_with(&req));
    }

    #[test]
    fn test_category_display() {
        assert_eq!(PluginCategory::Integration.to_string(), "Integration");
        assert_eq!(PluginCategory::Ai.to_string(), "AI");
    }
}