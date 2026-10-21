<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import AppIcon from './AppIcon.vue'
import { getDesktopWindow } from '../lib/desktop'

const desktopWindow = getDesktopWindow()
const maximized = ref(false)
const pending = ref(false)
const errorMessage = ref('')
let resizeFrame = 0

async function syncMaximized() {
  if (!desktopWindow) return
  try {
    maximized.value = await desktopWindow.isMaximized()
  } catch (error) {
    console.error('Could not read the Oriel window state.', error)
  }
}

function handleResize() {
  cancelAnimationFrame(resizeFrame)
  resizeFrame = requestAnimationFrame(() => { void syncMaximized() })
}

async function runAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  if (!desktopWindow || pending.value) return
  pending.value = true
  errorMessage.value = ''
  try {
    await desktopWindow[action]()
    if (action === 'toggleMaximize') await syncMaximized()
  } catch (error) {
    errorMessage.value = '窗口操作失败，请重试。'
    console.error('Could not change the Oriel window state.', error)
  } finally {
    pending.value = false
  }
}

onMounted(() => {
  if (!desktopWindow) return
  void syncMaximized()
  window.addEventListener('resize', handleResize)
})

onUnmounted(() => {
  cancelAnimationFrame(resizeFrame)
  window.removeEventListener('resize', handleResize)
})
</script>

<template>
  <div v-if="desktopWindow" class="window-controls" role="group" aria-label="窗口控制">
    <button type="button" aria-label="最小化" title="最小化" :disabled="pending" @click="runAction('minimize')">
      <AppIcon name="minimize" />
    </button>
    <button
      type="button"
      :aria-label="maximized ? '还原窗口' : '最大化'"
      :title="maximized ? '还原窗口' : '最大化'"
      :disabled="pending"
      @click="runAction('toggleMaximize')"
    >
      <AppIcon :name="maximized ? 'restore' : 'maximize'" />
    </button>
    <button class="window-close" type="button" aria-label="关闭" title="关闭" :disabled="pending" @click="runAction('close')">
      <AppIcon name="close" />
    </button>
    <p v-if="errorMessage" class="window-error" role="alert">{{ errorMessage }}</p>
  </div>
</template>
