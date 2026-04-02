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

export interface AgentBreakdown {
  agent_type: string
  description: string
  turns: number
  output_tokens: number
  cost: number
}

/**
 * Full session detail (mock data / JSON --format output).
 * Contains per-turn detail in `turns: Turn[]`.
 */
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
  mode?: string
  branch?: string
  autonomy_ratio?: number
  api_errors?: number
  service_tier?: string
  agents?: AgentBreakdown[]
  turns: Turn[]
}

/**
 * Lightweight session summary from Rust HtmlSessionSummary.
 * `turns` is a number (count), not an array.
 * Uses `id` instead of `session_id`, `cost` instead of `total_cost`.
 */
export interface HtmlSessionSummary {
  id: string
  project?: string
  turns: number
  agent_turns: number
  cost: number
  duration_minutes?: number
  model?: string
  cache_hit_rate?: number
  first_timestamp?: string
  last_timestamp?: string
  title?: string
  tags?: string[]
  mode?: string
}

/**
 * Union type: sessions array can contain either format.
 * Use helper functions (getSessionId, getTurnCount, etc.) for safe access.
 */
export type SessionEntry = SessionDetail | HtmlSessionSummary

// ─── Session entry helpers ────────────────────────────────────────────────

/** Returns a stable session ID regardless of which format the entry uses. */
export function getSessionId(s: SessionEntry): string {
  return 'session_id' in s ? s.session_id : s.id
}

/** Returns the turn count regardless of whether turns is a number or an array. */
export function getTurnCount(s: SessionEntry): number {
  return Array.isArray(s.turns) ? s.turns.length : s.turns
}

/** Returns total cost (field is `total_cost` in SessionDetail, `cost` in HtmlSessionSummary). */
export function getSessionCost(s: SessionEntry): number {
  return 'total_cost' in s ? s.total_cost : s.cost
}

/** Returns the first timestamp from a session entry. */
export function getFirstTimestamp(s: SessionEntry): string {
  if ('first_timestamp' in s && s.first_timestamp) return s.first_timestamp
  if (Array.isArray(s.turns) && s.turns.length > 0) return s.turns[0].timestamp
  return ''
}

/** True if this entry has full turn-level detail. */
export function hasDetailedTurns(s: SessionEntry): s is SessionDetail {
  return Array.isArray(s.turns)
}

/** Returns the project name, normalizing Option<String> from Rust. */
export function getProject(s: SessionEntry): string {
  return s.project ?? ''
}

/** Returns the model name, normalizing Option<String> from Rust. */
export function getModel(s: SessionEntry): string {
  return ('model' in s ? s.model : '') ?? ''
}

/** Returns the cache hit rate, normalizing Option from Rust. */
export function getCacheHitRate(s: SessionEntry): number {
  return s.cache_hit_rate ?? 0
}

/** Returns duration in minutes, normalizing Option from Rust. */
export function getDurationMinutes(s: SessionEntry): number {
  return s.duration_minutes ?? 0
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
  sessions?: SessionEntry[]
  wrapped?: WrappedData
  heatmap?: HeatmapData
  active_session_id?: string
}

// ─── Navigation ────────────────────────────────────────────────────────────

export type PageName =
  | 'overview'
  | 'trends'
  | 'projects'
  | 'sessions'
  | 'heatmap'
  | 'wrapped'
