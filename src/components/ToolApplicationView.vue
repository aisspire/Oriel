<script setup lang="ts">
import { computed, ref, useTemplateRef } from 'vue'
import AppIcon from './AppIcon.vue'
import WindowControls from './WindowControls.vue'
import PluginHelp from './PluginHelp.vue'
import PluginHeaderActions from './PluginHeaderActions.vue'
import { useWebSession } from '../composables/useWebSession'
import EntryActionDialog from './EntryActionDialog.vue'
import { useEntryOverlay } from '../composables/useEntryOverlay'
import { type OverlayShowOptions, type PluginViewProps } from '../lib/workspace'

const props = defineProps<PluginViewProps>()
const emit = defineEmits<{ settings: []; preparations: [entryId?: string] }>()
const { active: menuOpen, opening: menuOpening, error: menuError, open, close: closeMenu } = useEntryOverlay(() => props.entry.id)
const helpOpen = ref(false)
const surface = useTemplateRef<HTMLElement>('surface')
const { state, error, busy, preparing, run, retry } = useWebSession(() => props.entry, () => props.supported, () => props.suspended || menuOpen.value || helpOpen.value, surface)
const available = computed(() => props.supported && !busy.value)
function openMenu() { void open('tool-menu') }
function navigate(options: OverlayShowOptions) {
  closeMenu()
  if (options.mode === 'preparations') emit('preparations', props.entry.id)
  else emit('settings')
}
function settings() { emit('settings') }
</script>

<template>
  <section class="browser-view tool-application" :aria-label="entry.title">
    <header class="application-toolbar" aria-label="工具应用">
      <div class="application-drag-region" data-tauri-drag-region></div>
      <PluginHeaderActions :entry="entry" :busy="busy" :can-reload="available && Boolean(state.url)" @reload="run('web_action', { action: 'reload' })" @settings="settings" @preparations="emit('preparations', $event)" />
      <PluginHelp :entry="entry" @active="helpOpen = $event" />
      <button class="plain-icon" type="button" aria-label="应用操作" title="应用操作" aria-haspopup="dialog" :disabled="menuOpening || busy" @click="openMenu"><AppIcon name="more" /></button>
      <WindowControls />
    </header>
    <div v-if="preparing" class="browser-error" role="status">{{ entry.preparation ? '正在执行前置程序…' : '正在准备前置资源，可打开提供方处理认证。' }}</div>
    <div v-if="error || state.error" class="browser-error" role="alert"><span>{{ error || state.error }}</span><button class="text-button" type="button" :disabled="busy" @click="state.error ? run('web_action', { action: 'reload' }) : retry()">重试</button></div>
    <div class="load-progress" :class="{ 'is-loading': state.loading }" role="status" :aria-label="state.loading ? '工具加载中' : '加载完成'"></div>
    <div ref="surface" class="web-surface">
      <div v-if="!supported" class="browser-empty"><AppIcon name="panel" /><p>请在 Windows 桌面应用中打开此工具。</p></div>
    </div>
    <p v-if="menuError" class="form-error" role="alert">{{ menuError }}</p>
    <EntryActionDialog v-if="menuOpen" mode="tool-menu" :entry="entry" :workspace="workspace" :supported="supported" @close="closeMenu" @navigate="navigate" @retry="closeMenu(); retry()" />
  </section>
</template>

<style scoped>
.application-toolbar { display: flex; align-items: center; flex-shrink: 0; min-height: 36px; background: var(--shell); border-bottom: 1px solid var(--line); }
.application-drag-region { flex: 1; align-self: stretch; min-width: 32px; }
</style>
