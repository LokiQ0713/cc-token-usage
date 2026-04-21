use std::collections::HashMap;

use chrono::{Local, NaiveDate};

use crate::data::models::SessionData;
use crate::pricing::calculator::PricingCalculator;

// ─── Result Types ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct HeatmapResult {
    pub daily: Vec<DailyActivity>,
    /// Start date of the range (inclusive)
    pub start_date: NaiveDate,
    /// End date of the range (inclusive, always today in local time)
    pub end_date: NaiveDate,
    /// Percentile thresholds (P25, P50, P75) computed from non-zero days
    pub thresholds: (usize, usize, usize),
    /// Streak statistics
    pub stats: HeatmapStats,
}

#[derive(Debug, Clone)]
pub struct DailyActivity {
    pub date: NaiveDate,
    pub turns: usize,
    pub cost: f64,
    pub sessions: usize,
}

#[derive(Debug)]
pub struct HeatmapStats {
    pub total_days: usize,
    pub active_days: usize,
    pub current_streak: usize,
    pub longest_streak: usize,
    pub busiest_day: Option<(NaiveDate, usize)>,
}

// ─── Analysis ──────────────────────────────────────────────────────────────

pub fn analyze_heatmap(
    sessions: &[SessionData],
    calc: &PricingCalculator,
    days: u32,
) -> HeatmapResult {
    let today = Local::now().date_naive();

    let start_date = if days == 0 {
        // All history: find earliest timestamp across all sessions
        sessions
            .iter()
            .filter_map(|s| s.first_timestamp)
            .map(|ts| ts.with_timezone(&Local).date_naive())
            .min()
            .unwrap_or(today)
    } else {
        today - chrono::Duration::days(days as i64 - 1)
    };

    // Aggregate turns per day
    let mut day_map: HashMap<NaiveDate, (usize, f64, usize)> = HashMap::new();

    // Count sessions per day (by first_timestamp)
    for session in sessions {
        if let Some(first_ts) = session.first_timestamp {
            let date = first_ts.with_timezone(&Local).date_naive();
            if date >= start_date && date <= today {
                day_map.entry(date).or_default().2 += 1;
            }
        }

        // Count turns and cost per day
        for turn in session.all_responses() {
            let date = turn.timestamp.with_timezone(&Local).date_naive();
            if date < start_date || date > today {
                continue;
            }
            let entry = day_map.entry(date).or_default();
            entry.0 += 1;
            entry.1 += calc.calculate_turn_cost(&turn.model, &turn.usage).total;
        }
    }

    // Build daily activity vector for the full range
    let mut daily = Vec::new();
    let mut d = start_date;
    while d <= today {
        let (turns, cost, sessions) = day_map.get(&d).copied().unwrap_or_default();
        daily.push(DailyActivity {
            date: d,
            turns,
            cost,
            sessions,
        });
        d += chrono::Duration::days(1);
    }

    // Compute percentile thresholds from non-zero days
    let thresholds = compute_thresholds(&daily);

    // Compute streak stats
    let stats = compute_stats(&daily, today);

    HeatmapResult {
        daily,
        start_date,
        end_date: today,
        thresholds,
        stats,
    }
}

fn compute_thresholds(daily: &[DailyActivity]) -> (usize, usize, usize) {
    let mut non_zero: Vec<usize> = daily
        .iter()
        .filter(|d| d.turns > 0)
        .map(|d| d.turns)
        .collect();
    if non_zero.is_empty() {
        return (1, 2, 3);
    }
    non_zero.sort_unstable();
    let len = non_zero.len();
    let p25 = non_zero[(len as f64 * 0.25) as usize];
    let p50 = non_zero[(len as f64 * 0.50).min((len - 1) as f64) as usize];
    let p75 = non_zero[(len as f64 * 0.75).min((len - 1) as f64) as usize];

    // Ensure thresholds are at least 1 and strictly increasing where possible
    let p25 = p25.max(1);
    let p50 = p50.max(p25);
    let p75 = p75.max(p50);

    (p25, p50, p75)
}

fn compute_stats(daily: &[DailyActivity], today: NaiveDate) -> HeatmapStats {
    let total_days = daily.len();
    let active_days = daily.iter().filter(|d| d.turns > 0).count();

    // Find busiest day
    let busiest_day = daily
        .iter()
        .filter(|d| d.turns > 0)
        .max_by_key(|d| d.turns)
        .map(|d| (d.date, d.turns));

    // Current streak: count consecutive active days ending at today
    let current_streak = {
        let mut streak = 0usize;
        for d in daily.iter().rev() {
            if d.date > today {
                continue;
            }
            if d.turns > 0 {
                streak += 1;
            } else {
                break;
            }
        }
        streak
    };

    // Longest streak
    let longest_streak = {
        let mut longest = 0usize;
        let mut current = 0usize;
        for d in daily {
            if d.turns > 0 {
                current += 1;
                if current > longest {
                    longest = current;
                }
            } else {
                current = 0;
            }
        }
        longest
    };

    HeatmapStats {
        total_days,
        active_days,
        current_streak,
        longest_streak,
        busiest_day,
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{
        DataQuality, SessionData, SessionMetadata, TokenUsage, ValidatedTurn,
    };
    use chrono::Utc;

    fn make_turn(ts: &str) -> ValidatedTurn {
        ValidatedTurn {
            uuid: "u1".to_string(),
            request_id: None,
            timestamp: ts.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
            model: "claude-sonnet-4-20250514".to_string(),
            usage: TokenUsage {
                input_tokens: Some(100),
                output_tokens: Some(50),
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
        }
    }

    use chrono::DateTime;

    fn make_session(id: &str, turns: Vec<ValidatedTurn>) -> SessionData {
        let first = turns.first().map(|t| t.timestamp);
        let last = turns.last().map(|t| t.timestamp);
        SessionData {
            session_id: id.to_string(),
            project: Some("test-project".to_string()),
            turns,
            agent_turns: vec![],
            first_timestamp: first,
            last_timestamp: last,
            version: None,
            quality: DataQuality::default(),
            metadata: SessionMetadata::default(),
        }
    }

    #[test]
    fn test_thresholds_empty() {
        let daily = vec![];
        let (p25, p50, p75) = compute_thresholds(&daily);
        assert!(p25 >= 1);
        assert!(p50 >= p25);
        assert!(p75 >= p50);
    }

    #[test]
    fn test_thresholds_uniform() {
        let daily: Vec<DailyActivity> = (0..10)
            .map(|i| DailyActivity {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(i),
                turns: 5,
                cost: 0.0,
                sessions: 1,
            })
            .collect();
        let (p25, p50, p75) = compute_thresholds(&daily);
        assert_eq!(p25, 5);
        assert_eq!(p50, 5);
        assert_eq!(p75, 5);
    }

    #[test]
    fn test_stats_streaks() {
        let today = Local::now().date_naive();
        let daily: Vec<DailyActivity> = (0..7)
            .map(|i| DailyActivity {
                date: today - chrono::Duration::days(6 - i),
                turns: if i < 3 { 0 } else { 5 }, // 4 active days ending at today
                cost: 0.0,
                sessions: if i < 3 { 0 } else { 1 },
            })
            .collect();

        let stats = compute_stats(&daily, today);
        assert_eq!(stats.active_days, 4);
        assert_eq!(stats.current_streak, 4);
        assert_eq!(stats.longest_streak, 4);
        assert_eq!(stats.total_days, 7);
    }

    #[test]
    fn test_stats_broken_streak() {
        let today = Local::now().date_naive();
        let daily: Vec<DailyActivity> = (0..7)
            .map(|i| DailyActivity {
                date: today - chrono::Duration::days(6 - i),
                turns: if i == 4 { 0 } else { 3 }, // gap at day 4
                cost: 0.0,
                sessions: if i == 4 { 0 } else { 1 },
            })
            .collect();

        let stats = compute_stats(&daily, today);
        assert_eq!(stats.active_days, 6);
        assert_eq!(stats.current_streak, 2); // last 2 days
        assert_eq!(stats.longest_streak, 4); // first 4 days
    }

    #[test]
    fn test_analyze_with_sessions() {
        let calc = PricingCalculator::new();
        let now = Utc::now();
        let two_days_ago = (now - chrono::Duration::days(2)).to_rfc3339();
        let one_day_ago = (now - chrono::Duration::days(1)).to_rfc3339();
        let sessions = vec![make_session(
            "s1",
            vec![
                make_turn(&two_days_ago),
                make_turn(&two_days_ago),
                make_turn(&one_day_ago),
            ],
        )];

        let result = analyze_heatmap(&sessions, &calc, 30);
        assert!(result.daily.len() <= 30);
        assert!(result.stats.active_days >= 1);
    }

    #[test]
    fn test_busiest_day() {
        let today = Local::now().date_naive();
        let daily = vec![
            DailyActivity {
                date: today - chrono::Duration::days(2),
                turns: 3,
                cost: 0.0,
                sessions: 1,
            },
            DailyActivity {
                date: today - chrono::Duration::days(1),
                turns: 10,
                cost: 0.0,
                sessions: 2,
            },
            DailyActivity {
                date: today,
                turns: 1,
                cost: 0.0,
                sessions: 1,
            },
        ];

        let stats = compute_stats(&daily, today);
        assert_eq!(stats.busiest_day.unwrap().1, 10);
    }
}
