<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { Line } from 'vue-chartjs'
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  LineElement,
  PointElement,
  Filler,
  Tooltip,
  Legend,
} from 'chart.js'

ChartJS.register(CategoryScale, LinearScale, LineElement, PointElement, Filler, Tooltip, Legend)

const props = withDefaults(
  defineProps<{
    labels: string[]
    values: number[]
    label: string
    color?: string
    fill?: boolean
    formatValue?: (v: number) => string
    yLabel?: string
  }>(),
  {
    color: '#8b5cf6',
    fill: true,
    formatValue: (v: number) => v.toLocaleString(),
    yLabel: '',
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

onMounted(refreshTheme)
onMounted(() => {
  const observer = new MutationObserver(refreshTheme)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
})

const chartData = computed(() => ({
  labels: props.labels,
  datasets: [
    {
      label: props.label,
      data: props.values,
      borderColor: props.color,
      backgroundColor: props.fill ? props.color + '18' : 'transparent',
      borderWidth: 2,
      pointRadius: 3,
      pointHoverRadius: 5,
      pointBackgroundColor: props.color,
      tension: 0.3,
      fill: props.fill,
    },
  ],
}))

const chartOptions = computed(() => ({
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
        label: (ctx: any) => `${ctx.dataset.label}: ${props.formatValue(ctx.raw)}`,
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
    y: {
      grid: { color: themeColors.value.grid, lineWidth: 0.5 },
      ticks: {
        color: themeColors.value.text,
        font: { size: 11 },
      },
      border: { display: false },
      title: {
        display: !!props.yLabel,
        text: props.yLabel,
        color: themeColors.value.textSec,
        font: { size: 11 },
      },
    },
  },
}))
</script>

<template>
  <div class="line-chart-container">
    <Line :data="chartData" :options="chartOptions" />
  </div>
</template>

<style scoped>
.line-chart-container {
  width: 100%;
  height: 240px;
}
</style>
