import { ref } from 'vue'
import { getDesktopApi } from './desktop'

export interface OverlayShowOptions {
  mode: 'settings' | 'add' | 'edit' | 'preparations' | 'help'
  entryId?: string
  confirmRemove?: boolean
}
export interface OverlayRequest extends OverlayShowOptions { requestId: number }
export interface OverlayFailure { stage: string; message: string; infrastructure: boolean }
export interface OverlayStatus {
  supported: boolean
  ready: boolean
  disabled: OverlayFailure | null
  request: { requestId: number; mode: OverlayShowOptions['mode']; phase: string; elapsedMs: number; failure: OverlayFailure | null } | null
}
class ContentError extends Error {}
const delay = (ms: number) => new Promise(resolve => setTimeout(resolve, ms))

// One client across all main-window entry points. Exported class allows isolated
// failure/restart scenarios to exercise the real client rather than a replica.
export class OverlayClient {
  readonly active = ref(false)
  private sequence = 0
  private busy = false
  private disabled = false
  private trigger: HTMLElement | null = null
  private activeId: number | undefined
  private wake: (() => void) | undefined
  private listening = false
  private async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    const api = getDesktopApi()
    if (!api) throw new Error('悬浮层不可用。')
    return api.core.invoke<T>(command, args)
  }
  private restoreFocus() {
    this.activeId = undefined
    this.active.value = false
    if (this.trigger?.isConnected) this.trigger.focus()
    this.trigger = null
  }
  private async monitor(id: number) {
    while (this.activeId === id) {
      await delay(100)
      if (this.activeId !== id) return
      try {
        const status = await this.invoke<OverlayStatus>('overlay_status')
        if (this.activeId !== id) return
        if (status.disabled || status.request?.requestId !== id || status.request.phase !== 'shown') {
          if (status.disabled) this.disabled = true
          this.restoreFocus()
        }
      } catch (cause) {
        this.disabled = true
        void this.invoke('overlay_fail', { requestId: id, failure: { stage: 'status', message: String(cause), infrastructure: true } }).catch(() => {})
        this.restoreFocus()
      }
    }
  }
  async show(options: OverlayShowOptions): Promise<boolean> {
    const api = getDesktopApi()
    if (!api || this.disabled) return false
    if (this.busy || this.active.value) return true
    this.busy = true
    const requestId = ++this.sequence
    const started = performance.now()
    let stage = 'status'
    let expired = false
    let timeout: ReturnType<typeof setTimeout> | undefined
    this.trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const check = (status: OverlayStatus) => {
      if (expired) throw new Error('悬浮层请求已过期。')
      if (status.disabled) { stage = status.disabled.stage; throw new Error(status.disabled.message) }
      const request = status.request
      if (request?.requestId === requestId && request.failure) {
        stage = request.failure.stage
        if (!request.failure.infrastructure) throw new ContentError(request.failure.message)
        throw new Error(request.failure.message)
      }
      return request?.requestId === requestId && request.phase === 'shown'
    }
    try {
      const operation = async () => {
        const initial = await this.invoke<OverlayStatus>('overlay_status')
        check(initial)
        if (!initial.supported) { this.disabled = true; return false }
        // Events only shorten the next poll. Listener failure never gates opening.
        if (!this.listening) {
          this.listening = true
          void api.event.listen('overlay-active', () => this.wake?.()).catch(() => { this.listening = false })
        }
        stage = initial.ready ? 'submit' : 'initialization'
        const submitted = await this.invoke<OverlayStatus>('overlay_show', { request: { ...options, requestId } })
        if (check(submitted)) return true
        while (!expired) {
          await new Promise<void>(resolve => {
            const timer = setTimeout(() => { this.wake = undefined; resolve() }, 25)
            this.wake = () => { clearTimeout(timer); this.wake = undefined; resolve() }
          })
          if (expired) break
          const status = await this.invoke<OverlayStatus>('overlay_status')
          if (check(status)) return true
          if (status.request?.requestId === requestId) {
            stage = status.ready ? status.request.phase : 'initialization'
            if (status.request.phase === 'closed') return true // Closed immediately after becoming interactive.
          }
        }
        throw new Error('悬浮层请求已过期。')
      }
      const shown = await Promise.race([
        operation(),
        new Promise<never>((_, reject) => { timeout = setTimeout(() => { expired = true; reject(new Error('悬浮层无响应，已切换为本地弹窗。')) }, 5000) }),
      ])
      if (shown) {
        // The command response, never the event payload, authorizes active state.
        this.active.value = true
        this.activeId = requestId
        void this.monitor(requestId)
      }
      console.info('Oriel overlay', { requestId, mode: options.mode, stage: shown ? 'interactive' : 'unsupported', elapsedMs: performance.now() - started })
      return shown
    } catch (cause) {
      expired = true
      const infrastructure = !(cause instanceof ContentError)
      if (infrastructure) this.disabled = true
      // Do not wait on a broken IPC channel after a definitive failure. Native
      // deadlines and monotonic IDs also reject confirmations already in flight.
      if (infrastructure) void this.invoke('overlay_fail', { requestId, failure: { stage, message: (cause instanceof Error ? cause.message : String(cause)).slice(0, 4000), infrastructure } }).catch(() => {})
      void this.invoke('overlay_hide', { requestId }).catch(() => {})
      this.restoreFocus()
      console.warn('Oriel overlay', { requestId, mode: options.mode, stage, infrastructure, elapsedMs: performance.now() - started, cause })
      if (!infrastructure) throw cause
      return false
    } finally {
      clearTimeout(timeout)
      this.wake?.()
      this.busy = false
    }
  }
}
const client = new OverlayClient()
export const overlayActive = client.active
export const showOverlay = (options: OverlayShowOptions) => client.show(options)
