<script setup lang="ts">
import WindowControls from './WindowControls.vue'
import { getDesktopWindow } from '../lib/desktop'

const desktopWindow = getDesktopWindow()
</script>

<template>
  <header v-if="desktopWindow" class="window-chrome">
    <div class="window-drag-region" data-tauri-drag-region></div>
    <WindowControls />
  </header>
</template>
