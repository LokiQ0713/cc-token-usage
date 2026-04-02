<script setup lang="ts">
import { computed, ref } from 'vue'
import CalendarHeatmap from '../components/CalendarHeatmap.vue'
import KpiCard from '../components/KpiCard.vue'
import { useData } from '../composables/useData'
import { useI18n } from '../composables/useI18n'
import type { HeatmapMetric } from '../components/CalendarHeatmap.vue'

const { data } = useData()
const { t } = useI18n()

// ─── Metric Switcher ────────────────────────────────────────────────────────

const activeMetric = ref<HeatmapMetric>('turns')

const metricOptions: { key: HeatmapMetric; labelKey: string }[] = [
  { key: 'turns', labelKey: 'heatmap.metric_turns' },
  { key: 'cost', labelKey: 'heatmap.metric_cost' },
  { key: 'sessions', labelKey: 'heatmap.metric_sessions' },
]

// ─── Heatmap Data ───────────────────────────────────────────────────────────

const heatmapDays = computed(() => {
  if (data.heatmap?.days) return data.heatmap.days
  return []
})

// ─── Statistics ─────────────────────────────────────────────────────────────

const activeDays = computed(() => {
  return heatmapDays.value.filter(d => d.turns > 0).length
})

const totalContributions = computed(() => {
  return heatmapDays.value.reduce((sum, d) => sum + d.turns, 0)
})

const currentStreak = computed(() => {
  const sorted = [...heatmapDays.value]
    .filter(d => d.date <= '2026-04-02')
    .sort((a, b) => b.date.localeCompare(a.date))

  let streak = 0
  for (const day of sorted) {
    if (day.turns > 0) {
      streak++
    } else {
      break
    }
  }
  return streak
})

const longestStreak = computed(() => {
  const sorted = [...heatmapDays.value].sort((a, b) => a.date.localeCompare(b.date))
  let maxStreak = 0
  let current = 0
  for (const day of sorted) {
    if (day.turns > 0) {
      current++
      maxStreak = Math.max(maxStreak, current)
    } else {
      current = 0
    }
  }
  return maxStreak
})

const busiestDay = computed(() => {
  if (heatmapDays.value.length === 0) return null
  const sorted = [...heatmapDays.value].sort((a, b) => b.turns - a.turns)
  return sorted[0]
})

function formatDate(dateStr: string): string {
  const d = new Date(dateStr + 'T00:00:00')
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
                  'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
  return `${months[d.getMonth()]} ${d.getDate()}`
}

function formatCost(n: number): string {
  return '$' + n.toFixed(2)
}
</script>

<template>
  <div class="heatmap-page">
    <h1 class="page-title">{{ t('nav.heatmap') }}</h1>

    <!-- Contribution summary -->
    <div class="contribution-summary">
      <span class="contribution-count">{{ totalContributions.toLocaleString() }}</span>
      {{ t('heatmap.contributions_in_range') }}
    </div>

    <!-- Metric Switcher -->
    <div class="metric-switcher">
      <button
        v-for="opt in metricOptions"
        :key="opt.key"
        :class="['metric-pill', { active: activeMetric === opt.key }]"
        @click="activeMetric = opt.key"
      >
        {{ t(opt.labelKey) }}
      </button>
    </div>

    <!-- Calendar Heatmap -->
    <div class="card heatmap-card">
      <CalendarHeatmap
        :days="heatmapDays"
        :metric="activeMetric"
      />
    </div>

    <!-- Stats Cards -->
    <div class="card">
      <h2 class="card-title">{{ t('heatmap.stats') }}</h2>
      <div class="stats-grid">
        <KpiCard
          :value="activeDays"
          :label="t('heatmap.active_days')"
          :subtitle="`/ ${heatmapDays.length} ${t('heatmap.days')}`"
        />
        <KpiCard
          :value="`${currentStreak} ${t('heatmap.days')}`"
          :label="t('heatmap.current_streak')"
        />
        <KpiCard
          :value="`${longestStreak} ${t('heatmap.days')}`"
          :label="t('heatmap.longest_streak')"
        />
        <KpiCard
          v-if="busiestDay"
          :value="`${busiestDay.turns} ${t('heatmap.metric_turns').toLowerCase()}`"
          :label="t('heatmap.busiest_day')"
          :subtitle="`${formatDate(busiestDay.date)} - ${formatCost(busiestDay.cost)}`"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.heatmap-page {
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

/* ─── Contribution Summary ────────────────────────────────────────────────── */

.contribution-summary {
  font-size: 0.85rem;
  color: var(--text-secondary);
}

.contribution-count {
  font-weight: 600;
  color: var(--text-primary);
}

/* ─── Metric Switcher ─────────────────────────────────────────────────────── */

.metric-switcher {
  display: flex;
  gap: 4px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 3px;
  width: fit-content;
}

.metric-pill {
  padding: 6px 16px;
  border: none;
  border-radius: 6px;
  font-size: 0.8rem;
  font-weight: 500;
  cursor: pointer;
  background: transparent;
  color: var(--text-tertiary);
  transition: all 0.15s ease;
  font-family: inherit;
}

.metric-pill:hover {
  color: var(--text-secondary);
}

.metric-pill.active {
  background: var(--bg-tertiary);
  color: var(--text-primary);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
}

[data-theme="light"] .metric-pill.active {
  background: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

/* ─── Cards ───────────────────────────────────────────────────────────────── */

.card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 20px 24px;
}

.heatmap-card {
  overflow: hidden;
}

.card-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 16px;
}

/* ─── Stats Grid ──────────────────────────────────────────────────────────── */

.stats-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
}

@media (max-width: 800px) {
  .stats-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 500px) {
  .stats-grid {
    grid-template-columns: 1fr;
  }
}
</style>
