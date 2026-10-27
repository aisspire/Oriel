import { createApp } from 'vue'
import App from './App.vue'
import OverlayApp from './OverlayApp.vue'
import './styles.css'

const urlParams = new URLSearchParams(window.location.search)
const isOverlay = urlParams.get('overlay') === '1' || window.location.hash === '#overlay'

if (isOverlay) {
  document.documentElement.classList.add('is-overlay')
  document.body.classList.add('is-overlay')
}

createApp(isOverlay ? OverlayApp : App).mount('#app')
