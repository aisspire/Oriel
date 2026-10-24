<script setup lang="ts">
import { ref } from 'vue'
import AppIcon from './AppIcon.vue'
import { errorText, hostCommand, type Preparation } from '../lib/workspace'

const model = defineModel<Preparation | null>({ required: true })
const picking = ref(false)
const error = ref('')

function enable(value: boolean) {
  model.value = value ? { path: '', runner: 'auto', args: [], mode: 'once', timeoutSeconds: 120 } : null
}

function patch(value: Partial<Preparation>) {
  if (model.value) model.value = { ...model.value, ...value }
}

function setRunner(runner: Preparation['runner']) {
  patch({ runner, interpreter: runner === 'custom' ? { program: '', args: [] } : null })
}

function lines(value: string) {
  return value === '' ? [] : value.split(/\r?\n/)
}

async function pick() {
  picking.value = true
  error.value = ''
  try {
    const path = await hostCommand<string | null>('preparation_pick_file')
    if (path) patch({ path })
  } catch (cause) {
    error.value = errorText(cause)
  } finally {
    picking.value = false
  }
}
</script>

<template>
  <div class="preparation-fields">
    <div class="preparation-header">
      <label class="preparation-toggle">
        <input
          type="checkbox"
          :checked="Boolean(model)"
          @change="enable(($event.target as HTMLInputElement).checked)"
        />
        <span class="preparation-toggle-label">打开网页前执行程序</span>
      </label>
      <span v-if="model" class="preparation-status">已启用</span>
    </div>

    <div v-if="model" class="preparation-body">
      <!-- 程序或脚本路径（单行输入组合控件） -->
      <label class="field-label">
        程序或脚本路径
        <div class="input-action-group">
          <input
            :value="model.path"
            required
            spellcheck="false"
            autocomplete="off"
            placeholder="C:\tools\prepare.ps1"
            @input="patch({ path: ($event.target as HTMLInputElement).value })"
          />
          <button
            class="input-inline-btn"
            type="button"
            :disabled="picking"
            :title="picking ? '正在选择…' : '选择文件'"
            :aria-label="picking ? '正在选择…' : '选择文件'"
            @click="pick"
          >
            <AppIcon name="folder" />
          </button>
        </div>
      </label>

      <!-- 执行方式 -->
      <label class="field-label">
        执行方式
        <select :value="model.runner" @change="setRunner(($event.target as HTMLSelectElement).value as Preparation['runner'])">
          <option value="auto">自动</option>
          <option value="direct">直接执行</option>
          <option value="cmd">CMD</option>
          <option value="powershell">PowerShell</option>
          <option value="custom">自定义解释器</option>
        </select>
      </label>

      <!-- 自定义解释器 -->
      <template v-if="model.runner === 'custom'">
        <label class="field-label">
          解释器程序
          <input
            :value="model.interpreter?.program"
            required
            spellcheck="false"
            placeholder="node.exe"
            @input="patch({ interpreter: { program: ($event.target as HTMLInputElement).value, args: model.interpreter?.args ?? [] } })"
          />
        </label>
        <label class="field-label">
          解释器参数
          <textarea
            :value="model.interpreter?.args.join('\n')"
            rows="2"
            spellcheck="false"
            placeholder="每行一个，放在脚本路径之前"
            @input="patch({ interpreter: { program: model.interpreter?.program ?? '', args: lines(($event.target as HTMLTextAreaElement).value) } })"
          ></textarea>
        </label>
      </template>

      <!-- 程序参数 -->
      <label class="field-label">
        程序参数
        <textarea
          :value="model.args.join('\n')"
          rows="2"
          spellcheck="false"
          placeholder="每行一个参数，不需要添加包围引号"
          @input="patch({ args: lines(($event.target as HTMLTextAreaElement).value) })"
        ></textarea>
      </label>

      <!-- 运行模式 -->
      <label class="field-label">
        运行模式
        <select :value="model.mode" @change="patch({ mode: ($event.target as HTMLSelectElement).value as Preparation['mode'] })">
          <option value="once">一次性任务</option>
          <option value="persistent">持续程序</option>
        </select>
      </label>

      <!-- 高级设置两列并排网格布局 -->
      <div class="advanced-grid">
        <label class="field-label">
          <span class="label-with-icon">
            <AppIcon name="folder" class="field-sub-icon" />
            工作目录
          </span>
          <input
            :value="model.cwd ?? ''"
            spellcheck="false"
            placeholder="默认使用程序所在目录"
            @input="patch({ cwd: ($event.target as HTMLInputElement).value || null })"
          />
        </label>
        <label class="field-label">
          <span class="label-with-icon">
            <AppIcon name="clock" class="field-sub-icon" />
            准备超时（秒）
          </span>
          <input
            :value="model.timeoutSeconds"
            type="number"
            min="1"
            max="3600"
            required
            @input="patch({ timeoutSeconds: Number(($event.target as HTMLInputElement).value) })"
          />
        </label>
      </div>

      <p class="field-help">程序须输出约定的 JSON 结果。保存不会执行，打开网页或重新准备时才执行。</p>
    </div>

    <p v-if="error" class="form-error" role="alert">{{ error }}</p>
  </div>
</template>

<style scoped>
.preparation-fields {
  margin: 4px 0;
  padding: 14px 0;
  border-block: 1px solid var(--line);
}
.preparation-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.preparation-toggle {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  user-select: none;
}
.preparation-toggle input[type="checkbox"] {
  width: 15px;
  height: 15px;
  accent-color: var(--focus);
  cursor: pointer;
}
.preparation-status {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 12px;
  background: var(--line);
  color: var(--text-secondary);
}
.preparation-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin-top: 14px;
}
.input-action-group {
  position: relative;
  display: flex;
  align-items: center;
  width: 100%;
}
.input-action-group input {
  width: 100%;
  padding-right: 36px;
}
.input-inline-btn {
  position: absolute;
  right: 4px;
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 120ms, color 120ms;
}
.input-inline-btn:hover:not(:disabled) {
  background: var(--line);
  color: var(--text-primary);
}
.input-inline-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.input-inline-btn svg {
  width: 15px;
  height: 15px;
}
.label-with-icon {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.field-sub-icon {
  width: 14px;
  height: 14px;
  color: var(--text-secondary);
}
.advanced-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
@media (max-width: 440px) {
  .advanced-grid {
    grid-template-columns: 1fr;
  }
}
</style>
