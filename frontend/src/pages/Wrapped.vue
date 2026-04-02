<script setup lang="ts">
import { computed } from 'vue'
import type { DeveloperArchetype } from '../types'
import HBarChart from '../components/HBarChart.vue'
import { useData } from '../composables/useData'
import { useI18n } from '../composables/useI18n'

const { data } = useData()
const { t } = useI18n()

// ─── Archetype Config ─────────────────────────────────────────────────────

interface ArchetypeStyle {
  color: string
  gradientFrom: string
  gradientTo: string
  descKey: string
}

const archetypeStyles: Record<DeveloperArchetype, ArchetypeStyle> = {
  Architect: {
    color: '#3b82f6',
    gradientFrom: '#1e3a8a',
    gradientTo: '#3b82f6',
    descKey: 'wrapped.archetype_desc.Architect',
  },
  Sprinter: {
    color: '#f59e0b',
    gradientFrom: '#b45309',
    gradientTo: '#f59e0b',
    descKey: 'wrapped.archetype_desc.Sprinter',
  },
  NightOwl: {
    color: '#8b5cf6',
    gradientFrom: '#2e1065',
    gradientTo: '#8b5cf6',
    descKey: 'wrapped.archetype_desc.NightOwl',
  },
  Delegator: {
    color: '#22c55e',
    gradientFrom: '#14532d',
    gradientTo: '#22c55e',
    descKey: 'wrapped.archetype_desc.Delegator',
  },
  Explorer: {
    color: '#06b6d4',
    gradientFrom: '#0e7490',
    gradientTo: '#06b6d4',
    descKey: 'wrapped.archetype_desc.Explorer',
  },
  Marathoner: {
    color: '#ef4444',
    gradientFrom: '#7f1d1d',
    gradientTo: '#ef4444',
    descKey: 'wrapped.archetype_desc.Marathoner',
  },
}

// ─── Formatters ───────────────────────────────────────────────────────────

function formatCost(n: number): string {
  if (n >= 1000) {
    return '$' + n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }
  return '$' + n.toFixed(2)
}

function formatCompact(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + 'B'
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return n.toLocaleString()
}

function formatDuration(minutes: number): string {
  if (minutes >= 60) {
    const h = Math.floor(minutes / 60)
    const m = Math.round(minutes % 60)
    return m > 0 ? `${h}h ${m}m` : `${h}h`
  }
  return `${Math.round(minutes)}m`
}

function formatHour(h: number): string {
  return h.toString().padStart(2, '0') + ':00'
}

// ─── Computed ─────────────────────────────────────────────────────────────

const w = computed(() => data.wrapped!)

const hasWrapped = computed(() => !!data.wrapped)

const archetype = computed(() => w.value.archetype)
const style = computed(() => archetypeStyles[archetype.value] || archetypeStyles.Architect)

const activeDaysPct = computed(() =>
  Math.round((w.value.active_days / Math.max(w.value.total_days, 1)) * 100)
)

const agentPercent = computed(() =>
  Math.round((w.value.total_agent_turns / Math.max(w.value.total_turns, 1)) * 100)
)

const autonomyDisplay = computed(() => w.value.autonomy_ratio.toFixed(1) + 'x')

const avgDuration = computed(() => formatDuration(w.value.avg_session_duration_min))

const avgCostSession = computed(() => formatCost(w.value.avg_cost_per_session))

// Top tools
const toolLabels = computed(() => w.value.top_tools.map(([name]) => name))
const toolValues = computed(() => w.value.top_tools.map(([, count]) => count))

// Top projects
const projectLabels = computed(() => w.value.top_projects.map(([name]) => name))
const projectValues = computed(() => w.value.top_projects.map(([, cost]) => cost))

// Model distribution
const modelLabels = computed(() => w.value.model_distribution.map(([name]) => name))
const modelValues = computed(() => w.value.model_distribution.map(([, turns]) => turns))

// Records
const mostExpensive = computed(() => w.value.most_expensive_session)
const longestSession = computed(() => w.value.longest_session)

const expensivePct = computed(() => {
  if (!mostExpensive.value) return ''
  return ((mostExpensive.value[1] / Math.max(w.value.total_cost, 0.01)) * 100).toFixed(1)
})

const longestHours = computed(() => {
  if (!longestSession.value) return ''
  return (longestSession.value[1] / 60).toFixed(1)
})

</script>

<template>
  <div class="wrapped-page" v-if="hasWrapped">
    <!-- ═══ Hero Section ═══ -->
    <div
      class="hero-card"
      :style="{
        '--archetype-color': style.color,
        '--gradient-from': style.gradientFrom,
        '--gradient-to': style.gradientTo,
      }"
    >
      <div class="hero-year">{{ t('wrapped.hero_title_pre') }} {{ w.year }} {{ t('wrapped.hero_title_suf') }}</div>
      <div class="hero-archetype">{{ t('wrapped.the_archetype.' + archetype) }}</div>
      <div class="hero-desc">{{ t(style.descKey) }}</div>
      <div class="hero-date-range">
        {{ w.active_days }} {{ t('wrapped.active_of') }} {{ w.total_days }} {{ t('wrapped.days_in') }} {{ w.year }}
      </div>
    </div>

    <!-- ═══ Stats Grid (3x2) ═══ -->
    <div class="section-title">{{ t('wrapped.activity_stats') }}</div>
    <div class="stats-grid">
      <!-- Active Days -->
      <div class="stat-card">
        <div class="stat-label">{{ t('wrapped.active_days') }}</div>
        <div class="stat-big">{{ w.active_days }}<span class="stat-dim"> / {{ w.total_days }}</span></div>
        <div class="progress-bar">
          <div
            class="progress-fill"
            :style="{ width: activeDaysPct + '%', background: style.color }"
          ></div>
        </div>
        <div class="stat-sub">{{ activeDaysPct }}%</div>
      </div>

      <!-- Longest Streak -->
      <div class="stat-card">
        <div class="stat-label">{{ t('wrapped.longest_streak') }}</div>
        <div class="stat-big">
          {{ w.longest_streak }}
          <span class="streak-flame"></span>
        </div>
        <div class="stat-sub">{{ t('wrapped.consecutive_days') }}</div>
      </div>

      <!-- Ghost Days -->
      <div class="stat-card">
        <div class="stat-label">{{ t('wrapped.ghost_days') }}</div>
        <div class="stat-big">{{ w.ghost_days }}</div>
        <div class="stat-sub">{{ t('wrapped.days_offline') }}</div>
      </div>

      <!-- Total Sessions -->
      <div class="stat-card">
        <div class="stat-label">{{ t('wrapped.total_sessions') }}</div>
        <div class="stat-big">{{ w.total_sessions.toLocaleString() }}</div>
        <div class="stat-sub">{{ t('wrapped.sessions') }}</div>
      </div>

      <!-- Total Turns -->
      <div class="stat-card">
        <div class="stat-label">{{ t('wrapped.total_turns') }}</div>
        <div class="stat-big">{{ w.total_turns.toLocaleString() }}</div>
        <div class="stat-sub">{{ agentPercent }}% {{ t('wrapped.agent_driven') }}</div>
      </div>

      <!-- Total Cost -->
      <div class="stat-card">
        <div class="stat-label">{{ t('wrapped.total_cost') }}</div>
        <div class="stat-big stat-cost">{{ formatCost(w.total_cost) }}</div>
        <div class="stat-sub">{{ formatCompact(w.total_output_tokens) }} {{ t('wrapped.output_tokens') }}</div>
      </div>
    </div>

    <!-- ═══ Peak Patterns ═══ -->
    <div class="section-title">{{ t('wrapped.peak_patterns') }}</div>
    <div class="peak-grid">
      <div class="peak-card">
        <div class="peak-label">{{ t('wrapped.peak_hour') }}</div>
        <div class="peak-big">{{ formatHour(w.peak_hour) }}</div>
      </div>
      <div class="peak-card">
        <div class="peak-label">{{ t('wrapped.peak_day') }}</div>
        <div class="peak-big">{{ t('wrapped.weekday.' + w.peak_weekday) }}</div>
      </div>
      <div class="peak-card">
        <div class="peak-label">{{ t('wrapped.autonomy_ratio') }}</div>
        <div class="peak-big">{{ autonomyDisplay }}</div>
        <div class="peak-sub">{{ t('wrapped.turns_per_prompt') }}</div>
      </div>
      <div class="peak-card">
        <div class="peak-label">{{ t('wrapped.avg_duration') }}</div>
        <div class="peak-big">{{ avgDuration }}</div>
        <div class="peak-sub">{{ t('wrapped.per_session') }}</div>
      </div>
      <div class="peak-card">
        <div class="peak-label">{{ t('wrapped.avg_cost') }}</div>
        <div class="peak-big">{{ avgCostSession }}</div>
        <div class="peak-sub">{{ t('wrapped.per_session') }}</div>
      </div>
    </div>

    <!-- ═══ Rankings ═══ -->
    <div class="section-title">{{ t('wrapped.rankings') }}</div>
    <div class="panels-grid">
      <div class="card">
        <h2 class="card-title">{{ t('wrapped.top_tools') }}</h2>
        <HBarChart
          :labels="toolLabels"
          :values="toolValues"
          :format-value="(v: number) => v.toLocaleString()"
          :enable-log-toggle="true"
          scale-mode="linear"
        />
      </div>
      <div class="card">
        <h2 class="card-title">{{ t('wrapped.top_projects') }}</h2>
        <HBarChart
          :labels="projectLabels"
          :values="projectValues"
          :format-value="(v: number) => formatCost(v)"
          :colors="['#8b5cf6', '#3b82f6', '#06b6d4', '#f59e0b', '#10b981']"
        />
      </div>
    </div>
    <div class="card" style="margin-top: 16px;">
      <h2 class="card-title">{{ t('wrapped.models') }}</h2>
      <HBarChart
        :labels="modelLabels"
        :values="modelValues"
        :format-value="(v: number) => v.toLocaleString() + ` ${t('common.turns')}`"
        :enable-log-toggle="true"
        scale-mode="linear"
      />
    </div>

    <!-- ═══ Records ═══ -->
    <div class="section-title">{{ t('wrapped.records') }}</div>
    <div class="panels-grid">
      <div class="record-card" v-if="mostExpensive">
        <div class="record-icon record-icon-cost"></div>
        <div class="record-label">{{ t('wrapped.most_expensive_session') }}</div>
        <div class="record-big">{{ formatCost(mostExpensive[1]) }}</div>
        <div class="record-meta">
          <span class="record-id">{{ mostExpensive[0].slice(0, 8) }}...</span>
          <span class="record-project">{{ mostExpensive[2] }}</span>
        </div>
        <div class="record-sub">{{ expensivePct }}% {{ t('wrapped.of_total_spend') }}</div>
      </div>
      <div class="record-card" v-if="longestSession">
        <div class="record-icon record-icon-time"></div>
        <div class="record-label">{{ t('wrapped.longest_session') }}</div>
        <div class="record-big">{{ formatDuration(longestSession[1]) }}</div>
        <div class="record-meta">
          <span class="record-id">{{ longestSession[0].slice(0, 8) }}...</span>
          <span class="record-project">{{ longestSession[2] }}</span>
        </div>
        <div class="record-sub">{{ longestHours }} {{ t('wrapped.hours_total') }}</div>
      </div>
    </div>
  </div>

  <!-- Fallback when no wrapped data -->
  <div class="wrapped-page" v-else>
    <h1 class="page-title">{{ t('nav.wrapped') }}</h1>
    <div class="card" style="text-align: center; padding: 40px 24px;">
      <p style="color: var(--text-tertiary); font-style: italic;">{{ t('wrapped.no_data') }}</p>
    </div>
  </div>
</template>

<style scoped>
.wrapped-page {
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

/* ═══ Hero Card ═══════════════════════════════════════════════════════════ */

.hero-card {
  background: linear-gradient(135deg, var(--gradient-from), var(--gradient-to));
  border-radius: 16px;
  padding: 48px 32px;
  text-align: center;
  position: relative;
  overflow: hidden;
}

.hero-card::before {
  content: '';
  position: absolute;
  inset: 0;
  background: radial-gradient(circle at 30% 20%, rgba(255,255,255,0.08) 0%, transparent 60%);
  pointer-events: none;
}

.hero-year {
  font-size: 1rem;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.7);
  letter-spacing: 0.1em;
  text-transform: uppercase;
  margin-bottom: 16px;
}

.hero-archetype {
  font-size: 3rem;
  font-weight: 800;
  color: #fff;
  line-height: 1.1;
  margin-bottom: 12px;
  letter-spacing: -0.02em;
}

.hero-desc {
  font-size: 1.1rem;
  color: rgba(255, 255, 255, 0.8);
  font-style: italic;
  max-width: 500px;
  margin: 0 auto 20px;
  line-height: 1.5;
}

.hero-date-range {
  font-size: 0.85rem;
  color: rgba(255, 255, 255, 0.5);
}

/* ═══ Section Title ══════════════════════════════════════════════════════ */

.section-title {
  font-size: 1.15rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 8px 0 0;
}

/* ═══ Stats Grid (3x2) ══════════════════════════════════════════════════ */

.stats-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
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

.stat-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 20px;
  transition: border-color 0.15s ease;
}

.stat-card:hover {
  border-color: var(--text-tertiary);
}

.stat-label {
  font-size: 0.72rem;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 8px;
}

.stat-big {
  font-size: 2.5rem;
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1.1;
}

.stat-dim {
  font-size: 1.5rem;
  font-weight: 400;
  color: var(--text-tertiary);
}

.stat-cost {
  color: var(--archetype-color, var(--text-primary));
}

.stat-sub {
  font-size: 0.75rem;
  color: var(--text-secondary);
  margin-top: 6px;
}

/* Streak flame (CSS-only) */
.streak-flame {
  display: inline-block;
  width: 24px;
  height: 28px;
  margin-left: 6px;
  vertical-align: middle;
  position: relative;
}

.streak-flame::before {
  content: '';
  position: absolute;
  bottom: 2px;
  left: 50%;
  transform: translateX(-50%);
  width: 16px;
  height: 22px;
  background: linear-gradient(to top, #ef4444, #f59e0b, #fbbf24);
  border-radius: 50% 50% 50% 50% / 60% 60% 40% 40%;
  clip-path: polygon(50% 0%, 85% 35%, 75% 100%, 50% 80%, 25% 100%, 15% 35%);
}

/* Progress bar */
.progress-bar {
  width: 100%;
  height: 6px;
  background: var(--bg-deep);
  border-radius: 3px;
  margin-top: 10px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  border-radius: 3px;
  transition: width 0.6s ease;
}

/* ═══ Peak Patterns Grid ════════════════════════════════════════════════ */

.peak-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 12px;
}

@media (max-width: 900px) {
  .peak-grid {
    grid-template-columns: repeat(3, 1fr);
  }
}

@media (max-width: 500px) {
  .peak-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

.peak-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 20px;
  text-align: center;
  transition: border-color 0.15s ease;
}

.peak-card:hover {
  border-color: var(--text-tertiary);
}

.peak-label {
  font-size: 0.72rem;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 8px;
}

.peak-big {
  font-size: 2rem;
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1.1;
}

.peak-sub {
  font-size: 0.7rem;
  color: var(--text-secondary);
  margin-top: 4px;
}

/* ═══ Panels Grid (Rankings) ════════════════════════════════════════════ */

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

/* ═══ Record Cards ═══════════════════════════════════════════════════════ */

.record-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 24px;
  text-align: center;
  transition: border-color 0.15s ease;
}

.record-card:hover {
  border-color: var(--text-tertiary);
}

.record-icon {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  margin: 0 auto 12px;
  position: relative;
}

.record-icon::after {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: 10px;
}

.record-icon-cost {
  background: linear-gradient(135deg, #fbbf24, #ef4444);
}

.record-icon-cost::after {
  /* Dollar sign via CSS */
  content: '$';
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 1.1rem;
  font-weight: 700;
  color: #fff;
}

.record-icon-time {
  background: linear-gradient(135deg, #3b82f6, #8b5cf6);
}

.record-icon-time::after {
  /* Clock icon via CSS border trick */
  content: '';
  display: block;
  width: 18px;
  height: 18px;
  border: 2px solid #fff;
  border-radius: 50%;
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
}

.record-label {
  font-size: 0.72rem;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 8px;
}

.record-big {
  font-size: 2.5rem;
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1.1;
  margin-bottom: 8px;
}

.record-meta {
  display: flex;
  justify-content: center;
  gap: 12px;
  font-size: 0.8rem;
  color: var(--text-secondary);
  margin-bottom: 4px;
}

.record-id {
  font-family: 'SF Mono', 'Fira Code', monospace;
  color: var(--text-tertiary);
}

.record-project {
  color: var(--text-accent);
}

.record-sub {
  font-size: 0.75rem;
  color: var(--text-tertiary);
  font-style: italic;
}

/* ═══ Responsive Hero ═══════════════════════════════════════════════════ */

@media (max-width: 600px) {
  .hero-card {
    padding: 32px 20px;
  }
  .hero-archetype {
    font-size: 2.2rem;
  }
  .hero-desc {
    font-size: 0.95rem;
  }
  .stat-big {
    font-size: 2rem;
  }
  .peak-big {
    font-size: 1.6rem;
  }
  .record-big {
    font-size: 2rem;
  }
}
</style>
