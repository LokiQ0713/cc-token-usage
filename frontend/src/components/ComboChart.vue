<script setup lang="ts">
import { computed, ref, onMounted, watch, onBeforeUnmount } from 'vue'
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  LogarithmicScale,
  BarElement,
  LineElement,
  PointElement,
  LineController,
  BarController,
  Tooltip,
  Legend,
} from 'chart.js'

ChartJS.register(
  CategoryScale, LinearScale, LogarithmicScale,
  BarElement, LineElement, PointElement,
  LineController, BarController,
  Tooltip, Legend,
)

const props = withDefaults(
  defineProps<{
    labels: string[]
    barValues: number[]
    lineValues: number[]
    barLabel: string
    lineLabel: string
    barColor?: string
    lineColor?: string
    formatBar?: (v: number) => string
    formatLine?: (v: number) => string
    barYLabel?: string
    lineYLabel?: string
    /** Indices of extreme values to highlight in red */
    extremeIndices?: number[]
    /** Use logarithmic scale for bar Y-axis */
    logScale?: boolean
  }>(),
  {
    barColor: '#3b82f6',
    lineColor: '#f59e0b',
    formatBar: (v: number) => v.toLocaleString(),
    formatLine: (v: number) => v.toLocaleString(),
    barYLabel: '',
    lineYLabel: '',
    extremeIndices: () => [],
    logScale: false,
  },
)

function getCSSVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}

const themeColors = ref({
  text: '#a1a1aa',
  textSec: '#71717a',
  grid: '#27272a',
})

function refreshTheme() {
  themeColors.value = {
    text: getCSSVar('--text-secondary') || '#a1a1aa',
    textSec: getCSSVar('--text-tertiary') || '#71717a',
    grid: getCSSVar('--border-color') || '#27272a',
  }
}

const canvasRef = ref<HTMLCanvasElement | null>(null)
let chartInstance: ChartJS | null = null

function buildConfig() {
  const barBg = props.labels.map((_, i) =>
    props.extremeIndices.includes(i) ? '#ef4444' : props.barColor,
  )

  return {
    type: 'bar' as const,
    data: {
      labels: props.labels,
      datasets: [
        {
          type: 'bar' as const,
          label: props.barLabel,
          data: props.barValues,
          backgroundColor: barBg,
          borderRadius: 4,
          order: 2,
          yAxisID: 'yBar',
        },
        {
          type: 'line' as const,
          label: props.lineLabel,
          data: props.lineValues,
          borderColor: props.lineColor,
          backgroundColor: props.lineColor + '20',
          borderWidth: 2,
          pointRadius: 3,
          pointHoverRadius: 5,
          tension: 0.3,
          fill: false,
          order: 1,
          yAxisID: 'yLine',
        },
      ],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      interaction: {
        mode: 'index' as const,
        intersect: false,
      },
      plugins: {
        legend: {
          display: true,
          position: 'top' as const,
          align: 'end' as const,
          labels: {
            color: themeColors.value.text,
            font: { size: 11 },
            usePointStyle: true,
            pointStyleWidth: 10,
            padding: 16,
          },
        },
        tooltip: {
          backgroundColor: 'rgba(0,0,0,0.85)',
          titleFont: { size: 13 },
          bodyFont: { size: 12 },
          padding: 10,
          cornerRadius: 8,
          callbacks: {
            label: (ctx: any) => {
              if (ctx.datasetIndex === 0) {
                return `${ctx.dataset.label}: ${props.formatBar(ctx.raw)}`
              }
              return `${ctx.dataset.label}: ${props.formatLine(ctx.raw)}`
            },
          },
        },
      },
      scales: {
        x: {
          grid: { color: themeColors.value.grid, lineWidth: 0.5 },
          ticks: {
            color: themeColors.value.text,
            font: { size: 10 },
            maxRotation: 45,
            autoSkip: true,
            maxTicksLimit: 15,
          },
          border: { display: false },
        },
        yBar: {
          type: (props.logScale ? 'logarithmic' : 'linear') as any,
          position: 'left' as const,
          grid: { color: themeColors.value.grid, lineWidth: 0.5 },
          ticks: {
            color: themeColors.value.text,
            font: { size: 11 },
            callback: (value: any) => {
              if (typeof value !== 'number') return value
              if (props.logScale) {
                if (value === 0) return '0'
                const log = Math.log10(value)
                if (Math.abs(log - Math.round(log)) > 0.01) return ''
              }
              if (value >= 1000) return '$' + (value / 1000).toFixed(0) + 'K'
              if (value >= 1) return '$' + value.toFixed(0)
              return '$' + value.toFixed(2)
            },
          },
          border: { display: false },
          title: {
            display: !!props.barYLabel,
            text: props.barYLabel,
            color: themeColors.value.textSec,
            font: { size: 11 },
          },
        },
        yLine: {
          type: 'linear' as const,
          position: 'right' as const,
          grid: { drawOnChartArea: false },
          ticks: {
            color: props.lineColor,
            font: { size: 11 },
          },
          border: { display: false },
          title: {
            display: !!props.lineYLabel,
            text: props.lineYLabel,
            color: props.lineColor,
            font: { size: 11 },
          },
        },
      },
    },
  }
}

function createChart() {
  if (!canvasRef.value) return
  if (chartInstance) {
    chartInstance.destroy()
    chartInstance = null
  }
  chartInstance = new ChartJS(canvasRef.value, buildConfig() as any)
}

function updateChart() {
  if (!chartInstance) {
    createChart()
    return
  }
  const cfg = buildConfig()
  chartInstance.data = cfg.data as any
  chartInstance.options = cfg.options as any
  chartInstance.update()
}

onMounted(() => {
  refreshTheme()
  createChart()

  const observer = new MutationObserver(() => {
    refreshTheme()
    updateChart()
  })
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
})

watch(
  () => [props.labels, props.barValues, props.lineValues, props.logScale, props.extremeIndices, themeColors.value],
  () => updateChart(),
  { deep: true },
)

onBeforeUnmount(() => {
  if (chartInstance) {
    chartInstance.destroy()
    chartInstance = null
  }
})
</script>

<template>
  <div class="combo-chart-container">
    <canvas ref="canvasRef"></canvas>
  </div>
</template>

<style scoped>
.combo-chart-container {
  width: 100%;
  height: 320px;
  position: relative;
}
</style>
