import { execFileSync } from 'node:child_process'
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(process.argv[2] ?? fileURLToPath(new URL('..', import.meta.url)))
const source = join(root, 'public/oriel-icon.svg')
const dark = await readFile(source, 'utf8')
const light = dark.replace(/fill="#(000|fff)"/g, (_, color) => `fill="#${color === '000' ? 'fff' : '000'}"`)
await writeFile(join(root, 'public/oriel-icon-light.svg'), light)

const output = await mkdtemp(join(tmpdir(), 'oriel-icons-'))
try {
  execFileSync(process.execPath, [
    join(root, 'node_modules/@tauri-apps/cli/tauri.js'),
    'icon', source, '--output', output,
  ], { stdio: 'inherit' })

  const desktopIcons = join(root, 'src-tauri/icons')
  await mkdir(desktopIcons, { recursive: true })
  for (const name of ['32x32.png', '128x128.png', '128x128@2x.png', 'icon.png', 'icon.ico', 'icon.icns']) {
    await copyFile(join(output, name), join(desktopIcons, name))
  }
  await copyFile(join(output, 'ios/AppIcon-60x60@3x.png'), join(root, 'public/apple-touch-icon.png'))
  console.log('Generated light SVG, desktop icons, and touch icon from the Oriel SVG master.')
} finally {
  await rm(output, { recursive: true, force: true })
}
