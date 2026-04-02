<script setup lang="ts">
import { ref, computed } from 'vue'
import KpiCard from '../components/KpiCard.vue'
import DataTable from '../components/DataTable.vue'
import FilterPills from '../components/FilterPills.vue'
import type { Column } from '../components/DataTable.vue'
import type { SessionDetail, AgentBreakdown } from '../types'
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

function getFirstTimestamp(session: SessionDetail): string {
  if (session.turns.length > 0) return session.turns[0].timestamp
  return ''
}

// ─── Data ─────────────────────────────────────────────────────────────────

const sessions = computed<SessionDetail[]>(() => data.sessions ?? [])

// ─── Search & Filters ─────────────────────────────────────────────────────

const searchQuery = ref('')
const modelFilter = ref('all')
const sortBy = ref('cost')

const modelOptions = computed(() => {
  const models = new Set(sessions.value.map(s => {
    const short = shortenModel(s.model)
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
      s.session_id.toLowerCase().includes(query) ||
      s.project.toLowerCase().includes(query) ||
      (s.title?.toLowerCase().includes(query) ?? false)
    )
  }

  // Model filter
  if (modelFilter.value !== 'all') {
    result = result.filter(s => {
      const short = shortenModel(s.model).split('-')[0]?.toLowerCase() ?? ''
      return short === modelFilter.value
    })
  }

  // Sort
  switch (sortBy.value) {
    case 'cost':
      result.sort((a, b) => b.total_cost - a.total_cost)
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
      const getTotalTurns = (s: SessionDetail) => s.turns.length + s.agent_turns
      result.sort((a, b) => getTotalTurns(b) - getTotalTurns(a))
      break
    }
  }

  return result
})

// ─── KPIs ─────────────────────────────────────────────────────────────────

const totalSessions = computed(() => sessions.value.length)

const totalCost = computed(() =>
  sessions.value.reduce((sum, s) => sum + s.total_cost, 0)
)

const avgCostPerSession = computed(() =>
  totalSessions.value > 0 ? totalCost.value / totalSessions.value : 0
)

const avgDuration = computed(() => {
  if (totalSessions.value === 0) return 0
  const sum = sessions.value.reduce((acc, s) => acc + s.duration_minutes, 0)
  return sum / totalSessions.value
})

// ─── Session Table Columns ────────────────────────────────────────────────

const sessionColumns = computed<Column<SessionDetail>[]>(() => [
  {
    key: 'session_id',
    label: t('sessions.col_session_id'),
    sortable: true,
    align: 'left',
    format: (row: SessionDetail) => row.session_id.slice(0, 8),
  },
  {
    key: 'project',
    label: t('sessions.col_project'),
    sortable: true,
    align: 'left',
    format: (row: SessionDetail) => {
      // Clean up project path display
      const name = row.project
        .replace(/^-Users-[^-]+-/, '~/')
        .replace(/-/g, '/')
      return name.length > 20 ? '...' + name.slice(-17) : name
    },
  },
  {
    key: 'turns_display',
    label: t('sessions.col_turns'),
    sortable: true,
    align: 'right',
    format: (row: SessionDetail) => {
      const main = row.turns.length
      if (row.agent_turns > 0) return `${main + row.agent_turns} (+${row.agent_turns})`
      return String(main)
    },
  },
  {
    key: 'duration_minutes',
    label: t('sessions.col_duration'),
    sortable: true,
    align: 'right',
    format: (row: SessionDetail) => formatDuration(row.duration_minutes),
  },
  {
    key: 'total_cost',
    label: t('sessions.col_cost'),
    sortable: true,
    align: 'right',
    format: (row: SessionDetail) => formatCost(row.total_cost),
  },
  {
    key: 'model',
    label: t('sessions.col_model'),
    sortable: true,
    align: 'left',
    format: (row: SessionDetail) => shortenModel(row.model),
  },
  {
    key: 'cache_hit_rate',
    label: t('sessions.col_cache_hit'),
    sortable: true,
    align: 'right',
    hideOnNarrow: true,
    format: (row: SessionDetail) => formatPercent(row.cache_hit_rate),
  },
  {
    key: 'date_display',
    label: t('sessions.col_date'),
    sortable: true,
    align: 'right',
    hideOnNarrow: true,
    format: (row: SessionDetail) => {
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
          row-key="session_id"
          :expandable="true"
          :show-rank="false"
          default-sort-key="total_cost"
          default-sort-dir="desc"
        >
          <template #expand="{ row }">
            <div class="session-detail">
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
                <div class="info-item" v-if="row.branch">
                  <span class="info-label">{{ t('sessions.detail_branch') }}</span>
                  <span class="info-value branch-name">{{ row.branch }}</span>
                </div>
              </div>

              <!-- Agent Breakdown -->
              <div class="detail-section" v-if="row.agents && row.agents.length > 0">
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
                  <div class="meta-item" v-if="row.autonomy_ratio != null">
                    <span class="meta-label">{{ t('sessions.detail_autonomy') }}</span>
                    <span class="meta-value">1:{{ row.autonomy_ratio.toFixed(1) }}</span>
                  </div>
                  <div class="meta-item">
                    <span class="meta-label">{{ t('sessions.detail_api_errors') }}</span>
                    <span class="meta-value" :class="{ 'error-highlight': (row.api_errors ?? 0) > 0 }">{{ row.api_errors ?? 0 }}</span>
                  </div>
                  <div class="meta-item">
                    <span class="meta-label">{{ t('sessions.detail_max_context') }}</span>
                    <span class="meta-value">{{ formatTokens(row.max_context) }}</span>
                  </div>
                  <div class="meta-item">
                    <span class="meta-label">{{ t('sessions.detail_compactions') }}</span>
                    <span class="meta-value">{{ row.compaction_count }}</span>
                  </div>
                  <div class="meta-item">
                    <span class="meta-label">{{ t('sessions.detail_output_tokens') }}</span>
                    <span class="meta-value">{{ formatTokens(row.output_tokens) }}</span>
                  </div>
                  <div class="meta-item">
                    <span class="meta-label">{{ t('sessions.detail_agent_cost_label') }}</span>
                    <span class="meta-value">{{ formatCost(row.agent_cost) }}</span>
                  </div>
                  <div class="meta-item">
                    <span class="meta-label">{{ t('sessions.detail_cache_hit') }}</span>
                    <span class="meta-value">{{ formatPercent(row.cache_hit_rate) }}</span>
                  </div>
                  <div class="meta-item" v-if="row.service_tier">
                    <span class="meta-label">{{ t('sessions.detail_service_tier') }}</span>
                    <span class="meta-value">{{ row.service_tier }}</span>
                  </div>
                </div>
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
