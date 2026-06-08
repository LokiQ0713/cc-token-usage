use std::collections::HashMap;

use chrono::{Datelike, Local, Timelike};

use crate::data::models::{GlobalDataQuality, SessionData};
use crate::pricing::calculator::{PriceSource, PricingCalculator};

use super::{
    AggregatedTokens, CacheSavings, CostByCategory, OverviewResult, PricingWarning, SessionSummary,
    SubscriptionValue,
};

/// Accumulator for one unknown model encountered during overview aggregation.
#[derive(Default)]
struct FallbackAccum {
    fallback_to: String,
    turn_count: u64,
    fallback_cost: f64,
}

pub fn analyze_overview(
    sessions: &[SessionData],
    quality: GlobalDataQuality,
    calc: &PricingCalculator,
    subscription_price: Option<f64>,
) -> OverviewResult {
    let mut tokens_by_model: HashMap<String, AggregatedTokens> = HashMap::new();
    let mut cost_by_model: HashMap<String, f64> = HashMap::new();
    let mut total_cost = 0.0;
    let mut hourly_distribution = [0usize; 24];
    let mut weekday_hour_matrix = [[0usize; 24]; 7];
    let mut total_turns = 0usize;
    let mut total_agent_turns = 0usize;
    let mut cost_by_category = CostByCategory::default();
    let mut tool_count_map: HashMap<String, usize> = HashMap::new();
    // Collect fallback occurrences keyed by the *requested* (unknown) model.
    let mut fallback_map: HashMap<String, FallbackAccum> = HashMap::new();

    for session in sessions {
        for turn in session.all_responses() {
            process_turn(
                turn,
                calc,
                &mut tokens_by_model,
                &mut cost_by_model,
                &mut total_cost,
                &mut hourly_distribution,
                &mut weekday_hour_matrix,
                &mut cost_by_category,
                &mut fallback_map,
            );
            total_turns += 1;
            if turn.is_agent {
                total_agent_turns += 1;
            }

            // Aggregate tool usage
            for name in &turn.tool_names {
                *tool_count_map.entry(name.clone()).or_insert(0) += 1;
            }
        }
    }

    let mut tool_counts: Vec<(String, usize)> = tool_count_map.into_iter().collect();
    tool_counts.sort_by_key(|b| std::cmp::Reverse(b.1));

    // Compute totals from tokens_by_model
    let mut total_output_tokens: u64 = 0;
    let mut total_context_tokens: u64 = 0;
    for agg in tokens_by_model.values() {
        total_output_tokens += agg.output_tokens;
        total_context_tokens += agg.context_tokens();
    }

    // Average cache hit rate
    let total_cache_read: u64 = tokens_by_model.values().map(|a| a.cache_read_tokens).sum();
    let avg_cache_hit_rate = if total_context_tokens > 0 {
        (total_cache_read as f64 / total_context_tokens as f64) * 100.0
    } else {
        0.0
    };

    // Build session summaries
    let session_summaries: Vec<SessionSummary> = sessions
        .iter()
        .map(|s| build_session_summary(s, calc))
        .collect();

    // Note: turn_details intentionally left as None for overview.
    // Individual session details are only generated for the session subcommand.
    // This keeps the HTML report lightweight.

    // ── Cache savings calculation ───────────────────────────────────────────
    // Savings = what cache_read tokens would cost at base_input rate minus actual cache_read cost
    let cache_savings = {
        let mut total_saved = 0.0f64;
        let mut without_cache = 0.0f64;
        let mut with_cache = 0.0f64;
        let mut by_model: Vec<(String, f64)> = Vec::new();

        for (model, tokens) in &tokens_by_model {
            if let Some((price, _)) = calc.get_price(model) {
                let cache_read_mtok = tokens.cache_read_tokens as f64 / 1_000_000.0;
                let hypothetical = cache_read_mtok * price.base_input;
                let actual = cache_read_mtok * price.cache_read;
                let saved = hypothetical - actual;
                without_cache += hypothetical;
                with_cache += actual;
                total_saved += saved;
                if saved > 0.01 {
                    by_model.push((model.clone(), saved));
                }
            }
        }
        by_model.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let savings_pct = if without_cache > 0.0 {
            total_saved / without_cache * 100.0
        } else {
            0.0
        };

        CacheSavings {
            total_saved,
            without_cache_cost: without_cache,
            with_cache_cost: with_cache,
            savings_pct,
            by_model,
        }
    };

    let subscription_value = subscription_price.map(|monthly_price| {
        let value_multiplier = if total_cost > 0.0 {
            total_cost / monthly_price
        } else {
            0.0
        };
        SubscriptionValue {
            monthly_price,
            api_equivalent: total_cost,
            value_multiplier,
        }
    });

    // Efficiency metrics
    let output_ratio = if total_context_tokens > 0 {
        total_output_tokens as f64 / total_context_tokens as f64 * 100.0
    } else {
        0.0
    };
    let cost_per_turn = if total_turns > 0 {
        total_cost / total_turns as f64
    } else {
        0.0
    };
    let tokens_per_output_turn = if total_turns > 0 {
        total_output_tokens / total_turns as u64
    } else {
        0
    };

    // Build sorted, deterministic pricing warnings: cost desc, then model name asc.
    let mut pricing_warnings: Vec<PricingWarning> = fallback_map
        .into_iter()
        .map(|(unknown_model, acc)| PricingWarning {
            unknown_model,
            fallback_to: acc.fallback_to,
            turn_count: acc.turn_count,
            fallback_cost: acc.fallback_cost,
        })
        .collect();
    pricing_warnings.sort_by(|a, b| {
        b.fallback_cost
            .partial_cmp(&a.fallback_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.unknown_model.cmp(&b.unknown_model))
    });

    OverviewResult {
        total_sessions: sessions.len(),
        total_turns,
        total_agent_turns,
        tokens_by_model,
        cost_by_model,
        total_cost,
        hourly_distribution,
        quality,
        subscription_value,
        weekday_hour_matrix,
        tool_counts,
        cost_by_category,
        session_summaries,
        total_output_tokens,
        total_context_tokens,
        avg_cache_hit_rate,
        cache_savings,
        output_ratio,
        cost_per_turn,
        tokens_per_output_turn,
        pricing_warnings,
    }
}

#[allow(clippy::too_many_arguments)]
fn process_turn(
    turn: &crate::data::models::ValidatedTurn,
    calc: &PricingCalculator,
    tokens_by_model: &mut HashMap<String, AggregatedTokens>,
    cost_by_model: &mut HashMap<String, f64>,
    total_cost: &mut f64,
    hourly_distribution: &mut [usize; 24],
    weekday_hour_matrix: &mut [[usize; 24]; 7],
    cost_by_category: &mut CostByCategory,
    fallback_map: &mut HashMap<String, FallbackAccum>,
) {
    // Aggregate tokens by model
    tokens_by_model
        .entry(turn.model.clone())
        .or_default()
        .add_usage(&turn.usage);

    // Calculate cost
    let cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
    *cost_by_model.entry(turn.model.clone()).or_insert(0.0) += cost.total;
    *total_cost += cost.total;

    // Accumulate cost by category
    cost_by_category.input_cost += cost.input_cost;
    cost_by_category.output_cost += cost.output_cost;
    cost_by_category.cache_write_5m_cost += cost.cache_write_5m_cost;
    cost_by_category.cache_write_1h_cost += cost.cache_write_1h_cost;
    cost_by_category.cache_read_cost += cost.cache_read_cost;

    // Track unknown-model fallbacks so the user can be warned that those
    // dollars are estimates. Keyed by the model name as it actually appeared.
    if let PriceSource::Fallback {
        ref requested,
        ref fallback_to,
    } = cost.price_source
    {
        let entry = fallback_map.entry(requested.clone()).or_default();
        if entry.fallback_to.is_empty() {
            entry.fallback_to = fallback_to.clone();
        }
        entry.turn_count += 1;
        entry.fallback_cost += cost.total;
    }

    // Hourly distribution (local timezone)
    let local_ts = turn.timestamp.with_timezone(&Local);
    let hour = local_ts.hour() as usize;
    hourly_distribution[hour] += 1;

    // Weekday-hour matrix (local timezone)
    let weekday = local_ts.weekday().num_days_from_monday() as usize; // 0=Mon..6=Sun
    weekday_hour_matrix[weekday][hour] += 1;
}

/// Build a SessionSummary for a single session.
fn build_session_summary(session: &SessionData, calc: &PricingCalculator) -> SessionSummary {
    let session_id = if session.session_id.len() > 8 {
        session.session_id[..8].to_string()
    } else {
        session.session_id.clone()
    };

    let project_display_name = session
        .project
        .as_deref()
        .map(crate::analysis::project::project_display_name)
        .unwrap_or_else(|| "(unknown)".to_string());

    let all_turns = session.all_responses();
    let turn_count = all_turns.len();

    // Duration
    let duration_minutes = match (session.first_timestamp, session.last_timestamp) {
        (Some(first), Some(last)) => (last - first).num_seconds() as f64 / 60.0,
        _ => 0.0,
    };

    // Model frequency, output/context tokens, max_context, cache stats, compaction, cost
    let mut model_counts: HashMap<&str, usize> = HashMap::new();
    let mut output_tokens: u64 = 0;
    let mut context_tokens: u64 = 0;
    let mut max_context: u64 = 0;
    let mut total_cache_read: u64 = 0;
    let mut total_context: u64 = 0;
    let mut total_5m: u64 = 0;
    let mut total_1h: u64 = 0;
    let mut compaction_count: usize = 0;
    let mut agent_turn_count: usize = 0;
    let mut tool_use_count: usize = 0;
    let mut total_cost: f64 = 0.0;
    let mut prev_context_size: Option<u64> = None;
    let mut tool_map: HashMap<String, usize> = HashMap::new();

    for turn in &all_turns {
        *model_counts.entry(&turn.model).or_insert(0) += 1;

        let input = turn.usage.input_tokens.unwrap_or(0);
        let cache_create = turn.usage.cache_creation_input_tokens.unwrap_or(0);
        let cache_read = turn.usage.cache_read_input_tokens.unwrap_or(0);
        let out = turn.usage.output_tokens.unwrap_or(0);

        output_tokens += out;
        let ctx = input + cache_create + cache_read;
        context_tokens += ctx;
        total_context += ctx;
        total_cache_read += cache_read;

        if ctx > max_context {
            max_context = ctx;
        }

        // TTL breakdown
        if let Some(ref detail) = turn.usage.cache_creation {
            total_5m += detail.ephemeral_5m_input_tokens.unwrap_or(0);
            total_1h += detail.ephemeral_1h_input_tokens.unwrap_or(0);
        }

        // Compaction detection
        if let Some(prev) = prev_context_size {
            if prev > 0 && (ctx as f64) < (prev as f64 * 0.9) {
                compaction_count += 1;
            }
        }
        prev_context_size = Some(ctx);

        // Agent turns
        if turn.is_agent {
            agent_turn_count += 1;
        }

        // Tool use count
        if turn.stop_reason.as_deref() == Some("tool_use") {
            tool_use_count += 1;
        }
        for name in &turn.tool_names {
            *tool_map.entry(name.clone()).or_insert(0) += 1;
        }

        // Cost
        let cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
        total_cost += cost.total;
    }

    // Primary model
    let model = model_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(m, _)| m.to_string())
        .unwrap_or_default();

    // Cache hit rate
    let cache_hit_rate = if total_context > 0 {
        (total_cache_read as f64 / total_context as f64) * 100.0
    } else {
        0.0
    };

    // Cache write 5m percentage
    let total_cache_write = total_5m + total_1h;
    let cache_write_5m_pct = if total_cache_write > 0 {
        (total_5m as f64 / total_cache_write as f64) * 100.0
    } else {
        0.0
    };

    let output_ratio = if context_tokens > 0 {
        output_tokens as f64 / context_tokens as f64 * 100.0
    } else {
        0.0
    };
    let cost_per_turn = if turn_count > 0 {
        total_cost / turn_count as f64
    } else {
        0.0
    };

    SessionSummary {
        session_id,
        project_display_name,
        title: session.metadata.title.clone(),
        first_timestamp: session.first_timestamp,
        duration_minutes,
        model,
        turn_count,
        agent_turn_count,
        output_tokens,
        context_tokens,
        max_context,
        cache_hit_rate,
        cache_write_5m_pct,
        compaction_count,
        cost: total_cost,
        tool_use_count,
        top_tools: {
            let mut tools: Vec<(String, usize)> = tool_map.into_iter().collect();
            tools.sort_by_key(|b| std::cmp::Reverse(b.1));
            tools.truncate(5);
            tools
        },
        turn_details: None,
        output_ratio,
        cost_per_turn,
        is_orphan: session.is_orphan,
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{
        DataQuality, SessionData, SessionMetadata, TokenUsage, ValidatedTurn,
    };
    use chrono::{TimeZone, Utc};

    fn make_turn(model: &str, input: u64, output: u64) -> ValidatedTurn {
        ValidatedTurn {
            parent_uuid: None,
            uuid: format!("uuid-{}-{}", model, input),
            request_id: None,
            timestamp: Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap(),
            model: model.to_string(),
            usage: TokenUsage {
                input_tokens: Some(input),
                output_tokens: Some(output),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            },
            stop_reason: Some("end_turn".to_string()),
            content_types: vec!["text".to_string()],
            is_agent: false,
            agent_id: None,
            user_text: None,
            assistant_text: None,
            tool_names: vec![],
            service_tier: None,
            speed: None,
            inference_geo: None,
            tool_error_count: 0,
            git_branch: None,
            attribution_plugin: None,
            attribution_skill: None,
        }
    }

    fn make_session(turns: Vec<ValidatedTurn>) -> SessionData {
        SessionData {
            source_path: std::path::PathBuf::from("/tmp/test.jsonl"),
            session_id: "test-session".to_string(),
            project: Some("test-project".to_string()),
            turns,
            user_entries: vec![],
            subagents: vec![],
            plugins: vec![],
            skills: vec![],
            hooks: vec![],
            first_timestamp: Some(Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap()),
            last_timestamp: Some(Utc.with_ymd_and_hms(2026, 5, 1, 13, 0, 0).unwrap()),
            version: None,
            quality: DataQuality::default(),
            metadata: SessionMetadata::default(),
            is_orphan: false,
        }
    }

    /// Mix one known model (claude-opus-4-6) with two unknown models that
    /// each take the LATEST_FALLBACK_MODEL fallback. Aggregation must:
    ///   1. produce exactly one PricingWarning per distinct unknown model
    ///   2. sum turn_count and fallback_cost per unknown model
    ///   3. leave the known model out of the warnings list
    ///   4. sort by cost desc (tie-broken by model name asc)
    #[test]
    fn pricing_warnings_aggregated_across_session() {
        let calc = PricingCalculator::new();
        let session = make_session(vec![
            make_turn("claude-opus-4-6", 1_000_000, 1_000_000), // known
            make_turn("claude-future-x-1", 1_000_000, 1_000_000), // unknown -> fallback
            make_turn("claude-future-x-1", 500_000, 500_000),   // unknown, same model
            make_turn("claude-future-y-2", 2_000_000, 2_000_000), // unknown, distinct
        ]);

        let result = analyze_overview(&[session], GlobalDataQuality::default(), &calc, None);

        assert_eq!(
            result.pricing_warnings.len(),
            2,
            "expected one warning per distinct unknown model"
        );

        // future-y-2 has the higher cost (2M+2M vs 1.5M+1.5M) → first by sort.
        let first = &result.pricing_warnings[0];
        assert_eq!(first.unknown_model, "claude-future-y-2");
        assert_eq!(first.turn_count, 1);
        assert_eq!(
            first.fallback_to,
            crate::pricing::calculator::LATEST_FALLBACK_MODEL
        );
        // 2M input * $5 + 2M output * $25 = $60
        assert!(
            (first.fallback_cost - 60.0).abs() < 1e-9,
            "fallback_cost: {}",
            first.fallback_cost
        );

        let second = &result.pricing_warnings[1];
        assert_eq!(second.unknown_model, "claude-future-x-1");
        assert_eq!(second.turn_count, 2);
        assert_eq!(
            second.fallback_to,
            crate::pricing::calculator::LATEST_FALLBACK_MODEL
        );
        // (1M+0.5M) * $5 input + (1M+0.5M) * $25 output = $7.5 + $37.5 = $45
        assert!(
            (second.fallback_cost - 45.0).abs() < 1e-9,
            "fallback_cost: {}",
            second.fallback_cost
        );

        // The known model must not appear in pricing_warnings.
        assert!(
            !result
                .pricing_warnings
                .iter()
                .any(|w| w.unknown_model == "claude-opus-4-6"),
            "known model leaked into pricing_warnings"
        );
    }

    /// Empty input → all zeros, no panics. This is the loader's "no sessions
    /// found" path and must never crash.
    #[test]
    fn empty_sessions_produce_zero_totals() {
        let calc = PricingCalculator::new();
        let result = analyze_overview(&[], GlobalDataQuality::default(), &calc, None);

        assert_eq!(result.total_sessions, 0);
        assert_eq!(result.total_turns, 0);
        assert_eq!(result.total_agent_turns, 0);
        assert!((result.total_cost - 0.0).abs() < 1e-9);
        assert!(result.cost_by_model.is_empty());
        assert!(result.tokens_by_model.is_empty());
        assert!(result.pricing_warnings.is_empty());
        assert_eq!(result.total_output_tokens, 0);
    }

    /// Two models on the same session produce two distinct `cost_by_model`
    /// rows. Per-model cost must match what `calculate_turn_cost` would
    /// produce in isolation, and the sum across rows must equal `total_cost`.
    #[test]
    fn multi_model_breakdown_sums_to_total_cost() {
        let calc = PricingCalculator::new();
        let session = make_session(vec![
            make_turn("claude-opus-4-6", 1_000_000, 1_000_000), // contributes 1M+1M
            make_turn("claude-opus-4-6", 500_000, 500_000),     // contributes 0.5M+0.5M
            make_turn("claude-sonnet-4-5", 1_000_000, 1_000_000), // sonnet rate
        ]);
        let result = analyze_overview(&[session], GlobalDataQuality::default(), &calc, None);

        assert_eq!(
            result.cost_by_model.len(),
            2,
            "two distinct models => two cost rows"
        );

        // Sum across cost_by_model must equal total_cost.
        let breakdown_total: f64 = result.cost_by_model.values().sum();
        assert!(
            (breakdown_total - result.total_cost).abs() < 1e-6,
            "cost_by_model sum {} != total_cost {}",
            breakdown_total,
            result.total_cost
        );

        // opus-4-6 row aggregates both turns:
        // 1.5M input @ $5/MTok = $7.5; 1.5M output @ $25/MTok = $37.5; total = $45.
        let opus_cost = *result
            .cost_by_model
            .get("claude-opus-4-6")
            .expect("opus-4-6 row missing");
        assert!(
            (opus_cost - 45.0).abs() < 1e-6,
            "opus-4-6 cost: expected $45, got {}",
            opus_cost
        );
    }

    /// `total_turns` counts main turns; `total_agent_turns` counts subagent
    /// turns. A session with only main turns must report 0 agent turns.
    #[test]
    fn main_only_session_has_zero_agent_turns() {
        let calc = PricingCalculator::new();
        let session = make_session(vec![
            make_turn("claude-opus-4-6", 100, 200),
            make_turn("claude-opus-4-6", 100, 200),
            make_turn("claude-opus-4-6", 100, 200),
        ]);
        let result = analyze_overview(&[session], GlobalDataQuality::default(), &calc, None);

        assert_eq!(result.total_sessions, 1);
        assert_eq!(result.total_turns, 3);
        assert_eq!(result.total_agent_turns, 0);
    }

    /// `cost_by_category` (input/output/cache_write/cache_read) must sum to
    /// `total_cost`. Drift here means a category was dropped from the
    /// breakdown or double-counted.
    #[test]
    fn cost_by_category_sums_to_total() {
        let calc = PricingCalculator::new();
        let session = make_session(vec![make_turn("claude-opus-4-6", 1_000_000, 1_000_000)]);
        let result = analyze_overview(&[session], GlobalDataQuality::default(), &calc, None);

        let cb = &result.cost_by_category;
        let cat_sum = cb.input_cost
            + cb.output_cost
            + cb.cache_write_5m_cost
            + cb.cache_write_1h_cost
            + cb.cache_read_cost;

        assert!(
            (cat_sum - result.total_cost).abs() < 1e-6,
            "cost_by_category sum {} != total_cost {}",
            cat_sum,
            result.total_cost
        );
    }

    /// When `subscription_price` is provided, `subscription_value` is
    /// populated; when absent, it is None. Guards the optional ROI path.
    #[test]
    fn subscription_price_populates_value_field() {
        let calc = PricingCalculator::new();
        let session = make_session(vec![make_turn("claude-opus-4-6", 1_000_000, 1_000_000)]);

        let without = analyze_overview(
            std::slice::from_ref(&session),
            GlobalDataQuality::default(),
            &calc,
            None,
        );
        assert!(
            without.subscription_value.is_none(),
            "no subscription_price => subscription_value must be None"
        );

        let with = analyze_overview(
            &[session],
            GlobalDataQuality::default(),
            &calc,
            Some(20.0),
        );
        assert!(
            with.subscription_value.is_some(),
            "subscription_price=20 => subscription_value must be Some"
        );
    }
}
