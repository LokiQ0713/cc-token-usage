use std::collections::{BTreeSet, HashMap};

use chrono::{Datelike, Local, NaiveDate, Timelike, Utc};
use serde::Serialize;

use crate::data::models::SessionData;
use crate::pricing::calculator::PricingCalculator;

use super::project::project_display_name;

// ─── Developer Archetype ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub enum DeveloperArchetype {
    Architect,
    Sprinter,
    NightOwl,
    Delegator,
    Explorer,
    Marathoner,
}

impl DeveloperArchetype {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Architect => "The Architect",
            Self::Sprinter => "The Sprinter",
            Self::NightOwl => "The Night Owl",
            Self::Delegator => "The Delegator",
            Self::Explorer => "The Explorer",
            Self::Marathoner => "The Marathoner",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Architect => "You love orchestrating multi-agent teams for complex tasks.",
            Self::Sprinter => "Short, intense bursts of productivity — you get in, get it done, and get out.",
            Self::NightOwl => "The best code is written after dark. Your peak hours are when the world sleeps.",
            Self::Delegator => "You trust your agents more than yourself. Maximum delegation, maximum output.",
            Self::Explorer => "A polyglot of projects — always trying something new.",
            Self::Marathoner => "You settle in for the long haul. Deep work sessions are your superpower.",
        }
    }
}

// ─── Result ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct WrappedResult {
    pub year: i32,

    // Activity
    pub active_days: usize,
    pub total_days: usize,
    pub longest_streak: usize,
    pub ghost_days: usize,

    // Volume
    pub total_sessions: usize,
    pub total_turns: usize,
    pub total_agent_turns: usize,
    pub total_output_tokens: u64,
    pub total_input_tokens: u64,
    pub total_cost: f64,

    // Efficiency
    pub autonomy_ratio: f64,
    pub avg_session_duration_min: f64,
    pub avg_cost_per_session: f64,
    pub output_ratio: f64,

    // Peak patterns
    pub peak_hour: usize,
    pub peak_weekday: String,
    pub hourly_distribution: [usize; 24],
    pub weekday_distribution: [usize; 7],

    // Top items
    pub top_projects: Vec<(String, f64)>,
    pub top_tools: Vec<(String, usize)>,
    pub most_expensive_session: Option<(String, f64, String)>,
    pub longest_session: Option<(String, f64, String)>,

    // Models
    pub model_distribution: Vec<(String, usize)>,

    // Developer archetype
    pub archetype: DeveloperArchetype,

    // Metadata
    pub total_pr_count: usize,
    pub total_speculation_time_saved_ms: f64,
    pub total_collapse_count: usize,
}

// ─── Analysis ───────────────────────────────────────────────────────────────

pub fn analyze_wrapped(
    sessions: &[SessionData],
    calc: &PricingCalculator,
    year: i32,
) -> WrappedResult {
    // Filter sessions to the specified year
    let year_sessions: Vec<&SessionData> = sessions
        .iter()
        .filter(|s| {
            s.first_timestamp
                .map(|t| t.with_timezone(&Local).year() == year)
                .unwrap_or(false)
        })
        .collect();

    // ── Activity dates ──────────────────────────────────────────────────────
    let mut active_dates: BTreeSet<NaiveDate> = BTreeSet::new();

    // ── Volume accumulators ─────────────────────────────────────────────────
    let mut total_turns: usize = 0;
    let mut total_agent_turns: usize = 0;
    let mut total_output_tokens: u64 = 0;
    let mut total_input_tokens: u64 = 0;
    let mut total_cost: f64 = 0.0;

    // ── Distribution accumulators ───────────────────────────────────────────
    let mut hourly_distribution = [0usize; 24];
    let mut weekday_distribution = [0usize; 7]; // 0=Mon..6=Sun

    // ── Tool & model accumulators ───────────────────────────────────────────
    let mut tool_counts: HashMap<String, usize> = HashMap::new();
    let mut model_counts: HashMap<String, usize> = HashMap::new();

    // ── Project cost accumulator ────────────────────────────────────────────
    let mut project_costs: HashMap<String, f64> = HashMap::new();

    // ── Session-level tracking ──────────────────────────────────────────────
    let mut session_costs: Vec<(String, f64, String)> = Vec::new(); // (id, cost, project)
    let mut session_durations: Vec<(String, f64, String)> = Vec::new(); // (id, min, project)
    let mut total_duration_min: f64 = 0.0;
    let mut sessions_with_duration: usize = 0;

    // ── Metadata accumulators ───────────────────────────────────────────────
    let mut total_user_prompts: usize = 0;
    let mut total_pr_count: usize = 0;
    let mut total_speculation_time_saved_ms: f64 = 0.0;
    let mut total_collapse_count: usize = 0;
    let mut unique_projects: BTreeSet<String> = BTreeSet::new();

    for session in &year_sessions {
        let project = session
            .project
            .as_deref()
            .map(project_display_name)
            .unwrap_or_else(|| "(unknown)".to_string());

        unique_projects.insert(project.clone());

        // Session duration
        let duration_min = match (session.first_timestamp, session.last_timestamp) {
            (Some(first), Some(last)) => {
                let d = (last - first).num_seconds() as f64 / 60.0;
                if d > 0.0 {
                    total_duration_min += d;
                    sessions_with_duration += 1;
                }
                d
            }
            _ => 0.0,
        };

        // Metadata
        total_user_prompts += session.metadata.user_prompt_count;
        total_pr_count += session.metadata.pr_links.len();
        total_speculation_time_saved_ms += session.metadata.speculation_time_saved_ms;
        total_collapse_count += session.metadata.collapse_commits.len();

        let mut session_cost = 0.0f64;

        for turn in session.all_responses() {
            total_turns += 1;
            if turn.is_agent {
                total_agent_turns += 1;
            }

            let out = turn.usage.output_tokens.unwrap_or(0);
            let inp = turn.usage.input_tokens.unwrap_or(0)
                + turn.usage.cache_creation_input_tokens.unwrap_or(0)
                + turn.usage.cache_read_input_tokens.unwrap_or(0);

            total_output_tokens += out;
            total_input_tokens += inp;

            let cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
            session_cost += cost.total;

            // Hourly/weekday distribution (local time)
            let local_ts = turn.timestamp.with_timezone(&Local);
            let hour = local_ts.hour() as usize;
            hourly_distribution[hour] += 1;
            let weekday = local_ts.weekday().num_days_from_monday() as usize;
            weekday_distribution[weekday] += 1;

            // Active date
            active_dates.insert(local_ts.date_naive());

            // Tools
            for name in &turn.tool_names {
                *tool_counts.entry(name.clone()).or_insert(0) += 1;
            }

            // Models
            *model_counts.entry(turn.model.clone()).or_insert(0) += 1;
        }

        total_cost += session_cost;
        *project_costs.entry(project.clone()).or_insert(0.0) += session_cost;
        session_costs.push((session.session_id.clone(), session_cost, project.clone()));
        session_durations.push((session.session_id.clone(), duration_min, project));
    }

    // ── Compute total_days ──────────────────────────────────────────────────
    let now = Utc::now().with_timezone(&Local);
    let total_days = if now.year() == year {
        now.ordinal() as usize
    } else if year < now.year() {
        // Full year
        NaiveDate::from_ymd_opt(year, 12, 31)
            .map(|d| d.ordinal() as usize)
            .unwrap_or(365)
    } else {
        // Future year — shouldn't happen, but handle gracefully
        0
    };

    let active_days = active_dates.len();
    let ghost_days = total_days.saturating_sub(active_days);

    // ── Longest streak ──────────────────────────────────────────────────────
    let longest_streak = compute_longest_streak(&active_dates);

    // ── Efficiency ──────────────────────────────────────────────────────────
    let autonomy_ratio = if total_user_prompts > 0 {
        total_turns as f64 / total_user_prompts as f64
    } else {
        0.0
    };

    let avg_session_duration_min = if sessions_with_duration > 0 {
        total_duration_min / sessions_with_duration as f64
    } else {
        0.0
    };

    let avg_cost_per_session = if !year_sessions.is_empty() {
        total_cost / year_sessions.len() as f64
    } else {
        0.0
    };

    let output_ratio = if total_input_tokens > 0 {
        total_output_tokens as f64 / total_input_tokens as f64 * 100.0
    } else {
        0.0
    };

    // ── Peak patterns ───────────────────────────────────────────────────────
    let peak_hour = hourly_distribution
        .iter()
        .enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(h, _)| h)
        .unwrap_or(0);

    let weekday_names = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let peak_weekday_idx = weekday_distribution
        .iter()
        .enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(d, _)| d)
        .unwrap_or(0);
    let peak_weekday = weekday_names[peak_weekday_idx].to_string();

    // ── Top projects (by cost, top 5) ───────────────────────────────────────
    let mut top_projects: Vec<(String, f64)> = project_costs.into_iter().collect();
    top_projects.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    top_projects.truncate(5);

    // ── Top tools (by count, top 5) ─────────────────────────────────────────
    let mut top_tools: Vec<(String, usize)> = tool_counts.into_iter().collect();
    top_tools.sort_by(|a, b| b.1.cmp(&a.1));
    top_tools.truncate(5);

    // ── Most expensive session ──────────────────────────────────────────────
    session_costs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let most_expensive_session = session_costs.first().cloned();

    // ── Longest session ─────────────────────────────────────────────────────
    session_durations.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let longest_session = session_durations.first().cloned();

    // ── Model distribution ──────────────────────────────────────────────────
    let mut model_distribution: Vec<(String, usize)> = model_counts.into_iter().collect();
    model_distribution.sort_by(|a, b| b.1.cmp(&a.1));

    // ── Archetype classification ────────────────────────────────────────────
    let agent_ratio = if total_turns > 0 {
        total_agent_turns as f64 / total_turns as f64
    } else {
        0.0
    };

    let night_turns: usize = hourly_distribution[22..].iter().sum::<usize>()
        + hourly_distribution[..6].iter().sum::<usize>();
    let night_ratio = if total_turns > 0 {
        night_turns as f64 / total_turns as f64
    } else {
        0.0
    };

    let turns_per_session = if !year_sessions.is_empty() {
        total_turns as f64 / year_sessions.len() as f64
    } else {
        0.0
    };

    let archetype = classify_archetype(
        agent_ratio,
        night_ratio,
        avg_session_duration_min,
        turns_per_session,
        unique_projects.len(),
    );

    WrappedResult {
        year,
        active_days,
        total_days,
        longest_streak,
        ghost_days,
        total_sessions: year_sessions.len(),
        total_turns,
        total_agent_turns,
        total_output_tokens,
        total_input_tokens,
        total_cost,
        autonomy_ratio,
        avg_session_duration_min,
        avg_cost_per_session,
        output_ratio,
        peak_hour,
        peak_weekday,
        hourly_distribution,
        weekday_distribution,
        top_projects,
        top_tools,
        most_expensive_session,
        longest_session,
        model_distribution,
        archetype,
        total_pr_count,
        total_speculation_time_saved_ms,
        total_collapse_count,
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn compute_longest_streak(dates: &BTreeSet<NaiveDate>) -> usize {
    if dates.is_empty() {
        return 0;
    }

    let sorted: Vec<NaiveDate> = dates.iter().copied().collect();
    let mut longest = 1usize;
    let mut current = 1usize;

    for window in sorted.windows(2) {
        let diff = window[1].signed_duration_since(window[0]).num_days();
        if diff == 1 {
            current += 1;
            if current > longest {
                longest = current;
            }
        } else {
            current = 1;
        }
    }

    longest
}

fn classify_archetype(
    agent_ratio: f64,
    night_ratio: f64,
    avg_session_min: f64,
    turns_per_session: f64,
    unique_project_count: usize,
) -> DeveloperArchetype {
    // 1. Delegator: agent turns > 50% of total
    if agent_ratio > 0.5 {
        return DeveloperArchetype::Delegator;
    }
    // 2. NightOwl: >50% turns in 22:00-06:00
    if night_ratio > 0.5 {
        return DeveloperArchetype::NightOwl;
    }
    // 3. Marathoner: avg session > 120 min
    if avg_session_min > 120.0 {
        return DeveloperArchetype::Marathoner;
    }
    // 4. Architect: high agent + long sessions
    if agent_ratio > 0.4 && avg_session_min > 60.0 {
        return DeveloperArchetype::Architect;
    }
    // 5. Sprinter: short sessions with high turn density
    if avg_session_min < 30.0 && turns_per_session > 10.0 {
        return DeveloperArchetype::Sprinter;
    }
    // 6. Explorer: many projects
    if unique_project_count > 10 {
        return DeveloperArchetype::Explorer;
    }
    // 7. Default
    DeveloperArchetype::Architect
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_longest_streak_empty() {
        let dates = BTreeSet::new();
        assert_eq!(compute_longest_streak(&dates), 0);
    }

    #[test]
    fn test_compute_longest_streak_single() {
        let mut dates = BTreeSet::new();
        dates.insert(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(compute_longest_streak(&dates), 1);
    }

    #[test]
    fn test_compute_longest_streak_consecutive() {
        let mut dates = BTreeSet::new();
        dates.insert(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        dates.insert(NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        dates.insert(NaiveDate::from_ymd_opt(2026, 1, 3).unwrap());
        dates.insert(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()); // gap
        dates.insert(NaiveDate::from_ymd_opt(2026, 1, 6).unwrap());
        assert_eq!(compute_longest_streak(&dates), 3);
    }

    #[test]
    fn test_compute_longest_streak_all_consecutive() {
        let mut dates = BTreeSet::new();
        for d in 1..=10 {
            dates.insert(NaiveDate::from_ymd_opt(2026, 3, d).unwrap());
        }
        assert_eq!(compute_longest_streak(&dates), 10);
    }

    #[test]
    fn test_classify_delegator() {
        let arch = classify_archetype(0.6, 0.1, 45.0, 20.0, 3);
        assert!(matches!(arch, DeveloperArchetype::Delegator));
    }

    #[test]
    fn test_classify_night_owl() {
        let arch = classify_archetype(0.3, 0.6, 45.0, 20.0, 3);
        assert!(matches!(arch, DeveloperArchetype::NightOwl));
    }

    #[test]
    fn test_classify_marathoner() {
        let arch = classify_archetype(0.3, 0.1, 150.0, 20.0, 3);
        assert!(matches!(arch, DeveloperArchetype::Marathoner));
    }

    #[test]
    fn test_classify_architect() {
        let arch = classify_archetype(0.45, 0.1, 90.0, 20.0, 3);
        assert!(matches!(arch, DeveloperArchetype::Architect));
    }

    #[test]
    fn test_classify_sprinter() {
        let arch = classify_archetype(0.1, 0.1, 15.0, 15.0, 3);
        assert!(matches!(arch, DeveloperArchetype::Sprinter));
    }

    #[test]
    fn test_classify_explorer() {
        let arch = classify_archetype(0.1, 0.1, 45.0, 8.0, 15);
        assert!(matches!(arch, DeveloperArchetype::Explorer));
    }

    #[test]
    fn test_classify_default_architect() {
        let arch = classify_archetype(0.1, 0.1, 45.0, 5.0, 3);
        assert!(matches!(arch, DeveloperArchetype::Architect));
    }
}
