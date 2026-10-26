import browserManifest from '../../plugins/builtin/browser/plugin.json'
import { getDesktopApi } from './desktop'

export type ProxyPolicy = { mode: 'inherit' | 'system' | 'direct' } | { mode: 'custom'; address: string }
export interface Preparation {
  path: string
  runner: 'auto' | 'direct' | 'cmd' | 'powershell' | 'custom'
  args: string[]
  cwd?: string | null
  mode: 'once' | 'persistent'
  timeoutSeconds: number
  interpreter?: { program: string; args: string[] } | null
}
export interface PreparationState {
  entryId: string; title: string; generation: number
  state: 'starting' | 'ready' | 'completed' | 'failed' | 'stopping' | 'stopped'
  url: string | null; error: string | null; exitCode: number | null; cleaned: boolean; configurationChanged: boolean
}
export interface PluginSlot {
  target: 'sidebar:badge' | 'header:action' | string
  id?: string
  title?: string
  icon?: string
  action?: string
  badgeType?: 'dot' | 'count' | 'status' | string
  entryId?: string
}

export interface WebEntry {
  id: string
  title: string
  kind: 'browser' | 'web-page'
  presentation: 'browser' | 'application'
  url: string | null
  proxy: ProxyPolicy
  preparation?: Preparation | null
  editable: boolean
  pluginName: string
  pluginIcon?: 'terminal' | null
  documentation: string | null
  slots?: PluginSlot[]
}

export function getSlots(entry: WebEntry | undefined, target: string): PluginSlot[] {
  if (!entry?.slots) return []
  return entry.slots.filter(s => s.target === target)
}
export interface WorkspaceSnapshot {
  entries: WebEntry[]
  defaultProxy: ProxyPolicy
  pluginDirectories: string[]
  pluginRoots: string[]
  defaultPluginRoot: string
  diagnostics: string[]
  writable: boolean
  browserSupported: boolean
  configPath: string
  browserSidebar: BrowserSidebar
  theme: 'light' | 'dark' | null
}
export interface BrowserSidebar { collapsed: boolean; width: number }
export const defaultBrowserSidebar: BrowserSidebar = { collapsed: false, width: 240 }
export interface PluginViewProps {
  entry: WebEntry
  workspace: WorkspaceSnapshot
  selectedId: string
  supported: boolean
  writable: boolean
  suspended: boolean
}
export interface WebSessionState {
  entryId: string
  url: string
  title: string
  canGoBack: boolean
  canGoForward: boolean
  loading: boolean
  error: string | null
}
export interface ViewBounds { x: number; y: number; width: number; height: number }

export function hostCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const api = getDesktopApi()
  if (!api) return Promise.reject(new Error('请在 Oriel 桌面应用中使用此功能。'))
  return api.core.invoke<T>(command, args)
}
export function loadWorkspace(): Promise<WorkspaceSnapshot> {
  if (getDesktopApi()) return hostCommand('workspace_snapshot')
  return Promise.resolve({
    entries: browserManifest.entries.map(entry => ({
      id: `${browserManifest.id}.${entry.id}`, title: entry.title, kind: 'browser',
      url: null, proxy: { mode: 'inherit' }, editable: false, pluginName: browserManifest.name,
      presentation: 'browser', documentation: null,
    })),
    defaultProxy: { mode: 'system' }, pluginDirectories: [], diagnostics: [],
    pluginRoots: [], defaultPluginRoot: '',
    writable: false, browserSupported: false, configPath: '',
    browserSidebar: { ...defaultBrowserSidebar },
    theme: null,
  })
}
export function emptySession(entry: WebEntry): WebSessionState {
  return { entryId: entry.id, url: entry.url ?? '', title: entry.title, canGoBack: false, canGoForward: false, loading: false, error: null }
}
export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}
export function hideWebSurface(): Promise<unknown> {
  if (!getDesktopApi()) return Promise.resolve()
  return hostCommand('web_show', { entryId: null, bounds: { x: 0, y: 0, width: 1, height: 1 }, visible: false })
}

export interface OverlayShowOptions {
  mode: 'settings' | 'add' | 'edit' | 'preparations' | 'help'
  entryId?: string
  confirmRemove?: boolean
}
export interface OverlayRequest extends OverlayShowOptions { requestId: number }
let overlaySequence = 0
let overlayBusy = false
let overlayActive = false
let overlayListener: Promise<unknown> | undefined
let overlayTrigger: HTMLElement | null = null

export async function showOverlay(options: OverlayShowOptions): Promise<boolean> {
  const api = getDesktopApi()
  if (!api) return false
  // All callers share this lock, including plugin help and the main shell.
  if (overlayBusy || overlayActive) return true
  overlayBusy = true
  const requestId = ++overlaySequence
  let unlisten: (() => void) | undefined
  let timeout: ReturnType<typeof setTimeout> | undefined
  let expired = false
  overlayTrigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
  try {
    const operation = async () => {
      if (!await hostCommand<boolean>('overlay_supported')) return false
      if (expired) throw new Error('悬浮层请求已过期')
      overlayListener ??= api.event.listen<boolean>('overlay-active', ({ payload }) => {
        overlayActive = payload
        if (!payload) requestAnimationFrame(() => { if (overlayTrigger?.isConnected) overlayTrigger.focus() })
      }).catch(cause => { overlayListener = undefined; throw cause })
      await overlayListener
      while (!await hostCommand<boolean>('overlay_ready')) {
        if (expired) throw new Error('悬浮层初始化超时')
        await new Promise(resolve => setTimeout(resolve, 50))
      }
      if (expired) throw new Error('悬浮层请求已过期')
      let confirmPresented: () => void = () => {}
      const presented = new Promise<void>(resolve => { confirmPresented = resolve })
      const dispose = await api.event.listen<number>('overlay-presented', ({ payload }) => {
        if (!expired && payload === requestId) confirmPresented()
      })
      if (expired) { dispose(); throw new Error('悬浮层请求已过期') }
      unlisten = dispose
      await hostCommand('overlay_show', { request: { ...options, requestId } })
      await presented
      return true
    }
    const shown = await Promise.race([
      operation(),
      new Promise<never>((_, reject) => { timeout = setTimeout(() => { expired = true; reject(new Error('悬浮层呈现超时')) }, 5000) }),
    ])
    overlayActive = shown
    return shown
  } catch (cause) {
    expired = true
    await hostCommand('overlay_hide', { requestId }).catch(() => {})
    overlayActive = false
    console.warn('Overlay unavailable; opening local dialog:', cause)
    return false
  } finally {
    clearTimeout(timeout)
    unlisten?.()
    overlayBusy = false
  }
}
