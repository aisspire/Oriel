import { computed, onMounted, onUnmounted, ref, watch } from 'vue'

type Theme = 'light' | 'dark'
const themeKey = 'oriel.appearance.theme'

function readTheme(): Theme {
  try {
    return localStorage.getItem(themeKey) === 'dark' ? 'dark' : 'light'
  } catch {
    return 'light'
  }
}

export function useAppearance() {
  const theme = ref<Theme>(readTheme())
  const motionPreference = window.matchMedia('(prefers-reduced-motion: reduce)')
  const reducedMotion = ref(motionPreference.matches)
  const paused = ref(false)
  const pageHidden = ref(document.hidden)
  const motionPaused = computed(() => paused.value || reducedMotion.value || pageHidden.value)

  function syncMotionPreference(event: MediaQueryListEvent) {
    reducedMotion.value = event.matches
  }

  function syncVisibility() {
    pageHidden.value = document.hidden
  }

  function toggleTheme() {
    theme.value = theme.value === 'light' ? 'dark' : 'light'
  }

  watch(theme, (value) => {
    document.documentElement.dataset.theme = value
    document.querySelector('meta[name="theme-color"]')
      ?.setAttribute('content', value === 'light' ? '#eeedf1' : '#16171e')
    try {
      localStorage.setItem(themeKey, value)
    } catch {
      // An unavailable preference store must not prevent using the current page.
    }
  }, { immediate: true })

  onMounted(() => {
    motionPreference.addEventListener('change', syncMotionPreference)
    document.addEventListener('visibilitychange', syncVisibility)
  })

  onUnmounted(() => {
    motionPreference.removeEventListener('change', syncMotionPreference)
    document.removeEventListener('visibilitychange', syncVisibility)
  })

  return { theme, reducedMotion, paused, motionPaused, toggleTheme }
}
