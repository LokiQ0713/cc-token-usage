use std::fmt::Write as _;

use crate::analysis::validate::ValidationReport;
use crate::analysis::{OverviewResult, ProjectResult, SessionResult, TrendResult};
use crate::pricing::calculator::PricingCalculator;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

fn format_cost(c: f64) -> String {
    let abs = c.abs();
    let total_cents = (abs * 100.0).round() as u64;
    let whole = total_cents / 100;
    let cents = total_cents % 100;
    let sign = if c < 0.0 { "-" } else { "" };
    format!("{}${}.{:02}", sign, format_number(whole), cents)
}

fn format_duration(minutes: f64) -> String {
    if minutes < 1.0 {
        format!("{:.0}s", minutes * 60.0)
    } else if minutes < 60.0 {
        format!("{:.0}m", minutes)
    } else {
        let h = (minutes / 60.0).floor();
        let m = (minutes % 60.0).round();
        format!("{:.0}h{:.0}m", h, m)
    }
}

// ─── 1. Overview ────────────────────────────────────────────────────────────

pub fn render_overview(result: &OverviewResult, calc: &PricingCalculator) -> String {
    let mut out = String::new();
    let _ = calc;

    let range = result.quality.time_range
        .map(|(s, e)| format!("{} ~ {}", s.format("%Y-%m-%d"), e.format("%Y-%m-%d")))
        .unwrap_or_default();

    writeln!(out, "Claude Code Token Report").unwrap();
    writeln!(out, "{}", range).unwrap();
    writeln!(out).unwrap();

    writeln!(out, "  {} conversations, {} rounds of back-and-forth",
        format_number(result.total_sessions as u64),
        format_number(result.total_turns as u64)).unwrap();
    writeln!(out).unwrap();

    writeln!(out, "  Claude read  {} tokens",
        format_number(result.total_context_tokens)).unwrap();
    writeln!(out, "  Claude wrote {} tokens",
        format_number(result.total_output_tokens)).unwrap();
    writeln!(out).unwrap();

    writeln!(out, "  Cache saved you {} ({:.0}% of reads were free)",
        format_cost(result.cache_savings.total_saved),
        result.cache_savings.savings_pct).unwrap();
    writeln!(out, "  All that would cost {} at API rates",
        format_cost(result.total_cost)).unwrap();

    // Model breakdown
    writeln!(out).unwrap();
    writeln!(out, "  Model                      Wrote        Rounds     Cost").unwrap();
    writeln!(out, "  ---------------------------------------------------------").unwrap();

    let mut models: Vec<(&String, &crate::analysis::AggregatedTokens)> = result.tokens_by_model.iter().collect();
    models.sort_by(|a, b| {
        let ca = result.cost_by_model.get(a.0).unwrap_or(&0.0);
        let cb = result.cost_by_model.get(b.0).unwrap_or(&0.0);
        cb.partial_cmp(ca).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (model, tokens) in &models {
        let cost = result.cost_by_model.get(*model).unwrap_or(&0.0);
        let short = short_model(model);
        writeln!(out, "  {:<25} {:>10} {:>9} {:>9}",
            short,
            format_number(tokens.output_tokens),
            format_number(tokens.turns as u64),
            format_cost(*cost)).unwrap();
    }

    // Top 5 projects
    if !result.session_summaries.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "  Top Projects                              Sessions   Turns    Cost").unwrap();
        writeln!(out, "  -------------------------------------------------------------------").unwrap();

        // Group by project
        let mut project_map: std::collections::HashMap<&str, (usize, usize, f64)> = std::collections::HashMap::new();
        for s in &result.session_summaries {
            let e = project_map.entry(&s.project_display_name).or_default();
            e.0 += 1;
            e.1 += s.turn_count;
            e.2 += s.cost;
        }
        let mut projects: Vec<_> = project_map.into_iter().collect();
        projects.sort_by(|a, b| b.1.2.partial_cmp(&a.1.2).unwrap_or(std::cmp::Ordering::Equal));

        for (name, (sessions, turns, cost)) in projects.iter().take(5) {
            writeln!(out, "  {:<40} {:>5} {:>7} {:>9}",
                name, sessions, turns, format_cost(*cost)).unwrap();
        }
    }

    // Monthly trend
    writeln!(out).unwrap();

    out
}

fn short_model(name: &str) -> String {
    let s = name.strip_prefix("claude-").unwrap_or(name);
    if s.len() > 9 {
        let last_dash = s.rfind('-').unwrap_or(s.len());
        let suffix = &s[last_dash + 1..];
        if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit()) {
            return s[..last_dash].to_string();
        }
    }
    s.to_string()
}

// ─── 2. Projects ────────────────────────────────────────────────────────────

pub fn render_projects(result: &ProjectResult) -> String {
    let mut out = String::new();
    let mut total_cost = 0.0f64;

    writeln!(out, "Projects by Cost").unwrap();
    writeln!(out).unwrap();

    for (i, proj) in result.projects.iter().enumerate() {
        writeln!(out, "  {:>2}. {:<35} {:>5} sess  {:>6} turns  {}",
            i + 1, proj.display_name,
            proj.session_count, proj.total_turns,
            format_cost(proj.cost)).unwrap();
        total_cost += proj.cost;
    }

    writeln!(out).unwrap();
    writeln!(out, "  Total: {} projects, {}", result.projects.len(), format_cost(total_cost)).unwrap();
    out
}

// ─── 3. Session ─────────────────────────────────────────────────────────────

pub fn render_session(result: &SessionResult) -> String {
    let mut out = String::new();

    let main_turns = result.turn_details.iter().filter(|t| !t.is_agent).count();

    writeln!(out, "Session {}  {}", &result.session_id[..result.session_id.len().min(8)], result.project).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "  Turns:     {:>6} (+ {} agent)   Duration: {}",
        main_turns, result.agent_summary.total_agent_turns, format_duration(result.duration_minutes)).unwrap();
    writeln!(out, "  Model:     {:<20}  MaxCtx:   {}",
        result.model, format_number(result.max_context)).unwrap();
    writeln!(out, "  CacheHit:  {:>5.1}%                Compacts: {}",
        result.total_tokens.cache_read_tokens as f64 / result.total_tokens.context_tokens().max(1) as f64 * 100.0,
        result.compaction_count).unwrap();
    writeln!(out, "  Cost:      {}", format_cost(result.total_cost)).unwrap();

    out
}

// ─── 4. Trend ───────────────────────────────────────────────────────────────

pub fn render_trend(result: &TrendResult) -> String {
    let mut out = String::new();
    let mut total_cost = 0.0f64;

    writeln!(out, "Usage by {}", result.group_label).unwrap();
    writeln!(out).unwrap();

    for entry in &result.entries {
        writeln!(out, "  {:<10}  {:>4} sess  {:>6} turns  {:>10} output  {}",
            entry.label, entry.session_count, entry.turn_count,
            format_number(entry.tokens.output_tokens),
            format_cost(entry.cost)).unwrap();
        total_cost += entry.cost;
    }

    writeln!(out).unwrap();
    writeln!(out, "  Total: {}", format_cost(total_cost)).unwrap();
    out
}

pub fn render_validation(report: &ValidationReport, failures_only: bool) -> String {
    let mut out = String::new();

    writeln!(out, "Token Validation Report").unwrap();
    writeln!(out, "{}", "━".repeat(60)).unwrap();
    writeln!(out).unwrap();

    // Structure checks
    writeln!(out, "Structure Checks:").unwrap();
    for check in &report.structure_checks {
        if failures_only && check.passed { continue; }
        let status = if check.passed { "OK" } else { "FAIL" };
        if check.passed {
            writeln!(out, "  [{:>4}] {}: {}", status, check.name, check.actual).unwrap();
        } else {
            writeln!(out, "  [{:>4}] {}: expected={}, actual={}", status, check.name, check.expected, check.actual).unwrap();
        }
    }
    writeln!(out).unwrap();

    // Per-session results
    let mut fail_sessions = Vec::new();
    for sv in &report.session_results {
        let all_checks: Vec<_> = sv.token_checks.iter().chain(sv.agent_checks.iter()).collect();
        let has_failures = all_checks.iter().any(|c| !c.passed);

        if failures_only && !has_failures { continue; }

        if has_failures {
            fail_sessions.push(sv);
        }
    }

    if !failures_only {
        writeln!(out, "Session Validation: {} sessions checked", report.session_results.len()).unwrap();
        let sessions_ok = report.summary.sessions_passed;
        let sessions_fail = report.summary.sessions_validated - sessions_ok;
        writeln!(out, "  {} PASS, {} FAIL", sessions_ok, sessions_fail).unwrap();
        writeln!(out).unwrap();
    }

    // Show failed sessions in detail
    if !fail_sessions.is_empty() {
        writeln!(out, "Failed Sessions:").unwrap();
        writeln!(out).unwrap();
    }
    for sv in &fail_sessions {
        writeln!(out, "  Session {}  {}", &sv.session_id[..8.min(sv.session_id.len())], sv.project).unwrap();
        for check in sv.token_checks.iter().chain(sv.agent_checks.iter()) {
            if !check.passed {
                writeln!(out, "    [FAIL] {}: expected={}, actual={}", check.name, check.expected, check.actual).unwrap();
            }
        }
        writeln!(out).unwrap();
    }

    // Summary
    writeln!(out, "{}", "━".repeat(60)).unwrap();
    let result_text = if report.summary.failed == 0 { "PASS" } else { "FAIL" };
    writeln!(out, "Result: {} ({}/{} checks passed, {} sessions validated)",
        result_text,
        report.summary.passed,
        report.summary.total_checks,
        report.summary.sessions_validated,
    ).unwrap();

    out
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(1_234_567), "1,234,567");
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0), "$0.00");
        assert_eq!(format_cost(1.5), "$1.50");
        assert_eq!(format_cost(1234.56), "$1,234.56");
    }
}
