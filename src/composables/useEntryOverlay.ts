import { onUnmounted, ref, watch } from 'vue'
import { errorText, hideWebSurface, showOverlay, type OverlayShowOptions } from '../lib/workspace'

// Only unsupported platforms / native infrastructure failures hide the page.
// Normal overlay opening must never suspend the underlying WebView.
export function useEntryOverlay(entryId: () => string) {
  const active = ref(false)
  const opening = ref(false)
  const error = ref('')
  let generation = 0
  let trigger: HTMLElement | null = null
  function close() {
    ++generation
    active.value = false
    if (trigger?.isConnected) trigger.focus()
    trigger = null
  }
  async function open(mode: OverlayShowOptions['mode']) {
    if (opening.value || active.value) return
    const token = ++generation
    opening.value = true; error.value = ''
    trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
    try {
      if (await showOverlay({ mode, entryId: entryId() })) { trigger = null; return }
      if (token !== generation) return
      active.value = true
      await hideWebSurface()
    } catch (cause) {
      if (token === generation) { close(); error.value = errorText(cause) }
    } finally { opening.value = false }
  }
  watch(entryId, close)
  onUnmounted(close)
  return { active, opening, error, open, close }
}
