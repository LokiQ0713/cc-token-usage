use std::collections::HashMap;

use crate::data::models::SessionData;
use crate::pricing::calculator::PricingCalculator;

use super::{AgentSummary, AggregatedTokens, SessionResult, TurnCostBreakdown, TurnDetail};

pub fn analyze_session(
    session: &SessionData,
    calc: &PricingCalculator,
) -> SessionResult {
    let all_turns = session.all_responses();

    let mut turn_details = Vec::new();
    let mut total_tokens = AggregatedTokens::default();
    let mut total_cost = 0.0;
    let mut stop_reason_counts: HashMap<String, usize> = HashMap::new();
    let mut agent_summary = AgentSummary::default();
    let mut model_counts: HashMap<&str, usize> = HashMap::new();
    let mut max_context: u64 = 0;
    let mut prev_context_size: Option<u64> = None;

    for (i, turn) in all_turns.iter().enumerate() {
        let input = turn.usage.input_tokens.unwrap_or(0);
        let output = turn.usage.output_tokens.unwrap_or(0);
        let cache_create = turn.usage.cache_creation_input_tokens.unwrap_or(0);
        let cache_read = turn.usage.cache_read_input_tokens.unwrap_or(0);

        // Extract 5m/1h TTL breakdown
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

        // Track max context
        if context_size > max_context {
            max_context = context_size;
        }

        // Compaction detection and context delta
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

        // Track stop reasons
        if let Some(ref reason) = turn.stop_reason {
            *stop_reason_counts.entry(reason.clone()).or_insert(0) += 1;
        }

        // Aggregate tokens
        total_tokens.add_usage(&turn.usage);
        total_cost += pricing_cost.total;

        // Model frequency
        *model_counts.entry(&turn.model).or_insert(0) += 1;

        // Agent summary
        let is_agent = turn.is_agent;
        if is_agent {
            agent_summary.total_agent_turns += 1;
            agent_summary.agent_output_tokens += output;
            agent_summary.agent_cost += pricing_cost.total;
        }

        turn_details.push(TurnDetail {
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
            is_agent,
            is_compaction,
            context_delta,
            user_text: turn.user_text.clone(),
            assistant_text: turn.assistant_text.clone(),
            tool_names: turn.tool_names.clone(),
        });
    }

    // Duration
    let duration_minutes = match (session.first_timestamp, session.last_timestamp) {
        (Some(first), Some(last)) => (last - first).num_seconds() as f64 / 60.0,
        _ => 0.0,
    };

    // Compaction count
    let compaction_count = turn_details.iter().filter(|t| t.is_compaction).count();

    // Cache write percentages from total tokens
    let total_5m = total_tokens.cache_write_5m_tokens;
    let total_1h = total_tokens.cache_write_1h_tokens;
    let total_cache_write = total_5m + total_1h;
    let cache_write_5m_pct = if total_cache_write > 0 {
        (total_5m as f64 / total_cache_write as f64) * 100.0
    } else {
        0.0
    };
    let cache_write_1h_pct = if total_cache_write > 0 {
        (total_1h as f64 / total_cache_write as f64) * 100.0
    } else {
        0.0
    };

    // Primary model
    let model = model_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(m, _)| m.to_string())
        .unwrap_or_default();

    SessionResult {
        session_id: session.session_id.clone(),
        project: session
            .project
            .clone()
            .unwrap_or_else(|| "(unknown)".to_string()),
        turn_details,
        agent_summary,
        total_tokens,
        total_cost,
        stop_reason_counts,
        duration_minutes,
        max_context,
        compaction_count,
        cache_write_5m_pct,
        cache_write_1h_pct,
        model,
    }
}
