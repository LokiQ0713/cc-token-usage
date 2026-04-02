<script setup lang="ts">
import type { PageName } from '../types'
import { useTheme } from '../composables/useTheme'
import { useI18n } from '../composables/useI18n'

defineProps<{
  activePage: PageName
}>()

const emit = defineEmits<{
  navigate: [page: PageName]
}>()

const { theme, toggleTheme } = useTheme()
const { t, toggleLocale, localeLabel } = useI18n()

interface NavItem {
  page: PageName
  icon: string
  labelKey: string
}

const navItems: NavItem[] = [
  { page: 'overview', icon: '&#9673;', labelKey: 'nav.overview' },
  { page: 'trends', icon: '&#9650;', labelKey: 'nav.trends' },
  { page: 'projects', icon: '&#9632;', labelKey: 'nav.projects' },
  { page: 'sessions', icon: '&#9776;', labelKey: 'nav.sessions' },
  { page: 'heatmap', icon: '&#9618;', labelKey: 'nav.heatmap' },
  { page: 'wrapped', icon: '&#10022;', labelKey: 'nav.wrapped' },
]
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-header">
      <h1 class="sidebar-title">CC Token<br>Analyzer</h1>
    </div>

    <nav class="sidebar-nav">
      <button
        v-for="item in navItems"
        :key="item.page"
        :class="['nav-item', { active: activePage === item.page }]"
        @click="emit('navigate', item.page)"
      >
        <span class="nav-icon" v-html="item.icon"></span>
        <span class="nav-label">{{ t(item.labelKey) }}</span>
      </button>
    </nav>

    <div class="sidebar-footer">
      <button class="footer-btn" @click="toggleTheme" :title="t('common.theme_toggle')">
        <span v-if="theme === 'dark'">&#9788;</span>
        <span v-else>&#9790;</span>
      </button>
      <button class="footer-btn" @click="toggleLocale" :title="t('common.lang_toggle')">
        {{ localeLabel }}
      </button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-width);
  height: 100vh;
  position: fixed;
  top: 0;
  left: 0;
  background: var(--bg-secondary);
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  padding: 20px 12px;
  z-index: 100;
}

.sidebar-header {
  padding: 0 8px 20px;
  border-bottom: 1px solid var(--border-color);
  margin-bottom: 16px;
}

.sidebar-title {
  font-size: 1.1rem;
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1.3;
}

.sidebar-nav {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  font-size: 14px;
  font-family: inherit;
  text-align: left;
  transition: all 0.15s ease;
  width: 100%;
}

.nav-item:hover {
  color: var(--text-primary);
  background: var(--bg-tertiary);
}

.nav-item.active {
  color: var(--text-primary);
  background: var(--bg-tertiary);
  font-weight: 600;
}

.nav-icon {
  font-size: 16px;
  width: 20px;
  text-align: center;
  flex-shrink: 0;
}

.nav-label {
  white-space: nowrap;
}

.sidebar-footer {
  display: flex;
  gap: 8px;
  padding-top: 16px;
  border-top: 1px solid var(--border-color);
}

.footer-btn {
  flex: 1;
  padding: 8px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 14px;
  font-family: inherit;
  transition: all 0.15s ease;
}

.footer-btn:hover {
  color: var(--text-primary);
  border-color: var(--text-secondary);
}
</style>
