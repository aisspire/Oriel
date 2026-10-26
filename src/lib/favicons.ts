import { shallowRef } from 'vue'
import { getDesktopApi } from './desktop'
import { hostCommand, type WebEntry } from './workspace'

export interface FaviconResult { entryId: string; registeredUrl: string; dataUrl: string }
const faviconCache = shallowRef(new Map<string, string>())
const registrations = new Map<string, string>()
const generations = new Map<string, number>()
const pendingRequests = new Map<string, Promise<string | null>>()
const keyFor = (entryId: string, url: string) => JSON.stringify([entryId, url])

export function reconcileFavicons(entries: Pick<WebEntry, 'id' | 'url'>[]): void {
  const next = new Map(entries.filter(e => e.url).map(e => [e.id, e.url!]))
  for (const [id, url] of registrations) {
    if (next.get(id) !== url) {
      const key = keyFor(id, url)
      const cache = new Map(faviconCache.value)
      cache.delete(key)
      faviconCache.value = cache
      generations.set(key, (generations.get(key) ?? 0) + 1)
      pendingRequests.delete(key)
    }
  }
  registrations.clear()
  for (const [id, url] of next) registrations.set(id, url)
}
export function getCachedFavicon(entryId: string, registeredUrl: string): string | undefined {
  return faviconCache.value.get(keyFor(entryId, registeredUrl))
}
export function setCachedFavicon(result: FaviconResult): void {
  if (registrations.get(result.entryId) !== result.registeredUrl) return
  const key = keyFor(result.entryId, result.registeredUrl)
  generations.set(key, (generations.get(key) ?? 0) + 1)
  faviconCache.value = new Map(faviconCache.value).set(key, result.dataUrl)
}
export function requestFavicon(entryId: string, registeredUrl: string): Promise<string | null> {
  const key = keyFor(entryId, registeredUrl)
  const cached = getCachedFavicon(entryId, registeredUrl)
  if (cached) return Promise.resolve(cached)
  const pending = pendingRequests.get(key)
  if (pending) return pending
  if (!getDesktopApi() || registrations.get(entryId) !== registeredUrl) return Promise.resolve(null)
  const generation = generations.get(key) ?? 0
  const request = hostCommand<FaviconResult | null>('web_favicon', { entryId })
    .then(result => {
      if (!result || result.entryId !== entryId || result.registeredUrl !== registeredUrl
        || registrations.get(entryId) !== registeredUrl || (generations.get(key) ?? 0) !== generation) return null
      setCachedFavicon(result)
      return result.dataUrl
    })
    .catch(() => null)
    .finally(() => { if (pendingRequests.get(key) === request) pendingRequests.delete(key) })
  pendingRequests.set(key, request)
  return request
}
