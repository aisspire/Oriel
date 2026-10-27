import type { OverlayFailure, OverlayRequest, OverlayStatus } from './overlay'

export class OverlayContentError extends Error {}
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>

// The hidden renderer pulls authoritative native state. No animation frame or
// delivered event is needed for initialization, content mounting or confirmation.
export class OverlayReceiver {
  private generation = 0
  private current: number | undefined
  private polling = false
  private stopped = false
  constructor(private readonly invoke: Invoke,
    private readonly prepare: (request: OverlayRequest) => Promise<() => Promise<void>>,
    private readonly clear: () => void) {}
  reset() { ++this.generation; this.current = undefined; this.clear() }
  stop() { this.stopped = true; this.reset() }
  async fail(stage: string, cause: unknown, infrastructure = true) {
    const requestId = this.current
    this.reset()
    const failure: OverlayFailure = { stage, message: (cause instanceof Error ? cause.message : String(cause)).slice(0, 4000), infrastructure }
    console.error('Oriel overlay receiver', { requestId, ...failure })
    await this.invoke('overlay_fail', { requestId: requestId ?? null, failure }).catch(() => {})
  }
  async poll() {
    if (this.stopped || this.polling) return
    this.polling = true
    try {
      const status = await this.invoke<OverlayStatus>('overlay_status')
      if (this.stopped) return
      const pending = status.request
      if (status.disabled || !pending || ['closed', 'failed'].includes(pending.phase)) {
        if (this.current !== undefined) this.reset()
        return
      }
      if (this.current === pending.requestId) return
      const request = await this.invoke<OverlayRequest | null>('overlay_pending')
      if (!request || this.stopped) return
      this.reset()
      this.current = request.requestId
      const token = this.generation
      void this.open(request, token)
    } catch (cause) { if (!this.stopped) await this.fail('receive', cause) }
    finally { this.polling = false }
  }
  private async open(request: OverlayRequest, token: number) {
    let stage = 'content'
    try {
      const mount = await this.prepare(request)
      if (token !== this.generation || this.stopped) return
      // A missing reset event must not let a late content load mount or show.
      const status = await this.invoke<OverlayStatus>('overlay_status')
      if (token !== this.generation || this.stopped) return
      if (status.disabled || status.request?.requestId !== request.requestId || status.request.phase !== 'received') { this.reset(); return }
      stage = 'mount'
      await mount()
      if (token !== this.generation || this.stopped) return
      stage = 'show'
      const shown = await this.invoke<OverlayStatus>('overlay_presented', { requestId: request.requestId })
      if (token === this.generation && shown.request?.phase !== 'shown') this.reset()
    } catch (cause) {
      if (token === this.generation && !this.stopped) await this.fail(stage, cause, !(cause instanceof OverlayContentError))
    }
  }
}
