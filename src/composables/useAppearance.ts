import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { syncDesktopIcon } from '../lib/desktop'

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
  const iconSource = computed(() => theme.value === 'dark' ? '/oriel-icon.svg' : '/oriel-icon-light.svg')
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
      ?.setAttribute('content', value === 'light' ? '#f3f3f6' : '#1c1d26')
    document.querySelector('#app-icon')?.setAttribute('href', iconSource.value)
    void syncDesktopIcon(iconSource.value).catch((error) => {
      console.error('Could not update the Oriel window icon.', error)
    })
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

  return { theme, iconSource, reducedMotion, paused, motionPaused, toggleTheme }
}
