<script setup lang="ts">
import type { ProxyPolicy } from '../lib/workspace'
defineProps<{ modelValue: ProxyPolicy; allowInherit?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: ProxyPolicy] }>()
function choose(event: Event) {
  const mode = (event.target as HTMLSelectElement).value as ProxyPolicy['mode']
  emit('update:modelValue', mode === 'custom' ? { mode, address: '' } : { mode })
}
</script>

<template>
  <label class="field-label">
    代理
    <select :value="modelValue.mode" @change="choose">
      <option v-if="allowInherit" value="inherit">使用默认代理</option>
      <option v-else value="system">跟随系统</option>
      <option value="direct">直连</option>
      <option value="custom">自定义代理</option>
    </select>
  </label>
  <label v-if="modelValue.mode === 'custom'" class="field-label">
    代理地址
    <input
      :value="modelValue.address" type="text" placeholder="http://127.0.0.1:7890" spellcheck="false" required
      @input="emit('update:modelValue', { mode: 'custom', address: ($event.target as HTMLInputElement).value })"
    />
    <span class="field-help">支持 HTTP 和 SOCKS5，暂不支持代理账号密码。</span>
  </label>
</template>
