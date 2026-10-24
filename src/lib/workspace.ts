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
  documentation: string | null
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
  favicons: Record<string, number[]>
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
    favicons: {}, theme: null,
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
