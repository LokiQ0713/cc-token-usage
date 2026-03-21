use std::collections::HashMap;

use chrono::{Datelike, NaiveDate, Utc};

use crate::data::models::SessionData;
use crate::pricing::calculator::PricingCalculator;

use super::{AggregatedTokens, CostByCategory, TrendEntry, TrendResult};

pub fn analyze_trend(
    sessions: &[SessionData],
    calc: &PricingCalculator,
    days: u32,
    group_by_month: bool,
) -> TrendResult {
    // days=0 means all history
    let cutoff = if days == 0 {
        chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()
    } else {
        Utc::now().date_naive() - chrono::Duration::days(days as i64)
    };

    let mut accumulators: HashMap<String, Accumulator> = HashMap::new();
    let mut session_labels: HashMap<String, usize> = HashMap::new();

    for session in sessions {
        // Count session by its first_timestamp
        if let Some(first_ts) = session.first_timestamp {
            let date = first_ts.date_naive();
            if date >= cutoff {
                let label = make_label(date, group_by_month);
                *session_labels.entry(label).or_insert(0) += 1;
            }
        }

        // Process all turns
        let all_turns = session.turns.iter().chain(session.agent_turns.iter());
        for turn in all_turns {
            let date = turn.timestamp.date_naive();
            if date < cutoff {
                continue;
            }

            let label = make_label(date, group_by_month);
            let acc = accumulators.entry(label).or_insert_with(|| Accumulator {
                first_date: date,
                turn_count: 0,
                tokens: AggregatedTokens::default(),
                cost: 0.0,
                models: HashMap::new(),
                cost_by_category: CostByCategory::default(),
            });

            // Keep earliest date for sorting
            if date < acc.first_date {
                acc.first_date = date;
            }

            acc.turn_count += 1;
            acc.tokens.add_usage(&turn.usage);

            let pricing_cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
            acc.cost += pricing_cost.total;

            // Accumulate cost by category
            acc.cost_by_category.input_cost += pricing_cost.input_cost;
            acc.cost_by_category.output_cost += pricing_cost.output_cost;
            acc.cost_by_category.cache_write_5m_cost += pricing_cost.cache_write_5m_cost;
            acc.cost_by_category.cache_write_1h_cost += pricing_cost.cache_write_1h_cost;
            acc.cost_by_category.cache_read_cost += pricing_cost.cache_read_cost;

            *acc.models.entry(turn.model.clone()).or_insert(0) +=
                turn.usage.output_tokens.unwrap_or(0);
        }
    }

    let mut entries: Vec<TrendEntry> = accumulators
        .into_iter()
        .map(|(label, acc)| TrendEntry {
            label: label.clone(),
            date: acc.first_date,
            session_count: session_labels.get(&label).copied().unwrap_or(0),
            turn_count: acc.turn_count,
            tokens: acc.tokens,
            cost: acc.cost,
            models: acc.models,
            cost_by_category: acc.cost_by_category,
        })
        .collect();

    entries.sort_by_key(|e| e.date);

    TrendResult {
        entries,
        group_label: if group_by_month { "Month" } else { "Day" }.to_string(),
    }
}

fn make_label(date: NaiveDate, group_by_month: bool) -> String {
    if group_by_month {
        format!("{}-{:02}", date.year(), date.month())
    } else {
        date.format("%Y-%m-%d").to_string()
    }
}

struct Accumulator {
    first_date: NaiveDate,
    turn_count: usize,
    tokens: AggregatedTokens,
    cost: f64,
    models: HashMap<String, u64>,
    cost_by_category: CostByCategory,
}
