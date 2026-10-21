<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import AppIcon from './AppIcon.vue'
import ProxyFields from './ProxyFields.vue'
import { errorText, hostCommand, type ProxyPolicy, type WebEntry, type WorkspaceSnapshot } from '../lib/workspace'

const props = defineProps<{ mode: 'add' | 'edit' | 'settings'; entry?: WebEntry; confirmRemove?: boolean; workspace: WorkspaceSnapshot }>()
const emit = defineEmits<{ close: []; saved: [workspace: WorkspaceSnapshot] }>()
const dialog = ref<HTMLDialogElement>()
const title = ref(props.entry?.title ?? '')
const url = ref(props.entry?.url ?? '')
const proxy = ref<ProxyPolicy>({ ...(props.mode === 'settings' ? props.workspace.defaultProxy : props.entry?.proxy ?? { mode: 'inherit' }) })
const directories = ref(props.workspace.pluginDirectories.join('\n'))
const roots = ref(props.workspace.pluginRoots.join('\n'))
const busy = ref(false)
const error = ref('')
const confirmDelete = ref(props.confirmRemove ?? false)

async function save(remove = false) {
  if (busy.value || !props.workspace.writable) return
  busy.value = true; error.value = ''
  try {
    let result: WorkspaceSnapshot
    if (remove && props.entry) result = await hostCommand('workspace_delete_entry', { id: props.entry.id })
    else if (props.mode === 'settings') result = await hostCommand('workspace_settings', {
      defaultProxy: proxy.value, pluginDirectories: directories.value.split(/\r?\n/).map(v => v.trim()).filter(Boolean),
      pluginRoots: roots.value.split(/\r?\n/).map(v => v.trim()).filter(Boolean),
    })
    else result = await hostCommand('workspace_save_entry', { id: props.entry?.id ?? null, title: title.value, url: url.value, proxy: proxy.value })
    emit('saved', result); emit('close')
  } catch (cause) { error.value = errorText(cause) }
  finally { busy.value = false }
}
onMounted(() => dialog.value?.showModal())
onUnmounted(() => dialog.value?.close())
</script>

<template>
  <dialog ref="dialog" class="settings-dialog" aria-labelledby="workspace-dialog-title" @cancel="busy ? $event.preventDefault() : emit('close')">
    <form @submit.prevent="save()">
      <header class="dialog-heading">
        <h1 id="workspace-dialog-title">{{ mode === 'settings' ? '工作台设置' : mode === 'edit' ? '编辑网页' : '添加网页' }}</h1>
        <button class="plain-icon" type="button" aria-label="关闭" :disabled="busy" @click="emit('close')"><AppIcon name="close" /></button>
      </header>
      <fieldset :disabled="busy || !workspace.writable">
        <template v-if="mode !== 'settings'">
          <label class="field-label">名称<input v-model="title" maxlength="80" required autofocus autocomplete="off" /></label>
          <label class="field-label">网页地址<input v-model="url" type="text" required placeholder="https://example.com" spellcheck="false" autocomplete="off" /></label>
        </template>
        <ProxyFields v-model="proxy" :allow-inherit="mode !== 'settings'" />
        <template v-if="mode === 'settings'">
          <label class="field-label">默认插件读取目录<input :value="workspace.defaultPluginRoot" readonly spellcheck="false" />
            <span class="field-help">将插件交付文件夹放入此目录，保存或重新加载接入后自动发现。</span>
          </label>
          <label class="field-label">额外插件读取目录<textarea v-model="roots" rows="2" spellcheck="false" placeholder="每行一个绝对路径，扫描其直接子文件夹"></textarea></label>
          <details><summary>单独登记插件目录</summary><label class="field-label">外部插件目录<textarea v-model="directories" rows="2" spellcheck="false" placeholder="每行一个包含 plugin.json 的绝对路径"></textarea></label></details>
          <p class="field-help">仅放入可信插件；读取声明不会执行脚本，打开工具或依赖页面时才启动所需服务。</p>
          <p class="field-help">修改代理会重新打开受影响的网页，登录数据保留。</p>
        </template>
      </fieldset>
      <p v-if="!workspace.writable" class="form-error" role="alert">本地配置不可写，请处理配置错误后重新加载。</p>
      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <div v-if="confirmDelete" class="delete-confirmation">
        <p>移除此网页入口？已保存的密码和浏览数据将保留。</p>
        <button class="text-button" type="button" :disabled="busy" @click="confirmDelete = false">取消</button>
        <button class="text-button danger" type="button" :disabled="busy" @click="save(true)">确认移除</button>
      </div>
      <footer class="dialog-actions">
        <button v-if="mode === 'edit' && !confirmDelete" class="text-button danger delete-entry" type="button" :disabled="busy || !workspace.writable" @click="confirmDelete = true">移除网页</button>
        <button class="text-button" type="button" :disabled="busy" @click="emit('close')">取消</button>
        <button class="primary-button" type="submit" :disabled="busy || !workspace.writable">{{ busy ? '正在保存…' : '保存' }}</button>
      </footer>
    </form>
  </dialog>
</template>
