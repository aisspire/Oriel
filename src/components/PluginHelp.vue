<script setup lang="ts">
import { nextTick, onUnmounted, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import DocumentContent from './DocumentContent.vue'
import { errorText, hideWebSurface, hostCommand, showOverlay, type WebEntry } from '../lib/workspace'

const props = defineProps<{ entry: WebEntry }>()
const emit = defineEmits<{ active: [value: boolean] }>()
const dialog = ref<HTMLDialogElement>()
const trigger = ref<HTMLButtonElement>()
const active = ref(false)
const opening = ref(false)
const loading = ref(false)
const text = ref('')
const error = ref('')
let generation = 0
async function open() {
  if (active.value || opening.value) return
  opening.value = true
  const ok = await showOverlay({ mode: 'help', entryId: props.entry.id })
  opening.value = false
  if (ok) return

  const current = ++generation
  active.value = true; loading.value = true; text.value = ''; error.value = ''
  emit('active', true)
  try {
    await hideWebSurface(); await nextTick()
    if (current !== generation) return
    dialog.value?.showModal()
    const content = await hostCommand<string>('plugin_documentation', { entryId: props.entry.id })
    if (current === generation) text.value = content
  } catch (cause) {
    if (current === generation) {
      error.value = errorText(cause)
      if (!dialog.value?.open) close()
    }
  }
  finally { if (current === generation) loading.value = false }
}
function close() {
  const restoreFocus = active.value
  ++generation; dialog.value?.close(); active.value = false; emit('active', false)
  if (restoreFocus) trigger.value?.focus()
}
watch(() => props.entry.id, close)
onUnmounted(close)
</script>

<template>
  <button v-if="entry.documentation" ref="trigger" class="text-button plugin-help-button" type="button" @click="open">使用说明</button>
  <span v-if="error && !active" class="form-error" role="alert">{{ error }}</span>
  <dialog ref="dialog" class="settings-dialog plugin-help-dialog" aria-labelledby="plugin-help-title" @cancel.prevent="close" @close="active && close()">
    <header class="dialog-heading"><h1 id="plugin-help-title">{{ entry.pluginName }} · 使用说明</h1><button class="plain-icon" type="button" aria-label="关闭说明" @click="close"><AppIcon name="close" /></button></header>
    <p v-if="loading" role="status">正在读取说明…</p>
    <div v-else-if="error" role="alert"><p class="form-error">{{ error }}</p><button class="text-button" type="button" @click="close(); open()">重试</button></div>
    <DocumentContent v-else :text="text" hide-title />
  </dialog>
</template>

<style scoped>
.plugin-help-dialog { width: min(720px, calc(100vw - 32px)); }
.plugin-help-dialog > header { position: sticky; top: -24px; background: var(--surface-solid); padding: 12px 0; z-index: 1; }
.plugin-help-button { flex-shrink: 0; }
</style>
