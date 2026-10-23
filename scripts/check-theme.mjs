// Windows-only, isolated native smoke checks. Never uses a real SSH configuration.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { createServer } from 'node:http'
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { Cdp } from './lib/cdp.mjs'
if (process.platform !== 'win32') throw new Error('Run with Windows Node after building the debug application and frontend.')
const root = path.resolve('.')
const data = path.join(root, 'target', 'theme-check', String(Date.now()))
await mkdir(data, { recursive: true })
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
async function until(check, label, timeout = 25000) {
  const deadline = Date.now() + timeout
  let last
  while (Date.now() < deadline) {
    try { const value = await check(); if (value) return value } catch (error) { last = error }
    await delay(80)
  }
  throw new Error(`Timeout: ${label}: ${last?.message ?? ''}`)
}
async function port(directory) {
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, item.name)
    if (item.name === 'DevToolsActivePort') return Number((await readFile(file, 'utf8')).split('\n')[0])
    if (item.isDirectory()) { const found = await port(file); if (found) return found }
  }
}
async function profile(directory, predicate) {
  return until(async () => {
    const number = await port(directory)
    if (!number) return
    const targets = await (await fetch(`http://127.0.0.1:${number}/json/list`)).json()
    const target = targets.find(t => t.type === 'page' && predicate(t))
    if (target) return new Cdp().connect(target.webSocketDebuggerUrl)
  }, `profile ${directory}`)
}
const fixture = createServer((req, res) => {
  res.setHeader('Content-Type', 'text/html; charset=utf-8')
  res.end(`<title>Theme fixture</title><style>:root {color-scheme:light dark}</style><h1>Ready</h1><button onclick="window.open('/popup')">Popup</button><script>
  window.initialDark=matchMedia('(prefers-color-scheme: dark)').matches;
  window.loadId=Math.random();window.changes=[];
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change',e=>changes.push(e.matches));
  </script>`)
})
await new Promise(resolve => fixture.listen(0, '127.0.0.1', resolve))
const base = `http://127.0.0.1:${fixture.address().port}`
const shellServer = createServer(async (req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname
  const file = path.join(root, 'dist', pathname === '/' ? 'index.html' : pathname)
  try { const bytes = await readFile(file); res.setHeader('Content-Type', file.endsWith('.js') ? 'text/javascript' : file.endsWith('.css') ? 'text/css' : 'text/html'); res.end(bytes) }
  catch { res.writeHead(404); res.end() }
})
await new Promise((resolve, reject) => { shellServer.once('error', reject); shellServer.listen(1420, '127.0.0.1', resolve) })
await writeFile(path.join(data, 'workspace.json'), JSON.stringify({ version: 2, theme: 'dark', webEntries: [] }))
const app = spawn(path.join(root, 'target/windows/debug/oriel.exe'), [], { env: { ...process.env, ORIEL_DEV_DATA_DIR: data, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=0' }, stdio: ['ignore', 'pipe', 'pipe'] })
let diagnostics = ''
app.stdout.on('data', bytes => diagnostics += bytes)
app.stderr.on('data', bytes => diagnostics += bytes)
let shell
const pages = []
const passed = label => console.log(`PASS ${label}`)
const host = (command, args = {}) => shell.evaluate(`window.__TAURI__.core.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`)
const show = id => host('web_show', { entryId: id, bounds: { x: 100, y: 150, width: 650, height: 400 }, visible: true })
try {
  shell = await profile(path.join(data, 'shell'), t => t.url.startsWith('http://127.0.0.1:1420'))
  await until(() => host('workspace_snapshot'), 'main IPC')
  const ids = []
  for (const title of ['主题页面一', '主题页面二']) {
    const snapshot = await host('workspace_save_entry', { id: null, title, url: base, proxy: { mode: 'direct' } })
    ids.push(snapshot.entries.find(e => e.title === title).id)
  }
  await Promise.all(ids.map(entryId => host('web_prepare', { entryId, retry: false })))
  for (const id of ids) {
    await show(id)
    const page = await profile(path.join(data, 'web-profiles', id), t => t.url.startsWith(base))
    pages.push(page)
    await until(() => page.evaluate('document.readyState === "complete"'), 'page ready')
    assert.equal(await page.evaluate('window.initialDark'), true)
  }
  const loadIds = await Promise.all(pages.map(p => p.evaluate('window.loadId')))
  await host('workspace_theme', { theme: 'light' })
  for (let i = 0; i < pages.length; i++) {
    await until(() => pages[i].evaluate('!matchMedia("(prefers-color-scheme: dark)").matches'), 'hidden and visible theme')
    assert.equal(await pages[i].evaluate('window.loadId'), loadIds[i])
    assert.ok((await pages[i].evaluate('window.changes')).includes(false))
  }
  passed('theme synchronizes existing hidden and visible pages without refresh')
  await host('workspace_theme', { theme: 'dark' })
  await pages[1].evaluate('document.querySelector("button").click()')
  const popup = await profile(path.join(data, 'web-profiles', ids[1]), t => t.url.endsWith('/popup'))
  pages.push(popup)
  await until(() => popup.evaluate('document.readyState === "complete"'), 'popup ready')
  assert.equal(await popup.evaluate('window.initialDark'), true)
  await host('workspace_theme', { theme: 'light' })
  await until(() => popup.evaluate('!matchMedia("(prefers-color-scheme: dark)").matches'), 'popup theme follows')
  passed('popup receives the current theme before its first document and on changes')
  const persisted = JSON.parse(await readFile(path.join(data, 'workspace.json'), 'utf8'))
  assert.equal(persisted.version, 3); assert.equal(persisted.theme, 'light')
  assert.equal(await shell.evaluate('localStorage.getItem("oriel.appearance.theme")'), null)
  passed('successful v3 migration preserves local configuration as theme authority')
  await host('web_show', { entryId: null, bounds: { x: 0, y: 0, width: 1, height: 1 }, visible: false })
  await shell.evaluate('document.querySelector("button[aria-label=工作台设置]").click()')
  await until(() => shell.evaluate('Boolean(document.querySelector(".settings-dialog[open]"))'), 'theme settings UI')
  for (const theme of ['light', 'dark']) {
    await host('workspace_theme', { theme })
    await until(() => shell.evaluate(`document.documentElement.dataset.theme===${JSON.stringify(theme)}`), 'main theme follows host')
    const screenshot = await shell.command('Page.captureScreenshot', { format: 'png' })
    await writeFile(path.join(data, `settings-${theme}.png`), Buffer.from(screenshot.data, 'base64'))
  }
  passed('theme settings render in both themes; screenshots saved for review')
  await shell.evaluate('document.querySelector(".settings-dialog[open] .dialog-heading button").click()')
  await shell.evaluate('window.__TAURI__.window.getCurrentWindow().close()')
  await until(() => app.exitCode !== null, 'complete exit')
  passed('application completes exit')
  console.log(`Artifacts: ${data}`)
} catch (error) {
  console.error(diagnostics)
  throw error
} finally {
  for (const page of pages) { try { page.close() } catch {} }
  if (app.exitCode === null) {
    try { await shell?.evaluate('window.__TAURI__.window.getCurrentWindow().close()'); await until(() => app.exitCode !== null, 'cleanup exit', 15000) } catch { app.kill() }
  }
  shell?.close(); fixture.closeAllConnections(); fixture.close(); shellServer.closeAllConnections(); shellServer.close()
}
