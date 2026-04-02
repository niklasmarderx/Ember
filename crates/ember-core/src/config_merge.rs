//! Multi-source configuration loading and merging.
//!
//! Loads configuration from up to three sources in order of increasing priority:
//!
//! 1. **User** (`~/.ember/config.json`) — global defaults
//! 2. **Project** (`.ember/config.json` in the repository root) — project settings
//! 3. **Local** (`.ember/config.local.json` in the repository root) — machine-local overrides
//!
//! When the same key appears in multiple sources, the higher-priority source wins.
//! For nested JSON objects the merge is *deep* (keys are merged recursively); for
//! scalar values and arrays the overlay simply replaces the base value.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Error, Result};

// ── Source ────────────────────────────────────────────────────────────────────

/// Where a configuration entry came from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConfigSource {
    /// `~/.ember/config.json` — global user defaults.
    User,
    /// `.ember/config.json` in the project root — project-level settings.
    Project,
    /// `.ember/config.local.json` in the project root — machine-local overrides.
    Local,
}

impl ConfigSource {
    /// Human-readable label used in debug output.
    pub fn label(&self) -> &'static str {
        match self {
            ConfigSource::User => "user (~/.ember/config.json)",
            ConfigSource::Project => "project (.ember/config.json)",
            ConfigSource::Local => "local (.ember/config.local.json)",
        }
    }

    /// Numeric priority: higher value = higher priority (wins during merge).
    fn priority(&self) -> u8 {
        match self {
            ConfigSource::User => 0,
            ConfigSource::Project => 1,
            ConfigSource::Local => 2,
        }
    }
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ── Entry ─────────────────────────────────────────────────────────────────────

/// A single loaded configuration file.
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    /// Which source this entry came from.
    pub source: ConfigSource,
    /// Filesystem path from which the file was loaded.
    pub path: PathBuf,
    /// Top-level key→value pairs from the file.
    pub values: BTreeMap<String, Value>,
}

// ── MergedConfig ──────────────────────────────────────────────────────────────

/// The result of merging all discovered configuration sources.
#[derive(Debug, Clone, Default)]
pub struct MergedConfig {
    /// Every entry that contributed to this config, lowest priority first.
    pub entries: Vec<ConfigEntry>,
    /// The final merged key→value map.
    pub merged: BTreeMap<String, Value>,
}

impl MergedConfig {
    /// Deserialise a top-level key into `T`.
    ///
    /// Returns `None` if the key is absent.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let v = self.merged.get(key)?;
        serde_json::from_value(v.clone()).ok()
    }

    /// Return the highest-priority source that contributed a top-level key.
    ///
    /// Returns `None` if no entry contains the key.
    pub fn get_source(&self, key: &str) -> Option<ConfigSource> {
        // Iterate in reverse so we find the highest-priority source first.
        self.entries
            .iter()
            .rev()
            .find(|e| e.values.contains_key(key))
            .map(|e| e.source.clone())
    }

    /// Produce a human-readable summary of every source and the keys it provides.
    pub fn format_debug(&self) -> String {
        if self.entries.is_empty() {
            return "No configuration sources found.".to_string();
        }

        let mut out = String::new();
        for entry in &self.entries {
            out.push_str(&format!("[{}]\n  path: {}\n", entry.source, entry.path.display()));
            if entry.values.is_empty() {
                out.push_str("  (no keys)\n");
            } else {
                for key in entry.values.keys() {
                    out.push_str(&format!("  - {key}\n"));
                }
            }
        }
        out.push_str(&format!(
            "\nMerged keys ({}): {}\n",
            self.merged.len(),
            self.merged.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
        out
    }
}

// ── Merge helpers ─────────────────────────────────────────────────────────────

/// Recursively merge `overlay` into `base`.
///
/// * **Objects**: keys from `overlay` are merged into `base` recursively.
/// * **Everything else** (scalars, arrays, null): `overlay` replaces `base`.
pub fn deep_merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (key, ov) in o {
                let bv = b.entry(key.clone()).or_insert(Value::Null);
                deep_merge(bv, ov);
            }
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

// ── File I/O ──────────────────────────────────────────────────────────────────

/// Read and parse a JSON config file into a flat `BTreeMap`.
///
/// The file must contain a JSON object at the top level.  Returns an error if
/// the file is missing, unreadable, or does not parse as an object.
pub fn load_config_file(path: &Path) -> Result<BTreeMap<String, Value>> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::config(format!("Cannot read {}: {e}", path.display())))?;

    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| Error::config(format!("Invalid JSON in {}: {e}", path.display())))?;

    match value {
        Value::Object(map) => Ok(map.into_iter().collect()),
        _ => Err(Error::config(format!(
            "Config file must be a JSON object: {}",
            path.display()
        ))),
    }
}

// ── Discovery ─────────────────────────────────────────────────────────────────

/// Discover all config files reachable from `cwd` and return them as
/// [`ConfigEntry`] values ordered from lowest to highest priority
/// (User < Project < Local).
///
/// The function walks upward from `cwd` looking for a `.ember/` directory
/// (the project root).  It also checks `~/.ember/config.json` for the user
/// config.  Missing files are silently skipped; only *present* files are
/// included in the result.
pub fn discover_config_files(cwd: &Path) -> Vec<ConfigEntry> {
    let mut entries: Vec<ConfigEntry> = Vec::new();

    // ── 1. User config ────────────────────────────────────────────────────────
    if let Some(home) = home_dir() {
        let user_path = home.join(".ember").join("config.json");
        if user_path.is_file() {
            if let Ok(values) = load_config_file(&user_path) {
                entries.push(ConfigEntry {
                    source: ConfigSource::User,
                    path: user_path,
                    values,
                });
            }
        }
    }

    // ── 2 & 3. Walk up from cwd to find the project root ─────────────────────
    if let Some(project_root) = find_project_root(cwd) {
        let ember_dir = project_root.join(".ember");

        let project_path = ember_dir.join("config.json");
        if project_path.is_file() {
            if let Ok(values) = load_config_file(&project_path) {
                entries.push(ConfigEntry {
                    source: ConfigSource::Project,
                    path: project_path,
                    values,
                });
            }
        }

        let local_path = ember_dir.join("config.local.json");
        if local_path.is_file() {
            if let Ok(values) = load_config_file(&local_path) {
                entries.push(ConfigEntry {
                    source: ConfigSource::Local,
                    path: local_path,
                    values,
                });
            }
        }
    }

    entries
}

/// Walk the ancestor chain of `start` looking for the first directory that
/// contains a `.ember/` subdirectory.
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".ember").is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Return the current user's home directory.
fn home_dir() -> Option<PathBuf> {
    // std::env::home_dir is deprecated but still works.  We avoid the
    // `home` crate dependency by reading the environment variable directly.
    std::env::var_os("HOME").map(PathBuf::from)
}

// ── ConfigLoader ──────────────────────────────────────────────────────────────

/// High-level API for loading and merging configuration.
///
/// ```rust,no_run
/// use std::path::Path;
/// use ember_core::config_merge::ConfigLoader;
///
/// let config = ConfigLoader::new()
///     .discover(Path::new("."))
///     .merge();
///
/// if let Some(model) = config.get::<String>("model") {
///     println!("model = {model}");
/// }
/// ```
#[derive(Debug, Default)]
pub struct ConfigLoader {
    entries: Vec<ConfigEntry>,
}

impl ConfigLoader {
    /// Create a new, empty loader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Discover config files starting from `cwd` and add them to the loader.
    ///
    /// Can be called multiple times; duplicate paths are deduplicated by path.
    pub fn discover(mut self, cwd: &Path) -> Self {
        for entry in discover_config_files(cwd) {
            // Skip if we already have this path (idempotent on repeated calls).
            if !self.entries.iter().any(|e| e.path == entry.path) {
                self.entries.push(entry);
            }
        }
        self
    }

    /// Add a pre-loaded [`ConfigEntry`] directly (useful for testing).
    pub fn add_entry(mut self, entry: ConfigEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Produce a [`MergedConfig`] by deep-merging all loaded entries in
    /// priority order (lowest first, so higher-priority sources overwrite).
    pub fn merge(mut self) -> MergedConfig {
        // Sort by ascending priority so higher-priority sources are applied last.
        self.entries
            .sort_by_key(|e| e.source.priority());

        let mut merged_value = Value::Object(Default::default());

        for entry in &self.entries {
            for (k, v) in &entry.values {
                let slot = merged_value
                    .as_object_mut()
                    .expect("always an object")
                    .entry(k.clone())
                    .or_insert(Value::Null);
                deep_merge(slot, v);
            }
        }

        let merged: BTreeMap<String, Value> = match merged_value {
            Value::Object(m) => m.into_iter().collect(),
            _ => BTreeMap::new(),
        };

        MergedConfig {
            entries: self.entries,
            merged,
        }
    }

    /// Convenience: discover from `cwd` and merge in one call.
    pub fn load(cwd: &Path) -> MergedConfig {
        Self::new().discover(cwd).merge()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn entry(source: ConfigSource, path: PathBuf, json: Value) -> ConfigEntry {
        let values = match json {
            Value::Object(m) => m.into_iter().collect(),
            _ => panic!("test helper requires a JSON object"),
        };
        ConfigEntry { source, path, values }
    }

    fn loader_from_entries(entries: Vec<ConfigEntry>) -> MergedConfig {
        entries
            .into_iter()
            .fold(ConfigLoader::new(), |l, e| l.add_entry(e))
            .merge()
    }

    // ── deep_merge ────────────────────────────────────────────────────────────

    #[test]
    fn deep_merge_scalars_replaced() {
        let mut base = json!({"a": 1});
        deep_merge(&mut base, &json!({"a": 2}));
        assert_eq!(base["a"], 2);
    }

    #[test]
    fn deep_merge_new_key_added() {
        let mut base = json!({"a": 1});
        deep_merge(&mut base, &json!({"b": 2}));
        assert_eq!(base["a"], 1);
        assert_eq!(base["b"], 2);
    }

    #[test]
    fn deep_merge_nested_objects_merged() {
        let mut base = json!({"db": {"host": "localhost", "port": 5432}});
        deep_merge(&mut base, &json!({"db": {"port": 9999}}));
        assert_eq!(base["db"]["host"], "localhost"); // preserved
        assert_eq!(base["db"]["port"], 9999);        // overwritten
    }

    #[test]
    fn deep_merge_array_replaced_not_merged() {
        let mut base = json!({"tags": ["a", "b"]});
        deep_merge(&mut base, &json!({"tags": ["c"]}));
        assert_eq!(base["tags"], json!(["c"]));
    }

    #[test]
    fn deep_merge_object_over_scalar() {
        let mut base = json!({"key": "scalar"});
        deep_merge(&mut base, &json!({"key": {"nested": true}}));
        assert_eq!(base["key"]["nested"], true);
    }

    // ── priority: Local > Project > User ──────────────────────────────────────

    #[test]
    fn merge_priority_local_beats_project() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::Project, PathBuf::from("p"), json!({"x": "project"})),
            entry(ConfigSource::Local, PathBuf::from("l"), json!({"x": "local"})),
        ]);
        assert_eq!(config.get::<String>("x").unwrap(), "local");
    }

    #[test]
    fn merge_priority_project_beats_user() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::User, PathBuf::from("u"), json!({"x": "user"})),
            entry(ConfigSource::Project, PathBuf::from("p"), json!({"x": "project"})),
        ]);
        assert_eq!(config.get::<String>("x").unwrap(), "project");
    }

    #[test]
    fn merge_priority_local_beats_user() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::User, PathBuf::from("u"), json!({"x": "user"})),
            entry(ConfigSource::Local, PathBuf::from("l"), json!({"x": "local"})),
        ]);
        assert_eq!(config.get::<String>("x").unwrap(), "local");
    }

    #[test]
    fn merge_all_three_correct_winner() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::User, PathBuf::from("u"), json!({"x": "user"})),
            entry(ConfigSource::Project, PathBuf::from("p"), json!({"x": "project"})),
            entry(ConfigSource::Local, PathBuf::from("l"), json!({"x": "local"})),
        ]);
        assert_eq!(config.get::<String>("x").unwrap(), "local");
    }

    // ── get / get_source ──────────────────────────────────────────────────────

    #[test]
    fn get_returns_none_for_missing_key() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::User, PathBuf::from("u"), json!({"a": 1})),
        ]);
        assert!(config.get::<i64>("missing").is_none());
    }

    #[test]
    fn get_deserialises_nested_struct() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Db { host: String, port: u16 }

        let config = loader_from_entries(vec![
            entry(ConfigSource::Project, PathBuf::from("p"),
                  json!({"db": {"host": "127.0.0.1", "port": 5432}})),
        ]);
        let db: Db = config.get("db").unwrap();
        assert_eq!(db, Db { host: "127.0.0.1".to_string(), port: 5432 });
    }

    #[test]
    fn get_source_returns_highest_priority_source() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::User, PathBuf::from("u"), json!({"x": 1})),
            entry(ConfigSource::Project, PathBuf::from("p"), json!({"x": 2})),
        ]);
        assert_eq!(config.get_source("x"), Some(ConfigSource::Project));
    }

    #[test]
    fn get_source_returns_none_for_absent_key() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::User, PathBuf::from("u"), json!({"a": 1})),
        ]);
        assert!(config.get_source("nope").is_none());
    }

    #[test]
    fn get_source_only_in_user() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::User, PathBuf::from("u"), json!({"only_user": true})),
            entry(ConfigSource::Project, PathBuf::from("p"), json!({"other": 1})),
        ]);
        assert_eq!(config.get_source("only_user"), Some(ConfigSource::User));
    }

    // ── format_debug ──────────────────────────────────────────────────────────

    #[test]
    fn format_debug_empty() {
        let config = MergedConfig::default();
        assert!(config.format_debug().contains("No configuration sources"));
    }

    #[test]
    fn format_debug_lists_sources_and_keys() {
        let config = loader_from_entries(vec![
            entry(ConfigSource::User, PathBuf::from("/home/u/.ember/config.json"),
                  json!({"model": "gpt-4", "timeout": 30})),
            entry(ConfigSource::Local, PathBuf::from(".ember/config.local.json"),
                  json!({"debug": true})),
        ]);
        let s = config.format_debug();
        assert!(s.contains("user"), "should list user source");
        assert!(s.contains("local"), "should list local source");
        assert!(s.contains("model"), "should list key model");
        assert!(s.contains("debug"), "should list key debug");
    }

    // ── file I/O ──────────────────────────────────────────────────────────────

    #[test]
    fn load_config_file_parses_valid_json() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("cfg.json");
        fs::write(&p, r#"{"key": "value", "num": 42}"#).unwrap();

        let map = load_config_file(&p).unwrap();
        assert_eq!(map["key"], json!("value"));
        assert_eq!(map["num"], json!(42));
    }

    #[test]
    fn load_config_file_rejects_array() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("cfg.json");
        fs::write(&p, r#"[1, 2, 3]"#).unwrap();
        assert!(load_config_file(&p).is_err());
    }

    #[test]
    fn load_config_file_missing_returns_error() {
        let result = load_config_file(Path::new("/nonexistent/path/cfg.json"));
        assert!(result.is_err());
    }

    // ── discover + ConfigLoader ───────────────────────────────────────────────

    #[test]
    fn discover_finds_project_and_local_files() {
        let dir = TempDir::new().unwrap();
        let ember_dir = dir.path().join(".ember");
        fs::create_dir(&ember_dir).unwrap();
        fs::write(ember_dir.join("config.json"), r#"{"env": "dev"}"#).unwrap();
        fs::write(ember_dir.join("config.local.json"), r#"{"debug": true}"#).unwrap();

        let entries = discover_config_files(dir.path());
        let sources: Vec<_> = entries.iter().map(|e| &e.source).collect();
        assert!(sources.contains(&&ConfigSource::Project));
        assert!(sources.contains(&&ConfigSource::Local));
    }

    #[test]
    fn discover_walks_up_from_subdirectory() {
        let dir = TempDir::new().unwrap();
        let ember_dir = dir.path().join(".ember");
        fs::create_dir_all(&ember_dir).unwrap();
        fs::write(ember_dir.join("config.json"), r#"{"found": true}"#).unwrap();

        let sub = dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&sub).unwrap();

        let entries = discover_config_files(&sub);
        assert!(entries.iter().any(|e| e.source == ConfigSource::Project));
    }

    #[test]
    fn config_loader_load_convenience() {
        let dir = TempDir::new().unwrap();
        let ember_dir = dir.path().join(".ember");
        fs::create_dir(&ember_dir).unwrap();
        fs::write(ember_dir.join("config.json"), r#"{"hello": "world"}"#).unwrap();

        let config = ConfigLoader::load(dir.path());
        assert_eq!(config.get::<String>("hello").unwrap(), "world");
    }
}
