<script setup lang="ts">
import { ref, onMounted } from 'vue'
import type { PageName } from './types'
import Sidebar from './components/Sidebar.vue'
import Overview from './pages/Overview.vue'
import Trends from './pages/Trends.vue'
import Projects from './pages/Projects.vue'
import Sessions from './pages/Sessions.vue'
import Heatmap from './pages/Heatmap.vue'
import Wrapped from './pages/Wrapped.vue'
import { useTheme } from './composables/useTheme'
import { useData } from './composables/useData'

// Initialize theme system
useTheme()

const { data } = useData()
const activePage = ref<PageName>('overview')

function navigate(page: PageName) {
  activePage.value = page
}

onMounted(() => {
  if (data.active_session_id) {
    activePage.value = 'sessions'
  }
})
</script>

<template>
  <div class="app-layout">
    <Sidebar :active-page="activePage" @navigate="navigate" />
    <main class="main-content">
      <Overview v-if="activePage === 'overview'" />
      <Trends v-else-if="activePage === 'trends'" />
      <Projects v-else-if="activePage === 'projects'" />
      <Sessions v-else-if="activePage === 'sessions'" />
      <Heatmap v-else-if="activePage === 'heatmap'" />
      <Wrapped v-else-if="activePage === 'wrapped'" />
    </main>
  </div>
</template>

<style scoped>
.app-layout {
  display: flex;
  min-height: 100vh;
}

.main-content {
  margin-left: var(--sidebar-width);
  flex: 1;
  padding: 24px 32px;
  max-width: 1200px;
}
</style>
