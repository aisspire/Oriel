<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import AppIcon from '../../../src/components/AppIcon.vue'
import WindowControls from '../../../src/components/WindowControls.vue'
import { defaultBrowserSidebar, errorText, hideWebSurface, hostCommand, type BrowserSidebar, type PluginViewProps, type WebEntry, type WorkspaceSnapshot } from '../../../src/lib/workspace'
import BrowserView from './BrowserView.vue'

const props = defineProps<PluginViewProps>()
const emit = defineEmits<{
  add: []; settings: []; edit: [entry: WebEntry, remove?: boolean]
  select: [id: string]; updated: [workspace: WorkspaceSnapshot]
}>()
const root = ref<HTMLElement>()
const pages = computed(() => props.workspace.entries.filter(entry => entry.editable && entry.kind === 'web-page'))
const current = computed(() => pages.value.find(entry => entry.id === props.selectedId) ?? pages.value[0])
const sidebar = ref<BrowserSidebar>({ ...defaultBrowserSidebar, ...props.workspace.browserSidebar })
const availableWidth = ref(window.innerWidth - 86)
const collapsed = computed(() => sidebar.value.collapsed || availableWidth.value < 720)
const expandedWidth = computed(() => Math.min(sidebar.value.width, Math.max(200, availableWidth.value - 480)))
const saving = ref(false)
const error = ref('')
const resizing = ref(false)
const draggedId = ref('')
const dropTarget = ref('')
const dropAfter = ref(false)
const actions = ref<HTMLDialogElement>()
const menuEntry = ref<WebEntry>()
const menuOpen = ref(false)
const menuPosition = ref({ left: '0px', top: '0px' })
const menuIndex = computed(() => pages.value.findIndex(entry => entry.id === menuEntry.value?.id))
let menuTrigger: HTMLElement | undefined
let resizeStart = { x: 0, width: 240 }
let pageDrag: { id: string; x: number; y: number; trigger: HTMLElement; pointerId: number } | undefined
let suppressPageClick = false
let observer: ResizeObserver | undefined

watch(() => current.value?.id, id => {
  if (id && id !== props.selectedId) emit('select', id)
  void nextTick(() => root.value?.querySelector('.fixed-page.is-active')?.scrollIntoView({ block: 'nearest', inline: 'nearest' }))
}, { immediate: true })
watch(() => props.workspace.browserSidebar, value => {
  if (!resizing.value && !saving.value) sidebar.value = { ...defaultBrowserSidebar, ...value }
})
watch(() => props.suspended, value => { if (value) closeMenu(false) })

async function saveSidebar() {
  if (!props.writable || saving.value) return
  saving.value = true; error.value = ''
  try {
    emit('updated', await hostCommand<WorkspaceSnapshot>('workspace_browser_sidebar', { sidebar: { ...sidebar.value } }))
  } catch (cause) {
    sidebar.value = { ...props.workspace.browserSidebar }
    error.value = errorText(cause)
  } finally { saving.value = false }
}
function toggleSidebar() {
  if (availableWidth.value < 720) return
  sidebar.value.collapsed = !sidebar.value.collapsed
  void saveSidebar()
}
async function startResize(event: PointerEvent) {
  if (event.button !== 0 || collapsed.value || saving.value || !props.writable) return
  resizeStart = { x: event.clientX, width: expandedWidth.value }
  resizing.value = true
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  event.preventDefault()
  try { await hideWebSurface() }
  catch (cause) { resizing.value = false; error.value = errorText(cause) }
}
function moveResize(event: PointerEvent) {
  if (resizing.value) sidebar.value.width = Math.round(Math.max(200, Math.min(360, resizeStart.width + event.clientX - resizeStart.x)))
}
function finishResize() {
  if (!resizing.value) return
  resizing.value = false
  void saveSidebar()
}
function resizeByKeyboard(event: KeyboardEvent) {
  const width = sidebar.value.width
  const next = event.key === 'ArrowLeft' ? width - 10 : event.key === 'ArrowRight' ? width + 10 : event.key === 'Home' ? 200 : event.key === 'End' ? 360 : undefined
  if (next === undefined || saving.value || !props.writable) return
  event.preventDefault()
  sidebar.value.width = Math.max(200, Math.min(360, next))
  void saveSidebar()
}
async function reorder(ids: string[]) {
  if (saving.value || !props.writable) return
  saving.value = true; error.value = ''
  try { emit('updated', await hostCommand<WorkspaceSnapshot>('workspace_reorder_entries', { ids })) }
  catch (cause) { error.value = errorText(cause) }
  finally { saving.value = false }
}
function startPageDrag(event: PointerEvent, id: string) {
  if (event.button !== 0 || event.pointerType === 'touch' || saving.value || !props.writable) return
  const trigger = (event.target as HTMLElement).closest<HTMLElement>('.fixed-page-button')
  if (trigger) pageDrag = { id, x: event.clientX, y: event.clientY, trigger, pointerId: event.pointerId }
}
function movePageDrag(event: PointerEvent) {
  if (!pageDrag || event.pointerId !== pageDrag.pointerId) return
  if (!draggedId.value && Math.hypot(event.clientX - pageDrag.x, event.clientY - pageDrag.y) < 6) return
  if (!draggedId.value) {
    draggedId.value = pageDrag.id
    suppressPageClick = true
    pageDrag.trigger.setPointerCapture(event.pointerId)
  }
  event.preventDefault()
  const row = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>('[data-page-id]')
  const id = row?.dataset.pageId
  if (!row || !id || id === draggedId.value) { dropTarget.value = ''; return }
  const bounds = row.getBoundingClientRect()
  dropTarget.value = id; dropAfter.value = event.clientY > bounds.y + bounds.height / 2
  const list = row.parentElement
  if (list) {
    const viewport = list.getBoundingClientRect()
    if (event.clientY < viewport.top + 32) list.scrollTop -= 16
    if (event.clientY > viewport.bottom - 32) list.scrollTop += 16
  }
}
function cancelPageDrag() {
  pageDrag = undefined; draggedId.value = ''; dropTarget.value = ''
  window.setTimeout(() => { suppressPageClick = false }, 0)
}
function finishPageDrag() {
  const id = draggedId.value
  const target = dropTarget.value
  const after = dropAfter.value
  cancelPageDrag()
  if (!id || !target) return
  const ids = pages.value.map(entry => entry.id).filter(value => value !== id)
  const index = ids.indexOf(target)
  if (index < 0) return
  ids.splice(index + (after ? 1 : 0), 0, id)
  void reorder(ids)
}
function selectPage(id: string) {
  if (!suppressPageClick) emit('select', id)
}
function movePage(offset: number) {
  const index = menuIndex.value
  closeMenu()
  if (index < 0 || index + offset < 0 || index + offset >= pages.value.length) return
  const ids = pages.value.map(entry => entry.id)
  ;[ids[index], ids[index + offset]] = [ids[index + offset], ids[index]]
  void reorder(ids)
}
async function openMenu(event: MouseEvent | KeyboardEvent, entry: WebEntry) {
  if (saving.value || menuOpen.value) return
  const trigger = event.currentTarget as HTMLElement
  menuTrigger = trigger.matches('button') ? trigger : trigger.querySelector<HTMLButtonElement>('.fixed-page-button') ?? trigger
  const bounds = trigger.getBoundingClientRect()
  menuPosition.value = {
    left: `${Math.max(8, Math.min(bounds.left, window.innerWidth - 228))}px`,
    top: `${Math.max(8, Math.min(bounds.bottom + 4, window.innerHeight - 252))}px`,
  }
  menuEntry.value = entry; menuOpen.value = true
  try {
    await hideWebSurface()
    await nextTick()
    if (menuOpen.value && !props.suspended) actions.value?.showModal()
  } catch (cause) { closeMenu(); error.value = errorText(cause) }
}
function closeMenu(restoreFocus = true) {
  actions.value?.close(); menuOpen.value = false
  if (restoreFocus) menuTrigger?.focus()
}
function menuKey(event: KeyboardEvent, entry: WebEntry) {
  if (event.key === 'ContextMenu' || event.key === 'F10' && event.shiftKey) {
    event.preventDefault(); void openMenu(event, entry)
  }
}
function editPage(remove = false) {
  const entry = menuEntry.value
  closeMenu()
  if (entry) emit('edit', entry, remove)
}
function domain(entry: WebEntry) { try { return new URL(entry.url ?? '').host } catch { return '' } }

onMounted(() => {
  observer = new ResizeObserver(([entry]) => { if (entry) availableWidth.value = entry.contentRect.width })
  if (root.value) observer.observe(root.value)
  window.addEventListener('pointermove', moveResize)
  window.addEventListener('pointerup', finishResize)
  window.addEventListener('pointercancel', finishResize)
  window.addEventListener('blur', finishResize)
  window.addEventListener('pointermove', movePageDrag)
  window.addEventListener('pointerup', finishPageDrag)
  window.addEventListener('pointercancel', cancelPageDrag)
  window.addEventListener('blur', cancelPageDrag)
})
onUnmounted(() => {
  observer?.disconnect(); actions.value?.close()
  window.removeEventListener('pointermove', moveResize)
  window.removeEventListener('pointerup', finishResize)
  window.removeEventListener('pointercancel', finishResize)
  window.removeEventListener('blur', finishResize)
  window.removeEventListener('pointermove', movePageDrag)
  window.removeEventListener('pointerup', finishPageDrag)
  window.removeEventListener('pointercancel', cancelPageDrag)
  window.removeEventListener('blur', cancelPageDrag)
})
</script>

<template>
  <section ref="root" class="browser-workspace" :class="{ 'is-compact': collapsed, 'is-resizing': resizing }" :style="{ '--browser-sidebar-width': `${collapsed ? 52 : expandedWidth}px` }" aria-label="浏览器">
    <aside class="browser-sidebar" aria-label="固定网页">
      <header class="browser-sidebar-heading">
        <span v-if="!collapsed" class="browser-sidebar-title" data-tauri-drag-region>浏览器</span>
        <button class="plain-icon" type="button" :aria-label="collapsed ? '展开网页侧栏' : '收窄网页侧栏'" :title="availableWidth < 720 ? '窗口较窄，已使用图标模式' : collapsed ? '展开网页侧栏' : '收窄网页侧栏'" :aria-expanded="!collapsed" aria-controls="fixed-pages" :disabled="saving || availableWidth < 720" @click="toggleSidebar"><AppIcon name="panel" /></button>
      </header>
      <nav id="fixed-pages" class="fixed-pages" aria-label="固定页签">
        <div v-for="page in pages" :key="page.id" class="fixed-page" :class="{ 'is-active': current?.id === page.id, 'is-dragging': draggedId === page.id, 'drop-before': dropTarget === page.id && !dropAfter, 'drop-after': dropTarget === page.id && dropAfter }"
          :data-page-id="page.id" @pointerdown="startPageDrag($event, page.id)" @contextmenu.prevent="openMenu($event, page)" @keydown="menuKey($event, page)">
          <button class="fixed-page-button" type="button" :aria-label="page.title" :aria-description="domain(page)" :aria-current="current?.id === page.id ? 'page' : undefined" :title="`${page.title} · ${domain(page)}`" @click="selectPage(page.id)">
            <span class="fixed-page-icon" aria-hidden="true">{{ Array.from(page.title)[0] }}</span>
            <span v-if="!collapsed" class="fixed-page-name">{{ page.title }}</span>
          </button>
          <button v-if="!collapsed" class="plain-icon fixed-page-menu" type="button" :aria-label="`${page.title}的菜单`" title="网页菜单" aria-haspopup="dialog" :disabled="saving" @click="openMenu($event, page)"><AppIcon name="more" /></button>
        </div>
        <p v-if="!pages.length && !collapsed" class="fixed-pages-empty">还没有网页</p>
      </nav>
      <button class="add-fixed-page" type="button" aria-label="添加网页" title="添加网页" :disabled="!writable || saving" @click="emit('add')"><AppIcon name="plus" /><span v-if="!collapsed">添加网页</span></button>
      <button v-if="collapsed && pages.length" class="plain-icon compact-page-menu" type="button" aria-label="当前网页的菜单" title="当前网页的菜单" aria-haspopup="dialog" :disabled="saving" @click="openMenu($event, current!)"><AppIcon name="more" /></button>
      <div v-if="!collapsed" class="browser-sidebar-resizer" role="separator" aria-label="调整网页侧栏宽度" aria-orientation="vertical" :aria-valuenow="sidebar.width" :aria-valuemin="200" :aria-valuemax="360" :tabindex="writable && !saving ? 0 : -1" @pointerdown="startResize" @keydown="resizeByKeyboard"></div>
    </aside>
    <div class="browser-page-area">
      <div v-if="error" class="browser-error" role="alert"><span>{{ error }}</span><button class="text-button" type="button" @click="error = ''">关闭</button></div>
      <BrowserView v-if="current" :entry="current" :workspace="workspace" :selected-id="selectedId" :supported="supported" :writable="writable" :suspended="suspended || menuOpen || resizing" @edit="emit('edit', $event)" @settings="emit('settings')" />
      <template v-else>
        <header class="browser-toolbar browser-empty-toolbar"><div class="toolbar-drag-spacer" data-tauri-drag-region></div><WindowControls /></header>
        <div class="browser-empty fixed-browser-empty"><AppIcon name="globe" /><p>添加网页，放在手边。</p><button class="primary-button" type="button" :disabled="!writable" @click="emit('add')">添加网页</button></div>
      </template>
    </div>
    <dialog ref="actions" class="page-actions" :style="menuPosition" aria-labelledby="page-actions-title" @cancel.prevent="closeMenu()" @click="$event.target === actions && closeMenu()">
      <div class="page-actions-heading"><strong id="page-actions-title">{{ menuEntry?.title }}</strong><span>{{ menuEntry && domain(menuEntry) }}</span></div>
      <button type="button" :disabled="!writable" @click="editPage()">编辑网页</button>
      <button type="button" :disabled="!writable || menuIndex <= 0" @click="movePage(-1)">上移</button>
      <button type="button" :disabled="!writable || menuIndex >= pages.length - 1" @click="movePage(1)">下移</button>
      <button class="danger" type="button" :disabled="!writable" @click="editPage(true)">移除网页</button>
    </dialog>
    <span class="sr-only" role="status">{{ saving ? '正在保存' : '' }}</span>
  </section>
</template>
