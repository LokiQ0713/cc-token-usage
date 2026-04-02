<script setup lang="ts">
import { computed, ref, watch, onMounted } from 'vue'
import { Bar } from 'vue-chartjs'
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  LogarithmicScale,
  BarElement,
  Tooltip,
  Legend,
} from 'chart.js'

ChartJS.register(CategoryScale, LinearScale, LogarithmicScale, BarElement, Tooltip, Legend)

const props = withDefaults(
  defineProps<{
    labels: string[]
    values: number[]
    /** Palette of bar colors (cycles if fewer than labels) */
    colors?: string[]
    /** Show value label at end of each bar */
    showValues?: boolean
    /** Format function for value labels */
    formatValue?: (v: number) => string
    /** Enable log/linear toggle */
    enableLogToggle?: boolean
    /** Initial scale mode */
    scaleMode?: 'linear' | 'logarithmic'
    /** Extra tooltip lines per index: [index] => string[] */
    tooltipExtra?: Record<number, string[]>
  }>(),
  {
    colors: () => [
      '#3b82f6', '#8b5cf6', '#06b6d4', '#f59e0b', '#ef4444',
      '#10b981', '#ec4899', '#f97316', '#14b8a6', '#6366f1',
    ],
    showValues: true,
    formatValue: (v: number) => v.toLocaleString(),
    enableLogToggle: false,
    scaleMode: 'linear',
  },
)

const currentScale = ref(props.scaleMode)

function toggleScale() {
  currentScale.value = currentScale.value === 'linear' ? 'logarithmic' : 'linear'
}

function getCSSVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}

const themeColors = ref({
  text: '#a1a1aa',
  grid: '#27272a',
})

function refreshTheme() {
  themeColors.value = {
    text: getCSSVar('--text-secondary') || '#a1a1aa',
    grid: getCSSVar('--border-color') || '#27272a',
  }
}

onMounted(refreshTheme)

// Observe theme changes on <html> data-theme attribute
onMounted(() => {
  const observer = new MutationObserver(refreshTheme)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
})

const chartData = computed(() => ({
  labels: props.labels,
  datasets: [
    {
      data: props.values,
      backgroundColor: props.labels.map((_, i) => props.colors[i % props.colors.length]),
      borderRadius: 4,
      barThickness: 22,
      maxBarThickness: 28,
    },
  ],
}))

const chartOptions = computed(() => ({
  indexAxis: 'y' as const,
  responsive: true,
  maintainAspectRatio: false,
  plugins: {
    legend: { display: false },
    tooltip: {
      backgroundColor: 'rgba(0,0,0,0.85)',
      titleFont: { size: 13 },
      bodyFont: { size: 12 },
      padding: 10,
      cornerRadius: 8,
      callbacks: {
        label: (ctx: any) => {
          const val = props.formatValue(ctx.raw)
          const lines = [val]
          if (props.tooltipExtra && props.tooltipExtra[ctx.dataIndex]) {
            lines.push(...props.tooltipExtra[ctx.dataIndex])
          }
          return lines
        },
      },
    },
  },
  scales: {
    x: {
      type: currentScale.value as any,
      grid: { color: themeColors.value.grid, lineWidth: 0.5 },
      ticks: {
        color: themeColors.value.text,
        font: { size: 11 },
        callback: (value: any) => {
          if (typeof value === 'number') {
            if (value >= 1000) return (value / 1000).toFixed(0) + 'K'
            return value
          }
          return value
        },
      },
      border: { display: false },
    },
    y: {
      grid: { display: false },
      ticks: {
        color: themeColors.value.text,
        font: { size: 12, weight: 'bold' as const },
      },
      border: { display: false },
    },
  },
}))
</script>

<template>
  <div class="hbar-chart-wrapper">
    <button
      v-if="enableLogToggle"
      class="scale-toggle"
      @click="toggleScale"
    >
      {{ currentScale === 'linear' ? 'Log' : 'Linear' }}
    </button>
    <div class="hbar-chart-container" :style="{ height: Math.max(labels.length * 36, 120) + 'px' }">
      <Bar :data="chartData" :options="chartOptions" />
    </div>
  </div>
</template>

<style scoped>
.hbar-chart-wrapper {
  position: relative;
}

.scale-toggle {
  position: absolute;
  top: -4px;
  right: 0;
  padding: 3px 10px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  font-size: 0.7rem;
  cursor: pointer;
  z-index: 2;
  transition: all 0.15s ease;
}

.scale-toggle:hover {
  color: var(--text-primary);
  border-color: var(--text-tertiary);
}

.hbar-chart-container {
  width: 100%;
}
</style>
