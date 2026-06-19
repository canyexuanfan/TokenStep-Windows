//! Spend estimation — a port of the Python `estimate_cost` /
//! `match_pricing_model` plus the Swift `estimateCost` hardcoded fallbacks.

use crate::models::TokenUsageCounts;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;

/// One entry in the pricing table. A flat per-1M total rate, or per-part rates.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RateEntry {
    #[serde(default)]
    pub total_usd_per_1m: Option<f64>,
    #[serde(default)]
    pub input_usd_per_1m: Option<f64>,
    #[serde(default)]
    pub output_usd_per_1m: Option<f64>,
    #[serde(default)]
    pub cache_creation_usd_per_1m: Option<f64>,
    #[serde(default)]
    pub cache_read_usd_per_1m: Option<f64>,
    #[serde(default)]
    pub reasoning_usd_per_1m: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PricingFile {
    #[serde(default)]
    pub default_total_usd_per_1m: Option<f64>,
    #[serde(default)]
    pub tools: BTreeMap<String, RateEntry>,
    #[serde(default)]
    pub models: BTreeMap<String, RateEntry>,
}

/// Resolve the pricing file shipped alongside the original repo at
/// `config/pricing.json`, copied into the bundle.
pub fn load() -> PricingFile {
    let candidates = [
        // Bundled resource beside the exe (Tauri copies resources/ next to it).
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("resources").join("pricing.json"))),
        // Fallback: config/pricing.json or pricing.json beside the exe.
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("config").join("pricing.json"))),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("pricing.json"))),
        // Dev-time: repo config.
        Some(std::path::PathBuf::from("config/pricing.json")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if let Ok(text) = fs::read_to_string(&candidate) {
            if let Ok(parsed) = serde_json::from_str::<PricingFile>(&text) {
                return parsed;
            }
        }
    }
    PricingFile::default()
}

/// Match a model name to a pricing entry, mirroring the Python logic:
/// exact key first, then prefix/substring match (case-insensitive).
fn match_model<'a>(pricing: &'a PricingFile, model: &str) -> Option<&'a RateEntry> {
    if let Some(entry) = pricing.models.get(model) {
        return Some(entry);
    }
    let lower = model.to_lowercase();
    for (key, value) in &pricing.models {
        let kl = key.to_lowercase();
        if lower.starts_with(&kl) || lower.contains(&kl) {
            return Some(value);
        }
    }
    None
}

/// Estimate USD cost for a single usage record.
pub fn estimate_cost(
    usage: &TokenUsageCounts,
    tool: &str,
    model: &str,
    pricing: &PricingFile,
) -> f64 {
    // 1. Pricing-file match (model, then tool, then default flat rate).
    if let Some(rate) = match_model(pricing, model) {
        if let Some(total_rate) = rate.total_usd_per_1m {
            return usage.total_tokens as f64 / 1_000_000.0 * total_rate;
        }
        return cost_by_parts_from_rate(usage, rate);
    }
    if let Some(rate) = pricing.tools.get(tool) {
        if let Some(total_rate) = rate.total_usd_per_1m {
            return usage.total_tokens as f64 / 1_000_000.0 * total_rate;
        }
        return cost_by_parts_from_rate(usage, rate);
    }
    let default_rate = pricing.default_total_usd_per_1m.unwrap_or(0.0);
    if default_rate != 0.0 {
        return usage.total_tokens as f64 / 1_000_000.0 * default_rate;
    }

    // 2. Swift-style hardcoded fallbacks when no pricing file matches.
    let lower = model.to_lowercase();
    // Codex GPT pricing from OpenAI API rates (port of macOS commit 2d5b4da).
    if tool == "Codex" && lower.contains("gpt-5.5") {
        return openai_cost_by_parts(usage, 5.0, 0.5, 30.0);
    }
    if tool == "Codex" && lower.contains("gpt-5.4") {
        return openai_cost_by_parts(usage, 2.5, 0.25, 15.0);
    }
    if lower.contains("opus") {
        return cost_by_parts(usage, 15.0, 75.0, 18.75, 1.5);
    }
    if lower.contains("sonnet") {
        return cost_by_parts(usage, 3.0, 15.0, 3.75, 0.3);
    }
    if tool == "Claude Code" {
        return usage.total_tokens as f64 / 1_000_000.0 * 3.0;
    }
    usage.total_tokens as f64 / 1_000_000.0
}

/// OpenAI-style per-part cost: uncached input at `input` rate, cached input at
/// `cached_input` rate, output+reasoning at `output` rate (per 1M tokens).
/// Port of macOS `openAICostByParts`.
fn openai_cost_by_parts(usage: &TokenUsageCounts, input: f64, cached_input: f64, output: f64) -> f64 {
    let cached = usage.cache_read_input_tokens.max(0) as f64;
    let uncached_input = (usage.input_tokens as f64 - cached).max(0.0);
    (uncached_input + usage.cache_creation_input_tokens as f64) / 1_000_000.0 * input
        + cached / 1_000_000.0 * cached_input
        + (usage.output_tokens + usage.reasoning_output_tokens) as f64 / 1_000_000.0 * output
}

fn cost_by_parts_from_rate(usage: &TokenUsageCounts, rate: &RateEntry) -> f64 {
    let input = rate.input_usd_per_1m.unwrap_or(0.0);
    let output = rate.output_usd_per_1m.unwrap_or(0.0);
    let cache_creation = rate
        .cache_creation_usd_per_1m
        .or(rate.input_usd_per_1m)
        .unwrap_or(0.0);
    let cache_read = rate.cache_read_usd_per_1m.unwrap_or(0.0);
    let reasoning = rate
        .reasoning_usd_per_1m
        .or(rate.output_usd_per_1m)
        .unwrap_or(0.0);
    cost_by_parts_internal(usage, input, output, cache_creation, cache_read, reasoning)
}

fn cost_by_parts(
    usage: &TokenUsageCounts,
    input: f64,
    output: f64,
    cache_creation: f64,
    cache_read: f64,
) -> f64 {
    // Reasoning billed at the output rate (matches Swift costByParts).
    cost_by_parts_internal(usage, input, output, cache_creation, cache_read, output)
}

#[allow(clippy::too_many_arguments)]
fn cost_by_parts_internal(
    usage: &TokenUsageCounts,
    input: f64,
    output: f64,
    cache_creation: f64,
    cache_read: f64,
    reasoning: f64,
) -> f64 {
    usage.input_tokens as f64 / 1_000_000.0 * input
        + usage.output_tokens as f64 / 1_000_000.0 * output
        + usage.cache_creation_input_tokens as f64 / 1_000_000.0 * cache_creation
        + usage.cache_read_input_tokens as f64 / 1_000_000.0 * cache_read
        + usage.reasoning_output_tokens as f64 / 1_000_000.0 * reasoning
}
