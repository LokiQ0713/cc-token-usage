use std::collections::HashMap;

use chrono::Datelike;
use serde::Serialize;

use crate::analysis::wrapped::WrappedResult;
use crate::analysis::{OverviewResult, ProjectResult, SessionResult, TrendResult};
use crate::data::models::SessionData;
use crate::pricing::calculator::PricingCalculator;

// ─── Overview JSON ──────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OverviewJson {
    total_sessions: usize,
    total_turns: usize,
    total_agent_turns: usize,
    total_output_tokens: u64,
    total_context_tokens: u64,
    total_cost: f64,
    avg_cache_hit_rate: f64,
    // Efficiency
    output_ratio: f64,
    cost_per_turn: f64,
    tokens_per_output_turn: u64,
    // Cache savings
    cache_savings: CacheSavingsJson,
    // Subscription
    subscription_value: Option<SubscriptionValueJson>,
    // Cost breakdown
    cost_by_category: CostByCategoryJson,
    // Models
    models: Vec<ModelJson>,
    // Top tools
    top_tools: Vec<ToolJson>,
    // Sessions
    sessions: Vec<SessionSummaryJson>,
}

#[derive(Serialize)]
struct CacheSavingsJson {
    total_saved: f64,
    savings_pct: f64,
}

#[derive(Serialize)]
struct SubscriptionValueJson {
    monthly_price: f64,
    api_equivalent: f64,
    value_multiplier: f64,
}

#[derive(Serialize)]
struct CostByCategoryJson {
    input_cost: f64,
    output_cost: f64,
    cache_write_cost: f64,
    cache_read_cost: f64,
}

#[derive(Serialize)]
struct ModelJson {
    name: String,
    output_tokens: u64,
    turns: usize,
    cost: f64,
}

#[derive(Serialize)]
struct ToolJson {
    name: String,
    count: usize,
}

#[derive(Serialize)]
struct SessionSummaryJson {
    session_id: String,
    project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_timestamp: Option<String>,
    duration_minutes: f64,
    model: String,
    turn_count: usize,
    agent_turn_count: usize,
    output_tokens: u64,
    context_tokens: u64,
    max_context: u64,
    cache_hit_rate: f64,
    cost: f64,
    output_ratio: f64,
    cost_per_turn: f64,
}

pub fn render_overview_json(overview: &OverviewResult) -> String {
    let mut models: Vec<(&String, &crate::analysis::AggregatedTokens)> =
        overview.tokens_by_model.iter().collect();
    models.sort_by(|a, b| {
        let ca = overview.cost_by_model.get(a.0).unwrap_or(&0.0);
        let cb = overview.cost_by_model.get(b.0).unwrap_or(&0.0);
        cb.partial_cmp(ca).unwrap_or(std::cmp::Ordering::Equal)
    });

    let models_json: Vec<ModelJson> = models
        .iter()
        .map(|(name, tokens)| ModelJson {
            name: (*name).clone(),
            output_tokens: tokens.output_tokens,
            turns: tokens.turns,
            cost: *overview.cost_by_model.get(*name).unwrap_or(&0.0),
        })
        .collect();

    let top_tools: Vec<ToolJson> = overview
        .tool_counts
        .iter()
        .take(20)
        .map(|(name, count)| ToolJson {
            name: name.clone(),
            count: *count,
        })
        .collect();

    let sessions: Vec<SessionSummaryJson> = overview
        .session_summaries
        .iter()
        .map(|s| SessionSummaryJson {
            session_id: s.session_id.clone(),
            project: s.project_display_name.clone(),
            first_timestamp: s.first_timestamp.map(|t| t.to_rfc3339()),
            duration_minutes: s.duration_minutes,
            model: s.model.clone(),
            turn_count: s.turn_count,
            agent_turn_count: s.agent_turn_count,
            output_tokens: s.output_tokens,
            context_tokens: s.context_tokens,
            max_context: s.max_context,
            cache_hit_rate: s.cache_hit_rate,
            cost: s.cost,
            output_ratio: s.output_ratio,
            cost_per_turn: s.cost_per_turn,
        })
        .collect();

    let cat = &overview.cost_by_category;

    let json = OverviewJson {
        total_sessions: overview.total_sessions,
        total_turns: overview.total_turns,
        total_agent_turns: overview.total_agent_turns,
        total_output_tokens: overview.total_output_tokens,
        total_context_tokens: overview.total_context_tokens,
        total_cost: overview.total_cost,
        avg_cache_hit_rate: overview.avg_cache_hit_rate,
        output_ratio: overview.output_ratio,
        cost_per_turn: overview.cost_per_turn,
        tokens_per_output_turn: overview.tokens_per_output_turn,
        cache_savings: CacheSavingsJson {
            total_saved: overview.cache_savings.total_saved,
            savings_pct: overview.cache_savings.savings_pct,
        },
        subscription_value: overview.subscription_value.as_ref().map(|sv| {
            SubscriptionValueJson {
                monthly_price: sv.monthly_price,
                api_equivalent: sv.api_equivalent,
                value_multiplier: sv.value_multiplier,
            }
        }),
        cost_by_category: CostByCategoryJson {
            input_cost: cat.input_cost,
            output_cost: cat.output_cost,
            cache_write_cost: cat.cache_write_5m_cost + cat.cache_write_1h_cost,
            cache_read_cost: cat.cache_read_cost,
        },
        models: models_json,
        top_tools,
        sessions,
    };

    serde_json::to_string_pretty(&json).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

// ─── Session JSON ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct SessionJson {
    session_id: String,
    project: String,
    model: String,
    duration_minutes: f64,
    total_cost: f64,
    max_context: u64,
    compaction_count: usize,
    // Tokens
    output_tokens: u64,
    context_tokens: u64,
    cache_hit_rate: f64,
    // Agents
    agent_turns: usize,
    agent_output_tokens: u64,
    agent_cost: f64,
    // Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    // Turn details
    turns: Vec<TurnJson>,
}

#[derive(Serialize)]
struct TurnJson {
    turn_number: usize,
    timestamp: String,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    context_size: u64,
    cache_hit_rate: f64,
    cost: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    is_agent: bool,
    is_compaction: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_names: Vec<String>,
}

pub fn render_session_json(result: &SessionResult) -> String {
    let ctx = result.total_tokens.context_tokens();
    let cache_hit_rate = if ctx > 0 {
        result.total_tokens.cache_read_tokens as f64 / ctx as f64 * 100.0
    } else {
        0.0
    };

    let turns: Vec<TurnJson> = result
        .turn_details
        .iter()
        .map(|t| TurnJson {
            turn_number: t.turn_number,
            timestamp: t.timestamp.to_rfc3339(),
            model: t.model.clone(),
            input_tokens: t.input_tokens,
            output_tokens: t.output_tokens,
            cache_read_tokens: t.cache_read_tokens,
            context_size: t.context_size,
            cache_hit_rate: t.cache_hit_rate,
            cost: t.cost,
            stop_reason: t.stop_reason.clone(),
            is_agent: t.is_agent,
            is_compaction: t.is_compaction,
            tool_names: t.tool_names.clone(),
        })
        .collect();

    let json = SessionJson {
        session_id: result.session_id.clone(),
        project: result.project.clone(),
        model: result.model.clone(),
        duration_minutes: result.duration_minutes,
        total_cost: result.total_cost,
        max_context: result.max_context,
        compaction_count: result.compaction_count,
        output_tokens: result.total_tokens.output_tokens,
        context_tokens: ctx,
        cache_hit_rate,
        agent_turns: result.agent_summary.total_agent_turns,
        agent_output_tokens: result.agent_summary.agent_output_tokens,
        agent_cost: result.agent_summary.agent_cost,
        title: result.title.clone(),
        tags: result.tags.clone(),
        turns,
    };

    serde_json::to_string_pretty(&json).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

// ─── Projects JSON ──────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ProjectsJson {
    projects: Vec<ProjectJson>,
}

#[derive(Serialize)]
struct ProjectJson {
    name: String,
    display_name: String,
    session_count: usize,
    total_turns: usize,
    agent_turns: usize,
    output_tokens: u64,
    context_tokens: u64,
    cost: f64,
    primary_model: String,
}

pub fn render_projects_json(projects: &ProjectResult) -> String {
    let json = ProjectsJson {
        projects: projects
            .projects
            .iter()
            .map(|p| ProjectJson {
                name: p.name.clone(),
                display_name: p.display_name.clone(),
                session_count: p.session_count,
                total_turns: p.total_turns,
                agent_turns: p.agent_turns,
                output_tokens: p.tokens.output_tokens,
                context_tokens: p.tokens.context_tokens(),
                cost: p.cost,
                primary_model: p.primary_model.clone(),
            })
            .collect(),
    };

    serde_json::to_string_pretty(&json).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

// ─── Trend JSON ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct TrendJson {
    group_label: String,
    entries: Vec<TrendEntryJson>,
}

#[derive(Serialize)]
struct TrendEntryJson {
    label: String,
    session_count: usize,
    turn_count: usize,
    output_tokens: u64,
    context_tokens: u64,
    cost: f64,
    cost_per_turn: f64,
}

pub fn render_trend_json(trend: &TrendResult) -> String {
    let json = TrendJson {
        group_label: trend.group_label.clone(),
        entries: trend
            .entries
            .iter()
            .map(|e| {
                let cpt = if e.turn_count > 0 {
                    e.cost / e.turn_count as f64
                } else {
                    0.0
                };
                TrendEntryJson {
                    label: e.label.clone(),
                    session_count: e.session_count,
                    turn_count: e.turn_count,
                    output_tokens: e.tokens.output_tokens,
                    context_tokens: e.tokens.context_tokens(),
                    cost: e.cost,
                    cost_per_turn: cpt,
                }
            })
            .collect(),
    };

    serde_json::to_string_pretty(&json).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

// ─── Wrapped JSON ──────────────────────────────────────────────────────────

pub fn render_wrapped_json(result: &WrappedResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

// ─── Unified HTML Report Payload ───────────────────────────────────────────

/// Unified JSON payload for the HTML dashboard.
/// Combines data from all subcommands into a single structure.
#[derive(Serialize)]
pub struct HtmlReportPayload {
    pub overview: serde_json::Value,
    pub projects: serde_json::Value,
    pub trends: serde_json::Value,
    pub sessions: Vec<HtmlSessionSummary>,
    pub heatmap: HeatmapPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapped: Option<serde_json::Value>,
}

/// Per-session summary for the HTML dashboard.
#[derive(Serialize)]
pub struct HtmlSessionSummary {
    pub id: String,
    pub project: Option<String>,
    pub turns: usize,
    pub agent_turns: usize,
    pub cost: f64,
    pub duration_minutes: Option<f64>,
    pub model: Option<String>,
    pub cache_hit_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_timestamp: Option<String>,
    // metadata
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub mode: Option<String>,
}

/// Heatmap data for the HTML dashboard.
#[derive(Serialize)]
pub struct HeatmapPayload {
    pub days: Vec<DailyActivity>,
}

/// A single day's aggregated activity metrics.
#[derive(Serialize)]
pub struct DailyActivity {
    pub date: String,
    pub turns: usize,
    pub cost: f64,
    pub sessions: usize,
}

/// Build the unified HTML report payload.
///
/// Reuses existing `render_*_json` functions for overview/projects/trend,
/// then builds session summaries and heatmap data directly from `SessionData`.
pub fn render_html_payload(
    overview: &OverviewResult,
    projects: &ProjectResult,
    trend: &TrendResult,
    sessions: &[SessionData],
    calc: &PricingCalculator,
    wrapped: Option<&WrappedResult>,
) -> String {
    // Reuse existing JSON renderers and parse back into serde_json::Value
    let overview_json: serde_json::Value = serde_json::from_str(&render_overview_json(overview))
        .unwrap_or(serde_json::Value::Null);
    let projects_json: serde_json::Value = serde_json::from_str(&render_projects_json(projects))
        .unwrap_or(serde_json::Value::Null);
    let trends_json: serde_json::Value = serde_json::from_str(&render_trend_json(trend))
        .unwrap_or(serde_json::Value::Null);

    // Build per-session summaries
    let session_summaries: Vec<HtmlSessionSummary> = sessions
        .iter()
        .map(|s| build_html_session_summary(s, calc))
        .collect();

    // Build heatmap by aggregating sessions per date
    let heatmap = build_heatmap(sessions, calc);

    // Build wrapped data if available
    let wrapped_json: Option<serde_json::Value> = wrapped.and_then(|w| {
        serde_json::from_str(&render_wrapped_json(w)).ok()
    });

    let payload = HtmlReportPayload {
        overview: overview_json,
        projects: projects_json,
        trends: trends_json,
        sessions: session_summaries,
        heatmap,
        wrapped: wrapped_json,
    };

    serde_json::to_string(&payload).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

/// Build an `HtmlSessionSummary` from a single `SessionData`.
fn build_html_session_summary(session: &SessionData, calc: &PricingCalculator) -> HtmlSessionSummary {
    let all = session.all_responses();
    let turn_count = all.len();
    let agent_turn_count = session.agent_turn_count();

    // Compute total cost and cache hit rate
    let mut total_cost = 0.0;
    let mut total_cache_read: u64 = 0;
    let mut total_context: u64 = 0;
    let mut model_counts: HashMap<&str, usize> = HashMap::new();

    for turn in &all {
        let cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
        total_cost += cost.total;

        let input = turn.usage.input_tokens.unwrap_or(0);
        let cache_create = turn.usage.cache_creation_input_tokens.unwrap_or(0);
        let cache_read = turn.usage.cache_read_input_tokens.unwrap_or(0);
        let ctx = input + cache_create + cache_read;

        total_context += ctx;
        total_cache_read += cache_read;

        *model_counts.entry(&turn.model).or_insert(0) += 1;
    }

    let cache_hit_rate = if total_context > 0 {
        Some((total_cache_read as f64 / total_context as f64) * 100.0)
    } else {
        None
    };

    let primary_model = model_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(m, _)| m.to_string());

    let duration_minutes = match (session.first_timestamp, session.last_timestamp) {
        (Some(first), Some(last)) => Some((last - first).num_seconds() as f64 / 60.0),
        _ => None,
    };

    HtmlSessionSummary {
        id: session.session_id.clone(),
        project: session.project.clone(),
        turns: turn_count,
        agent_turns: agent_turn_count,
        cost: total_cost,
        duration_minutes,
        model: primary_model,
        cache_hit_rate,
        first_timestamp: session.first_timestamp.map(|t| t.to_rfc3339()),
        last_timestamp: session.last_timestamp.map(|t| t.to_rfc3339()),
        title: session.metadata.title.clone(),
        tags: session.metadata.tags.clone(),
        mode: session.metadata.mode.clone(),
    }
}

/// Aggregate sessions by date to build heatmap data.
fn build_heatmap(sessions: &[SessionData], calc: &PricingCalculator) -> HeatmapPayload {
    let mut daily_map: HashMap<String, (usize, f64, usize)> = HashMap::new(); // date -> (turns, cost, sessions)

    for session in sessions {
        // Use first_timestamp to determine the session's date
        let date_key = match session.first_timestamp {
            Some(ts) => {
                let local = ts.with_timezone(&chrono::Local);
                format!("{:04}-{:02}-{:02}", local.year(), local.month(), local.day())
            }
            None => continue,
        };

        let all = session.all_responses();
        let turn_count = all.len();
        let mut session_cost = 0.0;
        for turn in &all {
            let cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
            session_cost += cost.total;
        }

        let entry = daily_map.entry(date_key).or_insert((0, 0.0, 0));
        entry.0 += turn_count;
        entry.1 += session_cost;
        entry.2 += 1;
    }

    let mut days: Vec<DailyActivity> = daily_map
        .into_iter()
        .map(|(date, (turns, cost, session_count))| DailyActivity {
            date,
            turns,
            cost,
            sessions: session_count,
        })
        .collect();

    // Sort by date ascending
    days.sort_by(|a, b| a.date.cmp(&b.date));

    HeatmapPayload { days }
}
