// Platform-independent checks for stale favicon results and overlay handshakes.
// Run in the documented WSL isolated copy when local native bindings are absent.
import assert from 'node:assert/strict'
import { createServer } from 'vite'
import 'vue' // Initialize Vue's reactivity before installing the minimal document mock.
const listeners = new Map()
const calls = []
let resolveFavicon
const emit = (name, payload) => { for (const handler of listeners.get(name) ?? []) handler({ payload }) }
class Element { isConnected = true; focusCount = 0; focus() { this.focusCount++ } }
const trigger = new Element()
globalThis.HTMLElement = Element
globalThis.document = { activeElement: trigger }
globalThis.requestAnimationFrame = () => { throw new Error('Hidden overlay must not wait for animation frames') }
globalThis.window = { __TAURI__: {
  core: { invoke: async (command, args) => {
    calls.push([command, args])
    switch (command) {
      case 'web_favicon': return new Promise(resolve => { resolveFavicon = resolve })
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

  const { checkOverlayState } = await import('./lib/check-overlay-state.mjs')
  await checkOverlayState(vite, { listeners, calls, trigger, emit })
} finally { await vite.close() }
