use std::collections::HashMap;

use chrono::Datelike;
use serde::Serialize;

use crate::analysis::heatmap::HeatmapResult;
use crate::analysis::project::project_display_name;
use crate::analysis::wrapped::WrappedResult;
use crate::analysis::{OverviewResult, ProjectResult, SessionResult, TrendResult, WorkflowSummary};
use crate::data::models::{HookUsage, PluginUsage, SessionData, SkillUsage, SubagentTypeAggregate};
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
    /// Unknown-model pricing fallbacks. Empty array when every observed model
    /// has explicit pricing — emitted as `[]` (never elided) so the frontend
    /// type contract is stable.
    pricing_warnings: Vec<PricingWarningJson>,
}

#[derive(Serialize)]
struct PricingWarningJson {
    unknown_model: String,
    fallback_to: String,
    turn_count: u64,
    fallback_cost: f64,
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
    #[serde(rename = "agentTurnCount")]
    agent_turn_count: u64,
    output_tokens: u64,
    context_tokens: u64,
    max_context: u64,
    cache_hit_rate: f64,
    cost: f64,
    output_ratio: f64,
    cost_per_turn: f64,
    #[serde(rename = "isOrphan")]
    is_orphan: bool,
}

/// Build the typed `OverviewJson` struct from an `OverviewResult`.
fn build_overview_json(overview: &OverviewResult) -> OverviewJson {
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
            agent_turn_count: s.agent_turn_count as u64,
            output_tokens: s.output_tokens,
            context_tokens: s.context_tokens,
            max_context: s.max_context,
            cache_hit_rate: s.cache_hit_rate,
            cost: s.cost,
            output_ratio: s.output_ratio,
            cost_per_turn: s.cost_per_turn,
            is_orphan: s.is_orphan,
        })
        .collect();

    let cat = &overview.cost_by_category;

    OverviewJson {
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
        subscription_value: overview
            .subscription_value
            .as_ref()
            .map(|sv| SubscriptionValueJson {
                monthly_price: sv.monthly_price,
                api_equivalent: sv.api_equivalent,
                value_multiplier: sv.value_multiplier,
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
        pricing_warnings: overview
            .pricing_warnings
            .iter()
            .map(|w| PricingWarningJson {
                unknown_model: w.unknown_model.clone(),
                fallback_to: w.fallback_to.clone(),
                turn_count: w.turn_count,
                fallback_cost: w.fallback_cost,
            })
            .collect(),
    }
}

pub fn render_overview_json(overview: &OverviewResult) -> String {
    let json = build_overview_json(overview);
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
    // Agents (aggregate roll-ups; per-subagent detail lives in `subagents`)
    #[serde(rename = "agentTurnCount")]
    agent_turn_count: u64,
    agent_output_tokens: u64,
    agent_cost: f64,
    // Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    // Turn details (main session only)
    turns: Vec<TurnJson>,
    // Phase 2 capability inventory (always emitted as arrays, possibly empty).
    // PluginUsage / SkillUsage / HookUsage serialize their inner fields as
    // camelCase (matching the canonical Claude Code JSONL spelling).
    subagents: Vec<SubagentJson>,
    plugins: Vec<PluginUsage>,
    skills: Vec<SkillUsage>,
    hooks: Vec<HookUsage>,
    /// Per-`agent_type` rollup of `subagents[]`. UI chips group by type.
    /// SubagentTypeAggregate serializes to camelCase.
    #[serde(rename = "subagentTypes")]
    subagent_types: Vec<SubagentTypeAggregate>,
    /// Workflow runs (`agent()` orchestrations, Claude Code 2.1.159+) for this
    /// session. Always emitted (possibly empty). WorkflowSummary serializes to
    /// camelCase. See `analysis::WorkflowSummary` for the field contract.
    workflows: Vec<WorkflowSummary>,
    /// Orphan session: scanner reconstructed this session from subagent
    /// jsonl files only (parent jsonl deleted). Totals still include it.
    #[serde(rename = "isOrphan")]
    is_orphan: bool,
}

/// JSON shape for one subagent. Mirrors the spec's `subagents[]` schema
/// (camelCase, no `agentTurns[]` flat alias — superseded by `agentTurnCount`
/// scalar + `subagents[].turns[]` nested detail).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubagentJson {
    agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    turns: usize,
    output_tokens: u64,
    cost: f64,
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

    let subagents: Vec<SubagentJson> = result
        .subagents
        .iter()
        .map(|s| SubagentJson {
            agent_id: s.agent_id.clone(),
            agent_type: s.agent_type.clone(),
            description: s.description.clone(),
            turns: s.turns,
            output_tokens: s.output_tokens,
            cost: s.cost,
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
        agent_turn_count: result.agent_summary.total_agent_turns as u64,
        agent_output_tokens: result.agent_summary.agent_output_tokens,
        agent_cost: result.agent_summary.agent_cost,
        title: result.title.clone(),
        tags: result.tags.clone(),
        turns,
        subagents,
        plugins: result.plugins.clone(),
        skills: result.skills.clone(),
        hooks: result.hooks.clone(),
        subagent_types: result.subagent_types.clone(),
        workflows: result.workflows.clone(),
        is_orphan: result.is_orphan,
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

/// Build the typed `ProjectsJson` struct from a `ProjectResult`.
fn build_projects_json(projects: &ProjectResult) -> ProjectsJson {
    ProjectsJson {
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
    }
}

pub fn render_projects_json(projects: &ProjectResult) -> String {
    let json = build_projects_json(projects);
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

/// Build the typed `TrendJson` struct from a `TrendResult`.
fn build_trend_json(trend: &TrendResult) -> TrendJson {
    TrendJson {
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
    }
}

pub fn render_trend_json(trend: &TrendResult) -> String {
    let json = build_trend_json(trend);
    serde_json::to_string_pretty(&json).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

// ─── Wrapped JSON ──────────────────────────────────────────────────────────

pub fn render_wrapped_json(result: &WrappedResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

// ─── Heatmap JSON ──────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeatmapJson {
    start_date: String,
    end_date: String,
    /// Percentile thresholds (P25, P50, P75) computed from non-zero days.
    thresholds: [usize; 3],
    daily: Vec<DailyActivityJson>,
    stats: HeatmapStatsJson,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyActivityJson {
    date: String,
    turns: usize,
    cost: f64,
    sessions: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeatmapStatsJson {
    total_days: usize,
    active_days: usize,
    current_streak: usize,
    longest_streak: usize,
    /// `None` when no day in the range has activity.
    #[serde(skip_serializing_if = "Option::is_none")]
    busiest_day: Option<BusiestDayJson>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BusiestDayJson {
    date: String,
    turns: usize,
}

pub fn render_heatmap_json(result: &HeatmapResult) -> String {
    let (p25, p50, p75) = result.thresholds;
    let json = HeatmapJson {
        start_date: result.start_date.to_string(),
        end_date: result.end_date.to_string(),
        thresholds: [p25, p50, p75],
        daily: result
            .daily
            .iter()
            .map(|d| DailyActivityJson {
                date: d.date.to_string(),
                turns: d.turns,
                cost: d.cost,
                sessions: d.sessions,
            })
            .collect(),
        stats: HeatmapStatsJson {
            total_days: result.stats.total_days,
            active_days: result.stats.active_days,
            current_streak: result.stats.current_streak,
            longest_streak: result.stats.longest_streak,
            busiest_day: result.stats.busiest_day.map(|(d, n)| BusiestDayJson {
                date: d.to_string(),
                turns: n,
            }),
        },
    };
    serde_json::to_string_pretty(&json).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_session_id: Option<String>,
}

/// Per-session summary for the HTML dashboard.
#[derive(Serialize)]
pub struct HtmlSessionSummary {
    pub id: String,
    pub project: Option<String>,
    pub turns: usize,
    #[serde(rename = "agentTurnCount")]
    pub agent_turn_count: u64,
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
    // Phase 2: session-level capability inventory. Always emitted as arrays
    // (possibly empty) so the frontend type contract is stable.
    pub subagents: Vec<HtmlSubagentSummary>,
    pub plugins: Vec<PluginUsage>,
    pub skills: Vec<SkillUsage>,
    pub hooks: Vec<HookUsage>,
    /// Per-`agent_type` rollup of `subagents[]` for chip rendering.
    #[serde(rename = "subagentTypes")]
    pub subagent_types: Vec<SubagentTypeAggregate>,
    /// Workflow runs (`agent()` orchestrations, Claude Code 2.1.159+) for this
    /// session. Always emitted (possibly empty). Each entry combines the run
    /// snapshot (workflowName/status/durationMs/agentCount/totalTokens/phases)
    /// with measured parsed totals (parsedAgentCount/parsedTurns/
    /// parsedOutputTokens/parsedCost). WorkflowSummary serializes to camelCase.
    pub workflows: Vec<WorkflowSummary>,
    /// Orphan session: scanner reconstructed this session from subagent
    /// jsonl files only (parent jsonl deleted). Totals still include it.
    #[serde(rename = "isOrphan")]
    pub is_orphan: bool,
}

/// Per-subagent summary for the HTML dashboard. Mirrors `SubagentJson` but
/// lives under `HtmlSessionSummary` rather than the standalone `session`
/// subcommand output.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HtmlSubagentSummary {
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub turns: usize,
    pub output_tokens: u64,
    pub cost: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_timestamp: Option<String>,
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
#[allow(clippy::too_many_arguments)]
pub fn render_html_payload(
    overview: &OverviewResult,
    projects: &ProjectResult,
    trend: &TrendResult,
    sessions: &[SessionData],
    calc: &PricingCalculator,
    wrapped: Option<&WrappedResult>,
    active_session_id: Option<&str>,
    claude_home: &std::path::Path,
) -> String {
    // Build typed structs and convert directly to serde_json::Value
    let overview_json: serde_json::Value =
        serde_json::to_value(build_overview_json(overview)).unwrap_or(serde_json::Value::Null);
    let projects_json: serde_json::Value =
        serde_json::to_value(build_projects_json(projects)).unwrap_or(serde_json::Value::Null);
    let trends_json: serde_json::Value =
        serde_json::to_value(build_trend_json(trend)).unwrap_or(serde_json::Value::Null);

    // Build per-session summaries
    let session_summaries: Vec<HtmlSessionSummary> = sessions
        .iter()
        .map(|s| build_html_session_summary(s, calc, claude_home))
        .collect();

    // Build heatmap by aggregating sessions per date
    let heatmap = build_heatmap(sessions, calc);

    // Build wrapped data if available
    let wrapped_json: Option<serde_json::Value> =
        wrapped.and_then(|w| serde_json::to_value(w).ok());

    let payload = HtmlReportPayload {
        overview: overview_json,
        projects: projects_json,
        trends: trends_json,
        sessions: session_summaries,
        heatmap,
        wrapped: wrapped_json,
        active_session_id: active_session_id.map(|s| s.to_string()),
    };

    serde_json::to_string(&payload).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

/// Build an `HtmlSessionSummary` from a single `SessionData`.
fn build_html_session_summary(
    session: &SessionData,
    calc: &PricingCalculator,
    claude_home: &std::path::Path,
) -> HtmlSessionSummary {
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

    let subagents: Vec<HtmlSubagentSummary> = session
        .subagents
        .iter()
        .map(|sa| {
            let mut output_tokens: u64 = 0;
            let mut sa_cost = 0.0f64;
            for t in &sa.turns {
                output_tokens += t.usage.output_tokens.unwrap_or(0);
                sa_cost += calc.calculate_turn_cost(&t.model, &t.usage).total;
            }
            HtmlSubagentSummary {
                agent_id: sa.agent_id.clone(),
                agent_type: sa.agent_type.clone(),
                description: sa.description.clone(),
                turns: sa.turns.len(),
                output_tokens,
                cost: sa_cost,
                first_timestamp: sa.first_timestamp.map(|t| t.to_rfc3339()),
                last_timestamp: sa.last_timestamp.map(|t| t.to_rfc3339()),
            }
        })
        .collect();

    let subagent_types = session.subagent_type_aggregates(calc);
    let workflows = crate::analysis::session::build_workflow_summaries(session, calc, claude_home);

    HtmlSessionSummary {
        id: session.session_id.clone(),
        project: session.project.as_deref().map(project_display_name),
        turns: turn_count,
        agent_turn_count: agent_turn_count as u64,
        cost: total_cost,
        duration_minutes,
        model: primary_model,
        cache_hit_rate,
        first_timestamp: session.first_timestamp.map(|t| t.to_rfc3339()),
        last_timestamp: session.last_timestamp.map(|t| t.to_rfc3339()),
        title: session.metadata.title.clone(),
        tags: session.metadata.tags.clone(),
        mode: session.metadata.mode.clone(),
        subagents,
        plugins: session.plugins.clone(),
        skills: session.skills.clone(),
        hooks: session.hooks.clone(),
        subagent_types,
        workflows,
        is_orphan: session.is_orphan,
    }
}

/// Aggregate sessions by date to build heatmap data.
///
/// Each turn is attributed to its own local-time date (mirrors
/// `analysis::heatmap::analyze_heatmap`). Earlier versions of this function
/// dropped all of a multi-day session's turns onto its `first_timestamp`
/// date, producing inflated single-day buckets for long sessions.
fn build_heatmap(sessions: &[SessionData], calc: &PricingCalculator) -> HeatmapPayload {
    // date -> (turns, cost, session_count)
    let mut daily_map: HashMap<String, (usize, f64, usize)> = HashMap::new();

    for session in sessions {
        // Session is counted on the date of its `first_timestamp` (one
        // session = one start). Turns and cost are attributed per-turn below.
        if let Some(ts) = session.first_timestamp {
            let local = ts.with_timezone(&chrono::Local);
            let date_key = format!(
                "{:04}-{:02}-{:02}",
                local.year(),
                local.month(),
                local.day()
            );
            daily_map.entry(date_key).or_insert((0, 0.0, 0)).2 += 1;
        }

        for turn in session.all_responses() {
            let local = turn.timestamp.with_timezone(&chrono::Local);
            let date_key = format!(
                "{:04}-{:02}-{:02}",
                local.year(),
                local.month(),
                local.day()
            );
            let entry = daily_map.entry(date_key).or_insert((0, 0.0, 0));
            entry.0 += 1;
            entry.1 += calc.calculate_turn_cost(&turn.model, &turn.usage).total;
        }
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::heatmap::analyze_heatmap;
    use crate::data::models::{
        DataQuality, SessionData, SessionMetadata, TokenUsage, ValidatedTurn,
    };
    use chrono::{DateTime, Local, TimeZone, Utc};

    fn make_turn(ts: &str) -> ValidatedTurn {
        ValidatedTurn {
            uuid: format!("u-{ts}"),
            request_id: Some(format!("r-{ts}")),
            timestamp: ts.parse::<DateTime<Utc>>().unwrap(),
            model: "claude-sonnet-4-20250514".into(),
            usage: TokenUsage {
                input_tokens: Some(10),
                output_tokens: Some(20),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            },
            stop_reason: None,
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

    fn make_session(id: &str, turns: Vec<ValidatedTurn>) -> SessionData {
        let first = turns.iter().map(|t| t.timestamp).min();
        let last = turns.iter().map(|t| t.timestamp).max();
        SessionData {
            session_id: id.into(),
            project: Some("test".into()),
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

    /// Bug regression: `build_heatmap` used to attribute *all* of a session's
    /// turns to `first_timestamp.date`. A long-running session that spans two
    /// local days must split its turns across those two days, not lump them
    /// onto the start day.
    #[test]
    fn heatmap_html_payload_attributes_turns_per_day() {
        let calc = PricingCalculator::new();
        // Pick noon-local on two consecutive days so the test is timezone-independent.
        let local_today = Local::now().date_naive();
        let day_a = local_today - chrono::Duration::days(2);
        let day_b = local_today - chrono::Duration::days(1);
        // 12:00 local on each day, converted to UTC for the turn timestamp.
        let ts_a: DateTime<Utc> = Local
            .from_local_datetime(&day_a.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let ts_b: DateTime<Utc> = Local
            .from_local_datetime(&day_b.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .with_timezone(&Utc);

        let sessions = vec![make_session(
            "s1",
            vec![
                make_turn(&ts_a.to_rfc3339()),
                make_turn(&ts_a.to_rfc3339()),
                make_turn(&ts_b.to_rfc3339()),
            ],
        )];

        let hm = build_heatmap(&sessions, &calc);
        // Two distinct local dates -> two entries.
        let day_a_str = day_a.to_string();
        let day_b_str = day_b.to_string();
        let entry_a = hm.days.iter().find(|d| d.date == day_a_str).unwrap();
        let entry_b = hm.days.iter().find(|d| d.date == day_b_str).unwrap();
        assert_eq!(
            entry_a.turns, 2,
            "two turns at 12:00 local on day_a must stay on day_a"
        );
        assert_eq!(
            entry_b.turns, 1,
            "one turn at 12:00 local on day_b must be attributed to day_b"
        );
        // The session is counted once on day_a (its first_timestamp date).
        assert_eq!(entry_a.sessions, 1);
        assert_eq!(entry_b.sessions, 0);
    }

    /// `render_heatmap_json` must produce a parseable JSON object with the
    /// expected top-level keys (camelCase) and a `daily` array whose length
    /// matches the heatmap range.
    #[test]
    fn heatmap_json_output_has_expected_shape() {
        let calc = PricingCalculator::new();
        // One turn at 12:00 local yesterday.
        let local_today = Local::now().date_naive();
        let yesterday = local_today - chrono::Duration::days(1);
        let ts: DateTime<Utc> = Local
            .from_local_datetime(&yesterday.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let sessions = vec![make_session("s1", vec![make_turn(&ts.to_rfc3339())])];

        let result = analyze_heatmap(&sessions, &calc, 7);
        let json_str = render_heatmap_json(&result);
        let v: serde_json::Value = serde_json::from_str(&json_str).expect("must parse as JSON");
        assert!(v.get("daily").and_then(|d| d.as_array()).is_some());
        assert!(v.get("startDate").and_then(|s| s.as_str()).is_some());
        assert!(v.get("endDate").and_then(|s| s.as_str()).is_some());
        assert!(v.get("thresholds").and_then(|t| t.as_array()).is_some());
        assert!(v.get("stats").is_some());
        // Daily entries use camelCase too.
        let first = &v["daily"][0];
        assert!(first.get("date").is_some());
        assert!(first.get("turns").is_some());
        assert!(first.get("cost").is_some());
        assert!(first.get("sessions").is_some());
        // Yesterday's bucket has exactly one turn.
        let yesterday_str = yesterday.to_string();
        let y_entry = v["daily"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["date"].as_str() == Some(&yesterday_str))
            .expect("yesterday must be in the heatmap range");
        assert_eq!(y_entry["turns"].as_u64(), Some(1));
        // Busiest day matches.
        let bd = &v["stats"]["busiestDay"];
        assert_eq!(bd["date"].as_str(), Some(yesterday_str.as_str()));
        assert_eq!(bd["turns"].as_u64(), Some(1));
    }

    /// The unified HTML report payload always includes a `heatmap` section
    /// with a `days` array (possibly empty).
    #[test]
    fn html_report_payload_includes_heatmap_section() {
        use crate::analysis::overview::analyze_overview;
        use crate::analysis::project::analyze_projects;
        use crate::analysis::trend::analyze_trend;
        use crate::data::models::GlobalDataQuality;

        let calc = PricingCalculator::new();
        let local_today = Local::now().date_naive();
        let ts: DateTime<Utc> = Local
            .from_local_datetime(&local_today.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let sessions = vec![make_session("s1", vec![make_turn(&ts.to_rfc3339())])];

        let overview = analyze_overview(&sessions, GlobalDataQuality::default(), &calc, None);
        let projects = analyze_projects(&sessions, &calc, 10);
        let trend = analyze_trend(&sessions, &calc, 0, false);
        let payload = render_html_payload(
            &overview,
            &projects,
            &trend,
            &sessions,
            &calc,
            None,
            None,
            std::path::Path::new("/nonexistent-claude-home"),
        );
        let v: serde_json::Value = serde_json::from_str(&payload).expect("must parse as JSON");
        let days = v["heatmap"]["days"]
            .as_array()
            .expect("heatmap.days must be an array");
        assert!(
            days.iter().any(|d| d["turns"].as_u64() == Some(1)),
            "the one turn we wrote must appear in heatmap.days"
        );
    }
}
