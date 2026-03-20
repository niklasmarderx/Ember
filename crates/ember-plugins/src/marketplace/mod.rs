//! Plugin Marketplace Module
//!
//! Provides functionality for discovering, installing, and managing plugins
//! from the Ember plugin marketplace.

#[allow(missing_docs)]
mod types;
#[allow(missing_docs)]
mod registry;
#[allow(missing_docs)]
mod resolver;

pub use types::*;
pub use registry::*;
pub use resolver::*;
