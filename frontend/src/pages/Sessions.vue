<script setup lang="ts">
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()
</script>

<template>
  <div class="page">
    <h1 class="page-title">{{ t('nav.sessions') }}</h1>
    <div class="card">
      <p class="placeholder">{{ t('common.coming_soon') }}</p>
      <p class="placeholder-sub">Sortable, filterable session table with inline detail expansion, turn-by-turn analysis, and context growth charts.</p>
    </div>
  </div>
</template>

<style scoped>
.page {
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

.card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 40px 24px;
  text-align: center;
}

.placeholder {
  color: var(--text-tertiary);
  font-size: 1.1rem;
  font-style: italic;
  margin: 0 0 8px;
}

.placeholder-sub {
  color: var(--text-tertiary);
  font-size: 0.85rem;
  margin: 0;
}
</style>
