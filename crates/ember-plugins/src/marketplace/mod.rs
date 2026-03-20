//! Plugin Marketplace Module
//!
//! Provides functionality for discovering, installing, and managing plugins
//! from the Ember plugin marketplace.

#[allow(missing_docs)]
mod registry;
#[allow(missing_docs)]
mod resolver;
#[allow(missing_docs)]
mod types;

pub use registry::*;
pub use resolver::*;
pub use types::*;

// Type aliases for CLI compatibility
/// Alias for RegistryClient (used as MarketplaceClient in CLI)
pub type MarketplaceClient = RegistryClient;
/// Alias for SearchQuery (used as PluginSearchQuery in CLI)
pub type PluginSearchQuery = SearchQuery;
/// Alias for SearchSort (used as PluginSortField in CLI)
pub type PluginSortField = SearchSort;
