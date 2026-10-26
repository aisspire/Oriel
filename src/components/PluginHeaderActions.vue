<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from './AppIcon.vue'
import { getSlots, type PluginSlot, type WebEntry } from '../lib/workspace'
const props = defineProps<{ entry: WebEntry; busy: boolean; canReload: boolean }>()
const emit = defineEmits<{ reload: []; settings: []; preparations: [entryId: string] }>()
const slots = computed(() => getSlots(props.entry, 'header:action'))
const labels: Record<string, string> = { reload: '刷新界面', refresh: '刷新界面', settings: '工作台设置', preparations: '前置程序' }
const icons = { refresh: 'refresh', reload: 'refresh', settings: 'settings', terminal: 'terminal', play: 'play', pause: 'pause', panel: 'panel', more: 'more', home: 'home' } as const
function icon(slot: PluginSlot) {
  return icons[slot.icon as keyof typeof icons] ?? (slot.action === 'settings' ? 'settings' : slot.action === 'preparations' ? 'terminal' : 'refresh')
}
function disabled(slot: PluginSlot) {
  return props.busy || !labels[slot.action ?? ''] || (['reload', 'refresh'].includes(slot.action ?? '') && !props.canReload)
}
function act(slot: PluginSlot) {
  if (disabled(slot)) return
  switch (slot.action) {
    case 'reload': case 'refresh': emit('reload'); break
    case 'settings': emit('settings'); break
    case 'preparations': emit('preparations', props.entry.id); break
  }
}
</script>
<template>
  <button v-for="(slot, index) in slots" :key="slot.id ?? index" class="plain-icon slot-header-action" type="button"
    :aria-label="slot.title || labels[slot.action ?? ''] || '不支持的操作'" :title="slot.title || labels[slot.action ?? ''] || '不支持的操作'"
    :disabled="disabled(slot)" @click="act(slot)"><AppIcon :name="icon(slot)" /></button>
</template>
