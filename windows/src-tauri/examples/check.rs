//! Standalone check: run the Rust collector and print a summary that mirrors
//! the Python `print-summary` output, for numeric parity comparison.
//!
//! Run with: cargo run --example check

use tokenstep_lib as app;

fn human_tokens(tokens: i64) -> String {
    let v = tokens as f64;
    if v >= 100_000_000.0 {
        format!("{:.2}亿", v / 100_000_000.0)
    } else if v >= 10_000.0 {
        format!("{:.1}万", v / 10_000.0)
    } else {
        format!("{:.0}", v)
    }
}

fn main() {
    // The collector is currently a private module; expose via a shim function.
    let snapshot = app::collect_for_check();

    println!("generated_at: {}", snapshot.generated_at.unwrap_or_default());
    println!("total_tokens: {}", human_tokens(snapshot.totals.tokens));
    println!("estimated_cost: ${:.2}", snapshot.totals.cost);
    println!("active_days: {}", snapshot.totals.active_days);
    println!("daily_rows: {}", snapshot.daily.len());
    println!("model_rows: {}", snapshot.models.len());
    println!("tools:");
    for row in &snapshot.tools {
        let pct = row.percent.unwrap_or(0.0);
        println!("  - {}: {} ({:.1}%)", row.tool, human_tokens(row.tokens), pct);
    }
    println!("models (top 8):");
    for row in snapshot.models.iter().take(8) {
        let pct = row.percent.unwrap_or(0.0);
        println!("  - {} / {}: {} ({:.1}%)", row.tool.as_deref().unwrap_or(""), row.model, human_tokens(row.tokens), pct);
    }
    // Diagnostics: today's row with per-tool split + projects.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if let Some(row) = snapshot.daily.iter().find(|d| d.date == today) {
        println!("TODAY {}: {}", row.date, human_tokens(row.total_tokens));
        let mut tools: Vec<_> = row.tools.iter().collect();
        tools.sort_by(|a, b| b.1.cmp(a.1));
        for (t, v) in tools {
            println!("  - {}: {}", t, human_tokens(*v));
        }
    }
    println!("top days:");
    let mut days: Vec<_> = snapshot.daily.iter().collect();
    days.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));
    for d in days.iter().rev().take(3) {
        println!("  - {}: {}", d.date, human_tokens(d.total_tokens));
    }
    println!("sources:");
    for (name, info) in &snapshot.sources {
        println!("  - {}: status={:?} files={:?} records={:?}", name, info.status, info.files, info.records);
    }
}
