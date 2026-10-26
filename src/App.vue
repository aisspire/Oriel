<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch, nextTick } from 'vue'
import AppIcon from './components/AppIcon.vue'
import SiteIcon from './components/SiteIcon.vue'
import OrielShowcase from './components/OrielShowcase.vue'
import WindowChrome from './components/WindowChrome.vue'
import WorkspaceDialog from './components/WorkspaceDialog.vue'
import PreparationPanel from './components/PreparationPanel.vue'
import { useAppearance } from './composables/useAppearance'
import { getDesktopApi } from './lib/desktop'
import { reconcileFavicons, setCachedFavicon } from './lib/favicons'
import { resolvePluginView } from './lib/pluginViews'
import { errorText, getSlots, hideWebSurface, hostCommand, loadWorkspace, showOverlay, type WebEntry, type WorkspaceSnapshot } from './lib/workspace'

const { theme, iconSource, reducedMotion, paused, motionPaused, toggleTheme, setTheme, appearanceError } = useAppearance()
const workspace = ref<WorkspaceSnapshot>()
watch(() => workspace.value?.entries, entries => reconcileFavicons(entries ?? []), { flush: 'sync' })
const selectedId = ref('home')
const browserSelectedId = ref('')
const current = computed(() => workspace.value?.entries.find(e => e.id === selectedId.value))
const primaryEntries = computed(() => workspace.value?.entries.filter(entry => !entry.editable) ?? [])
const modal = ref<'add' | 'edit' | 'settings' | null>(null)
const dialogEntry = ref<WebEntry>()
const confirmRemove = ref(false)
const openingDialog = ref(false)
const preparationPanel = ref(false)
const preparationEntryId = ref<string>()
const overlayActive = ref(false)

let dialogTrigger: HTMLElement | null = null
function rememberTrigger() { dialogTrigger = document.activeElement instanceof HTMLElement ? document.activeElement : null }
async function closeDialog() {
  modal.value = null
  preparationPanel.value = false
  await nextTick()
  if (dialogTrigger?.isConnected) dialogTrigger.focus()
}
async function openPreparations(entryId?: string) {
  if (modal.value || preparationPanel.value || openingDialog.value || overlayActive.value) return
  rememberTrigger()
  openingDialog.value = true
  try {
    if (desktop && await showOverlay({ mode: 'preparations', entryId })) return
    preparationPanel.value = true
    preparationEntryId.value = entryId
    await hideWebSurface()
  } catch (cause) {
    error.value = errorText(cause)
    preparationPanel.value = false
  } finally { openingDialog.value = false }
}
const error = ref('')
const reloading = ref(false)
const desktop = Boolean(getDesktopApi())
let unlistenFavicon: (() => void) | undefined
let unlistenSaved: (() => void) | undefined
let unlistenOverlayActive: (() => void) | undefined
let mounted = false

async function reload(initial = false) {
  if (reloading.value) return
  reloading.value = true; error.value = ''
  try {
    workspace.value = initial || !desktop ? await loadWorkspace() : await hostCommand('workspace_reload')
    if (selectedId.value !== 'home' && !current.value) selectedId.value = 'home'
  } catch (cause) { error.value = errorText(cause) }
  finally { reloading.value = false }
}
async function openDialog(mode: 'add' | 'edit' | 'settings', entry?: WebEntry, remove = false) {
  if (modal.value || preparationPanel.value || openingDialog.value || overlayActive.value) return
  rememberTrigger()
  openingDialog.value = true
  try {
    if (desktop && await showOverlay({ mode, entryId: entry?.id, confirmRemove: remove })) return
    const snapshot = await loadWorkspace()
    workspace.value = snapshot
    const latest = snapshot.entries.find(e => e.id === entry?.id)
    if (mode === 'edit' && (!latest || !latest.editable)) throw new Error('要编辑的网页已不存在或不可编辑。')
    dialogEntry.value = latest
    confirmRemove.value = remove
    modal.value = mode
    await hideWebSurface()
  } catch (cause) { modal.value = null; error.value = errorText(cause) }
  finally { openingDialog.value = false }
}
function saved(snapshot: WorkspaceSnapshot) {
  const added = snapshot.entries.find(entry => entry.editable && !workspace.value?.entries.some(old => old.id === entry.id))
  if (added) {
    browserSelectedId.value = added.id
    selectedId.value = snapshot.entries.find(entry => entry.kind === 'browser')?.id ?? 'home'
  }
  workspace.value = snapshot
  if (selectedId.value !== 'home' && !current.value) selectedId.value = 'home'
}
function editEntry(entry: WebEntry, remove = false) { void openDialog('edit', entry, remove) }
onMounted(async () => {
  mounted = true
  await reload(true)
  const api = getDesktopApi()
  if (!api || !mounted) return
  try {
    const dispose = await api.event.listen<{ entryId: string; registeredUrl: string; image: number[] }>('web-favicon', event => {
      const entry = workspace.value?.entries.find(entry => entry.id === event.payload.entryId)
      if (workspace.value && entry?.url === event.payload.registeredUrl) {
        const b64 = btoa(event.payload.image.map(byte => String.fromCharCode(byte)).join(''))
        setCachedFavicon({ entryId: entry.id, registeredUrl: event.payload.registeredUrl, dataUrl: `data:image/png;base64,${b64}` })
      }
    })
    if (mounted) unlistenFavicon = dispose
    else dispose()

    const disposeSaved = await api.event.listen('workspace-saved', () => {
      void loadWorkspace().then(snapshot => { if (mounted) saved(snapshot) }).catch(cause => { error.value = errorText(cause) })
    })
    if (mounted) unlistenSaved = disposeSaved
    else disposeSaved()

    const disposeOverlayActive = await api.event.listen<boolean>('overlay-active', event => {
      if (mounted) overlayActive.value = Boolean(event.payload)
    })
    if (mounted) unlistenOverlayActive = disposeOverlayActive
    else disposeOverlayActive()
  } catch { /* Icons can still use the cache or monogram. */ }
})
onUnmounted(() => {
  mounted = false
  unlistenFavicon?.()
  unlistenSaved?.()
  unlistenOverlayActive?.()
})
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
          v-for="entry in primaryEntries" :key="entry.id" class="nav-link" :class="{ 'is-active': selectedId === entry.id }"
          type="button" :aria-current="selectedId === entry.id ? 'page' : undefined" :aria-label="entry.title" :title="entry.title"
          @click="selectedId = entry.id"
        >
          <AppIcon v-if="entry.kind === 'browser'" name="globe" />
          <AppIcon v-else-if="entry.pluginIcon" :name="entry.pluginIcon" />
          <SiteIcon v-else class="site-monogram" :title="entry.title" :entry-id="entry.id" :registered-url="entry.url" />
          <template v-for="slot in getSlots(entry, 'sidebar:badge')" :key="slot.id || slot.target">
            <span
              class="slot-badge"
              :class="slot.badgeType ? `badge-${slot.badgeType}` : 'badge-dot'"
              :title="slot.title || entry.title"
              aria-hidden="true"
            ></span>
          </template>
        </button>
      </nav>
      <div class="sidebar-bottom">
        <button v-if="workspace && desktop" class="icon-button" type="button" aria-label="重新加载接入" title="重新加载接入" :disabled="reloading" @click="reload()"><AppIcon name="refresh" /></button>
        <button v-if="workspace && desktop" class="icon-button" type="button" aria-label="工作台设置" title="工作台设置" @click="openDialog('settings')"><AppIcon name="settings" /></button>
        <button v-if="!reducedMotion && !current" class="icon-button" type="button" :aria-label="paused ? '播放首页动效' : '暂停首页动效'" :title="paused ? '播放首页动效' : '暂停首页动效'" :aria-pressed="paused" @click="paused = !paused"><AppIcon :name="paused ? 'play' : 'pause'" /></button>
        <button class="icon-button" type="button" :aria-label="theme === 'light' ? '切换为深色外观' : '切换为浅色外观'" :title="theme === 'light' ? '切换为深色外观' : '切换为浅色外观'" @click="toggleTheme"><AppIcon :name="theme === 'light' ? 'moon' : 'sun'" /></button>
      </div>
    </aside>
    <main id="workspace" class="workspace" :class="{ 'has-web-entry': current }" tabindex="-1">
      <WindowChrome v-if="!current" />
      <div v-if="error || appearanceError || workspace?.diagnostics.length" class="workspace-notice" role="alert">
        <p v-if="error">{{ error }}</p>
        <p v-if="appearanceError">主题同步：{{ appearanceError }}</p>
        <p v-for="message in workspace?.diagnostics" :key="message">{{ message }}</p>
        <span v-if="workspace?.diagnostics.length && workspace.configPath" class="config-location">配置：{{ workspace.configPath }}</span>
        <button class="text-button" type="button" :disabled="reloading" @click="reload()">重新加载</button>
      </div>
      <component
        :is="resolvePluginView(current)" v-if="current && workspace" :entry="current" :workspace="workspace" :selected-id="browserSelectedId" :supported="workspace.browserSupported"
        :writable="workspace.writable" :suspended="modal !== null || preparationPanel" @preparations="openPreparations" @edit="editEntry" @settings="openDialog('settings')" @add="openDialog('add')"
        @select="browserSelectedId = $event" @updated="saved"
      />
      <OrielShowcase v-else :paused="motionPaused" />
    </main>
  </div>
  <PreparationPanel v-if="preparationPanel && workspace" :workspace="workspace" :entry-id="preparationEntryId" @close="closeDialog" />
  <WorkspaceDialog v-if="modal && workspace" :mode="modal" :entry="dialogEntry" :confirm-remove="confirmRemove" :workspace="workspace" :theme="theme" @theme="setTheme" @close="closeDialog" @saved="saved" />
</template>
