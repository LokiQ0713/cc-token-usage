<script setup lang="ts">
export interface PillOption {
  value: string
  label: string
}

defineProps<{
  options: PillOption[]
  modelValue: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
}>()
</script>

<template>
  <div class="filter-pills">
    <button
      v-for="opt in options"
      :key="opt.value"
      :class="['pill', { active: modelValue === opt.value }]"
      @click="emit('update:modelValue', opt.value)"
    >
      {{ opt.label }}
    </button>
  </div>
</template>

<style scoped>
.filter-pills {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.pill {
  padding: 5px 14px;
  border: 1px solid var(--border-color);
  border-radius: 20px;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 0.8rem;
  font-family: inherit;
  cursor: pointer;
  transition: all 0.15s ease;
  white-space: nowrap;
}

.pill:hover {
  color: var(--text-primary);
  border-color: var(--text-secondary);
}

.pill.active {
  color: var(--text-primary);
  background: var(--bg-tertiary);
  border-color: var(--text-secondary);
  font-weight: 600;
}
</style>
