//! Benchmarking — run a prompt against multiple models and compare.

use std::time::Instant;

use crate::config::AppConfig;
use super::provider_factory::create_provider;

use ember_llm::{CompletionRequest, Message};

/// Benchmark result for a single model run.
pub struct BenchResult {
    pub model: String,
    pub provider: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u128,
    pub error: Option<String>,
}

/// Run a benchmark: send the same prompt to multiple models and return results.
pub async fn bench_models(
    config: &AppConfig,
    task: &str,
    model_names: &[String],
) -> Vec<BenchResult> {
    use ember_llm::router::{is_model_alias, resolve_model_alias};

    let mut results = Vec::new();
    for name in model_names {
        let (prov, model) = if is_model_alias(name) {
            let candidates = resolve_model_alias(name);
            match candidates.into_iter().next() {
                Some(c) => (c.provider.to_string(), c.model.to_string()),
                None => {
                    results.push(BenchResult {
                        model: name.clone(),
                        provider: "?".into(),
                        tokens_in: 0,
                        tokens_out: 0,
                        latency_ms: 0,
                        error: Some("No candidate for alias".into()),
                    });
                    continue;
                }
            }
        } else {
            (config.provider.default.clone(), name.clone())
        };

        let provider = match create_provider(config, &prov) {
            Ok(p) => p,
            Err(e) => {
                results.push(BenchResult {
                    model: model.clone(),
                    provider: prov,
                    tokens_in: 0,
                    tokens_out: 0,
                    latency_ms: 0,
                    error: Some(format!("{}", e)),
                });
                continue;
            }
        };

        let request = CompletionRequest::new(&model)
            .with_temperature(0.0)
            .with_message(Message::user(task));

        let start = Instant::now();
        match provider.complete(request).await {
            Ok(resp) => {
                results.push(BenchResult {
                    model,
                    provider: prov,
                    tokens_in: resp.usage.prompt_tokens,
                    tokens_out: resp.usage.completion_tokens,
                    latency_ms: start.elapsed().as_millis(),
                    error: None,
                });
            }
            Err(e) => {
                results.push(BenchResult {
                    model,
                    provider: prov,
                    tokens_in: 0,
                    tokens_out: 0,
                    latency_ms: start.elapsed().as_millis(),
                    error: Some(format!("{}", e)),
                });
            }
        }
    }
    results
}
