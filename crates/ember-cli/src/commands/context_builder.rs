//! Project context builder for `/init` seeding.
//!
//! Inspects the current working directory to detect project type, list
//! top-level files, and summarise git status — used to generate the
//! initial `EMBER.md` project context file.

/// Build a context string describing the current working directory and its contents.
/// Used by `/init` to seed the initial EMBER.md file.
pub fn build_working_directory_context() -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let mut ctx = format!("## Working Directory\n\nCurrent directory: `{}`\n", cwd);

    // Detect project type
    let project_root = std::env::current_dir().unwrap_or_default();
    let mut project_types = Vec::new();

    // Read and summarize Cargo.toml
    let cargo_toml = project_root.join("Cargo.toml");
    if cargo_toml.exists() {
        project_types.push("Rust (Cargo)");
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            // Extract name, version, and key info
            let mut name = None;
            let mut version = None;
            for line in content.lines().take(20) {
                if line.starts_with("name") {
                    name = line
                        .split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string());
                }
                if line.starts_with("version") {
                    version = line
                        .split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string());
                }
            }
            if let (Some(n), Some(v)) = (name, version) {
                ctx.push_str(&format!("Rust project: {} v{}\n", n, v));
            }
            // Count workspace members if present
            if content.contains("[workspace]") {
                let members: Vec<&str> = content
                    .lines()
                    .filter(|l| l.trim().starts_with('"') && l.contains("crates/"))
                    .collect();
                if !members.is_empty() {
                    ctx.push_str(&format!("Workspace with {} crates\n", members.len()));
                }
            }
        }
    }

    // Read and summarize package.json
    let package_json = project_root.join("package.json");
    if package_json.exists() {
        project_types.push("JavaScript/TypeScript (npm)");
        if let Ok(content) = std::fs::read_to_string(&package_json) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                let name = pkg
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let version = pkg
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0");
                ctx.push_str(&format!("Node project: {} v{}\n", name, version));
                // Show available scripts
                if let Some(scripts) = pkg.get("scripts").and_then(|v| v.as_object()) {
                    let script_names: Vec<&str> =
                        scripts.keys().map(|k| k.as_str()).take(10).collect();
                    ctx.push_str(&format!("Scripts: {}\n", script_names.join(", ")));
                }
                // Show key deps
                if let Some(deps) = pkg.get("dependencies").and_then(|v| v.as_object()) {
                    let dep_names: Vec<&str> = deps.keys().map(|k| k.as_str()).take(15).collect();
                    ctx.push_str(&format!("Dependencies: {}\n", dep_names.join(", ")));
                }
            }
        }
    }

    // Read and summarize pyproject.toml
    let pyproject = project_root.join("pyproject.toml");
    if pyproject.exists() {
        project_types.push("Python");
        if let Ok(content) = std::fs::read_to_string(&pyproject) {
            for line in content.lines().take(20) {
                if line.starts_with("name") {
                    if let Some(name) = line.split('=').nth(1) {
                        ctx.push_str(&format!(
                            "Python project: {}\n",
                            name.trim().trim_matches('"')
                        ));
                    }
                }
            }
        }
    } else if project_root.join("setup.py").exists() {
        project_types.push("Python");
    }

    if project_root.join("go.mod").exists() {
        project_types.push("Go");
        if let Ok(content) = std::fs::read_to_string(project_root.join("go.mod")) {
            if let Some(module_line) = content.lines().next() {
                ctx.push_str(&format!("{}\n", module_line));
            }
        }
    }
    if project_root.join("Makefile").exists() {
        project_types.push("Make");
    }
    if project_root.join(".git").exists() {
        project_types.push("Git repository");
    }

    if !project_types.is_empty() {
        ctx.push_str(&format!("Project type: {}\n", project_types.join(", ")));
    }

    // List top-level files (limited to keep context manageable)
    if let Ok(entries) = std::fs::read_dir(&project_root) {
        let mut files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    format!("  {}/", name)
                } else {
                    format!("  {}", name)
                }
            })
            .collect();
        files.sort();

        // Limit to 50 entries
        let total = files.len();
        if total > 50 {
            files.truncate(50);
            files.push(format!("  ... and {} more", total - 50));
        }

        ctx.push_str("\nTop-level contents:\n");
        ctx.push_str(&files.join("\n"));
        ctx.push('\n');
    }

    // Include git status if available
    if project_root.join(".git").exists() {
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--short", "--branch"])
            .current_dir(&project_root)
            .output()
        {
            if output.status.success() {
                let status = String::from_utf8_lossy(&output.stdout);
                let status = status.trim();
                if !status.is_empty() {
                    ctx.push_str(&format!("\nGit status:\n```\n{}\n```\n", status));
                }
            }
        }
    }

    ctx
}
