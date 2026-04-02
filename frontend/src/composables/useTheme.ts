import { ref, watchEffect } from 'vue'

export type Theme = 'dark' | 'light'

const STORAGE_KEY = 'cc-dashboard-theme'

function getInitialTheme(): Theme {
  if (typeof window === 'undefined') return 'dark'
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored === 'light' || stored === 'dark') return stored
  // Respect system preference
  if (window.matchMedia?.('(prefers-color-scheme: light)').matches) return 'light'
  return 'dark'
}

const theme = ref<Theme>(getInitialTheme())

export function useTheme() {
  watchEffect(() => {
    const html = document.documentElement
    if (theme.value === 'light') {
      html.setAttribute('data-theme', 'light')
    } else {
      html.removeAttribute('data-theme')
    }
    localStorage.setItem(STORAGE_KEY, theme.value)
  })

  function toggleTheme() {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
  }

  return {
    theme,
    toggleTheme,
    isDark: () => theme.value === 'dark',
  }
}
