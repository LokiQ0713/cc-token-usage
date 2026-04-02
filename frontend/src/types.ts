// ─── Overview Types ────────────────────────────────────────────────────────

export interface CacheSavings {
  total_saved: number
  savings_pct: number
}

export interface SubscriptionValue {
  monthly_price: number
  api_equivalent: number
  value_multiplier: number
}

export interface CostByCategory {
  input_cost: number
  output_cost: number
  cache_write_cost: number
  cache_read_cost: number
}

export interface Model {
  name: string
  output_tokens: number
  turns: number
  cost: number
}

export interface Tool {
  name: string
  count: number
}

export interface SessionSummary {
  session_id: string
  project: string
  first_timestamp: string | null
  duration_minutes: number
  model: string
  turn_count: number
  agent_turn_count: number
  output_tokens: number
  context_tokens: number
  max_context: number
  cache_hit_rate: number
  cost: number
  output_ratio: number
  cost_per_turn: number
}

export interface OverviewData {
  total_sessions: number
  total_turns: number
  total_agent_turns: number
  total_output_tokens: number
  total_context_tokens: number
  total_cost: number
  avg_cache_hit_rate: number
  output_ratio: number
  cost_per_turn: number
  tokens_per_output_turn: number
  cache_savings: CacheSavings
  subscription_value: SubscriptionValue | null
  cost_by_category: CostByCategory
  models: Model[]
  top_tools: Tool[]
  sessions: SessionSummary[]
}

// ─── Session Detail Types ──────────────────────────────────────────────────

export interface Turn {
  turn_number: number
  timestamp: string
  model: string
  input_tokens: number
  output_tokens: number
  cache_read_tokens: number
  context_size: number
  cache_hit_rate: number
  cost: number
  stop_reason?: string
  is_agent: boolean
  is_compaction: boolean
  tool_names: string[]
}

export interface SessionDetail {
  session_id: string
  project: string
  model: string
  duration_minutes: number
  total_cost: number
  max_context: number
  compaction_count: number
  output_tokens: number
  context_tokens: number
  cache_hit_rate: number
  agent_turns: number
  agent_output_tokens: number
  agent_cost: number
  title?: string
  tags: string[]
  turns: Turn[]
}

// ─── Projects Types ────────────────────────────────────────────────────────

export interface Project {
  name: string
  display_name: string
  session_count: number
  total_turns: number
  agent_turns: number
  output_tokens: number
  context_tokens: number
  cost: number
  primary_model: string
}

export interface ProjectsData {
  projects: Project[]
}

// ─── Trend Types ───────────────────────────────────────────────────────────

export interface TrendEntry {
  label: string
  session_count: number
  turn_count: number
  output_tokens: number
  context_tokens: number
  cost: number
  cost_per_turn: number
}

export interface TrendData {
  group_label: string
  entries: TrendEntry[]
}

// ─── Wrapped Types ─────────────────────────────────────────────────────────

export type DeveloperArchetype =
  | 'Architect'
  | 'Sprinter'
  | 'NightOwl'
  | 'Delegator'
  | 'Explorer'
  | 'Marathoner'

export interface WrappedData {
  year: number
  active_days: number
  total_days: number
  longest_streak: number
  ghost_days: number
  total_sessions: number
  total_turns: number
  total_agent_turns: number
  total_output_tokens: number
  total_input_tokens: number
  total_cost: number
  autonomy_ratio: number
  avg_session_duration_min: number
  avg_cost_per_session: number
  output_ratio: number
  peak_hour: number
  peak_weekday: string
  hourly_distribution: number[]
  weekday_distribution: number[]
  top_projects: [string, number][]
  top_tools: [string, number][]
  most_expensive_session: [string, number, string] | null
  longest_session: [string, number, string] | null
  model_distribution: [string, number][]
  archetype: DeveloperArchetype
  total_pr_count: number
  total_speculation_time_saved_ms: number
  total_collapse_count: number
}

// ─── Heatmap Types ─────────────────────────────────────────────────────────

export interface HeatmapDay {
  date: string
  turns: number
  cost: number
  sessions: number
}

export interface HeatmapData {
  days: HeatmapDay[]
}

// ─── Dashboard Root Type ───────────────────────────────────────────────────

export interface DashboardData {
  overview: OverviewData
  trends?: TrendData
  projects?: ProjectsData
  sessions?: SessionDetail[]
  wrapped?: WrappedData
  heatmap?: HeatmapData
}

// ─── Navigation ────────────────────────────────────────────────────────────

export type PageName =
  | 'overview'
  | 'trends'
  | 'projects'
  | 'sessions'
  | 'heatmap'
  | 'wrapped'
