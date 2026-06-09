use std::collections::HashMap;

use chrono::{Datelike, Local, NaiveDate};

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
        Local::now().date_naive() - chrono::Duration::days(days as i64)
    };

    let mut accumulators: HashMap<String, Accumulator> = HashMap::new();
    let mut session_labels: HashMap<String, usize> = HashMap::new();

    for session in sessions {
        // Count session by its first_timestamp
        if let Some(first_ts) = session.first_timestamp {
            let date = first_ts.with_timezone(&Local).date_naive();
            if date >= cutoff {
                let label = make_label(date, group_by_month);
                *session_labels.entry(label).or_insert(0) += 1;
            }
        }

        // Process all turns
        for turn in session.all_responses() {
            let date = turn.timestamp.with_timezone(&Local).date_naive();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{
        DataQuality, SessionData, SessionMetadata, TokenUsage, ValidatedTurn,
    };
    use chrono::{DateTime, Utc};

    fn turn(ts: &str, model: &str, input: u64, output: u64) -> ValidatedTurn {
        ValidatedTurn {
            uuid: format!("u-{ts}"),
            request_id: None,
            timestamp: ts.parse::<DateTime<Utc>>().unwrap(),
            model: model.into(),
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
            stop_reason: Some("end_turn".into()),
            content_types: vec!["text".into()],
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

    fn session(id: &str, turns: Vec<ValidatedTurn>) -> SessionData {
        let first = turns.iter().map(|t| t.timestamp).min();
        let last = turns.iter().map(|t| t.timestamp).max();
        SessionData {
            session_id: id.into(),
            project: Some("p".into()),
            turns,
            subagents: vec![],
            plugins: vec![],
            skills: vec![],
            hooks: vec![],
            first_timestamp: first,
            last_timestamp: last,
            version: None,
            quality: DataQuality::default(),
            metadata: SessionMetadata::default(),
            is_orphan: false,
        }
    }

    /// No sessions → no entries; `group_label` still reflects the requested
    /// granularity. Renderers must not panic on this empty shape.
    #[test]
    fn empty_sessions_produce_empty_entries() {
        let calc = PricingCalculator::new();
        let daily = analyze_trend(&[], &calc, 0, false);
        assert!(daily.entries.is_empty());
        assert_eq!(daily.group_label, "Day");

        let monthly = analyze_trend(&[], &calc, 0, true);
        assert!(monthly.entries.is_empty());
        assert_eq!(monthly.group_label, "Month");
    }

    /// Monthly grouping uses `YYYY-MM` labels. Two turns in March + one in
    /// April produce two entries; March precedes April (ascending sort).
    #[test]
    fn monthly_grouping_buckets_by_year_month() {
        let calc = PricingCalculator::new();
        let s = session(
            "s1",
            vec![
                turn("2025-03-10T10:00:00Z", "claude-opus-4-6", 100, 100),
                turn("2025-03-15T10:00:00Z", "claude-opus-4-6", 100, 100),
                turn("2025-04-02T10:00:00Z", "claude-opus-4-6", 100, 100),
            ],
        );
        let result = analyze_trend(&[s], &calc, 0, true);

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].label, "2025-03");
        assert_eq!(result.entries[1].label, "2025-04");
        assert_eq!(result.entries[0].turn_count, 2);
        assert_eq!(result.entries[1].turn_count, 1);
    }

    /// Daily grouping uses `YYYY-MM-DD` labels and produces one bucket per
    /// distinct local date — different dates same month must NOT merge.
    #[test]
    fn daily_grouping_buckets_by_full_date() {
        let calc = PricingCalculator::new();
        let s = session(
            "s1",
            vec![
                turn("2025-03-10T10:00:00Z", "claude-opus-4-6", 100, 100),
                turn("2025-03-15T10:00:00Z", "claude-opus-4-6", 100, 100),
            ],
        );
        let result = analyze_trend(&[s], &calc, 0, false);

        assert_eq!(result.entries.len(), 2, "two distinct dates => two entries");
        // Labels are dates; ascending order
        assert!(result.entries[0].label < result.entries[1].label);
        assert_eq!(result.group_label, "Day");
    }

    /// `session_count` per bucket counts sessions whose `first_timestamp`
    /// falls in that bucket — NOT turns. Two sessions starting in the same
    /// month give that bucket `session_count = 2` even with N turns each.
    #[test]
    fn session_count_uses_first_timestamp_not_turn_dates() {
        let calc = PricingCalculator::new();
        let s_march_a = session(
            "march-a",
            vec![turn("2025-03-01T10:00:00Z", "claude-opus-4-6", 100, 100)],
        );
        let s_march_b = session(
            "march-b",
            vec![turn("2025-03-20T10:00:00Z", "claude-opus-4-6", 100, 100)],
        );
        let s_april = session(
            "april",
            vec![turn("2025-04-05T10:00:00Z", "claude-opus-4-6", 100, 100)],
        );
        let result = analyze_trend(&[s_march_a, s_march_b, s_april], &calc, 0, true);

        let march = result
            .entries
            .iter()
            .find(|e| e.label == "2025-03")
            .expect("march bucket missing");
        let april = result
            .entries
            .iter()
            .find(|e| e.label == "2025-04")
            .expect("april bucket missing");
        assert_eq!(march.session_count, 2);
        assert_eq!(april.session_count, 1);
    }

    /// Per-bucket cost_by_category must sum to the bucket's `cost` field.
    /// Drift here means trend lost a category during aggregation.
    #[test]
    fn cost_by_category_per_bucket_sums_to_bucket_cost() {
        let calc = PricingCalculator::new();
        let s = session(
            "s1",
            vec![turn(
                "2025-03-10T10:00:00Z",
                "claude-opus-4-6",
                1_000_000,
                1_000_000,
            )],
        );
        let result = analyze_trend(&[s], &calc, 0, true);

        assert_eq!(result.entries.len(), 1);
        let entry = &result.entries[0];
        let cb = &entry.cost_by_category;
        let cat_sum = cb.input_cost
            + cb.output_cost
            + cb.cache_write_5m_cost
            + cb.cache_write_1h_cost
            + cb.cache_read_cost;
        assert!(
            (cat_sum - entry.cost).abs() < 1e-6,
            "cost_by_category sum {} != bucket cost {}",
            cat_sum,
            entry.cost
        );
        // input $5 + output $25 = $30
        assert!(
            (entry.cost - 30.0).abs() < 1e-6,
            "bucket cost: {}",
            entry.cost
        );
    }
}
