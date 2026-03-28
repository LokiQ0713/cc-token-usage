pub mod overview;
pub mod project;
pub mod session;
pub mod trend;
pub mod validate;

use crate::data::models::{GlobalDataQuality, TokenUsage};
use chrono::{DateTime, NaiveDate, Utc};
use std::collections::HashMap;

// ─── Common Aggregation ──────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct AggregatedTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,     // 保留总量
    pub cache_write_5m_tokens: u64,     // 5分钟TTL缓存写入
    pub cache_write_1h_tokens: u64,     // 1小时TTL缓存写入
    pub cache_read_tokens: u64,
    pub turns: usize,
}

impl AggregatedTokens {
    pub fn add_usage(&mut self, usage: &TokenUsage) {
        self.input_tokens += usage.input_tokens.unwrap_or(0);
        self.output_tokens += usage.output_tokens.unwrap_or(0);
        self.cache_creation_tokens += usage.cache_creation_input_tokens.unwrap_or(0);
        self.cache_read_tokens += usage.cache_read_input_tokens.unwrap_or(0);

        // Extract 5m/1h TTL breakdown from cache_creation detail
        if let Some(ref detail) = usage.cache_creation {
            self.cache_write_5m_tokens += detail.ephemeral_5m_input_tokens.unwrap_or(0);
            self.cache_write_1h_tokens += detail.ephemeral_1h_input_tokens.unwrap_or(0);
        }

        self.turns += 1;
    }

    pub fn context_tokens(&self) -> u64 {
        self.input_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }
}

// ─── Cost Breakdown ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct TurnCostBreakdown {
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_5m_cost: f64,
    pub cache_write_1h_cost: f64,
    pub cache_read_cost: f64,
    pub total: f64,
}

#[derive(Debug, Default)]
pub struct CostByCategory {
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_5m_cost: f64,
    pub cache_write_1h_cost: f64,
    pub cache_read_cost: f64,
}

// ─── Overview ────────────────────────────────────────────────────────────────

pub struct OverviewResult {
    pub total_sessions: usize,
    pub total_turns: usize,
    pub total_agent_turns: usize,
    pub tokens_by_model: HashMap<String, AggregatedTokens>,
    pub cost_by_model: HashMap<String, f64>,
    pub total_cost: f64,
    pub hourly_distribution: [usize; 24],
    pub quality: GlobalDataQuality,
    pub subscription_value: Option<SubscriptionValue>,
    // 新增
    pub weekday_hour_matrix: [[usize; 24]; 7],  // [weekday][hour] -> turn count
    pub tool_counts: Vec<(String, usize)>,       // 工具名 -> 使用次数，排序
    pub cost_by_category: CostByCategory,        // 费用按类别分拆
    pub session_summaries: Vec<SessionSummary>,   // 所有 session 的汇总
    pub total_output_tokens: u64,
    pub total_context_tokens: u64,
    pub avg_cache_hit_rate: f64,
    pub cache_savings: CacheSavings,
}

/// How much money was saved by cache hits vs paying full input price.
#[derive(Debug, Default)]
pub struct CacheSavings {
    pub total_saved: f64,           // $ saved by cache reads
    pub without_cache_cost: f64,    // hypothetical cost if all cache_read charged at base_input
    pub with_cache_cost: f64,       // actual cache_read cost
    pub savings_pct: f64,           // percentage saved
    pub by_model: Vec<(String, f64)>, // model -> savings, sorted desc
}

pub struct SubscriptionValue {
    pub monthly_price: f64,
    pub api_equivalent: f64,
    pub value_multiplier: f64,
}

// ─── Project ─────────────────────────────────────────────────────────────────

pub struct ProjectResult {
    pub projects: Vec<ProjectSummary>,
}

pub struct ProjectSummary {
    pub name: String,
    pub display_name: String,
    pub session_count: usize,
    pub total_turns: usize,
    pub agent_turns: usize,
    pub tokens: AggregatedTokens,
    pub cost: f64,
    pub primary_model: String,
}

// ─── Session ─────────────────────────────────────────────────────────────────

pub struct SessionResult {
    pub session_id: String,
    pub project: String,
    pub turn_details: Vec<TurnDetail>,
    pub agent_summary: AgentSummary,
    pub total_tokens: AggregatedTokens,
    pub total_cost: f64,
    pub stop_reason_counts: HashMap<String, usize>,
    // 新增
    pub duration_minutes: f64,
    pub max_context: u64,
    pub compaction_count: usize,
    pub cache_write_5m_pct: f64,  // 5m TTL 占比
    pub cache_write_1h_pct: f64,  // 1h TTL 占比
    pub model: String,             // 主力模型
}

#[derive(Debug)]
pub struct TurnDetail {
    pub turn_number: usize,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_5m_tokens: u64,   // 5分钟TTL缓存写入
    pub cache_write_1h_tokens: u64,   // 1小时TTL缓存写入
    pub cache_read_tokens: u64,
    pub context_size: u64,
    pub cache_hit_rate: f64,
    pub cost: f64,
    pub cost_breakdown: TurnCostBreakdown, // 费用分拆
    pub stop_reason: Option<String>,
    pub is_agent: bool,
    pub is_compaction: bool,          // 是否是 compaction 事件
    pub context_delta: i64,           // 与上一 turn 的 context 变化
    pub user_text: Option<String>,    // 用户消息文本
    pub assistant_text: Option<String>, // 模型回复文本
    pub tool_names: Vec<String>,      // 使用的工具名
}

#[derive(Debug, Default)]
pub struct AgentSummary {
    pub total_agent_turns: usize,
    pub agent_output_tokens: u64,
    pub agent_cost: f64,
}

// ─── Session Summary ────────────────────────────────────────────────────────

/// Session-level summary for overview reports and session ranking tables.
#[derive(Debug)]
pub struct SessionSummary {
    pub session_id: String,
    pub project_display_name: String,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub duration_minutes: f64,
    pub model: String,              // 主要使用的模型
    pub turn_count: usize,
    pub agent_turn_count: usize,
    pub output_tokens: u64,
    pub context_tokens: u64,
    pub max_context: u64,
    pub cache_hit_rate: f64,        // 平均
    pub cache_write_5m_pct: f64,    // 5m TTL 占比
    pub compaction_count: usize,
    pub cost: f64,
    pub tool_use_count: usize,      // tool_use stop_reason 的次数
    pub top_tools: Vec<(String, usize)>, // 工具名 -> 使用次数，前5
    pub turn_details: Option<Vec<TurnDetail>>, // 仅 top sessions 有详情
}

// ─── Trend ───────────────────────────────────────────────────────────────────

pub struct TrendResult {
    pub entries: Vec<TrendEntry>,
    pub group_label: String, // "Day" or "Month"
}

pub struct TrendEntry {
    pub label: String, // "2026-03-15" or "2026-03"
    pub date: NaiveDate,
    pub session_count: usize,
    pub turn_count: usize,
    pub tokens: AggregatedTokens,
    pub cost: f64,
    pub models: HashMap<String, u64>,
    // 新增
    pub cost_by_category: CostByCategory,
}

// Keep DailyStats as alias for internal use
pub type DailyStats = TrendEntry;
