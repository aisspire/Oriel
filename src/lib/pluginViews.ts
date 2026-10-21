import BrowserView from '../../plugins/builtin/browser/BrowserView.vue'
import BrowserWorkspace from '../../plugins/builtin/browser/BrowserWorkspace.vue'
import ToolApplicationView from '../components/ToolApplicationView.vue'
import type { WebEntry } from './workspace'

// A capability maps to a renderer. Individual plugin IDs and URLs never become host routes.
const renderers = { browser: BrowserWorkspace, 'web-page': BrowserView }
export function resolvePluginView(entry: WebEntry) {
  return entry.kind === 'web-page' && entry.presentation === 'application' ? ToolApplicationView : renderers[entry.kind]
}
