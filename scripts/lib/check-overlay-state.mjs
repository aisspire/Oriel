import assert from 'node:assert/strict'
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const until = async predicate => {
  for (let i = 0; i < 150; i++) { if (await predicate()) return; await sleep(10) }
  throw new Error('Overlay state check timed out')
}
export async function checkOverlayState(vite, { calls, trigger, emit }) {
  const { OverlayClient } = await vite.ssrLoadModule('/src/lib/overlay.ts')
  const { OverlayReceiver, OverlayContentError } = await vite.ssrLoadModule('/src/lib/overlayReceiver.ts')
  const { dialogDismissHandlers } = await vite.ssrLoadModule('/src/lib/dialogDismiss.ts')
  let status, request, autoShow, mounted, failedStage, shownCount
  function reset() {
    status = { supported: true, ready: true, disabled: null, request: null }
    request = undefined; autoShow = true; mounted = 0; failedStage = undefined; shownCount = 0
  }
  const snapshot = () => structuredClone(status)
  const invoke = async (command, args) => {
    calls.push([command, args])
    switch (command) {
      case 'overlay_status': return snapshot()
      case 'overlay_show':
        request = args.request
        status.request = { requestId: request.requestId, mode: request.mode, phase: autoShow ? 'shown' : 'queued', elapsedMs: 0, failure: null }
        if (autoShow) shownCount++
        return snapshot() // Deliberately never emits presentation/active events.
      case 'overlay_pending': status.request.phase = 'received'; return structuredClone(request)
      case 'overlay_presented':
        if (status.request?.requestId !== args.requestId || status.request.phase !== 'received') throw new Error('expired')
        status.request.phase = 'shown'; shownCount++; return snapshot()
      case 'overlay_fail':
        failedStage = args.failure.stage
        if (args.failure.infrastructure) status.disabled = args.failure
        if (status.request?.requestId === args.requestId) { status.request.phase = 'failed'; status.request.failure = args.failure }
        return snapshot()
      case 'overlay_hide':
        if (status.request?.requestId === args.requestId && status.request.phase !== 'failed') status.request.phase = 'closed'
        return snapshot()
      default: throw new Error(`Unexpected command ${command}`)
    }
  }
  window.__TAURI__.core.invoke = invoke
  const close = async client => {
    await invoke('overlay_hide', { requestId: request.requestId })
    await until(() => !client.active.value)
  }
  reset()
  let client = new OverlayClient()
  const opening = client.show({ mode: 'edit', entryId: 'page', confirmRemove: true })
  assert.equal(await client.show({ mode: 'settings' }), true)
  assert.equal(await opening, true)
  assert.deepEqual(request, { mode: 'edit', entryId: 'page', confirmRemove: true, requestId: 1 })
  assert.equal(shownCount, 1)
  const focusBefore = trigger.focusCount
  await close(client)
  assert.equal(trigger.focusCount, focusBefore + 1)
  assert.equal(await client.show({ mode: 'settings' }), true)
  await close(client)
  console.log('PASS command-authoritative opening/closing with every event lost, duplicate opens, focus and reuse')

  reset(); client = new OverlayClient(); status.supported = false
  assert.equal(await client.show({ mode: 'add' }), false)
  reset(); client = new OverlayClient(); status.disabled = { stage: 'initialization', message: 'init failed', infrastructure: true }
  const started = performance.now()
  assert.equal(await client.show({ mode: 'add' }), false)
  assert.ok(performance.now() - started < 1000)
  const callCount = calls.length
  assert.equal(await client.show({ mode: 'settings' }), false)
  assert.equal(calls.length, callCount)
  console.log('PASS unsupported platform, immediate initialization failure and session fast fallback')

  reset(); autoShow = false; status.ready = false; client = new OverlayClient()
  let receiver = new OverlayReceiver(invoke, async () => async () => { mounted++ }, () => {})
  const healthy = client.show({ mode: 'help', entryId: 'page' })
  await until(() => request)
  status.ready = true // A request submitted before the hidden renderer finishes initialization.
  await receiver.poll()
  await until(() => mounted === 1)
  assert.equal(await healthy, true)
  await close(client); await receiver.poll(); receiver.stop()
  console.log('PASS receiver pulls and mounts without events or requestAnimationFrame')

  reset(); autoShow = false; client = new OverlayClient()
  receiver = new OverlayReceiver(invoke, async () => { throw new OverlayContentError('目标网页已不存在。') }, () => {})
  const business = client.show({ mode: 'edit', entryId: 'removed' })
  await until(() => request); await receiver.poll()
  await assert.rejects(business, /目标网页已不存在/)
  assert.equal(status.disabled, null); receiver.stop()
  autoShow = true; assert.equal(await client.show({ mode: 'settings' }), true); await close(client)
  console.log('PASS business failure reports its reason and leaves overlay usable')

  reset(); autoShow = false; client = new OverlayClient()
  receiver = new OverlayReceiver(invoke, async () => async () => { throw new Error('mount failed') }, () => {})
  const broken = client.show({ mode: 'settings' })
  await until(() => request); await receiver.poll()
  assert.equal(await broken, false); assert.equal(failedStage, 'mount'); receiver.stop()
  const afterBroken = calls.length
  assert.equal(await client.show({ mode: 'add' }), false); assert.equal(calls.length, afterBroken)
  console.log('PASS mount failure reports immediately and disables further native attempts')

  reset(); autoShow = false; client = new OverlayClient()
  let finishMount
  receiver = new OverlayReceiver(invoke, async () => () => new Promise(resolve => { finishMount = resolve }), () => {})
  const cancelledMount = client.show({ mode: 'preparations' })
  await until(() => request); await receiver.poll(); await until(() => finishMount)
  await invoke('overlay_hide', { requestId: request.requestId }); await receiver.poll()
  finishMount(); await sleep(30)
  assert.equal(await cancelledMount, true) // Native cancellation is handled, never a second local dialog.
  assert.equal(shownCount, 0); await until(() => !client.active.value); receiver.stop()
  console.log('PASS cancellation during mounting cannot confirm or re-show the window')

  reset(); autoShow = false; client = new OverlayClient()
  let finishLoad
  receiver = new OverlayReceiver(invoke, () => new Promise(resolve => { finishLoad = resolve }), () => {})
  const late = client.show({ mode: 'add' })
  await until(() => request); await receiver.poll(); await until(() => finishLoad)
  const timedOut = request.requestId
  assert.equal(await late, false) // Exercise the real five-second final deadline once.
  finishLoad(async () => { mounted++ })
  await sleep(50)
  assert.equal(mounted, 0); assert.equal(shownCount, 0)
  await assert.rejects(invoke('overlay_presented', { requestId: timedOut }), /expired/)
  emit('overlay-active', true) // Late events cannot authorize native state.
  assert.equal(client.active.value, false)
  assert.equal(await client.show({ mode: 'settings' }), false); receiver.stop()
  console.log('PASS timeout cancellation, late content/confirmation/events and same-session fallback')

  // Geometry tests exercise the actual shared pointer handlers and cancel path.
  class Dialog extends EventTarget {
    open = true; cancels = 0; busy = false
    getBoundingClientRect() { return { left: 10, right: 90, top: 10, bottom: 90 } }
    constructor() { super(); this.addEventListener('cancel', e => { e.preventDefault(); if (!this.busy) this.cancels++ }) }
  }
  const dialog = new Dialog(), nested = new Dialog()
  let top = dialog
  const handlers = dialogDismissHandlers(() => dialog, () => top)
  const pointer = (x, id = 1) => ({ clientX: x, clientY: x, pointerId: id, isPrimary: true, button: 0, preventDefault() {}, stopPropagation() {} })
  function gesture(from, to, upId = 1) { handlers.pointerdown(pointer(from)); handlers.pointerup(pointer(to, upId)) }
  gesture(50, 50); gesture(50, 0); gesture(0, 50); gesture(0, 0, 2)
  assert.equal(dialog.cancels, 0)
  handlers.pointerdown(pointer(0)); handlers.pointercancel(); handlers.pointerup(pointer(0)); assert.equal(dialog.cancels, 0)
  top = nested; gesture(0, 0); assert.equal(dialog.cancels, 0)
  top = dialog; dialog.busy = true; gesture(0, 0); assert.equal(dialog.cancels, 0)
  dialog.busy = false; gesture(0, 0); assert.equal(dialog.cancels, 1)
  handlers.pointerdown(pointer(0)); dialog.open = false; handlers.pointerup(pointer(0)); assert.equal(dialog.cancels, 1)
  console.log('PASS inside/outside gestures, drag out/in, pointer identity/cancel, top dialog and save guard')
}
