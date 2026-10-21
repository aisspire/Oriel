<script setup lang="ts">
import AppIcon from './components/AppIcon.vue'
import OrielShowcase from './components/OrielShowcase.vue'
import WindowChrome from './components/WindowChrome.vue'
import { useAppearance } from './composables/useAppearance'

const { theme, iconSource, reducedMotion, paused, motionPaused, toggleTheme } = useAppearance()
</script>

<template>
  <a class="skip-link" href="#home">跳到首页内容</a>
  <div class="app-shell">
    <aside class="sidebar" aria-label="主侧边栏" data-tauri-drag-region>
      <a class="brand-link" href="#home" aria-label="Oriel 首页">
        <img :src="iconSource" alt="" width="44" height="44" />
      </a>

      <span class="sidebar-divider" aria-hidden="true"></span>

      <nav class="primary-nav" aria-label="主导航">
        <a class="nav-link is-active" href="#home" aria-current="page" aria-label="首页" title="首页">
          <AppIcon name="home" />
        </a>
      </nav>

      <div class="sidebar-bottom">
        <button
          v-if="!reducedMotion"
          class="icon-button"
          type="button"
          :aria-label="paused ? '播放首页动效' : '暂停首页动效'"
          :title="paused ? '播放首页动效' : '暂停首页动效'"
          :aria-pressed="paused"
          @click="paused = !paused"
        >
          <AppIcon :name="paused ? 'play' : 'pause'" />
        </button>
        <button
          class="icon-button"
          type="button"
          :aria-label="theme === 'light' ? '切换为深色外观' : '切换为浅色外观'"
          :title="theme === 'light' ? '切换为深色外观' : '切换为浅色外观'"
          @click="toggleTheme"
        >
          <AppIcon :name="theme === 'light' ? 'moon' : 'sun'" />
        </button>
      </div>
    </aside>

    <main id="home" class="workspace" tabindex="-1">
      <WindowChrome />
      <OrielShowcase :paused="motionPaused" />
    </main>
  </div>
</template>
