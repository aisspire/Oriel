<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import AppIcon from './AppIcon.vue'
import ProxyFields from './ProxyFields.vue'
import PreparationFields from './PreparationFields.vue'
import { errorText, hostCommand, type Preparation, type ProxyPolicy, type WebEntry, type WorkspaceSnapshot } from '../lib/workspace'

const props = defineProps<{
  mode: 'add' | 'edit' | 'settings'
  entry?: WebEntry
  confirmRemove?: boolean
  workspace: WorkspaceSnapshot
  theme?: 'light' | 'dark'
}>()

const emit = defineEmits<{
  close: []
  saved: [workspace: WorkspaceSnapshot]
  theme: [value: 'light' | 'dark']
}>()

const dialog = ref<HTMLDialogElement>()
const titleInput = ref<HTMLInputElement>()
const urlInput = ref<HTMLInputElement>()

const title = ref(props.entry?.title ?? '')
const url = ref(props.entry?.url ?? '')
const preparation = ref<Preparation | null>(props.entry?.preparation ? JSON.parse(JSON.stringify(props.entry.preparation)) : null)
const proxy = ref<ProxyPolicy>({ ...(props.mode === 'settings' ? props.workspace.defaultProxy : props.entry?.proxy ?? { mode: 'inherit' }) })
const directories = ref(props.workspace.pluginDirectories.join('\n'))
const roots = ref(props.workspace.pluginRoots.join('\n'))
const busy = ref(false)
const error = ref('')
const urlError = ref('')
const confirmDelete = ref(false)
const titleTouched = ref(false)
const autoSuggestedHost = ref('')

const isLightweightRemove = computed(() => Boolean(props.confirmRemove && props.entry))

function onTitleInput() {
  titleTouched.value = true
}

function extractHost(val: string): string {
  const trimmed = val.trim()
  if (!trimmed) return ''
  try {
    const testUrl = /^[a-zA-Z][a-zA-Z\d+\-.]*:\/\//.test(trimmed) ? trimmed : `https://${trimmed}`
    const parsed = new URL(testUrl)
    return parsed.hostname
  } catch {
    return ''
  }
}

function onUrlInput() {
  if (urlError.value) urlError.value = ''
  if (props.mode === 'add') {
    const host = extractHost(url.value)
    if (!titleTouched.value || !title.value.trim() || title.value === autoSuggestedHost.value) {
      if (host) {
        title.value = host
        autoSuggestedHost.value = host
      }
    }
  }
}

function normalizeUrl(val: string): string {
  const trimmed = val.trim()
  if (!trimmed) return ''
  if (/^[a-zA-Z][a-zA-Z\d+\-.]*:\/\//.test(trimmed)) {
    return trimmed
  }
  return `https://${trimmed}`
}

function isValidUrl(val: string): boolean {
  if (!val.trim()) return true
  try {
    const parsed = new URL(val.trim())
    return Boolean(parsed.protocol && parsed.host)
  } catch {
    return false
  }
}

function onUrlBlur() {
  const trimmed = url.value.trim()
  if (trimmed) {
    const normalized = normalizeUrl(trimmed)
    if (normalized !== url.value) {
      url.value = normalized
      if (props.mode === 'add' && (!titleTouched.value || !title.value.trim() || title.value === autoSuggestedHost.value)) {
        const host = extractHost(normalized)
        if (host) {
          title.value = host
          autoSuggestedHost.value = host
        }
      }
    }
    if (!isValidUrl(url.value)) {
      urlError.value = '网页地址格式无效，请检查'
    } else {
      urlError.value = ''
    }
  } else {
    urlError.value = ''
  }
}

function clearUrl() {
  url.value = ''
  urlError.value = ''
  urlInput.value?.focus()
}

async function save(remove = false) {
  if (busy.value || !props.workspace.writable) return
  busy.value = true
  error.value = ''
  try {
    let result: WorkspaceSnapshot
    if (remove && props.entry) {
      result = await hostCommand('workspace_delete_entry', { id: props.entry.id })
    } else if (props.mode === 'settings') {
      result = await hostCommand('workspace_settings', {
        defaultProxy: proxy.value,
        pluginDirectories: directories.value.split(/\r?\n/).map(v => v.trim()).filter(Boolean),
        pluginRoots: roots.value.split(/\r?\n/).map(v => v.trim()).filter(Boolean),
      })
    } else {
      result = await hostCommand('workspace_save_entry', {
        id: props.entry?.id ?? null,
        title: title.value,
        url: url.value,
        proxy: proxy.value,
        preparation: preparation.value,
        replacePreparation: true,
      })
    }
    emit('saved', result)
    emit('close')
  } catch (cause) {
    error.value = errorText(cause)
  } finally {
    busy.value = false
  }
}

function handleSubmit() {
  if (props.mode !== 'settings') {
    if (url.value.trim()) {
      url.value = normalizeUrl(url.value)
      if (!isValidUrl(url.value)) {
        urlError.value = '网页地址格式无效，请检查'
        urlInput.value?.focus()
        return
      }
    } else if (!preparation.value) {
      urlError.value = '请输入网页地址或配置前置程序'
      urlInput.value?.focus()
      return
    }
  }
  void save()
}

onMounted(() => {
  dialog.value?.showModal()
  if (props.mode === 'add' && !isLightweightRemove.value) {
    titleInput.value?.focus()
  }
})
onUnmounted(() => dialog.value?.close())
</script>

<template>
  <dialog
    ref="dialog"
    class="settings-dialog workspace-dialog"
    aria-labelledby="workspace-dialog-title"
    @cancel="busy ? $event.preventDefault() : emit('close')"
  >
    <form class="dialog-form" @submit.prevent="handleSubmit">
      <header class="dialog-heading">
        <h1 id="workspace-dialog-title">
          {{ isLightweightRemove ? '移除网页' : mode === 'settings' ? '工作台设置' : mode === 'edit' ? '编辑网页' : '添加网页' }}
        </h1>
        <button class="plain-icon" type="button" aria-label="关闭" :disabled="busy" @click="emit('close')">
          <AppIcon name="close" />
        </button>
      </header>

      <div class="dialog-body">
        <!-- 侧栏菜单直接点击“移除网页”唤起的专属轻量确认模式 -->
        <template v-if="isLightweightRemove">
          <input type="hidden" :value="title" />
          <div class="delete-confirmation lightweight-delete">
            <p class="delete-warning-text">
              确定要移除网页「{{ entry?.title || '未命名网页' }}」并停止其托管的前置程序吗？已保存的密码和浏览数据将保留。
            </p>
            <div class="delete-actions">
              <button class="text-button" type="button" :disabled="busy" @click="emit('close')">取消</button>
              <button class="text-button danger" type="button" :disabled="busy" @click="save(true)">确认移除</button>
            </div>
          </div>
        </template>

        <!-- 普通编辑、添加或设置模式 -->
        <template v-else>
          <fieldset :disabled="busy || !workspace.writable">
            <template v-if="mode !== 'settings'">
              <label class="field-label">
                名称
                <input
                  ref="titleInput"
                  v-model="title"
                  maxlength="80"
                  required
                  :autofocus="mode === 'add'"
                  autocomplete="off"
                  placeholder="例如 GitHub"
                  @input="onTitleInput"
                />
              </label>

              <label class="field-label">
                网页地址
                <div class="input-action-group" :class="{ 'has-error': Boolean(urlError) }">
                  <input
                    ref="urlInput"
                    v-model="url"
                    type="text"
                    :required="!preparation"
                    :placeholder="preparation ? '可留空，由前置程序返回' : 'https://example.com'"
                    spellcheck="false"
                    autocomplete="off"
                    :aria-invalid="Boolean(urlError)"
                    @input="onUrlInput"
                    @blur="onUrlBlur"
                  />
                  <button
                    v-if="url"
                    class="input-inline-btn"
                    type="button"
                    aria-label="清空网址"
                    title="清空网址"
                    tabindex="-1"
                    @click="clearUrl"
                  >
                    <AppIcon name="close" />
                  </button>
                </div>
                <span v-if="urlError" class="field-error-inline" role="alert">{{ urlError }}</span>
              </label>
            </template>

            <PreparationFields v-if="mode !== 'settings'" v-model="preparation" />
            <ProxyFields v-model="proxy" :allow-inherit="mode !== 'settings'" :default-proxy="workspace.defaultProxy" />

            <template v-if="mode === 'settings'">
              <label class="field-label">
                工作台主题
                <select :value="theme ?? 'light'" @change="emit('theme', ($event.target as HTMLSelectElement).value as 'light' | 'dark')">
                  <option value="light">浅色</option>
                  <option value="dark">深色</option>
                </select>
              </label>
              <label class="field-label">
                默认插件读取目录
                <input :value="workspace.defaultPluginRoot" readonly spellcheck="false" />
                <span class="field-help">将插件交付文件夹放入此目录，保存或重新加载接入后自动发现。</span>
              </label>
              <label class="field-label">
                额外插件读取目录
                <textarea v-model="roots" rows="2" spellcheck="false" placeholder="每行一个绝对路径，扫描其直接子文件夹"></textarea>
              </label>
              <details>
                <summary>单独登记插件目录</summary>
                <label class="field-label">
                  外部插件目录
                  <textarea v-model="directories" rows="2" spellcheck="false" placeholder="每行一个包含 plugin.json 的绝对路径"></textarea>
                </label>
              </details>
              <p class="field-help">仅放入可信插件；读取声明不会执行脚本，打开工具或依赖页面时才启动所需服务。</p>
              <p class="field-help">修改代理会重新打开受影响的网页，登录数据保留。</p>
            </template>
          </fieldset>

          <p v-if="!workspace.writable" class="form-error" role="alert">本地配置不可写，请处理配置错误后重新加载。</p>
          <p v-if="error" class="form-error" role="alert">{{ error }}</p>

          <!-- 编辑模式下弹窗内点击“移除网页”展开的删除确认区 -->
          <div v-if="confirmDelete" class="delete-confirmation inline-delete">
            <p class="delete-warning-text">移除此网页入口并停止其托管的前置程序？已保存的密码和浏览数据将保留。</p>
            <div class="delete-actions">
              <button class="text-button" type="button" :disabled="busy" @click="confirmDelete = false">暂不移除</button>
              <button class="text-button danger" type="button" :disabled="busy" @click="save(true)">确认移除</button>
            </div>
          </div>
        </template>
      </div>

      <!-- 仅在非轻量删除模式下渲染吸底操作栏 -->
      <footer v-if="!isLightweightRemove" class="dialog-actions">
        <button
          v-if="mode === 'edit' && !confirmDelete"
          class="text-button danger delete-entry"
          type="button"
          :disabled="busy || !workspace.writable"
          @click="confirmDelete = true"
        >
          移除网页
        </button>
        <button class="text-button" type="button" :disabled="busy" @click="emit('close')">取消</button>
        <button
          class="primary-button"
          type="submit"
          :disabled="busy || !workspace.writable || confirmDelete"
          :class="{ 'is-disabled-soft': confirmDelete }"
        >
          {{ busy ? '正在保存…' : '保存' }}
        </button>
      </footer>
    </form>
  </dialog>
</template>

<style scoped>
.workspace-dialog {
  display: flex;
  flex-direction: column;
  padding: 0;
  overflow: hidden;
}

.dialog-form {
  display: flex;
  flex-direction: column;
  height: 100%;
  max-height: calc(100svh - 40px);
  min-height: 0;
}

.dialog-heading {
  flex-shrink: 0;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding: 24px 24px 0 24px;
  margin: 0;
}

.dialog-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 18px 24px 8px 24px;
}

.dialog-actions {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  padding: 14px 24px 24px 24px;
  margin-top: 0;
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

.input-action-group.has-error input {
  border-color: #b42437;
  outline-color: #b42437;
}

[data-theme='dark'] .input-action-group.has-error input {
  border-color: #ff9ba8;
  outline-color: #ff9ba8;
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

.input-inline-btn svg {
  width: 14px;
  height: 14px;
}

.field-error-inline {
  font-size: 12px;
  line-height: 1.4;
  color: #b42437;
  margin-top: 2px;
  font-weight: 400;
}

[data-theme='dark'] .field-error-inline {
  color: #ff9ba8;
}

.delete-confirmation {
  margin-top: 16px;
  padding: 14px 16px;
  border-radius: 12px;
  border: 1px solid rgba(180, 36, 55, 0.25);
  background: rgba(180, 36, 55, 0.05);
  font-size: 13px;
  line-height: 1.6;
}

[data-theme='dark'] .delete-confirmation {
  border: 1px solid rgba(255, 100, 100, 0.24);
  background: rgba(255, 100, 100, 0.08);
}

.lightweight-delete {
  margin-top: 4px;
}

.delete-warning-text {
  margin: 0 0 14px 0;
  color: var(--text-primary);
  font-size: 13px;
  line-height: 1.6;
}

.delete-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.is-disabled-soft {
  opacity: 0.35;
  pointer-events: none;
}

@media (max-width: 480px) {
  .workspace-dialog {
    width: calc(100vw - 24px);
  }
  .dialog-heading {
    padding: 18px 16px 0 16px;
  }
  .dialog-body {
    padding: 14px 16px 6px 16px;
  }
  .dialog-actions {
    padding: 12px 16px 18px 16px;
  }
}
</style>
