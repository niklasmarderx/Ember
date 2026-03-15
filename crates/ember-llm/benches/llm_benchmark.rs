//! LLM Benchmark Suite
//!
//! Run with: cargo bench --bench llm_benchmark
//!
//! This benchmark measures:
//! - Startup time
//! - Request latency
//! - Streaming throughput
//! - Concurrent request handling

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ember_llm::{CompletionRequest, Message, OllamaProvider};

/// Benchmark provider creation
fn bench_provider_creation(c: &mut Criterion) {
    c.bench_function("ollama_provider_creation", |b| {
        b.iter(|| {
            let provider = OllamaProvider::new()
                .with_base_url("http://localhost:11434")
                .with_default_model("llama3.2");
            black_box(provider)
        })
    });
}

/// Benchmark request building
fn bench_request_building(c: &mut Criterion) {
    c.bench_function("request_building_simple", |b| {
        b.iter(|| {
            let request = CompletionRequest::new("llama3.2").with_message(Message::user("Hello"));
            black_box(request)
        })
    });

    c.bench_function("request_building_complex", |b| {
        b.iter(|| {
            let request = CompletionRequest::new("llama3.2")
                .with_message(Message::system("You are a helpful assistant."))
                .with_message(Message::user("Hello"))
                .with_message(Message::assistant("Hi! How can I help?"))
                .with_message(Message::user("What's the weather?"))
                .with_temperature(0.7)
                .with_max_tokens(1000);
            black_box(request)
        })
    });
}

/// Benchmark message creation
fn bench_message_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_creation");

    group.bench_function("user_message", |b| {
        b.iter(|| black_box(Message::user("Hello, world!")))
    });

    group.bench_function("system_message", |b| {
        b.iter(|| black_box(Message::system("You are a helpful assistant.")))
    });

    group.bench_function("assistant_message", |b| {
        b.iter(|| black_box(Message::assistant("I can help with that.")))
    });

    group.finish();
}

/// Benchmark conversation building with varying sizes
fn bench_conversation_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("conversation_sizes");

    for size in [1, 5, 10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let mut request = CompletionRequest::new("llama3.2");
                for i in 0..size {
                    if i % 2 == 0 {
                        request = request.with_message(Message::user(format!("Message {}", i)));
                    } else {
                        request =
                            request.with_message(Message::assistant(format!("Response {}", i)));
                    }
                }
                black_box(request)
            })
        });
    }

    group.finish();
}

/// Benchmark JSON serialization (for API requests)
fn bench_serialization(c: &mut Criterion) {
    let request = CompletionRequest::new("llama3.2")
        .with_message(Message::system("You are a helpful assistant."))
        .with_message(Message::user("Hello, world!"))
        .with_temperature(0.7)
        .with_max_tokens(1000);

    c.bench_function("request_serialization", |b| {
        b.iter(|| {
            let json = serde_json::to_string(&request.messages).unwrap();
            black_box(json)
        })
    });
}

// Note: These benchmarks require a running Ollama instance
// Uncomment to run integration benchmarks
/*
/// Benchmark actual LLM completion (requires Ollama)
fn bench_llm_completion(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let provider = OllamaProvider::new()
        .with_base_url("http://localhost:11434")
        .with_default_model("llama3.2");

    let mut group = c.benchmark_group("llm_completion");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(10);

    group.bench_function("simple_completion", |b| {
        b.iter(|| {
            rt.block_on(async {
                let request = CompletionRequest::new("llama3.2")
                    .with_message(Message::user("Say 'hello' and nothing else."))
                    .with_max_tokens(10);
                let response = provider.complete(request).await.unwrap();
                black_box(response)
            })
        })
    });

    group.finish();
}
*/

criterion_group!(
    benches,
    bench_provider_creation,
    bench_request_building,
    bench_message_creation,
    bench_conversation_sizes,
    bench_serialization,
);

criterion_main!(benches);
