use serde::Serialize;

use crate::analysis::{OverviewResult, ProjectResult, SessionResult, TrendResult};

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
