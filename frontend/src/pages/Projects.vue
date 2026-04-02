<script setup lang="ts">
import { computed } from 'vue'
import KpiCard from '../components/KpiCard.vue'
import DataTable from '../components/DataTable.vue'
import type { Column } from '../components/DataTable.vue'
import type { Project, SessionSummary } from '../types'
import { useData } from '../composables/useData'
import { useI18n } from '../composables/useI18n'

const { data } = useData()
const { t } = useI18n()

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

// ─── KPIs ──────────────────────────────────────────────────────────────────

const projects = computed(() => data.projects?.projects ?? [])

const totalProjects = computed(() => projects.value.length)

const totalCost = computed(() =>
  projects.value.reduce((sum, p) => sum + p.cost, 0)
)

const avgCostPerProject = computed(() =>
  totalProjects.value > 0 ? totalCost.value / totalProjects.value : 0
)

// ─── Project Table Columns ─────────────────────────────────────────────────

const projectColumns = computed<Column<Project>[]>(() => [
  {
    key: 'display_name',
    label: t('projects.col_name'),
    sortable: true,
    align: 'left',
    format: (row: Project) => row.display_name,
  },
  {
    key: 'session_count',
    label: t('projects.col_sessions'),
    sortable: true,
    align: 'right',
  },
  {
    key: 'total_turns',
    label: t('projects.col_turns'),
    sortable: true,
    align: 'right',
    format: (row: Project) => row.total_turns.toLocaleString(),
  },
  {
    key: 'agent_turns',
    label: t('projects.col_agent_turns'),
    sortable: true,
    align: 'right',
    hideOnNarrow: true,
    format: (row: Project) => row.agent_turns.toLocaleString(),
  },
  {
    key: 'cost_per_session',
    label: t('projects.col_cost_per_session'),
    sortable: false,
    align: 'right',
    hideOnNarrow: true,
    format: (row: Project) =>
      row.session_count > 0
        ? formatCost(row.cost / row.session_count)
        : '$0.00',
  },
  {
    key: 'primary_model',
    label: t('projects.col_model'),
    sortable: true,
    align: 'left',
    format: (row: Project) => shortenModel(row.primary_model),
  },
  {
    key: 'cost',
    label: t('projects.col_total_cost'),
    sortable: true,
    align: 'right',
    format: (row: Project) => formatCost(row.cost),
  },
])

// ─── Session Lookup ────────────────────────────────────────────────────────

const allSessions = computed<SessionSummary[]>(
  () => data.overview?.sessions ?? []
)

function sessionsForProject(projectName: string): SessionSummary[] {
  return [...allSessions.value]
    .filter(s => s.project === projectName)
    .sort((a, b) => b.cost - a.cost)
}

// ─── Session Sub-Table Columns ─────────────────────────────────────────────

const sessionColumns = computed<Column<SessionSummary>[]>(() => [
  {
    key: 'session_id',
    label: t('projects.col_session_id'),
    sortable: false,
    align: 'left',
    format: (row: SessionSummary) => row.session_id.slice(0, 8),
  },
  {
    key: 'turn_count',
    label: t('projects.col_turns'),
    sortable: false,
    align: 'right',
  },
  {
    key: 'duration_minutes',
    label: t('projects.col_duration'),
    sortable: false,
    align: 'right',
    format: (row: SessionSummary) => formatDuration(row.duration_minutes),
  },
  {
    key: 'cost',
    label: t('projects.col_cost'),
    sortable: false,
    align: 'right',
    format: (row: SessionSummary) => formatCost(row.cost),
  },
  {
    key: 'model',
    label: t('projects.col_model'),
    sortable: false,
    align: 'left',
    format: (row: SessionSummary) => shortenModel(row.model),
  },
  {
    key: 'cache_hit_rate',
    label: t('projects.col_cache_hit'),
    sortable: false,
    align: 'right',
    format: (row: SessionSummary) => formatPercent(row.cache_hit_rate),
  },
])
</script>

<template>
  <div class="projects-page">
    <h1 class="page-title">{{ t('nav.projects') }}</h1>

    <!-- KPI Cards -->
    <div class="kpi-grid-3">
      <KpiCard
        :value="totalProjects"
        :label="t('projects.kpi_total_projects')"
      />
      <KpiCard
        :value="formatCost(totalCost)"
        :label="t('projects.kpi_total_cost')"
      />
      <KpiCard
        :value="formatCost(avgCostPerProject)"
        :label="t('projects.kpi_avg_cost')"
      />
    </div>

    <!-- Project Ranking Table -->
    <div class="card">
      <h2 class="card-title">{{ t('projects.ranking_title') }}</h2>
      <DataTable
        :columns="projectColumns"
        :rows="projects"
        row-key="name"
        :expandable="true"
        :show-rank="true"
        default-sort-key="cost"
        default-sort-dir="desc"
      >
        <template #expand="{ row }">
          <div class="sub-table-section">
            <h3 class="sub-table-title">
              {{ t('projects.sessions_for') }} {{ row.display_name }}
            </h3>
            <template v-if="sessionsForProject(row.name).length > 0">
              <DataTable
                :columns="sessionColumns"
                :rows="sessionsForProject(row.name)"
                row-key="session_id"
                :expandable="false"
                :show-rank="false"
              />
            </template>
            <p v-else class="no-sessions">{{ t('projects.no_sessions') }}</p>
          </div>
        </template>
      </DataTable>
    </div>
  </div>
</template>

<style scoped>
.projects-page {
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

.kpi-grid-3 {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}

@media (max-width: 600px) {
  .kpi-grid-3 {
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
}

/* ─── Sub-Table ───────────────────────────────────────────────────────────── */

.sub-table-section {
  padding: 4px 0;
}

.sub-table-title {
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--text-secondary);
  margin: 0 0 10px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.no-sessions {
  color: var(--text-tertiary);
  font-size: 0.85rem;
  font-style: italic;
  margin: 0;
  padding: 8px 0;
}
</style>
