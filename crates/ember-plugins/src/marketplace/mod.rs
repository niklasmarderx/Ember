//! Plugin Marketplace Module
//!
//! Provides functionality for discovering, installing, and managing plugins
//! from the Ember plugin marketplace.

mod types;
mod registry;
mod resolver;

pub use types::*;
pub use registry::*;
pub use resolver::*;