import { onMounted, onUnmounted, type Ref } from 'vue'
import { dialogDismissHandlers } from '../lib/dialogDismiss'

export function useDialogDismiss(dialog: Ref<HTMLDialogElement | undefined>) {
  const handlers = dialogDismissHandlers(() => dialog.value, () => {
    const dialogs = document.querySelectorAll<HTMLDialogElement>('dialog[open]')
    return dialogs[dialogs.length - 1]
  })
  onMounted(() => {
    document.addEventListener('pointerdown', handlers.pointerdown, true)
    document.addEventListener('pointerup', handlers.pointerup, true)
    document.addEventListener('pointercancel', handlers.pointercancel, true)
  })
  onUnmounted(() => {
    document.removeEventListener('pointerdown', handlers.pointerdown, true)
    document.removeEventListener('pointerup', handlers.pointerup, true)
    document.removeEventListener('pointercancel', handlers.pointercancel, true)
  })
}
