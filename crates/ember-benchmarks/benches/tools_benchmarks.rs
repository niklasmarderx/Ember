//! Tools Benchmarks
//!
//! Benchmarks for ember-tools components including:
//! - Tool registry operations
//! - Tool parameter parsing
//! - JSON schema validation
//! - Tool execution simulation

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

/// Benchmark tool registry operations
fn bench_tool_registry(c: &mut Criterion) {
    use std::collections::HashMap;

    let mut group = c.benchmark_group("tool_registry");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark tool registration
    group.bench_function("register_tool", |b| {
        let mut registry: HashMap<String, serde_json::Value> = HashMap::new();
        let mut i = 0u64;

        b.iter(|| {
            let tool = serde_json::json!({
                "name": format!("tool_{}", i),
                "description": "A test tool",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "arg1": {"type": "string"},
                        "arg2": {"type": "integer"}
                    }
                }
            });
            registry.insert(format!("tool_{}", i), tool);
            i += 1;
            black_box(&registry);
        })
    });

    // Benchmark tool lookup
    group.bench_function("lookup_tool", |b| {
        let mut registry: HashMap<String, serde_json::Value> = HashMap::new();
        for i in 0..100 {
            registry.insert(
                format!("tool_{}", i),
                serde_json::json!({
                    "name": format!("tool_{}", i),
                    "description": "A test tool"
                }),
            );
        }

        let mut i = 0usize;
        b.iter(|| {
            let name = format!("tool_{}", i % 100);
            let tool = registry.get(black_box(&name));
            i += 1;
            black_box(tool)
        })
    });

    // Benchmark listing all tools
    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("list_tools", size), size, |b, &size| {
            let registry: HashMap<String, serde_json::Value> = (0..size)
                .map(|i| {
                    (
                        format!("tool_{}", i),
                        serde_json::json!({"name": format!("tool_{}", i)}),
                    )
                })
                .collect();

            b.iter(|| {
                let tools: Vec<_> = black_box(&registry).keys().collect();
                black_box(tools)
            })
        });
    }

    group.finish();
}

/// Benchmark tool parameter parsing
fn bench_parameter_parsing(c: &mut Criterion) {
    use serde_json::json;

    let mut group = c.benchmark_group("parameter_parsing");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark simple parameter parsing
    group.bench_function("parse_simple_params", |b| {
        let params_json = r#"{"name": "test", "count": 42}"#;

        b.iter(|| {
            let value: serde_json::Value = serde_json::from_str(black_box(params_json)).unwrap();
            black_box(value)
        })
    });

    // Benchmark complex parameter parsing
    group.bench_function("parse_complex_params", |b| {
        let params_json = r#"{
            "query": "search term",
            "filters": {
                "date_range": {"start": "2026-01-01", "end": "2026-03-15"},
                "categories": ["tech", "science", "ai"],
                "min_score": 0.8
            },
            "pagination": {
                "page": 1,
                "per_page": 20
            },
            "options": {
                "highlight": true,
                "include_metadata": true
            }
        }"#;

        b.iter(|| {
            let value: serde_json::Value = serde_json::from_str(black_box(params_json)).unwrap();
            black_box(value)
        })
    });

    // Benchmark parameter validation (simulated)
    group.bench_function("validate_params", |b| {
        let _schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "minLength": 1},
                "count": {"type": "integer", "minimum": 0}
            },
            "required": ["name"]
        });

        let params = json!({"name": "test", "count": 42});

        b.iter(|| {
            // Simulate basic validation
            let params = black_box(&params);
            let is_valid =
                params.is_object() && params.get("name").map(|v| v.is_string()).unwrap_or(false);
            black_box(is_valid)
        })
    });

    group.finish();
}

/// Benchmark tool call serialization
fn bench_tool_call_serialization(c: &mut Criterion) {
    use serde_json::json;

    let mut group = c.benchmark_group("tool_call_serialization");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark tool call creation
    group.bench_function("create_tool_call", |b| {
        b.iter(|| {
            let tool_call = json!({
                "id": "call_abc123",
                "type": "function",
                "function": {
                    "name": "search_web",
                    "arguments": "{\"query\": \"rust programming\"}"
                }
            });
            black_box(tool_call)
        })
    });

    // Benchmark tool response creation
    group.bench_function("create_tool_response", |b| {
        b.iter(|| {
            let response = json!({
                "tool_call_id": "call_abc123",
                "role": "tool",
                "content": "Search results: Found 10 articles about Rust programming..."
            });
            black_box(response)
        })
    });

    // Benchmark batch tool calls
    for batch_size in [1, 5, 10, 20].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("batch_tool_calls", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let calls: Vec<serde_json::Value> = (0..batch_size)
                        .map(|i| {
                            json!({
                                "id": format!("call_{}", i),
                                "type": "function",
                                "function": {
                                    "name": format!("tool_{}", i % 5),
                                    "arguments": format!("{{\"param\": {}}}", i)
                                }
                            })
                        })
                        .collect();
                    black_box(calls)
                })
            },
        );
    }

    group.finish();
}

/// Benchmark file path operations (common in tools)
fn bench_path_operations(c: &mut Criterion) {
    use std::path::PathBuf;

    let mut group = c.benchmark_group("path_operations");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark path construction
    group.bench_function("construct_path", |b| {
        b.iter(|| {
            let path = PathBuf::from("/home/user/projects/ember/src/main.rs");
            black_box(path)
        })
    });

    // Benchmark path joining
    group.bench_function("join_paths", |b| {
        let base = PathBuf::from("/home/user/projects");

        b.iter(|| {
            let path = black_box(&base).join("ember").join("src").join("main.rs");
            black_box(path)
        })
    });

    // Benchmark path canonicalization (simulated)
    group.bench_function("normalize_path", |b| {
        let path = "/home/user/../user/projects/./ember/../ember/src";

        b.iter(|| {
            // Simulate normalization
            let normalized: String = black_box(path)
                .split('/')
                .filter(|s| !s.is_empty() && *s != ".")
                .fold(Vec::new(), |mut acc, segment| {
                    if segment == ".." {
                        acc.pop();
                    } else {
                        acc.push(segment);
                    }
                    acc
                })
                .join("/");
            black_box(normalized)
        })
    });

    // Benchmark extension extraction
    group.bench_function("extract_extension", |b| {
        let paths = vec![
            "main.rs",
            "config.toml",
            "README.md",
            "script.py",
            "data.json",
        ];

        let mut i = 0usize;
        b.iter(|| {
            let path = black_box(paths[i % paths.len()]);
            let ext = path.rsplit('.').next();
            i += 1;
            black_box(ext)
        })
    });

    group.finish();
}

/// Benchmark command building (for shell tools)
fn bench_command_building(c: &mut Criterion) {
    let mut group = c.benchmark_group("command_building");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark simple command
    group.bench_function("build_simple_command", |b| {
        b.iter(|| {
            let cmd = format!("{} {}", black_box("ls"), black_box("-la"));
            black_box(cmd)
        })
    });

    // Benchmark command with arguments
    group.bench_function("build_command_with_args", |b| {
        let args = vec!["-la", "--color=auto", "/home/user"];

        b.iter(|| {
            let cmd = format!("ls {}", black_box(&args).join(" "));
            black_box(cmd)
        })
    });

    // Benchmark shell escaping
    group.bench_function("escape_shell_arg", |b| {
        let dangerous_input = "test; rm -rf /; echo 'pwned'";

        b.iter(|| {
            // Simple escaping by quoting
            let escaped = format!("'{}'", black_box(dangerous_input).replace('\'', "'\\''"));
            black_box(escaped)
        })
    });

    // Benchmark environment variable substitution
    group.bench_function("substitute_env_vars", |b| {
        let template = "Hello $USER, your home is $HOME";
        let vars: std::collections::HashMap<&str, &str> =
            [("USER", "alice"), ("HOME", "/home/alice")]
                .into_iter()
                .collect();

        b.iter(|| {
            let mut result = black_box(template).to_string();
            for (key, value) in black_box(&vars) {
                result = result.replace(&format!("${}", key), value);
            }
            black_box(result)
        })
    });

    group.finish();
}

/// Benchmark HTTP request/response handling (for web tools)
fn bench_http_handling(c: &mut Criterion) {
    use serde_json::json;

    let mut group = c.benchmark_group("http_handling");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark URL parsing
    group.bench_function("parse_url", |b| {
        let url = "https://api.example.com/v1/users?page=1&limit=10#section";

        b.iter(|| {
            // Simple URL parsing
            let parts: Vec<&str> = black_box(url).split(&['?', '#'][..]).collect();
            black_box(parts)
        })
    });

    // Benchmark query string building
    group.bench_function("build_query_string", |b| {
        let params = vec![("page", "1"), ("limit", "10"), ("sort", "name")];

        b.iter(|| {
            let query: String = black_box(&params)
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            black_box(query)
        })
    });

    // Benchmark header parsing
    group.bench_function("parse_headers", |b| {
        let headers_text = "Content-Type: application/json\r\nAuthorization: Bearer token123\r\nX-Request-ID: abc-123";

        b.iter(|| {
            let headers: std::collections::HashMap<&str, &str> = black_box(headers_text)
                .lines()
                .filter_map(|line| {
                    let mut parts = line.splitn(2, ": ");
                    Some((parts.next()?, parts.next()?))
                })
                .collect();
            black_box(headers)
        })
    });

    // Benchmark JSON response building
    group.bench_function("build_json_response", |b| {
        b.iter(|| {
            let response = json!({
                "status": "success",
                "data": {
                    "users": [
                        {"id": 1, "name": "Alice"},
                        {"id": 2, "name": "Bob"},
                        {"id": 3, "name": "Charlie"}
                    ]
                },
                "meta": {
                    "total": 3,
                    "page": 1,
                    "per_page": 10
                }
            });
            black_box(response)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_tool_registry,
    bench_parameter_parsing,
    bench_tool_call_serialization,
    bench_path_operations,
    bench_command_building,
    bench_http_handling,
);

criterion_main!(benches);
