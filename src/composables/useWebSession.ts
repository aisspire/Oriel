import { nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { getDesktopApi } from '../lib/desktop'
import { emptySession, errorText, hideWebSurface, hostCommand, type WebEntry, type WebSessionState } from '../lib/workspace'

export function useWebSession(entry: () => WebEntry, supported: () => boolean, suspended: () => boolean, surface: Readonly<Ref<HTMLElement | null>>) {
  const state = ref<WebSessionState>(emptySession(entry()))
  const error = ref('')
  const busy = ref(false)
  let alive = false
  let frame = 0
  let running = false
  let dirty = false
  let unlisten: (() => void) | undefined
  let observer: ResizeObserver | undefined

  async function synchronize() {
    if (!alive || !supported() || !surface.value) return
    if (running) { dirty = true; return }
    running = true
    do {
      dirty = false
      const id = entry().id
      const bounds = surface.value.getBoundingClientRect()
      try {
        const result = await hostCommand<WebSessionState | null>('web_show', {
          entryId: id, bounds: { x: bounds.x, y: bounds.y, width: Math.max(1, bounds.width), height: Math.max(1, bounds.height) },
          visible: !suspended(),
        })
        if (alive && id === entry().id && result) state.value = result
      } catch (cause) {
        if (alive && id === entry().id) error.value = errorText(cause)
      }
    } while (dirty && alive)
    running = false
  }
  function schedule() {
    cancelAnimationFrame(frame)
    frame = requestAnimationFrame(() => { void synchronize() })
  }
  async function run(command: string, args: Record<string, unknown> = {}) {
    if (busy.value || !supported()) return
    const id = entry().id
    error.value = ''; busy.value = true
    try { await hostCommand(command, { entryId: id, ...args }) }
    catch (cause) { if (id === entry().id) error.value = errorText(cause) }
    finally { busy.value = false; schedule() }
  }
  watch(() => entry().id, () => {
    state.value = emptySession(entry()); error.value = ''
    void nextTick(schedule)
  })
  watch(suspended, schedule)
  onMounted(async () => {
    alive = true
    observer = new ResizeObserver(schedule)
    if (surface.value) observer.observe(surface.value)
    window.addEventListener('resize', schedule)
    const api = getDesktopApi()
    if (supported() && api) {
      try {
        const dispose = await api.event.listen<WebSessionState>('web-session', event => {
          if (alive && event.payload.entryId === entry().id) state.value = event.payload
        })
        if (alive) unlisten = dispose
        else dispose()
      } catch (cause) { error.value = errorText(cause) }
    }
    if (alive) schedule()
  })
  onUnmounted(() => {
    alive = false; cancelAnimationFrame(frame); observer?.disconnect(); unlisten?.()
    window.removeEventListener('resize', schedule)
    void hideWebSurface().catch(() => {})
  })
  return { state, error, busy, run, synchronize: schedule }
}
