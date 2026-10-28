import { nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { getDesktopApi } from '../lib/desktop'
import { emptySession, errorText, hideWebSurface, hostCommand, type WebEntry, type WebSessionState } from '../lib/workspace'

export function useWebSession(entry: () => WebEntry, supported: () => boolean, suspended: () => boolean, surface: Readonly<Ref<HTMLElement | null>>) {
  const state = ref<WebSessionState>(emptySession(entry()))
  const error = ref('')
  const busy = ref(false)
  const preparing = ref(false)
  const retries = new Set<string>()
  const preparation = new Map<string, Promise<void>>()
  const attempted = new Set<string>()
  const pendingPreparations = new Set<string>()
  const running = new Set<string>()
  const dirty = new Set<string>()
  let alive = false
  let frame = 0
  let unlisten: (() => void) | undefined
  let unlistenRetry: (() => void) | undefined
  let unlistenPrepared: (() => void) | undefined
  let observer: ResizeObserver | undefined

  async function synchronize() {
    if (!alive || !supported() || !surface.value) return
    const id = entry().id
    if (running.has(id)) { dirty.add(id); return }
    running.add(id)
    do {
      dirty.delete(id)
      let awaited: Promise<void> | undefined
      try {
        if (!attempted.has(id)) {
          attempted.add(id)
          pendingPreparations.add(id)
          const task = hostCommand<void>('web_prepare', { entryId: id, retry: retries.delete(id) }).then(
            () => { pendingPreparations.delete(id) },
            cause => { pendingPreparations.delete(id); throw cause },
          )
          preparation.set(id, task)
        }
        awaited = preparation.get(id)
        preparing.value = pendingPreparations.has(id)
        if (awaited) await awaited
        if (!alive || id !== entry().id || !surface.value) break
        const bounds = surface.value.getBoundingClientRect()
        const result = await hostCommand<WebSessionState | null>('web_show', {
          entryId: id, bounds: { x: bounds.x, y: bounds.y, width: Math.max(1, bounds.width), height: Math.max(1, bounds.height) },
          visible: !suspended(),
        })
        if (alive && id === entry().id && result) state.value = result
      } catch (cause) {
        // A panel retry may already have superseded this failed request.
        if (alive && id === entry().id && preparation.get(id) === awaited) error.value = errorText(cause)
      } finally {
        if (alive && id === entry().id) preparing.value = false
      }
    } while (dirty.has(id) && alive && id === entry().id)
    running.delete(id)
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
    state.value = emptySession(entry()); error.value = ''; preparing.value = false
    void hideWebSurface().catch(() => {})
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
        const disposePrepared = await api.event.listen<string>('web-prepared', event => {
          pendingPreparations.delete(event.payload)
          attempted.add(event.payload)
          preparation.set(event.payload, Promise.resolve())
          if (alive && event.payload === entry().id) { error.value = ''; schedule() }
        })
        if (alive) unlistenPrepared = disposePrepared
        else disposePrepared()
        const disposeRetry = await api.event.listen<string>('web-retry-request', event => {
          if (alive && event.payload === entry().id) retry()
        })
        if (alive) unlistenRetry = disposeRetry
        else disposeRetry()
      } catch (cause) { error.value = errorText(cause) }
    }
    if (alive) schedule()
  })
  onUnmounted(() => {
    alive = false; cancelAnimationFrame(frame); observer?.disconnect(); unlisten?.(); unlistenPrepared?.(); unlistenRetry?.()
    window.removeEventListener('resize', schedule)
    void hideWebSurface().catch(() => {})
  })
  function retry() { retries.add(entry().id); attempted.delete(entry().id); preparation.delete(entry().id); error.value = ''; schedule() }
  return { state, error, busy, preparing, run, synchronize: schedule, retry }
}
