interface DesktopImage {
  close(): Promise<void>
}

interface DesktopWindow {
  minimize(): Promise<void>
  toggleMaximize(): Promise<void>
  isMaximized(): Promise<boolean>
  close(): Promise<void>
  setIcon(icon: DesktopImage): Promise<void>
}

interface TauriWindow extends Window {
  __TAURI__?: {
    core: { invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> }
    event: {
      listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>
      emitTo: (target: string, event: string, payload?: unknown) => Promise<void>
    }
    window: { getCurrentWindow(): DesktopWindow }
    image: { Image: { new: (rgba: number[], width: number, height: number) => Promise<DesktopImage> } }
  }
}

export function getDesktopApi() {
  return (window as TauriWindow).__TAURI__
}

// The public global API is injected by Tauri; browser previews have no native controls.
export function getDesktopWindow(): DesktopWindow | undefined {
  return (window as TauriWindow).__TAURI__?.window.getCurrentWindow()
}

let iconRequest = 0

export async function syncDesktopIcon(source: string): Promise<void> {
  const api = (window as TauriWindow).__TAURI__
  if (!api) return
  const request = ++iconRequest
  const image = new Image()
  image.src = source
  await image.decode()
  if (request !== iconRequest) return

  // Native windows need pixels; rasterize the same SVG used by the themed UI.
  const size = 128
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Cannot render the Oriel window icon.')
  context.drawImage(image, 0, 0, size, size)
  const rgba = Array.from(context.getImageData(0, 0, size, size).data)
  const icon = await api.image.Image.new(rgba, size, size)
  try {
    if (request === iconRequest) await api.window.getCurrentWindow().setIcon(icon)
  } finally {
    await icon.close()
  }
}
