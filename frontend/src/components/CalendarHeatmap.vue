<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from '../composables/useI18n'

export type HeatmapMetric = 'turns' | 'cost' | 'sessions'

interface DayData {
  date: string
  turns: number
  cost: number
  sessions: number
}

const props = defineProps<{
  days: DayData[]
  metric: HeatmapMetric
}>()

const { t, locale } = useI18n()

// ─── Tooltip State ──────────────────────────────────────────────────────────

const tooltip = ref<{
  visible: boolean
  x: number
  y: number
  data: DayData | null
}>({ visible: false, x: 0, y: 0, data: null })

function showTooltip(event: MouseEvent, day: DayData | null) {
  if (!day) {
    tooltip.value.visible = false
    return
  }
  const rect = (event.currentTarget as HTMLElement)?.closest('.heatmap-scroll-container')?.getBoundingClientRect()
  if (!rect) return
  tooltip.value = {
    visible: true,
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
    data: day,
  }
}

function hideTooltip() {
  tooltip.value.visible = false
}

// ─── Build Day Map ──────────────────────────────────────────────────────────

const dayMap = computed(() => {
  const map = new Map<string, DayData>()
  for (const d of props.days) {
    map.set(d.date, d)
  }
  return map
})

// ─── Calendar Grid Computation ──────────────────────────────────────────────

interface CellData {
  date: string
  dayData: DayData | null
  value: number
  level: number
  row: number // 0=Mon ... 6=Sun
  col: number // week index
}

const calendarGrid = computed(() => {
  // Determine date range: last ~365 days ending at latest data or today
  const today = new Date('2026-04-02')
  const endDate = new Date(today)
  // Go back ~365 days
  const startDate = new Date(endDate)
  startDate.setDate(startDate.getDate() - 364)

  // Align startDate to preceding Monday (Mon=1 in getDay(), Sun=0)
  const startDow = startDate.getDay()
  // Convert JS day (0=Sun) to Mon=0 system: Mon=0,Tue=1,...,Sun=6
  const monBasedDow = startDow === 0 ? 6 : startDow - 1
  startDate.setDate(startDate.getDate() - monBasedDow)

  // Collect all values for quantile calculation
  const values: number[] = []
  const tempCells: { date: string; dayData: DayData | null; value: number }[] = []

  for (let d = new Date(startDate); d <= endDate; d.setDate(d.getDate() + 1)) {
    const dateStr = d.toISOString().slice(0, 10)
    const dayData = dayMap.value.get(dateStr) ?? null
    const value = dayData ? getValue(dayData) : 0
    if (value > 0) values.push(value)
    tempCells.push({ date: dateStr, dayData, value })
  }

  // Calculate quantile thresholds
  const thresholds = computeQuantiles(values)

  // Build cells with position
  const cells: CellData[] = []
  for (let i = 0; i < tempCells.length; i++) {
    const cell = tempCells[i]
    const d = new Date(cell.date + 'T00:00:00')
    const jsDow = d.getDay() // 0=Sun
    const row = jsDow === 0 ? 6 : jsDow - 1 // Mon=0...Sun=6
    const col = Math.floor(i / 7)
    const level = cell.value === 0 ? 0 : getLevel(cell.value, thresholds)
    cells.push({ ...cell, level, row, col })
  }

  return {
    cells,
    totalCols: Math.ceil(tempCells.length / 7),
    thresholds,
    startDate: new Date(startDate),
    endDate: new Date(endDate),
  }
})

function getValue(day: DayData): number {
  switch (props.metric) {
    case 'turns': return day.turns
    case 'cost': return day.cost
    case 'sessions': return day.sessions
  }
}

function computeQuantiles(values: number[]): [number, number, number, number] {
  if (values.length === 0) return [1, 2, 3, 4]
  const sorted = [...values].sort((a, b) => a - b)
  const p = (pct: number) => {
    const idx = Math.floor(pct * (sorted.length - 1))
    return sorted[idx]
  }
  const p25 = p(0.25)
  const p50 = p(0.50)
  const p75 = p(0.75)
  const p95 = p(0.95)
  return [
    Math.max(p25, 1),
    Math.max(p50, p25 + 1),
    Math.max(p75, p50 + 1),
    Math.max(p95, p75 + 1),
  ]
}

function getLevel(value: number, thresholds: [number, number, number, number]): number {
  if (value <= 0) return 0
  if (value <= thresholds[0]) return 1
  if (value <= thresholds[1]) return 2
  if (value <= thresholds[2]) return 3
  return 4
}

// ─── Month Labels ───────────────────────────────────────────────────────────

const monthLabels = computed(() => {
  const labels: { text: string; col: number }[] = []
  const grid = calendarGrid.value
  if (grid.cells.length === 0) return labels

  let lastMonth = -1
  for (const cell of grid.cells) {
    if (cell.row !== 0) continue // Only check Mondays
    const d = new Date(cell.date + 'T00:00:00')
    const month = d.getMonth()
    if (month !== lastMonth) {
      const day = d.getDate()
      // Only show label if it's within first ~7 days of month
      if (day <= 7) {
        const loc = locale.value === 'zh' ? 'zh-CN' : 'en-US'
        const monthText = d.toLocaleString(loc, { month: 'short' })
        labels.push({ text: monthText, col: cell.col })
      }
      lastMonth = month
    }
  }
  return labels
})

// ─── Weekday Labels ─────────────────────────────────────────────────────────

const weekdayLabels = computed(() => {
  return [
    { text: t('heatmap.weekday_mon'), row: 0, show: true },
    { text: '', row: 1, show: false },
    { text: t('heatmap.weekday_wed'), row: 2, show: true },
    { text: '', row: 3, show: false },
    { text: t('heatmap.weekday_fri'), row: 4, show: true },
    { text: '', row: 5, show: false },
    { text: t('heatmap.weekday_sun'), row: 6, show: true },
  ]
})

// ─── Color Scheme ───────────────────────────────────────────────────────────

const colorClass = computed(() => {
  return props.metric === 'cost' ? 'scheme-cost' : 'scheme-turns'
})

// ─── Format Tooltip ─────────────────────────────────────────────────────────

function formatDate(dateStr: string): string {
  const d = new Date(dateStr + 'T00:00:00')
  const loc = locale.value === 'zh' ? 'zh-CN' : 'en-US'
  return d.toLocaleDateString(loc, { weekday: 'short', year: 'numeric', month: 'short', day: 'numeric' })
}

function formatCost(n: number): string {
  return '$' + n.toFixed(2)
}

// ─── Scroll Container ───────────────────────────────────────────────────────

const scrollContainer = ref<HTMLElement | null>(null)

onMounted(() => {
  // Scroll to the end (most recent) on mount
  if (scrollContainer.value) {
    scrollContainer.value.scrollLeft = scrollContainer.value.scrollWidth
  }
})

// ─── Responsive Cell Size ───────────────────────────────────────────────────

const cellSize = ref(13)
const cellGap = ref(3)

function updateCellSize() {
  if (typeof window === 'undefined') return
  if (window.innerWidth < 600) {
    cellSize.value = 10
    cellGap.value = 2
  } else {
    cellSize.value = 13
    cellGap.value = 3
  }
}

onMounted(() => {
  updateCellSize()
  window.addEventListener('resize', updateCellSize)
})

onUnmounted(() => {
  window.removeEventListener('resize', updateCellSize)
})

// ─── Grid Dimensions ────────────────────────────────────────────────────────

const gridWidth = computed(() => {
  return calendarGrid.value.totalCols * (cellSize.value + cellGap.value) - cellGap.value
})

const gridHeight = computed(() => {
  return 7 * (cellSize.value + cellGap.value) - cellGap.value
})
</script>

<template>
  <div :class="['calendar-heatmap', colorClass]">
    <!-- Month labels -->
    <div class="month-labels" :style="{ paddingLeft: '36px' }">
      <div
        class="month-labels-inner"
        :style="{ width: gridWidth + 'px', position: 'relative', height: '18px' }"
      >
        <span
          v-for="label in monthLabels"
          :key="label.col"
          class="month-label"
          :style="{ left: label.col * (cellSize + cellGap) + 'px' }"
        >
          {{ label.text }}
        </span>
      </div>
    </div>

    <!-- Grid with weekday labels -->
    <div class="heatmap-body">
      <!-- Weekday labels -->
      <div class="weekday-labels" :style="{ height: gridHeight + 'px' }">
        <span
          v-for="wd in weekdayLabels"
          :key="wd.row"
          class="weekday-label"
          :style="{
            top: wd.row * (cellSize + cellGap) + 'px',
            height: cellSize + 'px',
            lineHeight: cellSize + 'px',
            visibility: wd.show ? 'visible' : 'hidden',
          }"
        >
          {{ wd.text }}
        </span>
      </div>

      <!-- Scrollable heatmap grid -->
      <div class="heatmap-scroll-container" ref="scrollContainer">
        <div
          class="heatmap-grid"
          :style="{
            width: gridWidth + 'px',
            height: gridHeight + 'px',
            position: 'relative',
          }"
        >
          <div
            v-for="cell in calendarGrid.cells"
            :key="cell.date"
            :class="['heatmap-cell', `level-${cell.level}`]"
            :style="{
              left: cell.col * (cellSize + cellGap) + 'px',
              top: cell.row * (cellSize + cellGap) + 'px',
              width: cellSize + 'px',
              height: cellSize + 'px',
            }"
            @mouseenter="showTooltip($event, cell.dayData)"
            @mousemove="showTooltip($event, cell.dayData)"
            @mouseleave="hideTooltip"
          />
        </div>

        <!-- Tooltip -->
        <div
          v-if="tooltip.visible && tooltip.data"
          class="heatmap-tooltip"
          :style="{
            left: tooltip.x + 'px',
            top: (tooltip.y - 70) + 'px',
          }"
        >
          <div class="tooltip-date">{{ formatDate(tooltip.data.date) }}</div>
          <div class="tooltip-row">
            <span class="tooltip-label">{{ t('heatmap.tooltip_turns') }}:</span>
            <span class="tooltip-value">{{ tooltip.data.turns.toLocaleString() }}</span>
          </div>
          <div class="tooltip-row">
            <span class="tooltip-label">{{ t('heatmap.tooltip_cost') }}:</span>
            <span class="tooltip-value">{{ formatCost(tooltip.data.cost) }}</span>
          </div>
          <div class="tooltip-row">
            <span class="tooltip-label">{{ t('heatmap.tooltip_sessions') }}:</span>
            <span class="tooltip-value">{{ tooltip.data.sessions }}</span>
          </div>
        </div>

        <!-- Tooltip for empty days -->
        <div
          v-if="tooltip.visible && !tooltip.data"
          class="heatmap-tooltip"
          :style="{
            left: tooltip.x + 'px',
            top: (tooltip.y - 40) + 'px',
          }"
        >
          <div class="tooltip-date">{{ t('heatmap.no_activity') }}</div>
        </div>
      </div>
    </div>

    <!-- Legend -->
    <div class="heatmap-legend">
      <span class="legend-text">{{ t('heatmap.legend_less') }}</span>
      <div class="legend-cell level-0" />
      <div class="legend-cell level-1" />
      <div class="legend-cell level-2" />
      <div class="legend-cell level-3" />
      <div class="legend-cell level-4" />
      <span class="legend-text">{{ t('heatmap.legend_more') }}</span>
    </div>
  </div>
</template>

<style scoped>
.calendar-heatmap {
  width: 100%;
}

/* ─── Month Labels ────────────────────────────────────────────────────────── */

.month-labels {
  overflow: hidden;
  margin-bottom: 4px;
}

.month-label {
  position: absolute;
  font-size: 0.7rem;
  color: var(--text-tertiary);
  white-space: nowrap;
}

/* ─── Heatmap Body ────────────────────────────────────────────────────────── */

.heatmap-body {
  display: flex;
  gap: 4px;
}

.weekday-labels {
  position: relative;
  width: 32px;
  flex-shrink: 0;
}

.weekday-label {
  position: absolute;
  font-size: 0.65rem;
  color: var(--text-tertiary);
  text-align: right;
  width: 100%;
  padding-right: 4px;
}

.heatmap-scroll-container {
  overflow-x: auto;
  position: relative;
  flex: 1;
  min-width: 0;
}

/* Hide scrollbar but keep functionality */
.heatmap-scroll-container::-webkit-scrollbar {
  height: 4px;
}

.heatmap-scroll-container::-webkit-scrollbar-thumb {
  background: var(--bg-deep);
  border-radius: 2px;
}

.heatmap-grid {
  min-width: fit-content;
}

/* ─── Cells ───────────────────────────────────────────────────────────────── */

.heatmap-cell {
  position: absolute;
  border-radius: 2px;
  cursor: pointer;
  transition: outline-color 0.1s ease;
  outline: 1px solid transparent;
}

.heatmap-cell:hover {
  outline-color: var(--text-secondary);
}

/* Turns/Sessions color scheme (green) - dark mode (default) */
.scheme-turns .level-0 {
  background-color: #161b22;
}
.scheme-turns .level-1 {
  background-color: #0e4429;
}
.scheme-turns .level-2 {
  background-color: #006d32;
}
.scheme-turns .level-3 {
  background-color: #26a641;
}
.scheme-turns .level-4 {
  background-color: #39d353;
}

/* Turns/Sessions color scheme (green) - light mode */
[data-theme="light"] .scheme-turns .level-0 {
  background-color: #ebedf0;
}
[data-theme="light"] .scheme-turns .level-1 {
  background-color: #9be9a8;
}
[data-theme="light"] .scheme-turns .level-2 {
  background-color: #40c463;
}
[data-theme="light"] .scheme-turns .level-3 {
  background-color: #30a14e;
}
[data-theme="light"] .scheme-turns .level-4 {
  background-color: #216e39;
}

/* Cost color scheme (warm) - dark mode */
.scheme-cost .level-0 {
  background-color: #161b22;
}
.scheme-cost .level-1 {
  background-color: #5f1e1e;
}
.scheme-cost .level-2 {
  background-color: #a63226;
}
.scheme-cost .level-3 {
  background-color: #e64141;
}
.scheme-cost .level-4 {
  background-color: #ff6b6b;
}

/* Cost color scheme (warm) - light mode */
[data-theme="light"] .scheme-cost .level-0 {
  background-color: #ebedf0;
}
[data-theme="light"] .scheme-cost .level-1 {
  background-color: #fde68a;
}
[data-theme="light"] .scheme-cost .level-2 {
  background-color: #f59e0b;
}
[data-theme="light"] .scheme-cost .level-3 {
  background-color: #ea580c;
}
[data-theme="light"] .scheme-cost .level-4 {
  background-color: #dc2626;
}

/* ─── Tooltip ─────────────────────────────────────────────────────────────── */

.heatmap-tooltip {
  position: absolute;
  z-index: 100;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 8px 10px;
  pointer-events: none;
  white-space: nowrap;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  transform: translateX(-50%);
}

.tooltip-date {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 4px;
}

.tooltip-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 0.7rem;
  line-height: 1.5;
}

.tooltip-label {
  color: var(--text-tertiary);
}

.tooltip-value {
  color: var(--text-primary);
  font-weight: 500;
}

/* ─── Legend ───────────────────────────────────────────────────────────────── */

.heatmap-legend {
  display: flex;
  align-items: center;
  gap: 4px;
  justify-content: flex-end;
  margin-top: 10px;
  padding-right: 4px;
}

.legend-text {
  font-size: 0.65rem;
  color: var(--text-tertiary);
  margin: 0 2px;
}

.legend-cell {
  width: 12px;
  height: 12px;
  border-radius: 2px;
}

@media (max-width: 600px) {
  .legend-cell {
    width: 10px;
    height: 10px;
  }
  .weekday-labels {
    width: 24px;
  }
  .weekday-label {
    font-size: 0.55rem;
  }
  .month-label {
    font-size: 0.6rem;
  }
}
</style>
