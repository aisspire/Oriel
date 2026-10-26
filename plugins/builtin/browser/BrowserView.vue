<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch, useTemplateRef } from 'vue'
import AppIcon from '../../../src/components/AppIcon.vue'
import WindowControls from '../../../src/components/WindowControls.vue'
import PluginHelp from '../../../src/components/PluginHelp.vue'
import { useWebSession } from '../../../src/composables/useWebSession'
import { hideWebSurface, type PluginViewProps, type WebEntry } from '../../../src/lib/workspace'

const props = defineProps<PluginViewProps>()
const emit = defineEmits<{ edit: [entry: WebEntry]; settings: []; add: []; preparations: [entryId?: string] }>()
const passwords = ref<HTMLDialogElement>()
const passwordDialog = ref(false)
const helpOpen = ref(false)
const clearing = ref(false)
const cleared = ref(false)
const address = ref(props.entry.url ?? '')
const dirtyAddress = ref(false)
const surface = useTemplateRef<HTMLElement>('surface')
const { state, error, busy, preparing, run, retry } = useWebSession(() => props.entry, () => props.supported, () => props.suspended || passwordDialog.value || helpOpen.value, surface)
const available = computed(() => props.supported && !busy.value)

watch(() => props.entry.id, () => { address.value = props.entry.url ?? ''; dirtyAddress.value = false; closePasswords() })
watch(() => state.value.url, url => { if (!dirtyAddress.value) address.value = url })
async function navigate() {
  dirtyAddress.value = false
  await run('web_navigate', { url: address.value })
}
function browse(action: 'back' | 'forward' | 'reload' | 'stop') {
  dirtyAddress.value = false; address.value = state.value.url
  void run('web_action', { action })
}
async function openPasswords() {
  passwordDialog.value = true; cleared.value = false; error.value = ''
  try { await hideWebSurface(); await nextTick(); passwords.value?.showModal() }
  catch (cause) { passwordDialog.value = false; error.value = String(cause) }
}
function closePasswords() { passwords.value?.close(); passwordDialog.value = false; clearing.value = false; cleared.value = false }
async function clearPasswords() {
  clearing.value = true
  await run('web_clear_passwords')
  cleared.value = !error.value; clearing.value = false
}
onUnmounted(() => passwords.value?.close())
</script>

<template>
  <section class="browser-view" :aria-label="entry.title">
    <header class="browser-toolbar" aria-label="浏览工具栏">
      <div class="navigation-buttons">
        <button class="plain-icon" type="button" aria-label="上一页" title="上一页" :disabled="!available || !state.canGoBack" @click="browse('back')"><AppIcon name="back" /></button>
        <button class="plain-icon" type="button" aria-label="下一页" title="下一页" :disabled="!available || !state.canGoForward" @click="browse('forward')"><AppIcon name="forward" /></button>
        <button class="plain-icon" type="button" :aria-label="state.loading ? '停止加载' : '刷新'" :title="state.loading ? '停止加载' : '刷新'" :disabled="!available || !state.url" @click="browse(state.loading ? 'stop' : 'reload')"><AppIcon :name="state.loading ? 'close' : 'refresh'" /></button>
      </div>
      <form class="address-bar-form" @submit.prevent="navigate">
        <div class="address-field">
          <AppIcon name="globe" />
          <input v-model="address" aria-label="网页地址" type="text" placeholder="输入网页地址" spellcheck="false" autocomplete="off" :disabled="!supported" @input="dirtyAddress = true" @keydown.esc="address = state.url; dirtyAddress = false" />
          <button class="plain-icon inline-go" type="submit" aria-label="打开网页" title="打开网页" :disabled="!available || !address.trim()"><AppIcon name="forward" /></button>
        </div>
      </form>
      <div class="toolbar-drag-spacer" data-tauri-drag-region></div>
      <div class="browser-options">
        <button v-if="entry.preparation" class="plain-icon" type="button" aria-label="当前网页的前置程序" title="当前网页的前置程序" aria-haspopup="dialog" @click="emit('preparations', entry.id)"><AppIcon name="terminal" /></button>
        <PluginHelp :entry="entry" @active="helpOpen = $event" />
        <button class="plain-icon" type="button" aria-label="账号密码" title="账号密码" :disabled="!available" @click="openPasswords"><AppIcon name="key" /></button>
        <button v-if="entry.editable" class="plain-icon" type="button" aria-label="网页设置" title="网页设置" :disabled="!writable || busy" @click="emit('edit', entry)"><AppIcon name="settings" /></button>
        <button v-else class="plain-icon" type="button" aria-label="工作台设置" title="工作台设置" :disabled="!writable || busy" @click="emit('settings')"><AppIcon name="settings" /></button>
      </div>
      <WindowControls class="browser-window-controls" />
    </header>
    <div v-if="preparing" class="browser-error" role="status"><span>{{ entry.preparation ? '正在执行前置程序…' : '正在准备前置资源，可打开提供方页面处理认证。' }}</span></div>
    <div v-if="error || state.error" class="browser-error" role="alert">
      <span>{{ error || state.error }}</span><button class="text-button" type="button" :disabled="busy" @click="state.error ? run('web_action', { action: 'reload' }) : retry()">重试</button>
    </div>
    <button v-if="entry.preparation && (error || state.error)" class="text-button" type="button" :disabled="preparing" @click="retry">重新准备并加载</button>
    <div class="load-progress" :class="{ 'is-loading': state.loading }" role="status" :aria-label="state.loading ? '网页加载中' : '加载完成'"></div>
    <div ref="surface" class="web-surface">
      <div v-if="!supported" class="browser-empty"><AppIcon name="globe" /><p>请在 Windows 桌面应用中浏览网页。</p></div>
      <div v-else-if="!state.url" class="browser-empty">
        <AppIcon name="globe" /><p>输入网址，打开网页。</p>
        <button class="text-button" type="button" :disabled="!writable" @click="emit('add')">添加网页</button>
      </div>
    </div>
    <dialog ref="passwords" class="settings-dialog password-dialog" aria-labelledby="password-dialog-title" @cancel="clearing ? $event.preventDefault() : closePasswords()">
      <header class="dialog-heading"><h1 id="password-dialog-title">账号密码</h1><button class="plain-icon" type="button" aria-label="关闭" :disabled="clearing" @click="closePasswords"><AppIcon name="close" /></button></header>
      <p>登录时可选择保存账号密码，下次登录自动填充。</p>
      <p class="field-help">清除仅影响此入口保存的密码，当前登录状态会保留。</p>
      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <p v-if="cleared" class="form-success" role="status">已清除保存的密码。</p>
      <footer class="dialog-actions"><button class="text-button danger" type="button" :disabled="clearing || cleared" @click="clearPasswords">{{ clearing ? '正在清除…' : '清除此入口的密码' }}</button><button class="primary-button" type="button" :disabled="clearing" @click="closePasswords">完成</button></footer>
    </dialog>
  </section>
</template>
