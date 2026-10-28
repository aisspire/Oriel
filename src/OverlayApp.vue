<script setup lang="ts">
import { nextTick, onErrorCaptured, onMounted, onUnmounted, ref } from 'vue'
import AppIcon from './components/AppIcon.vue'
import EntryActionDialog from './components/EntryActionDialog.vue'
import WorkspaceDialog from './components/WorkspaceDialog.vue'
import PreparationPanel from './components/PreparationPanel.vue'
import DocumentContent from './components/DocumentContent.vue'
import { useAppearance } from './composables/useAppearance'
import { getDesktopApi } from './lib/desktop'
import { OverlayContentError, OverlayReceiver } from './lib/overlayReceiver'
import { useDialogDismiss } from './composables/useDialogDismiss'
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
let timer: ReturnType<typeof setTimeout> | undefined
let alive = true
const disposers: (() => void)[] = []
const receiver = new OverlayReceiver(hostCommand, prepare, () => {
  helpDialog.value?.close()
  mode.value = null
  currentRequest = undefined
  workspace.value = undefined
})
useDialogDismiss(helpDialog)
onErrorCaptured(cause => { void receiver.fail('mount', cause); return false })

async function close() {
  const requestId = currentRequest
  receiver.reset()
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
async function prepare(request: OverlayRequest) {
  const themeRevision = getThemeRevision()
  const snapshot = await loadWorkspace()
  const entry = snapshot.entries.find(e => e.id === request.entryId)
  if (['edit', 'help', 'passwords', 'tool-menu', 'page-menu'].includes(request.mode) && !entry) throw new OverlayContentError('目标网页已不存在。')
  if (['edit', 'page-menu'].includes(request.mode) && !entry?.editable) throw new OverlayContentError('此网页不能编辑。')
  let text = ''
  if (request.mode === 'help') {
    try { text = await hostCommand<string>('plugin_documentation', { entryId: entry!.id }) }
    catch (cause) { throw new OverlayContentError(String(cause)) }
  }
  return async () => {
    currentRequest = request.requestId
    workspace.value = snapshot
    if (snapshot.theme) calibrateTheme(snapshot.theme, themeRevision)
    dialogEntry.value = entry
    confirmRemove.value = Boolean(request.confirmRemove)
    preparationEntryId.value = request.entryId
    helpEntry.value = entry
    helpText.value = text
    mode.value = request.mode
    await nextTick()
    if (currentRequest !== request.requestId) return
    if (request.mode === 'help') helpDialog.value?.showModal()
    if (!document.querySelector('dialog[open]')) throw new Error('悬浮弹窗未完成挂载。')
  }
}
async function navigate(options: { mode: OverlayRequest['mode']; confirmRemove?: boolean }) {
  confirmRemove.value = Boolean(options.confirmRemove)
  mode.value = options.mode
  await nextTick()
}
async function retryEntry() {
  const entryId = dialogEntry.value?.id
  if (!entryId) return
  await getDesktopApi()?.event.emitTo('main', 'web-retry-request', entryId)
  await close()
}
function wake() { void receiver.poll() }
async function poll() {
  await receiver.poll()
  if (alive) timer = setTimeout(poll, 40)
}
onMounted(async () => {
  const api = getDesktopApi()
  if (!api) return
  try {
    disposers.push(await api.event.listen('overlay-open', () => { void receiver.poll() }))
    disposers.push(await api.event.listen('overlay-close-request', requestClose))
    disposers.push(await api.event.listen<number>('overlay-reset', ({ payload }) => {
      if (payload === currentRequest) receiver.reset()
    }))
    document.addEventListener('oriel-overlay-poll', wake)
    await hostCommand('overlay_ready')
    void poll()
  } catch (cause) { await receiver.fail('initialization', cause) }
})
onUnmounted(() => {
  alive = false
  clearTimeout(timer)
  document.removeEventListener('oriel-overlay-poll', wake)
  receiver.stop()
  disposers.forEach(dispose => dispose())
})
</script>

<template>
  <div v-if="mode && workspace" class="overlay-backdrop">
    <div class="overlay-container">
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

      <EntryActionDialog
        v-else-if="(mode === 'passwords' || mode === 'tool-menu' || mode === 'page-menu') && dialogEntry"
        :mode="mode"
        :entry="dialogEntry"
        :workspace="workspace"
        :supported="workspace.browserSupported"
        @close="close"
        @navigate="navigate"
        @retry="retryEntry"
        @saved="handleSaved"
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
/* The window receives input; the transparent surface exposes the live page. */
:root.is-overlay,
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
  background: rgba(18, 19, 26, 0.28);
  z-index: 10000;
  padding: 24px;
  box-sizing: border-box;
}

[data-theme='light'] .overlay-backdrop {
  background: rgba(18, 19, 26, 0.28);
}

[data-theme='dark'] .overlay-backdrop {
  background: rgba(10, 11, 16, 0.42);
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
