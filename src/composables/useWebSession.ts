import { nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { getDesktopApi } from '../lib/desktop'
import { emptySession, errorText, hideWebSurface, hostCommand, type WebEntry, type WebSessionState } from '../lib/workspace'

export function useWebSession(entry: () => WebEntry, supported: () => boolean, suspended: () => boolean, surface: Readonly<Ref<HTMLElement | null>>) {
  const state = ref<WebSessionState>(emptySession(entry()))
  const error = ref('')
  const busy = ref(false)
  const preparing = ref(false)
  const retries = new Set<string>()
  let alive = false
  let frame = 0
  let running = false
  const preparation = new Map<string,Promise<void>>()
  const attempted = new Set<string>()
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
        // Surface resizing and page switching never retry a cancelled preflight.
        if (!attempted.has(id)) {
          attempted.add(id)
          preparing.value=true
          const task=hostCommand<void>('web_prepare',{entryId:id,retry:retries.delete(id)})
          preparation.set(id,task)
          await task
        } else {
          const pending=preparation.get(id)
          if (pending) await pending
        }
        if (!alive) break
        if (id !== entry().id) {dirty=true;continue}
        const result = await hostCommand<WebSessionState | null>('web_show', {
          entryId: id, bounds: { x: bounds.x, y: bounds.y, width: Math.max(1, bounds.width), height: Math.max(1, bounds.height) },
          visible: !suspended(),
        })
        if (alive && id === entry().id && result) state.value = result
      } catch (cause) {
        if (alive && id === entry().id) error.value = errorText(cause)
      } finally { preparing.value=false }
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
  function retry() { retries.add(entry().id); attempted.delete(entry().id); preparation.delete(entry().id); error.value=''; schedule() }
  return { state, error, busy, preparing, run, synchronize: schedule, retry }
}
