<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, useTemplateRef, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import WindowControls from './WindowControls.vue'
import PluginHelp from './PluginHelp.vue'
import { useWebSession } from '../composables/useWebSession'
import { hideWebSurface, type PluginViewProps } from '../lib/workspace'

const props = defineProps<PluginViewProps>()
const emit = defineEmits<{ settings: []; preparations: [entryId?: string] }>()
const menu = ref<HTMLDialogElement>()
const menuOpen = ref(false)
const helpOpen = ref(false)
const surface = useTemplateRef<HTMLElement>('surface')
const { state, error, busy, preparing, run, retry } = useWebSession(() => props.entry, () => props.supported, () => props.suspended || menuOpen.value || helpOpen.value, surface)
const available = computed(() => props.supported && !busy.value)
async function openMenu() {
  menuOpen.value = true
  try { await hideWebSurface(); await nextTick(); if (menuOpen.value) menu.value?.showModal() }
  catch (cause) { error.value = String(cause); closeMenu() }
}
function closeMenu() { menu.value?.close(); menuOpen.value = false }
function reloadPage() { closeMenu(); void run('web_action', { action: 'reload' }) }
function settings() { closeMenu(); emit('settings') }
watch(() => props.entry.id, closeMenu)
onUnmounted(closeMenu)
</script>

<template>
  <section class="browser-view tool-application" :aria-label="entry.title">
    <header class="application-toolbar" aria-label="工具应用">
      <div class="application-drag-region" data-tauri-drag-region></div>
      <PluginHelp :entry="entry" @active="helpOpen = $event" />
      <button class="plain-icon" type="button" aria-label="应用操作" title="应用操作" aria-haspopup="dialog" @click="openMenu"><AppIcon name="more" /></button>
      <WindowControls />
    </header>
    <div v-if="preparing" class="browser-error" role="status">{{ entry.preparation ? '正在执行前置程序…' : '正在准备前置资源，可打开提供方处理认证。' }}</div>
    <div v-if="error || state.error" class="browser-error" role="alert"><span>{{ error || state.error }}</span><button class="text-button" type="button" :disabled="busy" @click="state.error ? run('web_action', { action: 'reload' }) : retry()">重试</button></div>
    <div class="load-progress" :class="{ 'is-loading': state.loading }" role="status" :aria-label="state.loading ? '工具加载中' : '加载完成'"></div>
    <div ref="surface" class="web-surface">
      <div v-if="!supported" class="browser-empty"><AppIcon name="panel" /><p>请在 Windows 桌面应用中打开此工具。</p></div>
    </div>
    <dialog ref="menu" class="settings-dialog application-menu" aria-labelledby="application-menu-title" @cancel.prevent="closeMenu" @close="menuOpen && closeMenu()">
      <header class="dialog-heading"><h1 id="application-menu-title">{{ entry.title }}</h1><button class="plain-icon" type="button" aria-label="关闭应用操作" @click="closeMenu"><AppIcon name="close" /></button></header>
      <button class="text-button" type="button" :disabled="!available || !state.url" @click="reloadPage">刷新界面</button>
      <button class="text-button" type="button" :disabled="!available" @click="closeMenu(); retry()">{{ entry.preparation ? '重新准备并加载' : '重试加载' }}</button>
      <button v-if="entry.preparation" class="text-button" type="button" @click="closeMenu(); emit('preparations', entry.id)">前置程序</button>
      <button class="text-button" type="button" @click="settings">工作台设置</button>
      <details><summary>诊断信息</summary><p class="field-help">入口：{{ entry.id }}</p><p class="field-help">{{ preparing ? '正在准备' : state.loading ? '正在加载' : error || state.error || '无加载错误' }}</p></details>
    </dialog>
  </section>
</template>

<style scoped>
.application-toolbar { display: flex; align-items: center; flex-shrink: 0; min-height: 36px; background: var(--shell); border-bottom: 1px solid var(--line); }
.application-drag-region { flex: 1; align-self: stretch; min-width: 32px; }
.application-menu { width: min(360px, calc(100vw - 32px)); }
.application-menu > button { display: block; width: 100%; text-align: left; }
.application-menu details { margin-top: 14px; font-size: 12px; }
</style>
