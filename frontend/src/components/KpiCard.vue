<script setup lang="ts">
defineProps<{
  value: string | number
  label: string
  subtitle?: string
}>()
</script>

<template>
  <div class="kpi-card">
    <div class="kpi-value">{{ value }}</div>
    <div class="kpi-label">{{ label }}</div>
    <div v-if="subtitle" class="kpi-subtitle">{{ subtitle }}</div>
  </div>
</template>

<style scoped>
.kpi-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 14px 16px;
  transition: border-color 0.15s ease;
}

.kpi-card:hover {
  border-color: var(--text-tertiary);
}

.kpi-value {
  font-size: 1.35rem;
  font-weight: 600;
  color: var(--text-primary);
  line-height: 1.1;
}

.kpi-label {
  font-size: 0.75rem;
  font-weight: 500;
  color: var(--text-tertiary);
  margin-top: 4px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.kpi-subtitle {
  font-size: 0.7rem;
  color: var(--text-secondary);
  margin-top: 2px;
}
</style>
