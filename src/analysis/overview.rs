use std::collections::HashMap;

use chrono::{Datelike, Timelike};

use crate::data::models::{GlobalDataQuality, SessionData};
use crate::pricing::calculator::PricingCalculator;

use super::{
    AggregatedTokens, CacheSavings, CostByCategory, OverviewResult, SessionSummary,
    SubscriptionValue, TurnCostBreakdown, TurnDetail,
};

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
            );
            total_turns += 1;
            if turn.is_agent { total_agent_turns += 1; }

            // Aggregate tool usage
            for name in &turn.tool_names {
                *tool_count_map.entry(name.clone()).or_insert(0) += 1;
            }
        }
    }

    let mut tool_counts: Vec<(String, usize)> = tool_count_map.into_iter().collect();
    tool_counts.sort_by(|a, b| b.1.cmp(&a.1));

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
    let mut session_summaries: Vec<SessionSummary> = sessions
        .iter()
        .map(|s| build_session_summary(s, calc))
        .collect();

    // Populate turn_details for all sessions
    for (idx, session) in sessions.iter().enumerate() {
        let details = build_turn_details(session, calc);
        session_summaries[idx].turn_details = Some(details);
    }

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

    // Hourly distribution
    let hour = turn.timestamp.hour() as usize;
    hourly_distribution[hour] += 1;

    // Weekday-hour matrix
    let weekday = turn.timestamp.weekday().num_days_from_monday() as usize; // 0=Mon..6=Sun
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

    SessionSummary {
        session_id,
        project_display_name,
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
            tools.sort_by(|a, b| b.1.cmp(&a.1));
            tools.truncate(5);
            tools
        },
        turn_details: None,
    }
}

/// Build turn-level details for a session (used by HTML report for expandable rows).
fn build_turn_details(session: &SessionData, calc: &PricingCalculator) -> Vec<TurnDetail> {
    let all_turns = session.all_responses();

    let mut details = Vec::new();
    let mut prev_context_size: Option<u64> = None;

    for (i, turn) in all_turns.iter().enumerate() {
        let input = turn.usage.input_tokens.unwrap_or(0);
        let output = turn.usage.output_tokens.unwrap_or(0);
        let cache_create = turn.usage.cache_creation_input_tokens.unwrap_or(0);
        let cache_read = turn.usage.cache_read_input_tokens.unwrap_or(0);

        let (cache_write_5m, cache_write_1h) = if let Some(ref detail) = turn.usage.cache_creation {
            (
                detail.ephemeral_5m_input_tokens.unwrap_or(0),
                detail.ephemeral_1h_input_tokens.unwrap_or(0),
            )
        } else {
            (0, 0)
        };

        let context_size = input + cache_create + cache_read;
        let cache_hit_rate = if context_size > 0 {
            (cache_read as f64 / context_size as f64) * 100.0
        } else {
            0.0
        };

        let is_compaction = match prev_context_size {
            Some(prev) => prev > 0 && (context_size as f64) < (prev as f64 * 0.9),
            None => false,
        };
        let context_delta = match prev_context_size {
            Some(prev) => context_size as i64 - prev as i64,
            None => 0,
        };
        prev_context_size = Some(context_size);

        let pricing_cost = calc.calculate_turn_cost(&turn.model, &turn.usage);

        let cost_breakdown = TurnCostBreakdown {
            input_cost: pricing_cost.input_cost,
            output_cost: pricing_cost.output_cost,
            cache_write_5m_cost: pricing_cost.cache_write_5m_cost,
            cache_write_1h_cost: pricing_cost.cache_write_1h_cost,
            cache_read_cost: pricing_cost.cache_read_cost,
            total: pricing_cost.total,
        };

        details.push(TurnDetail {
            turn_number: i + 1,
            timestamp: turn.timestamp,
            model: turn.model.clone(),
            input_tokens: input,
            output_tokens: output,
            cache_write_5m_tokens: cache_write_5m,
            cache_write_1h_tokens: cache_write_1h,
            cache_read_tokens: cache_read,
            context_size,
            cache_hit_rate,
            cost: pricing_cost.total,
            cost_breakdown,
            stop_reason: turn.stop_reason.clone(),
            is_agent: turn.is_agent,
            is_compaction,
            context_delta,
            user_text: turn.user_text.clone(),
            assistant_text: turn.assistant_text.clone(),
            tool_names: turn.tool_names.clone(),
        });
    }

    details
}
