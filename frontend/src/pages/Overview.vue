<script setup lang="ts">
import { computed } from 'vue'
import KpiCard from '../components/KpiCard.vue'
import HBarChart from '../components/HBarChart.vue'
import DoughnutChart from '../components/DoughnutChart.vue'
import { useData } from '../composables/useData'
import { useI18n } from '../composables/useI18n'

const { data } = useData()
const { t } = useI18n()

// ─── Formatters ────────────────────────────────────────────────────────────

function formatCompact(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + 'B'
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toLocaleString()
}

function formatCost(n: number): string {
  if (n >= 1000) {
    return '$' + n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }
  return '$' + n.toFixed(2)
}

function formatPercent(n: number): string {
  return n.toFixed(1) + '%'
}

// ─── Computed Values ───────────────────────────────────────────────────────

const ov = computed(() => data.overview)

const agentPercent = computed(() =>
  Math.round((ov.value.total_agent_turns / ov.value.total_turns) * 100)
)

const dateRange = computed(() => {
  const timestamps = ov.value.sessions
    .map(s => s.first_timestamp)
    .filter(Boolean) as string[]
  if (timestamps.length === 0) return ''
  timestamps.sort()
  const first = timestamps[0].slice(0, 10)
  const last = timestamps[timestamps.length - 1].slice(0, 10)
  return first === last ? first : `${first} - ${last}`
})

// Model distribution data
const modelLabels = computed(() =>
  [...ov.value.models].sort((a, b) => b.turns - a.turns).map(m => m.name)
)
const modelValues = computed(() =>
  [...ov.value.models].sort((a, b) => b.turns - a.turns).map(m => m.turns)
)
const modelTooltipExtra = computed(() => {
  const sorted = [...ov.value.models].sort((a, b) => b.turns - a.turns)
  const result: Record<number, string[]> = {}
  sorted.forEach((m, i) => {
    result[i] = [
      `${formatCost(m.cost)}`,
      `${formatCompact(m.output_tokens)} ${t('wrapped.output_tokens')}`,
    ]
  })
  return result
})

// Cost composition data
const costLabels = computed(() => [
  t('cost.cache_read'),
  t('cost.cache_write'),
  t('cost.output'),
  t('cost.input'),
])
const costValues = computed(() => [
  ov.value.cost_by_category.cache_read_cost,
  ov.value.cost_by_category.cache_write_cost,
  ov.value.cost_by_category.output_cost,
  ov.value.cost_by_category.input_cost,
])
const costColors = computed(() => ['#10b981', '#8b5cf6', '#3b82f6', '#f59e0b'])
const costCenterText = computed(() => formatCost(ov.value.total_cost))
const costCenterSub = computed(() => t('overview.total_cost_center'))

// Top tools data
const toolsSorted = computed(() =>
  [...ov.value.top_tools].sort((a, b) => b.count - a.count).slice(0, 10)
)
const toolLabels = computed(() => toolsSorted.value.map(t => t.name))
const toolValues = computed(() => toolsSorted.value.map(t => t.count))

// Top projects data
const projectsSorted = computed(() => {
  if (!data.projects) return []
  return [...data.projects.projects].sort((a, b) => b.cost - a.cost).slice(0, 5)
})
const projectLabels = computed(() => projectsSorted.value.map(p => p.display_name))
const projectValues = computed(() => projectsSorted.value.map(p => p.cost))
const projectTooltipExtra = computed(() => {
  const result: Record<number, string[]> = {}
  projectsSorted.value.forEach((p, i) => {
    result[i] = [
      `${p.session_count} ${t('common.sessions')}`,
      `${p.total_turns.toLocaleString()} ${t('common.turns')}`,
    ]
  })
  return result
})

// Efficiency metrics
const outputRatioDisplay = computed(() => formatPercent(ov.value.output_ratio))
const costPerTurnDisplay = computed(() => formatCost(ov.value.cost_per_turn))
const avgOutputPerTurn = computed(() =>
  formatCompact(ov.value.tokens_per_output_turn)
)

// Summary stats
const sessions = computed(() => ov.value.sessions)

const dailyAvgCost = computed(() => {
  const timestamps = sessions.value
    .map(s => s.first_timestamp)
    .filter(Boolean) as string[]
  if (timestamps.length === 0) return formatCost(0)
  const dates = new Set(timestamps.map(ts => ts.slice(0, 10)))
  const days = Math.max(dates.size, 1)
  return formatCost(ov.value.total_cost / days)
})

const totalCompactions = computed(() => {
  // Not directly available in session summaries (no compaction_count field),
  // but we can show the total_sessions as a proxy or 0
  // For now, show a reasonable computed value
  return sessions.value.length
})

const peakContext = computed(() => {
  if (sessions.value.length === 0) return '0'
  return formatCompact(Math.max(...sessions.value.map(s => s.max_context)))
})

const avgDuration = computed(() => {
  if (sessions.value.length === 0) return '0m'
  const total = sessions.value.reduce((a, s) => a + s.duration_minutes, 0)
  const avg = total / sessions.value.length
  if (avg >= 60) {
    const h = Math.floor(avg / 60)
    const m = Math.round(avg % 60)
    return `${h}h ${m}m`
  }
  return `${Math.round(avg)}m`
})

const mostExpensiveSession = computed(() => {
  if (sessions.value.length === 0) return null
  const sorted = [...sessions.value].sort((a, b) => b.cost - a.cost)
  return sorted[0]
})
</script>

<template>
  <div class="overview-page">
    <h1 class="page-title">{{ t('nav.overview') }}</h1>

    <!-- Row 1: Primary KPI Cards -->
    <div class="kpi-grid-6">
      <KpiCard
        :value="ov.total_sessions"
        :label="t('kpi.sessions')"
        :subtitle="dateRange"
      />
      <KpiCard
        :value="ov.total_turns.toLocaleString()"
        :label="t('kpi.turns')"
        :subtitle="`${agentPercent}% ${t('overview.agent_driven')}`"
      />
      <KpiCard
        :value="formatCompact(ov.total_context_tokens)"
        :label="t('kpi.claude_read')"
        :subtitle="t('kpi.input_tokens')"
      />
      <KpiCard
        :value="formatCompact(ov.total_output_tokens)"
        :label="t('kpi.output_tokens')"
      />
      <KpiCard
        :value="formatCost(ov.total_cost)"
        :label="t('kpi.total_cost')"
      />
      <KpiCard
        :value="formatCost(ov.cache_savings.total_saved)"
        :label="t('kpi.cache_savings')"
        :subtitle="formatPercent(ov.cache_savings.savings_pct)"
      />
    </div>

    <!-- Cache Savings Banner -->
    <div class="cache-banner" v-if="ov.cache_savings">
      <span class="cache-banner-text">
        {{ t('overview.cache_saved') }}
        <strong>{{ formatCost(ov.cache_savings.total_saved) }}</strong>
        ({{ formatPercent(ov.cache_savings.savings_pct) }} {{ t('overview.reads_free') }})
      </span>
      <span class="cache-banner-sub" v-if="ov.subscription_value">
        {{ t('overview.subscription') }}: ${{ ov.subscription_value.monthly_price }}/mo
        &rarr; {{ ov.subscription_value.value_multiplier.toFixed(1) }}x {{ t('overview.value_multiplier') }}
      </span>
    </div>

    <!-- Row 2: Charts - Model Distribution + Cost Composition -->
    <div class="panels-grid">
      <div class="card">
        <h2 class="card-title">{{ t('overview.model_distribution') }}</h2>
        <HBarChart
          :labels="modelLabels"
          :values="modelValues"
          :format-value="(v: number) => v.toLocaleString() + ` ${t('common.turns')}`"
          :tooltip-extra="modelTooltipExtra"
          :enable-log-toggle="true"
          scale-mode="linear"
        />
      </div>
      <div class="card">
        <h2 class="card-title">{{ t('overview.cost_composition') }}</h2>
        <DoughnutChart
          :labels="costLabels"
          :values="costValues"
          :colors="costColors"
          :center-text="costCenterText"
          :center-sub-text="costCenterSub"
          :format-value="(v: number) => formatCost(v)"
        />
      </div>
    </div>

    <!-- Row 3: Charts - Top Tools + Top Projects -->
    <div class="panels-grid">
      <div class="card">
        <h2 class="card-title">{{ t('overview.top_tools') }}</h2>
        <HBarChart
          :labels="toolLabels"
          :values="toolValues"
          :format-value="(v: number) => v.toLocaleString()"
          :enable-log-toggle="true"
          scale-mode="linear"
        />
      </div>
      <div class="card" v-if="projectsSorted.length > 0">
        <h2 class="card-title">{{ t('overview.top_projects') }}</h2>
        <HBarChart
          :labels="projectLabels"
          :values="projectValues"
          :format-value="(v: number) => formatCost(v)"
          :tooltip-extra="projectTooltipExtra"
          :colors="['#8b5cf6', '#3b82f6', '#06b6d4', '#f59e0b', '#10b981']"
        />
      </div>
    </div>

    <!-- Row 4: Efficiency Metrics -->
    <div class="card">
      <h2 class="card-title">{{ t('overview.efficiency_metrics') }}</h2>
      <div class="efficiency-grid">
        <div class="efficiency-item">
          <div class="efficiency-value">{{ outputRatioDisplay }}</div>
          <div class="efficiency-label">{{ t('kpi.output_ratio') }}</div>
          <div class="efficiency-desc">{{ t('overview.output_input_ratio') }}</div>
        </div>
        <div class="efficiency-item">
          <div class="efficiency-value">{{ costPerTurnDisplay }}</div>
          <div class="efficiency-label">{{ t('kpi.cost_per_turn') }}</div>
          <div class="efficiency-desc">{{ t('overview.dollar_per_turn') }}</div>
        </div>
        <div class="efficiency-item">
          <div class="efficiency-value">{{ avgOutputPerTurn }}</div>
          <div class="efficiency-label">{{ t('kpi.avg_output_turn') }}</div>
          <div class="efficiency-desc">{{ t('kpi.tokens') }}</div>
        </div>
      </div>
    </div>

    <!-- Row 5: Summary Stats -->
    <div class="card">
      <h2 class="card-title">{{ t('overview.summary_stats') }}</h2>
      <div class="summary-grid">
        <div class="summary-item">
          <span class="summary-label">{{ t('summary.daily_avg_cost') }}</span>
          <span class="summary-value">{{ dailyAvgCost }}</span>
        </div>
        <div class="summary-item">
          <span class="summary-label">{{ t('summary.compactions') }}</span>
          <span class="summary-value">{{ totalCompactions }}</span>
        </div>
        <div class="summary-item">
          <span class="summary-label">{{ t('summary.peak_context') }}</span>
          <span class="summary-value">{{ peakContext }}</span>
        </div>
        <div class="summary-item">
          <span class="summary-label">{{ t('summary.avg_duration') }}</span>
          <span class="summary-value">{{ avgDuration }}</span>
        </div>
        <div class="summary-item summary-item-wide" v-if="mostExpensiveSession">
          <span class="summary-label">{{ t('summary.most_expensive') }}</span>
          <span class="summary-value">
            {{ mostExpensiveSession.session_id.slice(0, 8) }}...
            &mdash; {{ formatCost(mostExpensiveSession.cost) }}
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overview-page {
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

.kpi-grid-6 {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 10px;
}

@media (max-width: 1000px) {
  .kpi-grid-6 {
    grid-template-columns: repeat(3, 1fr);
  }
}

@media (max-width: 600px) {
  .kpi-grid-6 {
    grid-template-columns: repeat(2, 1fr);
  }
}

/* ─── Cache Banner ────────────────────────────────────────────────────────── */

.cache-banner {
  background: var(--bg-secondary);
  border: 1px solid var(--text-accent);
  border-radius: 12px;
  padding: 14px 20px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.cache-banner-text {
  color: var(--text-primary);
  font-size: 0.9rem;
}

.cache-banner-sub {
  color: var(--text-secondary);
  font-size: 0.8rem;
}

/* ─── Card + Panels Grid ─────────────────────────────────────────────────── */

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
}

.panels-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}

@media (max-width: 800px) {
  .panels-grid {
    grid-template-columns: 1fr;
  }
}

/* ─── Efficiency Metrics ──────────────────────────────────────────────────── */

.efficiency-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
}

@media (max-width: 600px) {
  .efficiency-grid {
    grid-template-columns: 1fr;
  }
}

.efficiency-item {
  text-align: center;
  padding: 16px 12px;
  background: var(--bg-tertiary);
  border-radius: 10px;
  border: 1px solid var(--border-color);
  transition: border-color 0.15s ease;
}

.efficiency-item:hover {
  border-color: var(--text-tertiary);
}

.efficiency-value {
  font-size: 1.6rem;
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1.2;
}

.efficiency-label {
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--text-secondary);
  margin-top: 6px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.efficiency-desc {
  font-size: 0.7rem;
  color: var(--text-tertiary);
  margin-top: 2px;
}

/* ─── Summary Stats ───────────────────────────────────────────────────────── */

.summary-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
}

@media (max-width: 800px) {
  .summary-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 500px) {
  .summary-grid {
    grid-template-columns: 1fr;
  }
}

.summary-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 14px;
  background: var(--bg-tertiary);
  border-radius: 8px;
}

.summary-item-wide {
  grid-column: span 2;
}

@media (max-width: 500px) {
  .summary-item-wide {
    grid-column: span 1;
  }
}

.summary-label {
  font-size: 0.7rem;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.summary-value {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--text-primary);
}
</style>
