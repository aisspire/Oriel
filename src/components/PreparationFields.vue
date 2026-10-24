<script setup lang="ts">
import { ref } from 'vue'
import { errorText, hostCommand, type Preparation } from '../lib/workspace'
const model = defineModel<Preparation | null>({ required: true })
const picking = ref(false)
const error = ref('')
function enable(value: boolean) {
  model.value = value ? { path: '', runner: 'auto', args: [], mode: 'once', timeoutSeconds: 120 } : null
}
function patch(value: Partial<Preparation>) { if (model.value) model.value = { ...model.value, ...value } }
function setRunner(runner: Preparation['runner']) {
  patch({ runner, interpreter: runner === 'custom' ? { program: '', args: [] } : null })
}
function lines(value: string) { return value === '' ? [] : value.split(/\r?\n/) }
async function pick() {
  picking.value = true; error.value = ''
  try { const path = await hostCommand<string | null>('preparation_pick_file'); if (path) patch({ path }) }
  catch (cause) { error.value = errorText(cause) }
  finally { picking.value = false }
}
</script>
<template>
  <details class="preparation-fields" :open="Boolean(model)">
    <summary>前置程序{{ model ? ' · 已启用' : '' }}</summary>
    <label class="preparation-toggle"><input type="checkbox" :checked="Boolean(model)" @change="enable(($event.target as HTMLInputElement).checked)" />打开网页前执行程序</label>
    <template v-if="model">
      <label class="field-label">程序或脚本路径<input :value="model.path" required spellcheck="false" autocomplete="off" placeholder="C:\tools\prepare.ps1" @input="patch({ path: ($event.target as HTMLInputElement).value })" /></label>
      <button class="text-button" type="button" :disabled="picking" @click="pick">{{ picking ? '正在选择…' : '选择文件' }}</button>
      <label class="field-label">执行方式<select :value="model.runner" @change="setRunner(($event.target as HTMLSelectElement).value as Preparation['runner'])">
        <option value="auto">自动</option><option value="direct">直接执行</option><option value="cmd">CMD</option><option value="powershell">PowerShell</option><option value="custom">自定义解释器</option>
      </select></label>
      <template v-if="model.runner === 'custom'">
        <label class="field-label">解释器程序<input :value="model.interpreter?.program" required spellcheck="false" placeholder="node.exe" @input="patch({ interpreter: { program: ($event.target as HTMLInputElement).value, args: model.interpreter?.args ?? [] } })" /></label>
        <label class="field-label">解释器参数<textarea :value="model.interpreter?.args.join('\n')" rows="2" spellcheck="false" placeholder="每行一个，放在脚本路径之前" @input="patch({ interpreter: { program: model.interpreter?.program ?? '', args: lines(($event.target as HTMLTextAreaElement).value) } })"></textarea></label>
      </template>
      <label class="field-label">程序参数<textarea :value="model.args.join('\n')" rows="2" spellcheck="false" placeholder="每行一个参数，不需要添加包围引号" @input="patch({ args: lines(($event.target as HTMLTextAreaElement).value) })"></textarea></label>
      <label class="field-label">运行模式<select :value="model.mode" @change="patch({ mode: ($event.target as HTMLSelectElement).value as Preparation['mode'] })"><option value="once">一次性任务</option><option value="persistent">持续程序</option></select></label>
      <details><summary>高级设置</summary>
        <label class="field-label">工作目录<input :value="model.cwd ?? ''" spellcheck="false" placeholder="默认使用程序所在目录" @input="patch({ cwd: ($event.target as HTMLInputElement).value || null })" /></label>
        <label class="field-label">准备超时（秒）<input :value="model.timeoutSeconds" type="number" min="1" max="3600" required @input="patch({ timeoutSeconds: Number(($event.target as HTMLInputElement).value) })" /></label>
      </details>
      <p class="field-help">程序须输出约定的 JSON 结果。保存不会执行，打开网页或重新准备时才执行。</p>
    </template>
    <p v-if="error" class="form-error" role="alert">{{ error }}</p>
  </details>
</template>
<style scoped>
.preparation-fields { margin: 12px 0; padding: 10px 0; border-block: 1px solid var(--line); }
summary { cursor: pointer; font-size: 13px; }
.preparation-toggle { display: flex; align-items: center; gap: 8px; margin: 14px 0; font-size: 13px; }
.preparation-toggle input { width: auto; }
.preparation-fields details { margin-top: 12px; }
</style>
