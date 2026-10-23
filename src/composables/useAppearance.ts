import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { getDesktopApi, syncDesktopIcon } from '../lib/desktop'
import { errorText, hostCommand } from '../lib/workspace'

type Theme = 'light' | 'dark'
interface ThemeResult { theme: Theme; diagnostics: string[]; persisted: boolean }
const themeKey = 'oriel.appearance.theme'

function readTheme(): Theme {
  try {
    return localStorage.getItem(themeKey) === 'dark' ? 'dark' : 'light'
  } catch {
    return 'light'
  }
}

export function useAppearance() {
  const desktop = Boolean(getDesktopApi())
  const theme = ref<Theme>(desktop ? 'light' : readTheme())
  const appearanceError = ref('')
  let queue: Promise<unknown> = Promise.resolve()
  let initialized = !desktop
  let desired: Theme | null = null
  let intent = 0
  let unlisten: (() => void) | undefined
  let alive = true
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

  function setTheme(value: Theme) {
    theme.value = value
    if (!desktop) return
    desired = value
    const request = ++intent
    queue = queue.then(async () => {
      if (request !== intent) return
      try {
        const result = await hostCommand<ThemeResult>('workspace_theme', { theme: value })
        if (request === intent) {appearanceError.value = result.diagnostics.join('；'); desired = null}
        if (result.persisted) { try { localStorage.removeItem(themeKey) } catch { /* optional migration cleanup */ } }
      } catch (error) { if (request === intent) {appearanceError.value = errorText(error); desired = null} }
    })
  }
  function toggleTheme() { if (initialized) setTheme(theme.value === 'light' ? 'dark' : 'light') }

  watch(theme, (value) => {
    document.documentElement.dataset.theme = value
    document.querySelector('meta[name="theme-color"]')
      ?.setAttribute('content', value === 'light' ? '#f3f3f6' : '#1c1d26')
    document.querySelector('#app-icon')?.setAttribute('href', iconSource.value)
    void syncDesktopIcon(iconSource.value).catch((error) => {
      console.error('Could not update the Oriel window icon.', error)
    })
    try {
      if (!desktop) localStorage.setItem(themeKey, value)
    } catch {
      // An unavailable preference store must not prevent using the current page.
    }
  }, { immediate: true })

  onMounted(async () => {
    motionPreference.addEventListener('change', syncMotionPreference)
    document.addEventListener('visibilitychange', syncVisibility)
    const api = getDesktopApi()
    if (api) {
      try {
        const dispose = await api.event.listen<ThemeResult>('workspace-theme', ({ payload }) => {
          if (alive && (desired === null || desired === payload.theme)) {theme.value = payload.theme; appearanceError.value = payload.diagnostics.join('；')}
        })
        if (alive) unlisten = dispose
        else dispose()
      } catch (error) {appearanceError.value = errorText(error)}
    }
    if (desktop) {
      try {
        const result = await hostCommand<ThemeResult>('workspace_theme', { legacy: readTheme() })
        if (desired === null) theme.value = result.theme; appearanceError.value = result.diagnostics.join('；')
        if (result.persisted) { try { localStorage.removeItem(themeKey) } catch { /* optional migration cleanup */ } }
      } catch (error) { appearanceError.value = errorText(error) }
      finally { initialized = true }
    }
  })

  onUnmounted(() => {
    alive = false; unlisten?.()
    motionPreference.removeEventListener('change', syncMotionPreference)
    document.removeEventListener('visibilitychange', syncVisibility)
  })

  return { theme, iconSource, reducedMotion, paused, motionPaused, toggleTheme, setTheme, appearanceError }
}
