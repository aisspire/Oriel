// Native dialog backdrops target the dialog itself, so target equality cannot
// distinguish the content from the backdrop. Track both ends geometrically.
export function dialogDismissHandlers(getDialog: () => HTMLDialogElement | undefined, getTop: () => HTMLDialogElement | undefined) {
  let press: { id: number; dialog: HTMLDialogElement } | undefined
  const outside = (dialog: HTMLDialogElement, event: PointerEvent) => {
    const r = dialog.getBoundingClientRect()
    return event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom
  }
  return {
    pointerdown(event: PointerEvent) {
      press = undefined
      const dialog = getDialog()
      if (!event.isPrimary || event.button !== 0 || !dialog?.open || getTop() !== dialog) return
      if (outside(dialog, event)) {
        press = { id: event.pointerId, dialog }
        event.preventDefault()
      }
    },
    pointerup(event: PointerEvent) {
      const start = press
      press = undefined
      if (!start || start.id !== event.pointerId || !event.isPrimary || event.button !== 0) return
      const dialog = getDialog()
      if (dialog !== start.dialog || !dialog.open || getTop() !== dialog || !outside(dialog, event)) return
      event.preventDefault()
      event.stopPropagation()
      dialog.dispatchEvent(new Event('cancel', { cancelable: true }))
    },
    pointercancel() { press = undefined },
  }
}
