import { createApp } from 'vue'
import App from './App.vue'
import OverlayApp from './OverlayApp.vue'
import './styles.css'
import { getDesktopApi } from './lib/desktop'

const urlParams = new URLSearchParams(window.location.search)
const isOverlay = urlParams.get('overlay') === '1' || window.location.hash === '#overlay'

if (isOverlay) {
  document.documentElement.classList.add('is-overlay')
  document.body.classList.add('is-overlay')
}

const app = createApp(isOverlay ? OverlayApp : App)
if (isOverlay) {
  const report = (cause: unknown) => {
    console.error('Oriel overlay initialization failed:', cause)
    void getDesktopApi()?.core.invoke('overlay_fail', {
      requestId: null,
      failure: { stage: 'initialization', message: String(cause).slice(0, 4000), infrastructure: true },
    }).catch(() => {})
  }
  app.config.errorHandler = report
  window.addEventListener('unhandledrejection', event => report(event.reason))
  window.addEventListener('error', event => report(event.message))
  try { app.mount('#app') } catch (cause) { report(cause) }
} else { app.mount('#app') }
