//! Full Benchmark Suite for Ember
//!
//! This benchmark combines all individual benchmark groups into a single
//! comprehensive performance test. Run with:
//!
//! ```bash
//! cargo bench -p ember-benchmarks --bench full_suite
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;
use std::time::Duration;

// =============================================================================
// Core Operations Benchmarks
// =============================================================================

fn full_core_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_suite/core");
    group.measurement_time(Duration::from_secs(3));

    // Config parsing
    group.bench_function("config_json_parse", |b| {
        let json = r#"{"model":"gpt-4","temperature":0.7,"max_tokens":1000}"#;
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(json)).unwrap();
        });
    });

    // Message creation
    group.bench_function("message_creation", |b| {
        b.iter(|| {
            serde_json::json!({
                "role": "user",
                "content": black_box("Hello, how are you today?"),
                "timestamp": chrono::Utc::now().to_rfc3339()
            })
        });
    });

    // Conversation serialization
    for size in [10, 100, 500] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("conversation_serialize", size),
            &size,
            |b, &size| {
                let messages: Vec<serde_json::Value> = (0..size)
                    .map(|i| {
                        serde_json::json!({
                            "role": if i % 2 == 0 { "user" } else { "assistant" },
                            "content": format!("Message {}", i)
                        })
                    })
                    .collect();
                b.iter(|| serde_json::to_string(black_box(&messages)).unwrap());
            },
        );
    }

    // Context operations
    group.bench_function("context_create", |b| {
        b.iter(|| {
            let mut ctx = HashMap::new();
            ctx.insert("model".to_string(), "gpt-4".to_string());
            ctx.insert("temperature".to_string(), "0.7".to_string());
            ctx.insert("max_tokens".to_string(), "2000".to_string());
            ctx.insert("system_prompt".to_string(), "You are helpful".to_string());
            ctx
        });
    });

    // Token counting approximation
    for size in [100, 1000, 5000] {
        let text: String = "word ".repeat(size);
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("token_count_approx", size),
            &text,
            |b, text| {
                b.iter(|| {
                    let words = black_box(text).split_whitespace().count();
                    let chars = text.chars().count();
                    (words + chars / 4) / 2 // Simple approximation
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Storage Operations Benchmarks
// =============================================================================

fn full_storage_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_suite/storage");
    group.measurement_time(Duration::from_secs(3));

    // Memory store operations
    group.bench_function("memory_store_insert_100", |b| {
        b.iter(|| {
            let mut store: HashMap<String, String> = HashMap::new();
            for i in 0..100 {
                store.insert(format!("key_{}", i), format!("value_{}", i));
            }
            store
        });
    });

    group.bench_function("memory_store_get", |b| {
        let mut store: HashMap<String, String> = HashMap::new();
        for i in 0..1000 {
            store.insert(format!("key_{}", i), format!("value_{}", i));
        }
        b.iter(|| store.get(black_box("key_500")).cloned());
    });

    // Vector operations
    for dim in [128, 384, 768, 1536] {
        group.bench_with_input(
            BenchmarkId::new("vector_cosine_similarity", dim),
            &dim,
            |b, &dim| {
                let v1: Vec<f32> = (0..dim).map(|i| (i as f32).sin()).collect();
                let v2: Vec<f32> = (0..dim).map(|i| (i as f32).cos()).collect();
                b.iter(|| {
                    let dot: f32 = black_box(&v1)
                        .iter()
                        .zip(black_box(&v2).iter())
                        .map(|(a, b)| a * b)
                        .sum();
                    let norm1: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let norm2: f32 = v2.iter().map(|x| x * x).sum::<f32>().sqrt();
                    dot / (norm1 * norm2)
                });
            },
        );
    }

    // Text chunking
    let long_text = "This is a sentence. ".repeat(500);
    group.throughput(Throughput::Bytes(long_text.len() as u64));

    group.bench_function("text_chunk_fixed", |b| {
        b.iter(|| {
            black_box(&long_text)
                .as_bytes()
                .chunks(512)
                .map(|c| String::from_utf8_lossy(c).to_string())
                .collect::<Vec<_>>()
        });
    });

    group.bench_function("text_chunk_sentence", |b| {
        b.iter(|| {
            black_box(&long_text)
                .split(". ")
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        });
    });

    group.finish();
}

// =============================================================================
// Tool Operations Benchmarks
// =============================================================================

fn full_tools_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_suite/tools");
    group.measurement_time(Duration::from_secs(3));

    // Tool registry
    group.bench_function("registry_insert_50", |b| {
        b.iter(|| {
            let mut registry: HashMap<String, serde_json::Value> = HashMap::new();
            for i in 0..50 {
                registry.insert(
                    format!("tool_{}", i),
                    serde_json::json!({
                        "name": format!("tool_{}", i),
                        "description": "A test tool",
                        "parameters": {}
                    }),
                );
            }
            registry
        });
    });

    group.bench_function("registry_lookup", |b| {
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
        b.iter(|| registry.get(black_box("tool_50")).cloned());
    });

    // Parameter parsing
    group.bench_function("params_parse_complex", |b| {
        let params = r#"{
            "command": "ls",
            "args": ["-la", "/home"],
            "env": {"PATH": "/usr/bin"},
            "timeout": 30000,
            "capture_output": true
        }"#;
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(params)).unwrap();
        });
    });

    // Tool call creation
    group.bench_function("tool_call_create", |b| {
        b.iter(|| {
            serde_json::json!({
                "id": black_box("call_123"),
                "tool": "shell_execute",
                "parameters": {"command": "echo hello"},
                "timestamp": chrono::Utc::now().to_rfc3339()
            })
        });
    });

    // Path operations
    group.bench_function("path_join", |b| {
        b.iter(|| {
            std::path::PathBuf::from(black_box("/home"))
                .join("user")
                .join("documents")
                .join("file.txt")
        });
    });

    // HTTP URL parsing
    group.bench_function("url_parse", |b| {
        let url = "https://api.example.com/v1/chat?model=gpt-4&stream=true";
        b.iter(|| {
            let parts: Vec<&str> = black_box(url).split([':', '/', '?', '&']).collect();
            parts
        });
    });

    // JSON response handling
    group.bench_function("json_response_parse", |b| {
        let response = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 9, "completion_tokens": 12, "total_tokens": 21}
        }"#;
        b.iter(|| {
            let _: serde_json::Value = serde_json::from_str(black_box(response)).unwrap();
        });
    });

    group.finish();
}

// =============================================================================
// End-to-End Workflow Benchmarks
// =============================================================================

fn full_workflow_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_suite/workflow");
    group.measurement_time(Duration::from_secs(5));

    // Simulate complete chat request processing
    group.bench_function("chat_request_process", |b| {
        b.iter(|| {
            // 1. Parse request
            let request = serde_json::json!({
                "model": "gpt-4",
                "messages": [
                    {"role": "system", "content": "You are helpful"},
                    {"role": "user", "content": "Hello"}
                ],
                "temperature": 0.7
            });
            let _parsed: serde_json::Value =
                serde_json::from_str(&serde_json::to_string(black_box(&request)).unwrap()).unwrap();

            // 2. Validate context
            let mut context = HashMap::new();
            context.insert("model", "gpt-4");
            context.insert("temperature", "0.7");

            // 3. Token estimation
            let content = "You are helpful Hello";
            let _tokens = content.split_whitespace().count() * 4 / 3;

            // 4. Build response
            serde_json::json!({
                "id": "resp_123",
                "model": "gpt-4",
                "message": {"role": "assistant", "content": "Hi there!"},
                "usage": {"total_tokens": 25}
            })
        });
    });

    // Simulate tool execution pipeline
    group.bench_function("tool_execution_pipeline", |b| {
        b.iter(|| {
            // 1. Parse tool call
            let call = serde_json::json!({
                "tool": "shell_execute",
                "parameters": {"command": "echo test"}
            });
            let _: serde_json::Value =
                serde_json::from_str(&serde_json::to_string(black_box(&call)).unwrap()).unwrap();

            // 2. Validate parameters
            let params: HashMap<String, String> =
                HashMap::from([("command".to_string(), "echo test".to_string())]);
            let _valid = params.contains_key("command");

            // 3. Build result
            serde_json::json!({
                "tool": "shell_execute",
                "success": true,
                "output": "test\n",
                "exit_code": 0
            })
        });
    });

    // Simulate RAG retrieval
    group.bench_function("rag_retrieval_simulate", |b| {
        // Setup: Create document store
        let documents: Vec<(String, Vec<f32>)> = (0..100)
            .map(|i| {
                let embedding: Vec<f32> = (0..384).map(|j| ((i * j) as f32).sin()).collect();
                (format!("Document {}", i), embedding)
            })
            .collect();

        let query_embedding: Vec<f32> = (0..384).map(|i| (i as f32 * 0.5).cos()).collect();

        b.iter(|| {
            // Find top-5 similar documents
            let mut scores: Vec<(usize, f32)> = documents
                .iter()
                .enumerate()
                .map(|(idx, (_, emb))| {
                    let dot: f32 = black_box(&query_embedding)
                        .iter()
                        .zip(emb.iter())
                        .map(|(a, b)| a * b)
                        .sum();
                    (idx, dot)
                })
                .collect();

            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            scores.truncate(5);

            scores
                .iter()
                .map(|(idx, score)| (&documents[*idx].0, *score))
                .collect::<Vec<_>>()
        });
    });

    group.finish();
}

// =============================================================================
// Memory and Allocation Benchmarks
// =============================================================================

fn full_memory_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_suite/memory");
    group.measurement_time(Duration::from_secs(3));

    // String allocation patterns
    for size in [100, 1000, 10000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("string_alloc", size), &size, |b, &size| {
            b.iter(|| {
                let strings: Vec<String> =
                    (0..size).map(|i| format!("String number {}", i)).collect();
                black_box(strings)
            });
        });
    }

    // HashMap growth
    for size in [100, 1000, 5000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("hashmap_growth", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let mut map: HashMap<String, serde_json::Value> = HashMap::new();
                    for i in 0..size {
                        map.insert(format!("key_{}", i), serde_json::json!({"value": i}));
                    }
                    black_box(map)
                });
            },
        );
    }

    // Vec capacity management
    group.bench_function("vec_with_capacity", |b| {
        b.iter(|| {
            let mut vec: Vec<serde_json::Value> = Vec::with_capacity(black_box(1000));
            for i in 0..1000 {
                vec.push(serde_json::json!({"idx": i}));
            }
            vec
        });
    });

    group.bench_function("vec_without_capacity", |b| {
        b.iter(|| {
            let mut vec: Vec<serde_json::Value> = Vec::new();
            for i in 0..black_box(1000) {
                vec.push(serde_json::json!({"idx": i}));
            }
            vec
        });
    });

    group.finish();
}

// =============================================================================
// Criterion Configuration
// =============================================================================

criterion_group!(
    name = full_suite;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(1));
    targets =
        full_core_benchmarks,
        full_storage_benchmarks,
        full_tools_benchmarks,
        full_workflow_benchmarks,
        full_memory_benchmarks
);

criterion_main!(full_suite);
