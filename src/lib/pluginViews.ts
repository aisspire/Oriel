import BrowserView from '../../plugins/builtin/browser/BrowserView.vue'
import type { WebEntry } from './workspace'

// A capability maps to a renderer. Individual plugin IDs and URLs never become host routes.
const renderers = { browser: BrowserView, 'web-page': BrowserView }
export function resolvePluginView(kind: WebEntry['kind']) { return renderers[kind] }
