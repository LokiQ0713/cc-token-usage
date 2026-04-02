<script setup lang="ts">
import { computed, ref } from 'vue'
import KpiCard from '../components/KpiCard.vue'
import ComboChart from '../components/ComboChart.vue'
import BarChart from '../components/BarChart.vue'
import LineChart from '../components/LineChart.vue'
import { useData } from '../composables/useData'
import { useI18n } from '../composables/useI18n'

const { data } = useData()
const { t } = useI18n()

// ─── Controls ─────────────────────────────────────────────────────────────

type Granularity = 'daily' | 'monthly'
const granularity = ref<Granularity>('daily')
const logScale = ref(false)

// ─── Computed Data ────────────────────────────────────────────────────────

const entries = computed(() => data.trends?.entries ?? [])

/** Aggregate daily entries into monthly buckets */
const monthlyEntries = computed(() => {
  const map = new Map<string, {
    session_count: number
    turn_count: number
    output_tokens: number
    context_tokens: number
    cost: number
  }>()

  for (const e of entries.value) {
    const month = e.label.slice(0, 7) // "2026-03"
    const cur = map.get(month) ?? {
      session_count: 0,
      turn_count: 0,
      output_tokens: 0,
      context_tokens: 0,
      cost: 0,
    }
    cur.session_count += e.session_count
    cur.turn_count += e.turn_count
    cur.output_tokens += e.output_tokens
    cur.context_tokens += e.context_tokens
    cur.cost += e.cost
    map.set(month, cur)
  }

  return Array.from(map.entries()).map(([label, v]) => ({
    label,
    session_count: v.session_count,
    turn_count: v.turn_count,
    output_tokens: v.output_tokens,
    context_tokens: v.context_tokens,
    cost: round2(v.cost),
    cost_per_turn: v.turn_count > 0 ? round4(v.cost / v.turn_count) : 0,
  }))
})

const displayEntries = computed(() =>
  granularity.value === 'daily' ? entries.value : monthlyEntries.value,
)

// ─── Chart Data ───────────────────────────────────────────────────────────

const labels = computed(() =>
  displayEntries.value.map(e => formatLabel(e.label)),
)

const costValues = computed(() =>
  displayEntries.value.map(e => e.cost),
)

const turnValues = computed(() =>
  displayEntries.value.map(e => e.turn_count),
)

const sessionValues = computed(() =>
  displayEntries.value.map(e => e.session_count),
)

const costPerTurnValues = computed(() =>
  displayEntries.value.map(e => e.cost_per_turn),
)

/** Detect extreme cost values (> P90) to highlight in red */
const extremeIndices = computed(() => {
  const costs = [...costValues.value].sort((a, b) => a - b)
  if (costs.length < 3) return []
  const p90Index = Math.floor(costs.length * 0.9)
  const p90 = costs[p90Index]
  return costValues.value
    .map((v, i) => (v >= p90 && v > 0 ? i : -1))
    .filter(i => i >= 0)
})

// ─── Summary KPIs ─────────────────────────────────────────────────────────

const totalCost = computed(() =>
  displayEntries.value.reduce((a, e) => a + e.cost, 0),
)

const totalTurns = computed(() =>
  displayEntries.value.reduce((a, e) => a + e.turn_count, 0),
)

const avgPeriodCost = computed(() => {
  const n = displayEntries.value.length
  return n > 0 ? totalCost.value / n : 0
})

const avgCostPerTurn = computed(() => {
  return totalTurns.value > 0 ? totalCost.value / totalTurns.value : 0
})

// ─── Formatters ───────────────────────────────────────────────────────────

function formatCost(n: number): string {
  if (n >= 1000) {
    return '$' + n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }
  return '$' + n.toFixed(2)
}

function formatLabel(label: string): string {
  // For daily: "03-21" style; for monthly: "Mar 2026"
  if (label.length === 10) {
    // daily label like "2026-03-21" => "03/21"
    return label.slice(5)
  }
  // monthly label like "2026-03" => "2026-03"
  return label
}

function round2(n: number): number {
  return Math.round(n * 100) / 100
}

function round4(n: number): number {
  return Math.round(n * 10000) / 10000
}
</script>

<template>
  <div class="trends-page">
    <h1 class="page-title">{{ t('trends.title') }}</h1>

    <!-- Control Bar -->
    <div class="control-bar">
      <div class="toggle-group">
        <button
          class="toggle-btn"
          :class="{ active: granularity === 'daily' }"
          @click="granularity = 'daily'"
        >
          {{ t('trends.daily') }}
        </button>
        <button
          class="toggle-btn"
          :class="{ active: granularity === 'monthly' }"
          @click="granularity = 'monthly'"
        >
          {{ t('trends.monthly') }}
        </button>
      </div>
      <div class="toggle-group">
        <button
          class="toggle-btn"
          :class="{ active: !logScale }"
          @click="logScale = false"
        >
          {{ t('trends.linear_scale') }}
        </button>
        <button
          class="toggle-btn"
          :class="{ active: logScale }"
          @click="logScale = true"
        >
          {{ t('trends.log_scale') }}
        </button>
      </div>
    </div>

    <!-- Row 1: Main Combo Chart — Cost (Bar) + Turns (Line) -->
    <div class="card">
      <h2 class="card-title">{{ t('trends.usage_trend') }}</h2>
      <ComboChart
        :labels="labels"
        :bar-values="costValues"
        :line-values="turnValues"
        :bar-label="t('trends.cost')"
        :line-label="t('trends.turns')"
        bar-color="#3b82f6"
        line-color="#f59e0b"
        :format-bar="(v: number) => formatCost(v)"
        :format-line="(v: number) => v.toLocaleString() + ' ' + t('common.turns')"
        :bar-y-label="t('trends.cost')"
        :line-y-label="t('trends.turns')"
        :extreme-indices="extremeIndices"
        :log-scale="logScale"
      />
    </div>

    <!-- Row 2: Two-column charts -->
    <div class="panels-grid">
      <!-- Sessions per Day/Month -->
      <div class="card">
        <h2 class="card-title">
          {{ granularity === 'daily' ? t('trends.sessions_per_day') : t('trends.sessions_per_month') }}
        </h2>
        <BarChart
          :labels="labels"
          :values="sessionValues"
          :label="t('trends.sessions')"
          color="#06b6d4"
          :format-value="(v: number) => v.toLocaleString()"
          :y-label="t('trends.sessions')"
        />
      </div>

      <!-- Cost per Turn Trend -->
      <div class="card">
        <h2 class="card-title">{{ t('trends.cost_per_turn_trend') }}</h2>
        <LineChart
          :labels="labels"
          :values="costPerTurnValues"
          :label="t('trends.cost_per_turn')"
          color="#8b5cf6"
          :format-value="(v: number) => '$' + v.toFixed(3)"
          :y-label="t('trends.cost_per_turn')"
        />
      </div>
    </div>

    <!-- Row 3: Summary KPI Cards -->
    <div class="card">
      <h2 class="card-title">{{ t('trends.summary') }}</h2>
      <div class="kpi-grid-4">
        <KpiCard
          :value="formatCost(totalCost)"
          :label="t('trends.total_cost')"
        />
        <KpiCard
          :value="formatCost(avgPeriodCost)"
          :label="granularity === 'daily' ? t('trends.avg_daily_cost') : t('trends.avg_monthly_cost')"
        />
        <KpiCard
          :value="totalTurns.toLocaleString()"
          :label="t('trends.total_turns')"
        />
        <KpiCard
          :value="formatCost(avgCostPerTurn)"
          :label="t('trends.avg_cost_per_turn')"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.trends-page {
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

/* ─── Control Bar ──────────────────────────────────────────────────────── */

.control-bar {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}

.toggle-group {
  display: flex;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  overflow: hidden;
}

.toggle-btn {
  padding: 6px 14px;
  border: none;
  background: var(--bg-secondary);
  color: var(--text-secondary);
  font-size: 0.8rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
}

.toggle-btn:not(:last-child) {
  border-right: 1px solid var(--border-color);
}

.toggle-btn.active {
  background: var(--text-accent);
  color: #fff;
}

.toggle-btn:hover:not(.active) {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

/* ─── Card ─────────────────────────────────────────────────────────────── */

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

/* ─── Panels Grid (2-column) ───────────────────────────────────────────── */

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

/* ─── KPI Grid (4-column inside card) ──────────────────────────────────── */

.kpi-grid-4 {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
}

@media (max-width: 800px) {
  .kpi-grid-4 {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 500px) {
  .kpi-grid-4 {
    grid-template-columns: 1fr;
  }
}
</style>
