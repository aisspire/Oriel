<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import AppIcon from './AppIcon.vue'
import { errorText, hostCommand, type PreparationState, type WorkspaceSnapshot } from '../lib/workspace'
const props = defineProps<{ workspace: WorkspaceSnapshot; entryId?: string }>()
const emit = defineEmits<{ close: [] }>()
const dialog = ref<HTMLDialogElement>()
const runs = ref<PreparationState[]>([])
const selected = ref(props.entryId ?? '')
const logs = ref<string[]>([])
const error = ref('')
const busy = ref(false)
const stopping = ref(false)
let alive = false
let polling = false
let timer: ReturnType<typeof setTimeout> | undefined
const names: Record<PreparationState['state'], string> = { starting: '准备中', ready: '程序运行中', completed: '任务已完成', failed: '失败', stopping: '停止中', stopped: '已停止' }
const entries = computed(() => {
  const pages = props.workspace.entries.filter(e => e.preparation).map(e => ({ id: e.id, title: e.title }))
  for (const run of runs.value) if (!pages.some(e => e.id === run.entryId)) pages.push({ id: run.entryId, title: run.title })
  return pages
})
const current = computed(() => runs.value.find(r => r.entryId === selected.value))
const configured = computed(() => props.workspace.entries.some(e => e.id === selected.value && e.preparation))
async function poll() {
  if (!alive || polling) return
  polling = true
  try {
    const data = await hostCommand<PreparationState[]>('preparation_states')
    if (!alive) return
    runs.value = data
    if (!selected.value && entries.value[0]) selected.value = entries.value[0].id
    const id = selected.value
    const output = id ? await hostCommand<string[]>('preparation_logs', { entryId: id }) : []
    if (alive && id === selected.value) logs.value = output
  } catch (cause) { if (alive) error.value = errorText(cause) }
  finally { polling = false; if (alive) timer = setTimeout(poll, 1000) }
}
async function control(action: 'stop' | 'retry') {
  if (!selected.value || (action === 'stop' ? stopping.value : busy.value)) return
  if (action === 'stop') stopping.value = true
  else busy.value = true
  error.value = ''
  try {
    if (action === 'stop') await hostCommand('preparation_stop', { entryId: selected.value })
    else await hostCommand('web_prepare', { entryId: selected.value, retry: true })
  } catch (cause) { error.value = errorText(cause) }
  finally { if (action === 'stop') stopping.value = false; else busy.value = false }
}
function close() { emit('close') }
onMounted(() => { alive = true; dialog.value?.showModal(); void poll() })
onUnmounted(() => { alive = false; clearTimeout(timer); dialog.value?.close() })
</script>
<template>
  <dialog ref="dialog" class="settings-dialog preparation-panel" aria-labelledby="preparation-panel-title" @cancel.prevent="close">
    <header class="dialog-heading"><h1 id="preparation-panel-title">前置程序</h1><button class="plain-icon" type="button" aria-label="关闭前置程序" @click="close"><AppIcon name="close" /></button></header>
    <p v-if="!entries.length" class="field-help">尚未配置前置程序，可在网页设置中添加。</p>
    <template v-else>
      <label class="field-label">关联网页<select v-model="selected" :disabled="busy" @change="logs = []"><option v-for="entry in entries" :key="entry.id" :value="entry.id">{{ entry.title }}</option></select></label>
      <p class="preparation-status" role="status">{{ current ? names[current.state] : '未启动' }}<span v-if="current?.exitCode !== null && current?.exitCode !== undefined"> · 退出码 {{ current.exitCode }}</span></p>
      <p v-if="current?.configurationChanged" class="field-help">配置已变更或入口已移除，当前记录来自上次执行。</p>
      <p v-if="current?.url" class="field-help preparation-url">{{ current.url }}</p>
      <p v-if="current?.error" class="form-error">{{ current.error }}</p>
      <div class="preparation-actions">
        <button class="text-button" type="button" :disabled="stopping || !current || current.cleaned" @click="control('stop')">停止程序</button>
        <button class="primary-button" type="button" :disabled="busy || !configured" @click="control('retry')">{{ busy ? '正在处理…' : '重新准备并加载' }}</button>
      </div>
      <details open><summary>运行日志</summary><pre class="preparation-logs" tabindex="0" aria-label="运行日志">{{ logs.length ? logs.join('\n') : '暂无输出' }}</pre></details>
    </template>
    <p v-if="error" class="form-error" role="alert">{{ error }}</p>
  </dialog>
</template>
<style scoped>
.preparation-panel { width: min(640px, calc(100vw - 32px)); }
.preparation-status { margin: 16px 0 8px; font-size: 13px; }
.preparation-url { overflow-wrap: anywhere; }
.preparation-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 10px; margin: 16px 0; }
.preparation-logs { max-height: 280px; padding: 12px; overflow: auto; border: 1px solid var(--line); border-radius: 8px; background: var(--surface); color: var(--text-primary); font: 12px/1.6 monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
summary { cursor: pointer; font-size: 13px; }
</style>
