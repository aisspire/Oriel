import BrowserView from '../../plugins/builtin/browser/BrowserView.vue'
import BrowserWorkspace from '../../plugins/builtin/browser/BrowserWorkspace.vue'
import type { WebEntry } from './workspace'

// A capability maps to a renderer. Individual plugin IDs and URLs never become host routes.
const renderers = { browser: BrowserWorkspace, 'web-page': BrowserView }
export function resolvePluginView(kind: WebEntry['kind']) { return renderers[kind] }
