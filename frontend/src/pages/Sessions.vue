<script setup lang="ts">
import { ref, computed, onMounted, nextTick } from 'vue'
import KpiCard from '../components/KpiCard.vue'
import DataTable from '../components/DataTable.vue'
import FilterPills from '../components/FilterPills.vue'
import type { Column } from '../components/DataTable.vue'
import type {
  SessionEntry,
  AgentBreakdown,
  PluginUsage,
  SkillUsage,
  HookUsage,
  SubagentTypeAggregate,
  WorkflowSummary,
} from '../types'
import {
  getSessionId,
  getTurnCount,
  getSessionCost,
  getFirstTimestamp as getFirstTs,
  hasDetailedTurns,
  getProject,
  getModel,
  getCacheHitRate,
  getDurationMinutes,
} from '../types'
import { useData } from '../composables/useData'
import { useI18n } from '../composables/useI18n'

const { data } = useData()
const { t, locale } = useI18n()

// ─── Formatters ────────────────────────────────────────────────────────────

function formatCost(n: number): string {
  if (n >= 1000) {
    return '$' + n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }
  return '$' + n.toFixed(2)
}

function formatPercent(n: number): string {
  return n.toFixed(1) + '%'
}

function formatDuration(mins: number): string {
  if (mins >= 60) {
    const h = Math.floor(mins / 60)
    const m = Math.round(mins % 60)
    return `${h}h ${m}m`
  }
  return `${Math.round(mins)}m`
}

function shortenModel(model: string): string {
  return model
    .replace('claude-', '')
    .replace('-20250514', '')
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toString()
}

function formatDate(ts: string): string {
  const d = new Date(ts)
  return d.toLocaleDateString(locale.value === 'zh' ? 'zh-CN' : 'en-US', { month: 'short', day: 'numeric' })
}

function getFirstTimestamp(session: SessionEntry): string {
  return getFirstTs(session)
}

// ─── Chip helpers (Phase 3: subagents/plugins/skills/hooks) ───────────────

/** Truncate long chip labels (>40 chars) with an ellipsis. */
function truncateChipLabel(label: string, max = 40): string {
  if (label.length <= max) return label
  return label.slice(0, max - 1) + '…'
}

function formatDurationMs(ms: number): string {
  if (ms >= 1000) return (ms / 1000).toFixed(1) + 's'
  return ms + 'ms'
}

/**
 * Format a wall-clock duration between two ISO timestamps as a human-readable
 * string (e.g. "12m 34s", "1h 5m"). Returns null if either input is missing or
 * unparsable, or if the range is non-positive — caller should omit the field.
 */
function formatTimeRangeDuration(firstIso?: string, lastIso?: string): string | null {
  if (!firstIso || !lastIso) return null
  const a = Date.parse(firstIso)
  const b = Date.parse(lastIso)
  if (Number.isNaN(a) || Number.isNaN(b)) return null
  const ms = b - a
  if (ms <= 0) return null
  const totalSec = Math.round(ms / 1000)
  if (totalSec < 60) return `${totalSec}s`
  if (totalSec < 3600) {
    const m = Math.floor(totalSec / 60)
    const s = totalSec % 60
    return s > 0 ? `${m}m ${s}s` : `${m}m`
  }
  const h = Math.floor(totalSec / 3600)
  const m = Math.floor((totalSec % 3600) / 60)
  return m > 0 ? `${h}h ${m}m` : `${h}h`
}

/**
 * Chip label for an aggregated subagent type. The session-level `subagent_types`
 * array groups per-agent_id subagents by their `agentType`, so one chip
 * represents N calls of the same type (e.g. `builder × 7`).
 */
function subagentTypeLabel(agg: SubagentTypeAggregate): string {
  return truncateChipLabel(`${agg.agentType} ${t('sessions.chip_times')} ${agg.count}`)
}

/**
 * Tooltip for an aggregated subagent chip. Shows cost, total turns, and
 * up to 3 invocation descriptions; if more exist they're summarized as
 * "...and N more". Newlines separate the lines (modern browsers honor them
 * inside `title=""`).
 */
function subagentTypeTooltip(agg: SubagentTypeAggregate): string {
  const head = [
    `${agg.count} ${t('sessions.chip_calls')}`,
    `${agg.totalTurns} ${t('common.turns')}`,
    formatCost(agg.totalCost),
  ].join(' · ')
  const lines = [head]
  const shown = agg.descriptions.slice(0, 3)
  for (const d of shown) {
    lines.push('• ' + truncateChipLabel(d, 80))
  }
  if (agg.descriptions.length > 3) {
    const more = agg.descriptions.length - 3
    lines.push(t('sessions.chip_and_more').replace('{n}', String(more)))
  }
  return lines.join('\n')
}

function pluginTooltip(p: PluginUsage): string {
  return [
    `${p.turns} ${t('common.turns')}`,
    formatCost(p.cost),
    `${formatTokens(p.inputTokens)} in / ${formatTokens(p.outputTokens)} out`,
  ].join(' · ')
}

function skillTooltip(s: SkillUsage): string {
  return [
    `${s.turns} ${t('common.turns')}`,
    formatCost(s.cost),
    `${formatTokens(s.inputTokens)} in / ${formatTokens(s.outputTokens)} out`,
  ].join(' · ')
}

function hookTooltip(h: HookUsage): string {
  const parts = [
    `${h.invocations} ${t('sessions.hook_invocations_unit')}`,
    `${formatDurationMs(h.totalDurationMs)} total`,
  ]
  if (h.errorCount > 0) parts.push(`${h.errorCount} ${t('sessions.hook_errors_unit')}`)
  if (h.preventedContinuationCount > 0) {
    parts.push(`${h.preventedContinuationCount} ${t('sessions.hook_prevented_unit')}`)
  }
  return parts.join(' · ')
}

// ─── Workflow helpers (script-orchestrated agent runs) ────────────────────

/** Display label for a workflow run: name if present, else the run id. */
function workflowLabel(wf: WorkflowSummary): string {
  return truncateChipLabel(wf.workflowName ?? wf.runId, 60)
}

/**
 * Localized status text. Known statuses (completed/running/failed) get i18n
 * strings; anything else is shown verbatim. Returns null when status is absent.
 */
function workflowStatusText(wf: WorkflowSummary): string | null {
  if (!wf.status) return null
  const key = `sessions.workflow_status_${wf.status.toLowerCase()}`
  const translated = t(key)
  // t() returns the key itself when no message exists → fall back to raw status.
  return translated === key ? wf.status : translated
}

/** CSS modifier class for the status badge, normalized to a known set. */
function workflowStatusClass(wf: WorkflowSummary): string {
  const s = (wf.status ?? '').toLowerCase()
  if (s === 'completed' || s === 'running' || s === 'failed') return `wf-status-${s}`
  return 'wf-status-other'
}

// ─── Data ─────────────────────────────────────────────────────────────────

const sessions = computed<SessionEntry[]>(() => data.sessions ?? [])

// ─── Search & Filters ─────────────────────────────────────────────────────

const activeSessionId = data.active_session_id ?? ''
const searchQuery = ref('')
const modelFilter = ref('all')
const sortBy = ref('cost')

// Keys to auto-expand on mount (for session --latest deep-link)
const initialExpanded = computed<string[]>(() => {
  if (!activeSessionId) return []
  // Find the matching session by full or prefix match
  const match = sessions.value.find(s => {
    const sid = getSessionId(s)
    return sid === activeSessionId || sid.startsWith(activeSessionId)
  })
  return match ? [getSessionId(match)] : []
})

onMounted(() => {
  if (activeSessionId) {
    // Set search to the session ID prefix so the table filters to show it
    searchQuery.value = activeSessionId.slice(0, 8)
    // Sort by date so the targeted session appears prominently
    sortBy.value = 'date'
  }
})

const modelOptions = computed(() => {
  const models = new Set(sessions.value.map(s => {
    const short = shortenModel(getModel(s))
    // Extract family name (opus, sonnet, haiku)
    const parts = short.split('-')
    return parts[0] ?? short
  }))
  const opts = [{ value: 'all', label: t('sessions.filter_all') }]
  for (const m of models) {
    if (m) opts.push({ value: m.toLowerCase(), label: m })
  }
  return opts
})

const sortOptions = computed(() => [
  { value: 'cost', label: t('sessions.sort_by_cost') },
  { value: 'date', label: t('sessions.sort_by_date') },
  { value: 'turns', label: t('sessions.sort_by_turns') },
])

const filteredSessions = computed(() => {
  let result = [...sessions.value]

  // Text search
  const query = searchQuery.value.toLowerCase().trim()
  if (query) {
    result = result.filter(s =>
      getSessionId(s).toLowerCase().includes(query) ||
      getProject(s).toLowerCase().includes(query) ||
      (s.title?.toLowerCase().includes(query) ?? false)
    )
  }

  // Model filter
  if (modelFilter.value !== 'all') {
    result = result.filter(s => {
      const short = shortenModel(getModel(s)).split('-')[0]?.toLowerCase() ?? ''
      return short === modelFilter.value
    })
  }

  // Sort
  switch (sortBy.value) {
    case 'cost':
      result.sort((a, b) => getSessionCost(b) - getSessionCost(a))
      break
    case 'date': {
      result.sort((a, b) => {
        const ta = getFirstTimestamp(a)
        const tb = getFirstTimestamp(b)
        return tb.localeCompare(ta)
      })
      break
    }
    case 'turns': {
      result.sort((a, b) => (getTurnCount(b) + b.agentTurnCount) - (getTurnCount(a) + a.agentTurnCount))
      break
    }
  }

  return result
})

// ─── KPIs ─────────────────────────────────────────────────────────────────

const totalSessions = computed(() => sessions.value.length)

const totalCost = computed(() =>
  sessions.value.reduce((sum, s) => sum + getSessionCost(s), 0)
)

const avgCostPerSession = computed(() =>
  totalSessions.value > 0 ? totalCost.value / totalSessions.value : 0
)

const avgDuration = computed(() => {
  if (totalSessions.value === 0) return 0
  const sum = sessions.value.reduce((acc, s) => acc + getDurationMinutes(s), 0)
  return sum / totalSessions.value
})

// ─── Session Table Columns ────────────────────────────────────────────────

const sessionColumns = computed<Column<SessionEntry>[]>(() => [
  {
    key: 'session_id',
    label: t('sessions.col_session_id'),
    sortable: true,
    align: 'left',
    format: (row: SessionEntry) => getSessionId(row).slice(0, 8),
  },
  {
    key: 'project',
    label: t('sessions.col_project'),
    sortable: true,
    align: 'left',
    format: (row: SessionEntry) => {
      // Clean up project path display
      const name = getProject(row)
        .replace(/^-Users-[^-]+-/, '~/')
        .replace(/-/g, '/')
      const trimmed = name.length > 20 ? '...' + name.slice(-17) : name
      // Mark orphan sessions inline so users can spot them without expanding.
      // Totals still include orphans; this is a display-only flag.
      return ('isOrphan' in row && row.isOrphan) ? `${trimmed} ${t('sessions.orphan_tag')}` : trimmed
    },
  },
  {
    key: 'turns_display',
    label: t('sessions.col_turns'),
    sortable: true,
    align: 'right',
    format: (row: SessionEntry) => {
      const main = getTurnCount(row)
      if (row.agentTurnCount > 0) return `${main + row.agentTurnCount} (+${row.agentTurnCount})`
      return String(main)
    },
  },
  {
    key: 'duration_minutes',
    label: t('sessions.col_duration'),
    sortable: true,
    align: 'right',
    format: (row: SessionEntry) => formatDuration(getDurationMinutes(row)),
  },
  {
    key: 'total_cost',
    label: t('sessions.col_cost'),
    sortable: true,
    align: 'right',
    format: (row: SessionEntry) => formatCost(getSessionCost(row)),
  },
  {
    key: 'model',
    label: t('sessions.col_model'),
    sortable: true,
    align: 'left',
    format: (row: SessionEntry) => shortenModel(getModel(row)),
  },
  {
    key: 'cache_hit_rate',
    label: t('sessions.col_cache_hit'),
    sortable: true,
    align: 'right',
    hideOnNarrow: true,
    format: (row: SessionEntry) => formatPercent(getCacheHitRate(row)),
  },
  {
    key: 'date_display',
    label: t('sessions.col_date'),
    sortable: true,
    align: 'right',
    hideOnNarrow: true,
    format: (row: SessionEntry) => {
      const ts = getFirstTimestamp(row)
      return ts ? formatDate(ts) : '-'
    },
  },
])

// ─── Agent Breakdown Columns ──────────────────────────────────────────────

const agentColumns = computed<Column<AgentBreakdown>[]>(() => [
  {
    key: 'agent_type',
    label: t('sessions.detail_agent_type'),
    sortable: false,
    align: 'left',
  },
  {
    key: 'description',
    label: t('sessions.detail_agent_desc'),
    sortable: false,
    align: 'left',
  },
  {
    key: 'turns',
    label: t('sessions.detail_agent_turns'),
    sortable: false,
    align: 'right',
  },
  {
    key: 'output_tokens',
    label: t('sessions.detail_agent_output'),
    sortable: false,
    align: 'right',
    format: (row: AgentBreakdown) => formatTokens(row.output_tokens),
  },
  {
    key: 'cost',
    label: t('sessions.detail_agent_cost'),
    sortable: false,
    align: 'right',
    format: (row: AgentBreakdown) => formatCost(row.cost),
  },
])
</script>

<template>
  <div class="sessions-page">
    <h1 class="page-title">{{ t('nav.sessions') }}</h1>

    <!-- KPI Cards -->
    <div class="kpi-grid-4">
      <KpiCard
        :value="totalSessions"
        :label="t('sessions.kpi_total_sessions')"
      />
      <KpiCard
        :value="formatCost(totalCost)"
        :label="t('sessions.kpi_total_cost')"
      />
      <KpiCard
        :value="formatCost(avgCostPerSession)"
        :label="t('sessions.kpi_avg_cost')"
      />
      <KpiCard
        :value="formatDuration(avgDuration)"
        :label="t('sessions.kpi_avg_duration')"
      />
    </div>

    <!-- Search & Filters -->
    <div class="card">
      <div class="filter-bar">
        <input
          v-model="searchQuery"
          type="text"
          class="search-input"
          :placeholder="t('sessions.search_placeholder')"
        />
        <div class="filter-group">
          <FilterPills
            v-model="modelFilter"
            :options="modelOptions"
          />
        </div>
        <div class="filter-group">
          <FilterPills
            v-model="sortBy"
            :options="sortOptions"
          />
        </div>
      </div>
    </div>

    <!-- Session Table -->
    <div class="card">
      <h2 class="card-title">
        {{ t('sessions.table_title') }}
        <span class="count-badge">{{ filteredSessions.length }}</span>
      </h2>
      <template v-if="filteredSessions.length > 0">
        <DataTable
          :columns="sessionColumns"
          :rows="filteredSessions"
          :row-key="(row: SessionEntry) => getSessionId(row)"
          :expandable="true"
          :show-rank="false"
          default-sort-key="total_cost"
          default-sort-dir="desc"
          :initial-expanded-keys="initialExpanded"
        >
          <template #expand="{ row }">
            <div class="session-detail">
              <!-- Orphan banner: parent main jsonl was deleted but the
                   subagent files are still on disk. The session's totals are
                   still included in global aggregates. -->
              <div
                v-if="'isOrphan' in row && row.isOrphan"
                class="orphan-banner"
              >
                {{ t('sessions.orphan_banner') }}
              </div>
              <!-- Basic Info -->
              <div class="detail-section info-row">
                <div class="info-item" v-if="row.title">
                  <span class="info-label">{{ t('sessions.detail_title') }}</span>
                  <span class="info-value">{{ row.title }}</span>
                </div>
                <div class="info-item" v-if="row.tags && row.tags.length > 0">
                  <span class="info-label">{{ t('sessions.detail_tags') }}</span>
                  <span class="info-value">
                    <span v-for="tag in row.tags" :key="tag" class="tag-chip">{{ tag }}</span>
                  </span>
                </div>
                <div class="info-item" v-if="row.mode">
                  <span class="info-label">{{ t('sessions.detail_mode') }}</span>
                  <span class="info-value mode-badge" :class="'mode-' + row.mode">{{ row.mode }}</span>
                </div>
                <div class="info-item" v-if="'branch' in row && row.branch">
                  <span class="info-label">{{ t('sessions.detail_branch') }}</span>
                  <span class="info-value branch-name">{{ row.branch }}</span>
                </div>
              </div>

              <!-- Agent Breakdown (only available in SessionDetail format) -->
              <div class="detail-section" v-if="'agents' in row && row.agents && row.agents.length > 0">
                <h3 class="detail-section-title">{{ t('sessions.detail_agent_breakdown') }}</h3>
                <DataTable
                  :columns="agentColumns"
                  :rows="row.agents"
                  row-key="description"
                  :expandable="false"
                  :show-rank="false"
                />
              </div>

              <!-- Metadata Grid -->
              <div class="detail-section">
                <h3 class="detail-section-title">{{ t('sessions.detail_metadata') }}</h3>
                <div class="metadata-grid">
                  <div class="meta-item" v-if="'autonomy_ratio' in row && row.autonomy_ratio != null">
                    <span class="meta-label">{{ t('sessions.detail_autonomy') }}</span>
                    <span class="meta-value">1:{{ row.autonomy_ratio.toFixed(1) }}</span>
                  </div>
                  <div class="meta-item" v-if="'api_errors' in row">
                    <span class="meta-label">{{ t('sessions.detail_api_errors') }}</span>
                    <span class="meta-value" :class="{ 'error-highlight': (row.api_errors ?? 0) > 0 }">{{ row.api_errors ?? 0 }}</span>
                  </div>
                  <div class="meta-item" v-if="'max_context' in row">
                    <span class="meta-label">{{ t('sessions.detail_max_context') }}</span>
                    <span class="meta-value">{{ formatTokens(row.max_context) }}</span>
                  </div>
                  <div class="meta-item" v-if="'compaction_count' in row">
                    <span class="meta-label">{{ t('sessions.detail_compactions') }}</span>
                    <span class="meta-value">{{ row.compaction_count }}</span>
                  </div>
                  <div class="meta-item" v-if="'output_tokens' in row">
                    <span class="meta-label">{{ t('sessions.detail_output_tokens') }}</span>
                    <span class="meta-value">{{ formatTokens(row.output_tokens) }}</span>
                  </div>
                  <div class="meta-item" v-if="'agent_cost' in row">
                    <span class="meta-label">{{ t('sessions.detail_agent_cost_label') }}</span>
                    <span class="meta-value">{{ formatCost(row.agent_cost) }}</span>
                  </div>
                  <div class="meta-item">
                    <span class="meta-label">{{ t('sessions.detail_cache_hit') }}</span>
                    <span class="meta-value">{{ formatPercent(getCacheHitRate(row)) }}</span>
                  </div>
                  <div class="meta-item" v-if="'service_tier' in row && row.service_tier">
                    <span class="meta-label">{{ t('sessions.detail_service_tier') }}</span>
                    <span class="meta-value">{{ row.service_tier }}</span>
                  </div>
                </div>
              </div>

              <!-- Phase 2 capability chips: subagents / plugins / skills / hooks.
                   Empty / missing arrays → no row is rendered (consistent with
                   how PR-link / branch behave above). -->
              <!-- Subagents render aggregated by agentType (Phase 3):
                   `builder × 7` instead of seven per-agent_id chips. Falls
                   back to nothing if the array is missing or empty. -->
              <div
                class="chip-row"
                v-if="row.subagentTypes && row.subagentTypes.length > 0"
              >
                <span class="chip-row-label">{{ t('session.subagents') }}</span>
                <span class="chip-row-list">
                  <span
                    v-for="agg in row.subagentTypes"
                    :key="agg.agentType"
                    class="capability-chip subagent-chip"
                    :title="subagentTypeTooltip(agg)"
                  >{{ subagentTypeLabel(agg) }}</span>
                </span>
              </div>
              <!-- Workflows: script-orchestrated agent runs. Visually distinct
                   from the Task-tool subagent chips above (full cards, not
                   chips). Rendered only when at least one run exists. -->
              <div
                class="detail-section"
                v-if="row.workflows && row.workflows.length > 0"
              >
                <h3 class="detail-section-title">{{ t('session.workflows') }}</h3>
                <div class="workflow-list">
                  <div
                    v-for="wf in row.workflows"
                    :key="wf.runId"
                    class="workflow-card"
                  >
                    <div class="workflow-header">
                      <span class="workflow-name">{{ workflowLabel(wf) }}</span>
                      <span
                        v-if="workflowStatusText(wf)"
                        class="workflow-status"
                        :class="workflowStatusClass(wf)"
                      >{{ workflowStatusText(wf) }}</span>
                    </div>
                    <div class="workflow-stats">
                      <span class="workflow-stat">
                        {{ wf.parsedAgentCount }} {{ t('sessions.workflow_agents_unit') }}
                      </span>
                      <span class="workflow-stat">
                        {{ wf.parsedTurns }} {{ t('sessions.workflow_turns_unit') }}
                      </span>
                      <span class="workflow-stat workflow-stat-cost">
                        {{ formatCost(wf.parsedCost) }}
                      </span>
                      <span class="workflow-stat">
                        {{ formatTokens(wf.parsedOutputTokens) }} out
                      </span>
                      <span
                        v-if="wf.snapshotTotalTokens != null"
                        class="workflow-stat workflow-stat-muted"
                        :title="t('sessions.workflow_snapshot_tokens_note')"
                      >
                        ~{{ formatTokens(wf.snapshotTotalTokens) }} {{ t('sessions.workflow_snapshot_tokens') }}
                      </span>
                    </div>
                    <ol
                      v-if="wf.phases && wf.phases.length > 0"
                      class="workflow-phases"
                    >
                      <li
                        v-for="(phase, i) in wf.phases"
                        :key="i"
                        class="workflow-phase"
                      >
                        <span v-if="phase.title" class="workflow-phase-title">{{ phase.title }}</span>
                        <span v-if="phase.detail" class="workflow-phase-detail">{{ phase.detail }}</span>
                      </li>
                    </ol>
                  </div>
                </div>
              </div>

              <div
                class="chip-row"
                v-if="row.plugins && row.plugins.length > 0"
              >
                <span class="chip-row-label">{{ t('session.plugins') }}</span>
                <span class="chip-row-list">
                  <span
                    v-for="p in row.plugins"
                    :key="p.plugin"
                    class="capability-chip plugin-chip"
                    :title="pluginTooltip(p)"
                  >{{ truncateChipLabel(p.plugin) }}</span>
                </span>
              </div>
              <div
                class="chip-row"
                v-if="row.skills && row.skills.length > 0"
              >
                <span class="chip-row-label">{{ t('session.skills') }}</span>
                <span class="chip-row-list">
                  <span
                    v-for="s in row.skills"
                    :key="s.skill"
                    class="capability-chip skill-chip"
                    :title="skillTooltip(s)"
                  >{{ truncateChipLabel(s.skill) }}</span>
                </span>
              </div>
              <div
                class="chip-row"
                v-if="row.hooks && row.hooks.length > 0"
              >
                <span class="chip-row-label">{{ t('session.hooks') }}</span>
                <span class="chip-row-list">
                  <span
                    v-for="h in row.hooks"
                    :key="h.command"
                    class="capability-chip hook-chip"
                    :title="hookTooltip(h)"
                  >{{ truncateChipLabel(h.command) }}</span>
                </span>
              </div>
            </div>
          </template>
        </DataTable>
      </template>
      <p v-else class="no-sessions">{{ t('sessions.no_sessions') }}</p>
    </div>
  </div>
</template>

<style scoped>
.sessions-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.page-title {
  font-size: 1.5rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
}

/* ─── KPI Grid ────────────────────────────────────────────────────────────── */

.kpi-grid-4 {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
}

@media (max-width: 768px) {
  .kpi-grid-4 {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 480px) {
  .kpi-grid-4 {
    grid-template-columns: 1fr;
  }
}

/* ─── Card ────────────────────────────────────────────────────────────────── */

.card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 20px 24px;
}

.card-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 16px;
  display: flex;
  align-items: center;
  gap: 8px;
}

.count-badge {
  font-size: 0.75rem;
  font-weight: 500;
  color: var(--text-tertiary);
  background: var(--bg-tertiary);
  border-radius: 10px;
  padding: 2px 8px;
}

/* ─── Filter Bar ──────────────────────────────────────────────────────────── */

.filter-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
}

.search-input {
  flex: 1 1 200px;
  min-width: 180px;
  padding: 8px 14px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: 0.85rem;
  font-family: inherit;
  outline: none;
  transition: border-color 0.15s ease;
}

.search-input::placeholder {
  color: var(--text-tertiary);
}

.search-input:focus {
  border-color: var(--text-secondary);
}

.filter-group {
  flex-shrink: 0;
}

/* ─── Session Detail (Expanded) ───────────────────────────────────────────── */

.session-detail {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.detail-section {
  padding: 0;
}

.detail-section-title {
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--text-secondary);
  margin: 0 0 10px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

/* ─── Info Row ────────────────────────────────────────────────────────────── */

.info-row {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  padding: 12px 16px;
  background: var(--bg-tertiary);
  border-radius: 8px;
}

.info-item {
  display: flex;
  align-items: center;
  gap: 6px;
}

.info-label {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.info-value {
  font-size: 0.85rem;
  color: var(--text-primary);
}

/* Tags */

.tag-chip {
  display: inline-block;
  padding: 2px 8px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  font-size: 0.75rem;
  color: var(--text-secondary);
  margin-right: 4px;
}

/* Mode */

.mode-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 0.75rem;
  font-weight: 600;
}

.mode-agent {
  background: rgba(139, 92, 246, 0.15);
  color: #a78bfa;
}

.mode-normal {
  background: rgba(59, 130, 246, 0.15);
  color: #60a5fa;
}

/* Branch */

.branch-name {
  font-family: 'SF Mono', 'Monaco', 'Menlo', monospace;
  font-size: 0.8rem;
  padding: 2px 6px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 4px;
}

/* ─── Metadata Grid ───────────────────────────────────────────────────────── */

.metadata-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 12px;
}

.meta-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px 12px;
  background: var(--bg-tertiary);
  border-radius: 8px;
}

.meta-label {
  font-size: 0.7rem;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.meta-value {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--text-primary);
}

.error-highlight {
  color: #ef4444;
}

/* ─── Capability chip rows (Phase 3) ──────────────────────────────────────── */

.chip-row {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
  padding: 8px 12px;
  background: var(--bg-tertiary);
  border-radius: 8px;
}

.chip-row-label {
  font-size: 0.7rem;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  flex-shrink: 0;
}

.chip-row-list {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 6px;
}

.capability-chip {
  display: inline-block;
  padding: 2px 8px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  font-size: 0.75rem;
  color: var(--text-secondary);
  font-family: 'SF Mono', 'Monaco', 'Menlo', monospace;
  white-space: nowrap;
  cursor: default;
}

.subagent-chip {
  border-color: rgba(139, 92, 246, 0.4);
  color: #a78bfa;
}

.plugin-chip {
  border-color: rgba(59, 130, 246, 0.4);
  color: #60a5fa;
}

.skill-chip {
  border-color: rgba(16, 185, 129, 0.4);
  color: #34d399;
}

.hook-chip {
  border-color: rgba(245, 158, 11, 0.4);
  color: #fbbf24;
}

/* ─── Workflows (script-orchestrated runs) ────────────────────────────────── */

.workflow-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* Cyan accent + left rail to visually separate orchestrated workflows from the
   purple Task-tool subagent chips. */
.workflow-card {
  padding: 12px 14px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-left: 3px solid #22d3ee;
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.workflow-header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
}

.workflow-name {
  font-family: 'SF Mono', 'Monaco', 'Menlo', monospace;
  font-size: 0.85rem;
  font-weight: 600;
  color: #67e8f9;
}

.workflow-status {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 10px;
  font-size: 0.68rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.wf-status-completed {
  background: rgba(16, 185, 129, 0.15);
  color: #34d399;
}

.wf-status-running {
  background: rgba(59, 130, 246, 0.15);
  color: #60a5fa;
}

.wf-status-failed {
  background: rgba(239, 68, 68, 0.15);
  color: #f87171;
}

.wf-status-other {
  background: var(--bg-secondary);
  color: var(--text-secondary);
}

.workflow-stats {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 14px;
  font-size: 0.78rem;
  color: var(--text-secondary);
}

.workflow-stat {
  white-space: nowrap;
}

.workflow-stat-cost {
  font-weight: 600;
  color: var(--text-primary);
}

.workflow-stat-muted {
  color: var(--text-tertiary);
  font-style: italic;
  cursor: help;
}

.workflow-phases {
  list-style: decimal;
  margin: 0;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.workflow-phase {
  font-size: 0.78rem;
  color: var(--text-secondary);
}

.workflow-phase-title {
  font-weight: 600;
  color: var(--text-primary);
}

.workflow-phase-detail {
  margin-left: 6px;
  color: var(--text-tertiary);
}

/* ─── Orphan Banner ───────────────────────────────────────────────────────── */

.orphan-banner {
  padding: 8px 12px;
  background: rgba(245, 158, 11, 0.1);
  border: 1px solid rgba(245, 158, 11, 0.4);
  border-radius: 8px;
  color: #fbbf24;
  font-size: 0.75rem;
  font-weight: 500;
}

/* ─── No Sessions ─────────────────────────────────────────────────────────── */

.no-sessions {
  color: var(--text-tertiary);
  font-size: 0.85rem;
  font-style: italic;
  margin: 0;
  padding: 8px 0;
  text-align: center;
}
</style>
