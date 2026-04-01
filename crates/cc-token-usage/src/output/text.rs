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
        .map(|(s, e)| {
            let ls = s.with_timezone(&chrono::Local);
            let le = e.with_timezone(&chrono::Local);
            format!("{} ~ {}", ls.format("%Y-%m-%d"), le.format("%Y-%m-%d"))
        })
        .unwrap_or_default();

    writeln!(out, "Claude Code Token Report").unwrap();
    writeln!(out, "{}", range).unwrap();
    writeln!(out).unwrap();

    writeln!(out, "  {} conversations, {} rounds of back-and-forth",
        format_number(result.total_sessions as u64),
        format_number(result.total_turns as u64)).unwrap();
    if result.total_agent_turns > 0 {
        writeln!(out, "  ({} agent turns, {:.0}% of total)",
            format_number(result.total_agent_turns as u64),
            result.total_agent_turns as f64 / result.total_turns.max(1) as f64 * 100.0).unwrap();
    }
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

    // Subscription value
    if let Some(ref sub) = result.subscription_value {
        writeln!(out, "  Subscription: {}/mo -> {:.1}x value multiplier",
            format_cost(sub.monthly_price), sub.value_multiplier).unwrap();
    }

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

    // Cost by category
    writeln!(out).unwrap();
    let cat = &result.cost_by_category;
    let total = result.total_cost.max(0.001);
    writeln!(out, "  Cost Breakdown").unwrap();
    writeln!(out, "    Output:      {:>9}  ({:.0}%)", format_cost(cat.output_cost), cat.output_cost / total * 100.0).unwrap();
    writeln!(out, "    Cache Write: {:>9}  ({:.0}%)", format_cost(cat.cache_write_5m_cost + cat.cache_write_1h_cost),
        (cat.cache_write_5m_cost + cat.cache_write_1h_cost) / total * 100.0).unwrap();
    writeln!(out, "    Input:       {:>9}  ({:.0}%)", format_cost(cat.input_cost), cat.input_cost / total * 100.0).unwrap();
    writeln!(out, "    Cache Read:  {:>9}  ({:.0}%)", format_cost(cat.cache_read_cost), cat.cache_read_cost / total * 100.0).unwrap();

    // Tool usage top 10
    if !result.tool_counts.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "  Top Tools").unwrap();
        for (name, count) in result.tool_counts.iter().take(10) {
            let bar_len = (*count as f64 / result.tool_counts[0].1.max(1) as f64 * 20.0).round() as usize;
            writeln!(out, "    {:<18} {:>6}  {}", name, format_number(*count as u64), "█".repeat(bar_len)).unwrap();
        }
    }

    // Top 5 projects
    if !result.session_summaries.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "  Top Projects                              Sessions   Turns    Cost").unwrap();
        writeln!(out, "  -------------------------------------------------------------------").unwrap();

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

    // Usage insights
    if !result.session_summaries.is_empty() {
        let summaries = &result.session_summaries;

        // Daily average cost
        if let Some((start, end)) = result.quality.time_range {
            let days = (end - start).num_days().max(1) as f64;
            writeln!(out).unwrap();
            writeln!(out, "  Daily avg: {} / day  ({} days)",
                format_cost(result.total_cost / days), days as u64).unwrap();
        }

        // Compaction stats
        let total_compactions: usize = summaries.iter().map(|s| s.compaction_count).sum();
        let sessions_with_compaction = summaries.iter().filter(|s| s.compaction_count > 0).count();
        if total_compactions > 0 {
            writeln!(out, "  Compactions: {} total across {} sessions",
                total_compactions, sessions_with_compaction).unwrap();
        }

        // Max context
        let max_ctx = summaries.iter().map(|s| s.max_context).max().unwrap_or(0);
        if max_ctx > 0 {
            writeln!(out, "  Peak context: {} tokens", format_number(max_ctx)).unwrap();
        }

        // Average session duration
        let durations: Vec<f64> = summaries.iter()
            .map(|s| s.duration_minutes)
            .filter(|d| *d > 0.0)
            .collect();
        if !durations.is_empty() {
            let avg_dur = durations.iter().sum::<f64>() / durations.len() as f64;
            writeln!(out, "  Avg session: {}", format_duration(avg_dur)).unwrap();
        }

        // Top 3 most expensive sessions
        let mut by_cost: Vec<&crate::analysis::SessionSummary> = summaries.iter().collect();
        by_cost.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
        writeln!(out).unwrap();
        writeln!(out, "  Most Expensive Sessions").unwrap();
        for s in by_cost.iter().take(3) {
            let dur = format_duration(s.duration_minutes);
            writeln!(out, "    {} {} {:>5} turns  {}  {}",
                &s.session_id[..s.session_id.len().min(8)],
                truncate_str(&s.project_display_name, 25),
                s.turn_count,
                dur,
                format_cost(s.cost),
            ).unwrap();
        }
    }

    // Data quality summary
    writeln!(out).unwrap();
    writeln!(out, "  Data: {} session files, {} agent files",
        result.quality.total_session_files, result.quality.total_agent_files).unwrap();
    if result.quality.orphan_agents > 0 {
        writeln!(out, "  ({} orphan agents without parent session)", result.quality.orphan_agents).unwrap();
    }

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
    writeln!(out, "  #   Project                          Sessions  Turns  Agent  $/Sess  Model          Cost").unwrap();
    writeln!(out, "  ─────────────────────────────────────────────────────────────────────────────────────────").unwrap();

    for (i, proj) in result.projects.iter().enumerate() {
        let avg_cost = if proj.session_count > 0 { proj.cost / proj.session_count as f64 } else { 0.0 };
        let model_short = short_model(&proj.primary_model);
        writeln!(out, "  {:>2}. {:<30} {:>5}  {:>6}  {:>5}  {:>6}  {:<12}  {:>9}",
            i + 1,
            truncate_str(&proj.display_name, 30),
            proj.session_count,
            proj.total_turns,
            proj.agent_turns,
            format_cost(avg_cost),
            truncate_str(&model_short, 12),
            format_cost(proj.cost),
        ).unwrap();
        total_cost += proj.cost;
    }

    writeln!(out).unwrap();
    writeln!(out, "  Total: {} projects, {}", result.projects.len(), format_cost(total_cost)).unwrap();
    out
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() }
    else { format!("{}...", &s[..s.floor_char_boundary(max.saturating_sub(3))]) }
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

    // ── Metadata section ──
    let has_metadata = result.title.is_some()
        || !result.tags.is_empty()
        || result.mode.is_some()
        || !result.git_branches.is_empty()
        || !result.pr_links.is_empty();

    if has_metadata {
        writeln!(out).unwrap();
        writeln!(out, "  ── Metadata ──────────────────────────────────").unwrap();
        if let Some(ref title) = result.title {
            writeln!(out, "  Title:        {}", truncate_str(title, 60)).unwrap();
        }
        if !result.tags.is_empty() {
            writeln!(out, "  Tags:         {}", result.tags.join(", ")).unwrap();
        }
        if let Some(ref mode) = result.mode {
            writeln!(out, "  Mode:         {}", mode).unwrap();
        }
        if !result.git_branches.is_empty() {
            let mut branches: Vec<_> = result.git_branches.iter().collect();
            branches.sort_by(|a, b| b.1.cmp(a.1));
            let parts: Vec<String> = branches.iter()
                .map(|(name, count)| format!("{} ({} turns)", name, count))
                .collect();
            writeln!(out, "  Branch:       {}", parts.join(", ")).unwrap();
        }
        for pr in &result.pr_links {
            writeln!(out, "  PR:           {}#{}", pr.repository, pr.number).unwrap();
        }
    }

    // ── Performance section ──
    let has_performance = result.user_prompt_count > 0
        || result.truncated_count > 0
        || result.speculation_accepts > 0
        || !result.service_tiers.is_empty()
        || !result.speeds.is_empty()
        || !result.inference_geos.is_empty()
        || result.api_error_count > 0
        || result.tool_error_count > 0;

    if has_performance {
        writeln!(out).unwrap();
        writeln!(out, "  ── Performance ───────────────────────────────").unwrap();
        if result.user_prompt_count > 0 {
            let total_turns = result.turn_details.len();
            writeln!(out, "  Autonomy:     1:{:.1} ({} turns / {} user prompts)",
                result.autonomy_ratio, total_turns, result.user_prompt_count).unwrap();
        }
        if result.truncated_count > 0 {
            writeln!(out, "  Truncated:    {} turns hit max_tokens", result.truncated_count).unwrap();
        }
        if result.api_error_count > 0 || result.tool_error_count > 0 {
            let mut parts = Vec::new();
            if result.api_error_count > 0 {
                parts.push(format!("{} API errors", result.api_error_count));
            }
            if result.tool_error_count > 0 {
                parts.push(format!("{} tool errors", result.tool_error_count));
            }
            writeln!(out, "  Errors:       {}", parts.join(", ")).unwrap();
        }
        if result.speculation_accepts > 0 {
            let saved_secs = result.speculation_time_saved_ms / 1000.0;
            writeln!(out, "  Speculation:  saved {:.1}s across {} accepts",
                saved_secs, result.speculation_accepts).unwrap();
        }
        if !result.service_tiers.is_empty() {
            let total: usize = result.service_tiers.values().sum();
            let mut tiers: Vec<_> = result.service_tiers.iter().collect();
            tiers.sort_by(|a, b| b.1.cmp(a.1));
            let parts: Vec<String> = tiers.iter()
                .map(|(name, count)| format!("{} ({:.0}%)", name, **count as f64 / total as f64 * 100.0))
                .collect();
            writeln!(out, "  Service:      {}", parts.join(", ")).unwrap();
        }
        if !result.speeds.is_empty() {
            let total: usize = result.speeds.values().sum();
            let mut spds: Vec<_> = result.speeds.iter().collect();
            spds.sort_by(|a, b| b.1.cmp(a.1));
            let parts: Vec<String> = spds.iter()
                .map(|(name, count)| format!("{} ({:.0}%)", name, **count as f64 / total as f64 * 100.0))
                .collect();
            writeln!(out, "  Speed:        {}", parts.join(", ")).unwrap();
        }
        if !result.inference_geos.is_empty() {
            let total: usize = result.inference_geos.values().sum();
            let mut geos: Vec<_> = result.inference_geos.iter().collect();
            geos.sort_by(|a, b| b.1.cmp(a.1));
            let parts: Vec<String> = geos.iter()
                .map(|(name, count)| format!("{} ({:.0}%)", name, **count as f64 / total as f64 * 100.0))
                .collect();
            writeln!(out, "  Geo:          {}", parts.join(", ")).unwrap();
        }
    }

    // Per-agent breakdown
    if !result.agent_summary.agents.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "  Agent Breakdown").unwrap();
        writeln!(out, "  {:<14} {:<40} {:>6} {:>10} {:>9}",
            "Type", "Description", "Turns", "Output", "Cost").unwrap();
        writeln!(out, "  {}", "-".repeat(83)).unwrap();

        // Main agent line
        let main_turns = result.turn_details.iter().filter(|t| !t.is_agent).count();
        let main_output: u64 = result.turn_details.iter()
            .filter(|t| !t.is_agent).map(|t| t.output_tokens).sum();
        let main_cost = result.total_cost - result.agent_summary.agent_cost;
        writeln!(out, "  {:<14} {:<40} {:>6} {:>10} {:>9}",
            "main", "(this conversation)",
            main_turns, format_number(main_output), format_cost(main_cost)).unwrap();

        for agent in &result.agent_summary.agents {
            let desc = if agent.description.len() > 40 {
                format!("{}...", &agent.description[..agent.description.floor_char_boundary(37)])
            } else {
                agent.description.clone()
            };
            writeln!(out, "  {:<14} {:<40} {:>6} {:>10} {:>9}",
                agent.agent_type,
                desc,
                agent.turns,
                format_number(agent.output_tokens),
                format_cost(agent.cost),
            ).unwrap();
        }
    }

    out
}

// ─── 4. Trend ───────────────────────────────────────────────────────────────

pub fn render_trend(result: &TrendResult) -> String {
    let mut out = String::new();
    let mut total_cost = 0.0f64;
    let mut total_turns = 0usize;

    // Find max cost for sparkline scaling
    let max_cost = result.entries.iter().map(|e| e.cost).fold(0.0f64, f64::max);

    writeln!(out, "Usage by {}", result.group_label).unwrap();
    writeln!(out).unwrap();

    for entry in &result.entries {
        // Sparkline bar
        let bar_len = if max_cost > 0.0 { (entry.cost / max_cost * 16.0).round() as usize } else { 0 };
        let bar = "▇".repeat(bar_len);

        // Primary model for this period
        let top_model = entry.models.iter()
            .max_by_key(|(_, tokens)| *tokens)
            .map(|(m, _)| short_model(m))
            .unwrap_or_default();

        // Cost per turn
        let cpt = if entry.turn_count > 0 { entry.cost / entry.turn_count as f64 } else { 0.0 };

        writeln!(out, "  {:<10}  {:>4} sess  {:>6} turns  {:>9}  ${:.3}/t  {:<12} {}",
            entry.label, entry.session_count, entry.turn_count,
            format_cost(entry.cost), cpt,
            truncate_str(&top_model, 12),
            bar,
        ).unwrap();
        total_cost += entry.cost;
        total_turns += entry.turn_count;
    }

    writeln!(out).unwrap();
    let avg_cpt = if total_turns > 0 { total_cost / total_turns as f64 } else { 0.0 };
    writeln!(out, "  Total: {}  ({} turns, avg ${:.3}/turn)", format_cost(total_cost), format_number(total_turns as u64), avg_cpt).unwrap();
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
