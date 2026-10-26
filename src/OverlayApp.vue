<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from 'vue'
import AppIcon from './components/AppIcon.vue'
import WorkspaceDialog from './components/WorkspaceDialog.vue'
import PreparationPanel from './components/PreparationPanel.vue'
import DocumentContent from './components/DocumentContent.vue'
import { useAppearance } from './composables/useAppearance'
import { getDesktopApi } from './lib/desktop'
import { hostCommand, loadWorkspace, type OverlayRequest, type WebEntry, type WorkspaceSnapshot } from './lib/workspace'

const { theme, setTheme, calibrateTheme, getThemeRevision } = useAppearance({ readOnlyInitialization: true })
const workspace = ref<WorkspaceSnapshot>()
const mode = ref<OverlayRequest['mode'] | null>(null)
const dialogEntry = ref<WebEntry>()
const confirmRemove = ref(false)
const preparationEntryId = ref<string>()
const helpEntry = ref<WebEntry>()
const helpDialog = ref<HTMLDialogElement>()
const helpText = ref('')
let currentRequest: number | undefined
let generation = 0
let queue = Promise.resolve()
const disposers: (() => void)[] = []

function reset() {
  ++generation
  helpDialog.value?.close()
  mode.value = null
  currentRequest = undefined
  workspace.value = undefined
}
async function close() {
  const requestId = currentRequest
  reset()
  if (requestId !== undefined) await hostCommand('overlay_hide', { requestId })
}
function requestClose() {
  // Native close and backdrop clicks follow the same cancel path as Escape.
  // The top dialog owns nested-panel and saving guards.
  const dialogs = document.querySelectorAll<HTMLDialogElement>('dialog[open]')
  dialogs[dialogs.length - 1]?.dispatchEvent(new Event('cancel', { cancelable: true }))
}
async function handleSaved(snapshot: WorkspaceSnapshot) {
  workspace.value = snapshot
  await getDesktopApi()?.event.emitTo('main', 'workspace-saved').catch(cause => console.error('Workspace notification failed:', cause))
}
async function open(request: OverlayRequest, token: number) {
  try {
    const themeRevision = getThemeRevision()
    const snapshot = await loadWorkspace()
    if (token !== generation) return
    const entry = snapshot.entries.find(e => e.id === request.entryId)
    if ((request.mode === 'edit' || request.mode === 'help') && !entry) throw new Error('目标网页已不存在。')
    if (request.mode === 'edit' && !entry?.editable) throw new Error('此网页不能编辑。')
    let text = ''
    if (request.mode === 'help') text = await hostCommand<string>('plugin_documentation', { entryId: entry!.id })
    if (token !== generation) return
    workspace.value = snapshot
    if (snapshot.theme) calibrateTheme(snapshot.theme, themeRevision)
    dialogEntry.value = entry
    confirmRemove.value = Boolean(request.confirmRemove)
    preparationEntryId.value = request.entryId
    helpEntry.value = entry
    helpText.value = text
    mode.value = request.mode
    await nextTick()
    if (token !== generation) return
    if (request.mode === 'help') helpDialog.value?.showModal()
    await hostCommand('overlay_presented', { requestId: request.requestId })
  } catch (cause) {
    if (token === generation) { mode.value = null; console.error('Overlay request failed:', cause) }
    // The main window's five-second deadline clears native state and opens its dialog.
  }
}
onMounted(async () => {
  document.documentElement.classList.add('is-overlay')
  document.body.classList.add('is-overlay')
  const api = getDesktopApi()
  if (!api) return
  disposers.push(await api.event.listen<OverlayRequest>('overlay-open', ({ payload }) => {
    reset()
    currentRequest = payload.requestId
    const token = generation
    queue = queue.then(() => { if (token === generation) return open(payload, token) })
  }))
  disposers.push(await api.event.listen('overlay-close-request', requestClose))
  disposers.push(await api.event.listen<number>('overlay-reset', ({ payload }) => {
    if (payload === currentRequest) reset()
  }))
  await hostCommand('overlay_ready')
})
onUnmounted(() => {
  reset()
  disposers.forEach(dispose => dispose())
  document.documentElement.classList.remove('is-overlay')
  document.body.classList.remove('is-overlay')
})
</script>

<template>
  <div v-if="mode && workspace" class="overlay-backdrop" @click.self="requestClose">
    <div class="overlay-container" @click.stop>
      <!-- 工作台设置 / 添加网页 / 编辑网页 -->
      <WorkspaceDialog
        v-if="mode === 'settings' || mode === 'add' || mode === 'edit'"
        :mode="mode"
        :entry="dialogEntry"
        :confirm-remove="confirmRemove"
        :workspace="workspace"
        :theme="theme"
        @theme="setTheme"
        @close="close"
        @saved="handleSaved"
      />

      <!-- 前置程序与实时日志面板 -->
      <PreparationPanel
        v-else-if="mode === 'preparations'"
        :workspace="workspace"
        :entry-id="preparationEntryId"
        @close="close"
      />

      <!-- 插件使用说明文档 -->
      <dialog
        v-else-if="mode === 'help'"
        ref="helpDialog"
        class="settings-dialog plugin-help-dialog"
        aria-labelledby="overlay-help-title"
        @cancel.prevent="close"
      >
        <header class="dialog-heading">
          <h1 id="overlay-help-title">{{ helpEntry?.pluginName || helpEntry?.title }} · 使用说明</h1>
          <button class="plain-icon" type="button" aria-label="关闭说明" @click="close">
            <AppIcon name="close" />
          </button>
        </header>
        <DocumentContent :text="helpText" hide-title />
      </dialog>
    </div>
  </div>
</template>

<style>
/* 全局透明穿透保障 */
html.is-overlay,
body.is-overlay {
  background: transparent !important;
  margin: 0;
  padding: 0;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
}

/* 消除在 Overlay 模式下原生 dialog 自带的暗色遮罩冲突 */
.is-overlay .settings-dialog::backdrop {
  background: transparent !important;
  backdrop-filter: none !important;
}
</style>

<style scoped>
.overlay-backdrop {
  position: fixed;
  inset: 0;
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(18, 19, 26, 0.45);
  backdrop-filter: blur(14px) saturate(140%);
  -webkit-backdrop-filter: blur(14px) saturate(140%);
  z-index: 10000;
  padding: 24px;
  box-sizing: border-box;
}

[data-theme='light'] .overlay-backdrop {
  background: rgba(235, 236, 242, 0.5);
}

.overlay-container {
  display: contents;
}

.plugin-help-dialog {
  width: min(720px, calc(100vw - 32px));
  max-height: calc(100svh - 48px);
  margin: auto;
}

.plugin-help-dialog > header {
  position: sticky;
  top: -24px;
  background: var(--surface-solid);
  padding: 12px 0;
  z-index: 1;
}
</style>
