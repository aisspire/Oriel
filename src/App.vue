<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppIcon from './components/AppIcon.vue'
import OrielShowcase from './components/OrielShowcase.vue'
import WindowChrome from './components/WindowChrome.vue'
import WorkspaceDialog from './components/WorkspaceDialog.vue'
import { useAppearance } from './composables/useAppearance'
import { getDesktopApi } from './lib/desktop'
import { resolvePluginView } from './lib/pluginViews'
import { errorText, hideWebSurface, hostCommand, loadWorkspace, type WorkspaceSnapshot } from './lib/workspace'

const { theme, iconSource, reducedMotion, paused, motionPaused, toggleTheme } = useAppearance()
const workspace = ref<WorkspaceSnapshot>()
const selectedId = ref('home')
const current = computed(() => workspace.value?.entries.find(e => e.id === selectedId.value))
const modal = ref<'add' | 'edit' | 'settings' | null>(null)
const error = ref('')
const reloading = ref(false)
const desktop = Boolean(getDesktopApi())

async function reload(initial = false) {
  if (reloading.value) return
  reloading.value = true; error.value = ''
  try {
    workspace.value = initial || !desktop ? await loadWorkspace() : await hostCommand('workspace_reload')
    if (selectedId.value !== 'home' && !current.value) selectedId.value = 'home'
  } catch (cause) { error.value = errorText(cause) }
  finally { reloading.value = false }
}
async function openDialog(mode: 'add' | 'edit' | 'settings') {
  if (modal.value || !desktop) return
  try { await hideWebSurface(); modal.value = mode }
  catch (cause) { error.value = errorText(cause) }
}
function saved(snapshot: WorkspaceSnapshot) {
  if (modal.value === 'add') {
    const added = snapshot.entries.find(entry => entry.editable && !workspace.value?.entries.some(old => old.id === entry.id))
    if (added) selectedId.value = added.id
  }
  workspace.value = snapshot
  if (selectedId.value !== 'home' && !current.value) selectedId.value = 'home'
}
onMounted(() => { void reload(true) })
</script>

<template>
  <a class="skip-link" href="#workspace">跳到主要内容</a>
  <div class="app-shell">
    <aside class="sidebar" aria-label="主侧边栏" data-tauri-drag-region>
      <a class="brand-link" href="#workspace" aria-label="Oriel 首页" @click="selectedId = 'home'">
        <img :src="iconSource" alt="" width="44" height="44" />
      </a>
      <span class="sidebar-divider" aria-hidden="true"></span>
      <nav class="primary-nav" aria-label="主导航">
        <button class="nav-link" :class="{ 'is-active': !current }" type="button" :aria-current="!current ? 'page' : undefined" aria-label="首页" title="首页" @click="selectedId = 'home'"><AppIcon name="home" /></button>
        <button
          v-for="entry in workspace?.entries" :key="entry.id" class="nav-link" :class="{ 'is-active': selectedId === entry.id }"
          type="button" :aria-current="selectedId === entry.id ? 'page' : undefined" :aria-label="entry.title" :title="entry.title"
          @click="selectedId = entry.id"
        >
          <AppIcon v-if="entry.kind === 'browser'" name="globe" />
          <span v-else class="site-monogram" aria-hidden="true">{{ Array.from(entry.title)[0] }}</span>
        </button>
        <button v-if="workspace" class="nav-link add-webpage" type="button" aria-label="添加网页" title="添加网页" :disabled="!desktop || !workspace.writable" @click="openDialog('add')"><AppIcon name="plus" /></button>
      </nav>
      <div class="sidebar-bottom">
        <button v-if="workspace && desktop" class="icon-button" type="button" aria-label="重新加载接入" title="重新加载接入" :disabled="reloading" @click="reload()"><AppIcon name="refresh" /></button>
        <button v-if="workspace && desktop" class="icon-button" type="button" aria-label="浏览设置" title="浏览设置" @click="openDialog('settings')"><AppIcon name="settings" /></button>
        <button v-if="!reducedMotion && !current" class="icon-button" type="button" :aria-label="paused ? '播放首页动效' : '暂停首页动效'" :title="paused ? '播放首页动效' : '暂停首页动效'" :aria-pressed="paused" @click="paused = !paused"><AppIcon :name="paused ? 'play' : 'pause'" /></button>
        <button class="icon-button" type="button" :aria-label="theme === 'light' ? '切换为深色外观' : '切换为浅色外观'" :title="theme === 'light' ? '切换为浅色外观' : '切换为浅色外观'" @click="toggleTheme"><AppIcon :name="theme === 'light' ? 'moon' : 'sun'" /></button>
      </div>
    </aside>
    <main id="workspace" class="workspace" :class="{ 'has-web-entry': current }" tabindex="-1">
      <WindowChrome />
      <div v-if="error || workspace?.diagnostics.length" class="workspace-notice" role="alert">
        <p v-if="error">{{ error }}</p>
        <p v-for="message in workspace?.diagnostics" :key="message">{{ message }}</p>
        <span v-if="workspace?.diagnostics.length && workspace.configPath" class="config-location">配置：{{ workspace.configPath }}</span>
        <button class="text-button" type="button" :disabled="reloading" @click="reload()">重新加载</button>
      </div>
      <component
        :is="resolvePluginView(current.kind)" v-if="current && workspace" :entry="current" :supported="workspace.browserSupported"
        :writable="workspace.writable" :suspended="modal !== null" @edit="openDialog('edit')" @settings="openDialog('settings')" @add="openDialog('add')"
      />
      <OrielShowcase v-else :paused="motionPaused" />
    </main>
  </div>
  <WorkspaceDialog v-if="modal && workspace" :mode="modal" :entry="modal === 'edit' ? current : undefined" :workspace="workspace" @close="modal = null" @saved="saved" />
</template>
