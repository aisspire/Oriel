// Platform-independent checks for stale favicon results and overlay handshakes.
// Run in the documented WSL isolated copy when local native bindings are absent.
import assert from 'node:assert/strict'
import { createServer } from 'vite'
import 'vue' // Initialize Vue's reactivity before installing the minimal document mock.
const listeners = new Map()
const calls = []
let supported = true
let ready = true
let present = true
let request
let resolveFavicon
const emit = (name, payload) => { for (const handler of listeners.get(name) ?? []) handler({ payload }) }
class Element { isConnected = true; focusCount = 0; focus() { this.focusCount++ } }
const trigger = new Element()
globalThis.HTMLElement = Element
globalThis.document = { activeElement: trigger }
globalThis.requestAnimationFrame = callback => { callback(); return 0 }
globalThis.window = { __TAURI__: {
  core: { invoke: async (command, args) => {
    calls.push([command, args])
    switch (command) {
      case 'web_favicon': return new Promise(resolve => { resolveFavicon = resolve })
      case 'overlay_supported': return supported
      case 'overlay_ready': return ready
      case 'overlay_show':
        request = args.request
        if (present) { emit('overlay-active', true); emit('overlay-presented', request.requestId) }
        return
      case 'overlay_hide': emit('overlay-active', false); return
      default: throw new Error(`Unexpected command ${command}`)
    }
  } },
  event: { listen: async (name, handler) => {
    const handlers = listeners.get(name) ?? new Set()
    handlers.add(handler); listeners.set(name, handlers)
    return () => handlers.delete(handler)
  } },
} }
const vite = await createServer({ configFile: false, server: { middlewareMode: true }, appType: 'custom' })
try {
  const icons = await vite.ssrLoadModule('/src/lib/favicons.ts')
  const workspace = await vite.ssrLoadModule('/src/lib/workspace.ts')
  const a = 'https://a.example/', b = 'https://b.example/'
  icons.reconcileFavicons([{ id: 'page', url: a }])
  const old = icons.requestFavicon('page', a)
  assert.equal(icons.requestFavicon('page', a), old)
  icons.reconcileFavicons([{ id: 'page', url: b }])
  resolveFavicon({ entryId: 'page', registeredUrl: a, dataUrl: 'old' })
  assert.equal(await old, null)
  assert.equal(icons.getCachedFavicon('page', a), undefined)
  const fresh = icons.requestFavicon('page', b)
  icons.setCachedFavicon({ entryId: 'page', registeredUrl: b, dataUrl: 'event' })
  resolveFavicon({ entryId: 'page', registeredUrl: b, dataUrl: 'late-cache' })
  assert.equal(await fresh, null)
  assert.equal(icons.getCachedFavicon('page', b), 'event')
  icons.reconcileFavicons([])
  assert.equal(icons.getCachedFavicon('page', b), undefined)
  icons.reconcileFavicons([{ id: 'page', url: b }])
  const deleted = icons.requestFavicon('page', b)
  icons.reconcileFavicons([])
  icons.reconcileFavicons([{ id: 'page', url: b }])
  resolveFavicon({ entryId: 'page', registeredUrl: b, dataUrl: 'deleted' })
  assert.equal(await deleted, null)
  console.log('PASS favicon identity, URL invalidation, deduplication, event priority and delete/re-add generations')

  const opening = workspace.showOverlay({ mode: 'edit', entryId: 'page', confirmRemove: true })
  assert.equal(await workspace.showOverlay({ mode: 'settings' }), true)
  assert.equal(await opening, true)
  assert.equal(calls.filter(([c]) => c === 'overlay_show').length, 1)
  assert.deepEqual(request, { mode: 'edit', entryId: 'page', confirmRemove: true, requestId: 1 })
  assert.equal(listeners.get('overlay-presented').size, 0)
  emit('overlay-active', false)
  assert.equal(trigger.focusCount, 1)
  supported = false
  assert.equal(await workspace.showOverlay({ mode: 'add' }), false)
  supported = true; ready = false
  assert.equal(await workspace.showOverlay({ mode: 'settings' }), false)
  assert.equal(calls.at(-1)[0], 'overlay_hide')
  ready = true
  assert.equal(await workspace.showOverlay({ mode: 'preparations', entryId: 'page' }), true)
  emit('overlay-active', false)
  present = false
  assert.equal(await workspace.showOverlay({ mode: 'add' }), false)
  const timedOut = request.requestId
  emit('overlay-presented', timedOut)
  present = true
  assert.equal(await workspace.showOverlay({ mode: 'settings' }), true)
  assert.ok(request.requestId > timedOut)
  assert.equal(listeners.get('overlay-presented').size, 0)
  console.log('PASS overlay typed requests, repeat-open guard, focus restoration, unsupported platform, readiness/presentation timeouts and recovery')
} finally { await vite.close() }
