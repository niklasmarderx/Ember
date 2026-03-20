//! Dependency Resolver
//!
//! Resolves plugin dependencies using a SAT-solver-like approach.

use super::types::*;
use super::registry::{RegistryClient, RegistryError};
use std::collections::{HashMap, HashSet, VecDeque};

/// Dependency resolution error
#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    #[error("Registry error: {0}")]
    Registry(#[from] RegistryError),

    #[error("Dependency not found: {0}")]
    DependencyNotFound(String),

    #[error("Version conflict for {plugin}: {requirement1} conflicts with {requirement2}")]
    VersionConflict {
        plugin: String,
        requirement1: String,
        requirement2: String,
    },

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("No compatible version found for {plugin} matching {requirement}")]
    NoCompatibleVersion { plugin: String, requirement: String },

    #[error("Maximum resolution depth exceeded")]
    MaxDepthExceeded,
}

/// A resolved dependency
#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    /// Plugin metadata
    pub plugin: PluginMetadata,
    /// Resolved version
    pub version: PluginVersion,
    /// Whether this is a direct dependency or transitive
    pub direct: bool,
    /// Dependencies that led to this resolution
    pub required_by: Vec<String>,
}

/// Resolution result
#[derive(Debug, Clone)]
pub struct Resolution {
    /// Resolved dependencies in installation order
    pub dependencies: Vec<ResolvedDependency>,
    /// Total download size
    pub total_size: u64,
    /// Plugins that will be updated
    pub updates: Vec<(String, SemVer, SemVer)>,
    /// Plugins that will be newly installed
    pub new_installs: Vec<String>,
}

/// Dependency resolver configuration
#[derive(Debug, Clone)]
pub struct ResolverConfig {
    /// Maximum resolution depth
    pub max_depth: u32,
    /// Whether to include optional dependencies
    pub include_optional: bool,
    /// Whether to prefer stable versions
    pub prefer_stable: bool,
    /// Minimum Ember version for compatibility
    pub ember_version: SemVer,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            max_depth: 10,
            include_optional: false,
            prefer_stable: true,
            ember_version: SemVer::new(1, 0, 0),
        }
    }
}

/// Dependency resolver
pub struct DependencyResolver {
    client: RegistryClient,
    config: ResolverConfig,
    /// Cache of fetched plugin metadata
    cache: HashMap<String, PluginMetadata>,
    /// Currently installed plugins
    installed: HashMap<String, SemVer>,
}

impl DependencyResolver {
    /// Create a new resolver with default configuration
    pub fn new(client: RegistryClient) -> Self {
        Self::with_config(client, ResolverConfig::default())
    }

    /// Create a new resolver with custom configuration
    pub fn with_config(client: RegistryClient, config: ResolverConfig) -> Self {
        Self {
            client,
            config,
            cache: HashMap::new(),
            installed: HashMap::new(),
        }
    }

    /// Set installed plugins for upgrade detection
    pub fn set_installed(&mut self, installed: HashMap<String, SemVer>) {
        self.installed = installed;
    }

    /// Resolve dependencies for a plugin
    pub async fn resolve(&mut self, plugin_id: &str, version_req: Option<&str>) -> Result<Resolution, ResolverError> {
        let requirement = match version_req {
            Some(v) => VersionRequirement::parse(v).map_err(|_| {
                ResolverError::NoCompatibleVersion {
                    plugin: plugin_id.to_string(),
                    requirement: v.to_string(),
                }
            })?,
            None => VersionRequirement::Any,
        };

        self.resolve_with_requirement(plugin_id, &requirement).await
    }

    /// Resolve dependencies for multiple plugins
    pub async fn resolve_many(&mut self, plugins: &[(String, Option<String>)]) -> Result<Resolution, ResolverError> {
        let mut all_deps: Vec<ResolvedDependency> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for (plugin_id, version_req) in plugins {
            let resolution = self.resolve(plugin_id, version_req.as_deref()).await?;
            
            for dep in resolution.dependencies {
                if !seen.contains(&dep.plugin.id) {
                    seen.insert(dep.plugin.id.clone());
                    all_deps.push(dep);
                }
            }
        }

        // Sort in installation order (dependencies first)
        let sorted = self.topological_sort(&all_deps)?;

        let total_size = sorted.iter().map(|d| d.version.size).sum();
        let updates = self.find_updates(&sorted);
        let new_installs = self.find_new_installs(&sorted);

        Ok(Resolution {
            dependencies: sorted,
            total_size,
            updates,
            new_installs,
        })
    }

    /// Resolve with a specific version requirement
    async fn resolve_with_requirement(
        &mut self,
        plugin_id: &str,
        requirement: &VersionRequirement,
    ) -> Result<Resolution, ResolverError> {
        let mut resolved: HashMap<String, ResolvedDependency> = HashMap::new();
        let mut requirements: HashMap<String, Vec<(VersionRequirement, String)>> = HashMap::new();
        let mut queue: VecDeque<(String, VersionRequirement, String, u32)> = VecDeque::new();

        // Start with the root dependency
        queue.push_back((plugin_id.to_string(), requirement.clone(), "root".to_string(), 0));

        while let Some((current_id, req, required_by, depth)) = queue.pop_front() {
            if depth > self.config.max_depth {
                return Err(ResolverError::MaxDepthExceeded);
            }

            // Get plugin metadata
            let metadata = self.get_plugin_metadata(&current_id).await?;

            // Find compatible version
            let version = self.find_compatible_version(&metadata, &req)?;

            // Check for conflicts with existing requirements
            if let Some(existing_reqs) = requirements.get(&current_id) {
                for (existing_req, existing_by) in existing_reqs {
                    if !version.version.is_compatible_with(existing_req) {
                        return Err(ResolverError::VersionConflict {
                            plugin: current_id.clone(),
                            requirement1: existing_req.to_string(),
                            requirement2: req.to_string(),
                        });
                    }
                }
            }

            // Store requirement
            requirements
                .entry(current_id.clone())
                .or_default()
                .push((req.clone(), required_by.clone()));

            // Check if already resolved
            if resolved.contains_key(&current_id) {
                // Update required_by
                if let Some(dep) = resolved.get_mut(&current_id) {
                    if !dep.required_by.contains(&required_by) {
                        dep.required_by.push(required_by);
                    }
                }
                continue;
            }

            // Add to resolved
            resolved.insert(
                current_id.clone(),
                ResolvedDependency {
                    plugin: metadata.clone(),
                    version: version.clone(),
                    direct: depth == 0,
                    required_by: vec![required_by],
                },
            );

            // Queue dependencies
            for dep in &metadata.dependencies {
                if dep.optional && !self.config.include_optional {
                    continue;
                }

                queue.push_back((
                    dep.name.clone(),
                    dep.version.clone(),
                    current_id.clone(),
                    depth + 1,
                ));
            }
        }

        // Convert to sorted list
        let deps: Vec<_> = resolved.into_values().collect();
        let sorted = self.topological_sort(&deps)?;

        let total_size = sorted.iter().map(|d| d.version.size).sum();
        let updates = self.find_updates(&sorted);
        let new_installs = self.find_new_installs(&sorted);

        Ok(Resolution {
            dependencies: sorted,
            total_size,
            updates,
            new_installs,
        })
    }

    /// Get plugin metadata (with caching)
    async fn get_plugin_metadata(&mut self, plugin_id: &str) -> Result<PluginMetadata, ResolverError> {
        if let Some(metadata) = self.cache.get(plugin_id) {
            return Ok(metadata.clone());
        }

        let metadata = self.client.get_plugin(plugin_id).await?;
        self.cache.insert(plugin_id.to_string(), metadata.clone());
        Ok(metadata)
    }

    /// Find a compatible version
    fn find_compatible_version(
        &self,
        metadata: &PluginMetadata,
        requirement: &VersionRequirement,
    ) -> Result<PluginVersion, ResolverError> {
        let ember_req = VersionRequirement::GreaterThanOrEqual(self.config.ember_version.clone());

        // Filter compatible versions
        let mut compatible: Vec<_> = metadata
            .versions
            .iter()
            .filter(|v| {
                !v.deprecated
                    && v.version.is_compatible_with(requirement)
                    && self.config.ember_version.is_compatible_with(&v.ember_version)
            })
            .collect();

        if compatible.is_empty() {
            return Err(ResolverError::NoCompatibleVersion {
                plugin: metadata.id.clone(),
                requirement: requirement.to_string(),
            });
        }

        // Sort by version (highest first)
        compatible.sort_by(|a, b| b.version.cmp(&a.version));

        // Prefer stable versions if configured
        if self.config.prefer_stable {
            if let Some(stable) = compatible.iter().find(|v| v.version.prerelease.is_none()) {
                return Ok((*stable).clone());
            }
        }

        Ok(compatible[0].clone())
    }

    /// Topological sort of dependencies
    fn topological_sort(&self, deps: &[ResolvedDependency]) -> Result<Vec<ResolvedDependency>, ResolverError> {
        let mut result = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut visiting: HashSet<String> = HashSet::new();

        // Build adjacency list
        let dep_map: HashMap<_, _> = deps.iter().map(|d| (d.plugin.id.clone(), d)).collect();

        fn visit(
            id: &str,
            dep_map: &HashMap<String, &ResolvedDependency>,
            visited: &mut HashSet<String>,
            visiting: &mut HashSet<String>,
            result: &mut Vec<ResolvedDependency>,
        ) -> Result<(), ResolverError> {
            if visited.contains(id) {
                return Ok(());
            }

            if visiting.contains(id) {
                return Err(ResolverError::CircularDependency(id.to_string()));
            }

            visiting.insert(id.to_string());

            if let Some(dep) = dep_map.get(id) {
                for sub_dep in &dep.plugin.dependencies {
                    visit(&sub_dep.name, dep_map, visited, visiting, result)?;
                }

                visited.insert(id.to_string());
                visiting.remove(id);
                result.push((*dep).clone());
            }

            Ok(())
        }

        for dep in deps {
            visit(&dep.plugin.id, &dep_map, &mut visited, &mut visiting, &mut result)?;
        }

        Ok(result)
    }

    /// Find plugins that will be updated
    fn find_updates(&self, deps: &[ResolvedDependency]) -> Vec<(String, SemVer, SemVer)> {
        deps.iter()
            .filter_map(|d| {
                self.installed.get(&d.plugin.id).and_then(|installed| {
                    if installed < &d.version.version {
                        Some((d.plugin.id.clone(), installed.clone(), d.version.version.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// Find plugins that will be newly installed
    fn find_new_installs(&self, deps: &[ResolvedDependency]) -> Vec<String> {
        deps.iter()
            .filter(|d| !self.installed.contains_key(&d.plugin.id))
            .map(|d| d.plugin.id.clone())
            .collect()
    }

    /// Clear the metadata cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

/// Installation plan builder
pub struct InstallPlan {
    /// Plugins to install in order
    pub install: Vec<(PluginMetadata, PluginVersion)>,
    /// Plugins to update in order
    pub update: Vec<(PluginMetadata, PluginVersion, SemVer)>,
    /// Plugins to remove
    pub remove: Vec<String>,
    /// Total download size
    pub download_size: u64,
}

impl InstallPlan {
    /// Create an installation plan from a resolution
    pub fn from_resolution(resolution: Resolution, installed: &HashMap<String, SemVer>) -> Self {
        let mut install = Vec::new();
        let mut update = Vec::new();

        for dep in resolution.dependencies {
            if let Some(current) = installed.get(&dep.plugin.id) {
                if current < &dep.version.version {
                    update.push((dep.plugin, dep.version, current.clone()));
                }
            } else {
                install.push((dep.plugin, dep.version));
            }
        }

        Self {
            install,
            update,
            remove: Vec::new(),
            download_size: resolution.total_size,
        }
    }

    /// Get human-readable summary
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();

        if !self.install.is_empty() {
            lines.push(format!("Install {} new plugins:", self.install.len()));
            for (plugin, version) in &self.install {
                lines.push(format!("  - {} v{}", plugin.name, version.version));
            }
        }

        if !self.update.is_empty() {
            lines.push(format!("Update {} plugins:", self.update.len()));
            for (plugin, version, from) in &self.update {
                lines.push(format!(
                    "  - {} v{} -> v{}",
                    plugin.name, from, version.version
                ));
            }
        }

        if !self.remove.is_empty() {
            lines.push(format!("Remove {} plugins:", self.remove.len()));
            for id in &self.remove {
                lines.push(format!("  - {}", id));
            }
        }

        lines.push(format!(
            "\nTotal download size: {}",
            format_size(self.download_size)
        ));

        lines.join("\n")
    }
}

/// Format byte size as human-readable string
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_resolver_config_default() {
        let config = ResolverConfig::default();
        assert_eq!(config.max_depth, 10);
        assert!(!config.include_optional);
        assert!(config.prefer_stable);
    }
}