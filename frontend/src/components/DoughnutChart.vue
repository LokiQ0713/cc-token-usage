<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { Doughnut } from 'vue-chartjs'
import {
  Chart as ChartJS,
  ArcElement,
  Tooltip,
  Legend,
} from 'chart.js'

ChartJS.register(ArcElement, Tooltip, Legend)

const props = withDefaults(
  defineProps<{
    labels: string[]
    values: number[]
    colors?: string[]
    /** Text shown in the center of the doughnut */
    centerText?: string
    /** Smaller text below center */
    centerSubText?: string
    /** Format function for tooltip values */
    formatValue?: (v: number) => string
  }>(),
  {
    colors: () => ['#3b82f6', '#8b5cf6', '#06b6d4', '#f59e0b', '#ef4444', '#10b981'],
    centerText: '',
    centerSubText: '',
    formatValue: (v: number) => '$' + v.toFixed(2),
  },
)

function getCSSVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}

const themeColors = ref({
  text: '#fafafa',
  textSec: '#a1a1aa',
})

function refreshTheme() {
  themeColors.value = {
    text: getCSSVar('--text-primary') || '#fafafa',
    textSec: getCSSVar('--text-secondary') || '#a1a1aa',
  }
}

onMounted(refreshTheme)
onMounted(() => {
  const observer = new MutationObserver(refreshTheme)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
})

const total = computed(() => props.values.reduce((a, b) => a + b, 0))

const chartData = computed(() => ({
  labels: props.labels,
  datasets: [
    {
      data: props.values,
      backgroundColor: props.colors.slice(0, props.labels.length),
      borderWidth: 0,
      hoverOffset: 6,
    },
  ],
}))

const chartOptions = computed(() => ({
  responsive: true,
  maintainAspectRatio: false,
  cutout: '65%',
  plugins: {
    legend: {
      position: 'bottom' as const,
      labels: {
        color: themeColors.value.textSec,
        font: { size: 12 },
        padding: 16,
        usePointStyle: true,
        pointStyleWidth: 10,
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
          const val = props.formatValue(ctx.raw)
          const pct = ((ctx.raw / total.value) * 100).toFixed(1)
          return `${ctx.label}: ${val} (${pct}%)`
        },
      },
    },
  },
}))

// Center text plugin
const centerPlugin = {
  id: 'centerText',
  afterDraw(chart: any) {
    if (!props.centerText) return
    const { ctx, chartArea } = chart
    const centerX = (chartArea.left + chartArea.right) / 2
    const centerY = (chartArea.top + chartArea.bottom) / 2

    ctx.save()
    ctx.textAlign = 'center'
    ctx.textBaseline = 'middle'

    // Main text
    ctx.font = 'bold 1.3rem Inter, sans-serif'
    ctx.fillStyle = themeColors.value.text
    const offsetY = props.centerSubText ? -10 : 0
    ctx.fillText(props.centerText, centerX, centerY + offsetY)

    // Sub text
    if (props.centerSubText) {
      ctx.font = '0.7rem Inter, sans-serif'
      ctx.fillStyle = themeColors.value.textSec
      ctx.fillText(props.centerSubText, centerX, centerY + 12)
    }

    ctx.restore()
  },
}
</script>

<template>
  <div class="doughnut-chart-container">
    <Doughnut
      :data="chartData"
      :options="chartOptions"
      :plugins="[centerPlugin]"
    />
  </div>
</template>

<style scoped>
.doughnut-chart-container {
  width: 100%;
  height: 280px;
  display: flex;
  align-items: center;
  justify-content: center;
}
</style>
