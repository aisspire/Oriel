<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import AppIcon from './AppIcon.vue'
import { useDialogDismiss } from '../composables/useDialogDismiss'
import { errorText, hostCommand, type OverlayShowOptions, type WebEntry, type WebSessionState, type WorkspaceSnapshot } from '../lib/workspace'

const props = defineProps<{
  mode: 'passwords' | 'tool-menu' | 'page-menu'
  entry: WebEntry
  workspace: WorkspaceSnapshot
  supported: boolean
}>()
const emit = defineEmits<{
  close: []
  navigate: [options: OverlayShowOptions]
  retry: []
  saved: [workspace: WorkspaceSnapshot]
}>()
const dialog = ref<HTMLDialogElement>()
const busy = ref(false)
const error = ref('')
const cleared = ref(false)
const state = ref<WebSessionState | null>(null)
const pages = computed(() => props.workspace.entries.filter(e => e.editable && e.kind === 'web-page'))
const index = computed(() => pages.value.findIndex(e => e.id === props.entry.id))
useDialogDismiss(dialog)
function close() { if (!busy.value) emit('close') }
async function perform(action: () => Promise<void>) {
  if (busy.value) return
  busy.value = true; error.value = ''
  try { await action() }
  catch (cause) { error.value = errorText(cause) }
  finally { busy.value = false }
}
function clearPasswords() {
  void perform(async () => {
    await hostCommand('web_clear_passwords', { entryId: props.entry.id })
    cleared.value = true
  })
}
function reload() {
  void perform(async () => {
    await hostCommand('web_action', { entryId: props.entry.id, action: 'reload' })
    emit('close')
  })
}
function retry() {
  // The main renderer owns preparation and its error/busy states. The overlay
  // closes as it delegates, so long-running preparation never traps a modal.
  if (busy.value) return
  busy.value = true
  emit('retry')
}
function navigate(mode: OverlayShowOptions['mode'], confirmRemove = false) {
  if (!busy.value) emit('navigate', { mode, entryId: props.entry.id, confirmRemove })
}
function move(offset: number) {
  if (index.value < 0 || !pages.value[index.value + offset]) return
  const ids = pages.value.map(e => e.id)
  ;[ids[index.value], ids[index.value + offset]] = [ids[index.value + offset], ids[index.value]]
  void perform(async () => {
    emit('saved', await hostCommand<WorkspaceSnapshot>('workspace_reorder_entries', { ids }))
    emit('close')
  })
}
onMounted(async () => {
  dialog.value?.showModal()
  if (props.mode === 'tool-menu' && props.supported) {
    try { state.value = await hostCommand<WebSessionState | null>('web_session_state', { entryId: props.entry.id }) }
    catch (cause) { error.value = errorText(cause) }
  }
})
onUnmounted(() => dialog.value?.close())
</script>

<template>
  <dialog ref="dialog" class="settings-dialog entry-action-dialog" :class="{ 'password-dialog': mode === 'passwords', 'application-menu': mode === 'tool-menu', 'page-action-dialog': mode === 'page-menu' }" aria-labelledby="entry-action-title" :aria-busy="busy" @cancel.prevent="close">
    <header class="dialog-heading">
      <h1 id="entry-action-title">{{ mode === 'passwords' ? '账号密码' : entry.title }}</h1>
      <button class="plain-icon" type="button" aria-label="关闭" :disabled="busy" @click="close"><AppIcon name="close" /></button>
    </header>
    <template v-if="mode === 'passwords'">
      <p>登录时可选择保存账号密码，下次登录自动填充。</p>
      <p class="field-help">清除仅影响此入口保存的密码，当前登录状态会保留。</p>
      <p v-if="cleared" class="form-success" role="status">已清除保存的密码。</p>
      <footer class="dialog-actions"><button class="text-button danger" type="button" :disabled="!supported || busy || cleared" @click="clearPasswords">{{ busy ? '正在清除…' : '清除此入口的密码' }}</button><button class="primary-button" type="button" :disabled="busy" @click="close">完成</button></footer>
    </template>
    <template v-else-if="mode === 'tool-menu'">
      <button class="text-button" type="button" :disabled="!supported || busy || !state?.url" @click="reload">刷新界面</button>
      <button class="text-button" type="button" :disabled="!supported || busy" @click="retry">{{ entry.preparation ? '重新准备并加载' : '重试加载' }}</button>
      <button v-if="entry.preparation" class="text-button" type="button" :disabled="busy" @click="navigate('preparations')">前置程序</button>
      <button class="text-button" type="button" :disabled="busy" @click="navigate('settings')">工作台设置</button>
      <details><summary>诊断信息</summary><p class="field-help">入口：{{ entry.id }}</p><p class="field-help">{{ state?.loading ? '正在加载' : state?.error || (state?.url ? '无加载错误' : '尚未加载') }}</p></details>
    </template>
    <template v-else>
      <button class="text-button" type="button" :disabled="!workspace.writable || busy" @click="navigate('edit')">编辑网页</button>
      <button class="text-button" type="button" :disabled="!workspace.writable || busy || index <= 0" @click="move(-1)">上移</button>
      <button class="text-button" type="button" :disabled="!workspace.writable || busy || index >= pages.length - 1" @click="move(1)">下移</button>
      <button class="text-button danger" type="button" :disabled="!workspace.writable || busy" @click="navigate('edit', true)">移除网页</button>
    </template>
    <p v-if="error" class="form-error" role="alert">{{ error }}</p>
  </dialog>
</template>

<style scoped>
.application-menu, .page-action-dialog { width: min(360px, calc(100vw - 32px)); }
.entry-action-dialog > button { display: block; width: 100%; text-align: left; }
.entry-action-dialog details { margin-top: 14px; font-size: 12px; }
</style>
