// Windows-only, isolated fixture. Captures the OS-composited Oriel client area.
// Run once with Transparency effects enabled and again disabled; never edits it.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { createServer } from 'node:http'
import { mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { Cdp } from './lib/cdp.mjs'
import { connectHostTarget } from './lib/host-target.mjs'
if (process.platform !== 'win32') throw new Error('Run with Windows Node and the Windows debug app; native acceptance cannot run in WSL.')
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const material = process.argv.find(arg => arg.startsWith('--material='))?.split('=')[1]
assert.ok(['enabled', 'disabled'].includes(material), 'Specify --material=enabled or --material=disabled after setting Windows Transparency effects manually.')
await mkdir(path.join(root, 'target'), { recursive: true })
const data = await mkdtemp(path.join(root, 'target', 'overlay-check-'))
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
async function until(fn, label, timeout = 15000) {
  const end = Date.now() + timeout
  while (Date.now() < end) { const value = await fn(); if (value) return value; await delay(15) }
  throw new Error(`Timeout: ${label}`)
}
async function listen(server, port = 0) {
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port, '127.0.0.1', resolve) })
  return server.address().port
}
async function activePort(directory) {
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, item.name)
    if (item.name === 'DevToolsActivePort') return Number((await readFile(file, 'utf8')).split('\n')[0])
    if (item.isDirectory()) { const port = await activePort(file); if (port) return port }
  }
}
async function connect(directory, label) {
  return until(async () => {
    try {
      const port = await activePort(directory)
      if (!port) return
      const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
      if (label) return connectHostTarget(targets, label, Cdp)
      const page = targets.find(t => t.type === 'page' && t.url.startsWith(origin))
      if (page) return new Cdp().connect(page.webSocketDebuggerUrl)
    } catch { return undefined }
  }, `connect ${label ?? 'fixture'}`)
}
const fixture = createServer((_req, res) => {
  res.setHeader('Content-Type', 'text/html; charset=utf-8')
  res.end(`<title>Overlay fixture</title><style>body{background:repeating-conic-gradient(#20c5c5 0% 25%,#f6a43b 0% 50%) 0/60px 60px;font:32px sans-serif}h1{background:white;padding:24px}</style><script>sessionStorage.loads=Number(sessionStorage.loads||0)+1;window.fixtureIdentity=Math.random()</script><h1>Oriel 磨砂验收</h1><input aria-label="底层输入" placeholder="弹窗期间不应收到点击">`)
})
const origin = `http://127.0.0.1:${await listen(fixture)}`
const plugin = path.join(data, 'plugins', 'overlay-fixture')
await mkdir(plugin, { recursive: true })
await writeFile(path.join(plugin, 'help.md'), '# 说明验收\n\n这是隔离测试插件。')
await writeFile(path.join(plugin, 'plugin.json'), JSON.stringify({
  schemaVersion: 2, id: 'check.overlay', name: '弹窗验收', version: '1', capabilities: ['web-page'], documentation: 'help.md',
  entries: [
    { id: 'browser', title: '浏览布局验收', kind: 'web-page', presentation: 'browser', url: origin },
    { id: 'app', title: '应用布局验收', kind: 'web-page', presentation: 'application', url: origin },
  ],
  slots: [
    { target: 'header:action', action: 'settings', icon: 'settings', title: '验收设置' },
    { target: 'header:action', action: 'preparations', icon: 'terminal', title: '验收前置程序' },
  ],
}))
await writeFile(path.join(data, 'workspace.json'), JSON.stringify({ version: 4, theme: 'light', webEntries: [], defaultProxy: { mode: 'direct' } }))
const shellServer = createServer(async (req, res) => {
  const pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname)
  const file = path.resolve(root, 'dist', pathname === '/' ? 'index.html' : `.${pathname}`)
  if (!file.startsWith(path.join(root, 'dist') + path.sep)) { res.writeHead(403); res.end(); return }
  try {
    const bytes = await readFile(file)
    res.setHeader('Content-Type', ({ '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml' })[path.extname(file)] ?? 'text/html')
    res.end(bytes)
  } catch { res.writeHead(404); res.end() }
})
await listen(shellServer, 1420)
const executable = path.resolve(root, process.env.ORIEL_CHECK_EXE ?? 'target/windows/debug/oriel.exe')
const app = spawn(executable, [], { env: { ...process.env, ORIEL_DEV_DATA_DIR: data, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=0' }, stdio: ['ignore', 'pipe', 'pipe'] })
app.stdout.resume()
let nativeLogs = ''; app.stderr.on('data', b => { nativeLogs += b })
let shell, overlay, page
const host = (command, args = {}) => shell.evaluate(`window.__TAURI__.core.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`)
const opened = () => host('overlay_status').then(s => s.request?.phase === 'shown')
const closed = async () => {
  await until(async () => !(await opened()), 'overlay closed')
  // Main restores focus after its authoritative close poll. Do not race its guard.
  await until(() => shell.evaluate("document.hasFocus() && !document.activeElement?.classList.contains('app-shell')"), 'main focus')
  await delay(120)
}
async function clickMain(selector) {
  await shell.evaluate(`(() => { const b=document.querySelector(${JSON.stringify(selector)}); if(!b || b.disabled)throw Error('Missing or disabled trigger'); b.focus(); b.click() })()`)
  await until(opened, 'interactive overlay')
  assert.equal(await overlay.evaluate('Boolean(document.querySelector("dialog[open]"))'), true)
}
async function pointer(from, to) {
  await overlay.command('Input.dispatchMouseEvent', { type: 'mousePressed', x: from[0], y: from[1], button: 'left', clickCount: 1 })
  await overlay.command('Input.dispatchMouseEvent', { type: 'mouseReleased', x: to[0], y: to[1], button: 'left', clickCount: 1 })
}
async function center() { return overlay.evaluate('(() => { const r=document.querySelector("dialog[open]").getBoundingClientRect(); return [r.x+r.width/2,r.y+r.height/2] })()') }
async function outsideClose() { await pointer([2, 2], [2, 2]); await closed() }
async function escape() {
  await overlay.command('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 })
  await overlay.command('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 })
}
async function composedScreenshot(name) {
  // CopyFromScreen captures DWM output including the underlying child WebView,
  // Acrylic and overlay; Page.captureScreenshot cannot establish this result.
  const output = path.join(data, `${name}.png`)
  const script = `Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public class OrielCapture {
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int left,top,right,bottom; }
[StructLayout(LayoutKind.Sequential)] public struct POINT { public int x,y; }
[DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h,out RECT r);
[DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h,ref POINT p);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls,string title);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
}
'@
$orielWindow=(Get-Process -Id ${app.pid}).MainWindowHandle
$orielOverlay=[OrielCapture]::FindWindow($null,'Oriel Overlay')
[uint32]$orielOwner=0
[OrielCapture]::GetWindowThreadProcessId($orielOverlay,[ref]$orielOwner) | Out-Null
if($orielOwner -ne ${app.pid}){throw 'Overlay process identity mismatch'}
[OrielCapture]::SetForegroundWindow($orielOverlay) | Out-Null
Start-Sleep -Milliseconds 250
$orielRect=New-Object OrielCapture+RECT
$orielPoint=New-Object OrielCapture+POINT
if(-not [OrielCapture]::GetClientRect($orielWindow,[ref]$orielRect)){throw 'GetClientRect failed'}
if(-not [OrielCapture]::ClientToScreen($orielWindow,[ref]$orielPoint)){throw 'ClientToScreen failed'}
$orielBitmap=New-Object System.Drawing.Bitmap($orielRect.right,$orielRect.bottom)
$orielGraphics=[System.Drawing.Graphics]::FromImage($orielBitmap)
try { $orielGraphics.CopyFromScreen($orielPoint.x,$orielPoint.y,0,0,$orielBitmap.Size); $orielBitmap.Save('${output.replaceAll("'", "''")}') }
finally { $orielGraphics.Dispose(); $orielBitmap.Dispose() }
`
  const scriptPath = path.join(data, 'capture.ps1'); await writeFile(scriptPath, '\ufeff' + script)
  await new Promise((resolve, reject) => {
    const ps = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-File', scriptPath], { stdio: 'pipe' })
    let error = ''; ps.stderr.on('data', b => { error += b }); ps.on('error', reject); ps.on('exit', code => code === 0 ? resolve() : reject(new Error(error)))
  })
}
try {
  shell = await connect(path.join(data, 'shell'), 'main')
  overlay = await connect(path.join(data, 'shell'), 'overlay')
  await until(() => host('overlay_status').then(s => s.ready), 'receiver ready')
  const snapshot = await host('workspace_save_entry', { id: null, title: '隔离网页', url: origin, proxy: { mode: 'direct' }, preparation: null, replacePreparation: true })
  const id = snapshot.entries.find(e => e.title === '隔离网页').id
  await shell.evaluate("window.__TAURI__.event.emitTo('main','workspace-saved')")
  await shell.evaluate('document.querySelector("[aria-label=浏览器]").click()')
  await until(() => shell.evaluate('Boolean(document.querySelector("[aria-label=隔离网页]"))'), 'workspace snapshot refreshed')
  await host('web_show', { entryId: id, bounds: { x: 310, y: 90, width: 660, height: 530 }, visible: true })
  page = await connect(path.join(data, 'web-profiles', id))
  const identity = await until(() => page.evaluate('window.fixtureIdentity'), 'fixture load')
  const loadCount = await page.evaluate('sessionStorage.loads')
  const settings = '.sidebar-bottom [aria-label="工作台设置"]'
  await clickMain(settings); await outsideClose() // Warm up.
  const timings = []
  for (let i = 0; i < 20; i++) {
    await shell.evaluate('window.overlayClickStart=performance.now()')
    await clickMain(settings)
    timings.push(await shell.evaluate('performance.now()-window.overlayClickStart'))
    const point = await center()
    await pointer(point, point); assert.equal(await opened(), true)
    await pointer(point, [2, 2]); assert.equal(await opened(), true)
    if (i === 0) await composedScreenshot(`light-${material}`)
    await outsideClose()
    await until(() => shell.evaluate(`document.activeElement?.matches(${JSON.stringify(settings)})`), 'trigger focus restored')
  }
  const sorted = [...timings].sort((a, b) => a - b), p95 = sorted[Math.ceil(sorted.length * .95) - 1]
  assert.equal(await page.evaluate('window.fixtureIdentity'), identity)
  assert.equal(await page.evaluate('sessionStorage.loads'), loadCount)
  assert.ok((nativeLogs.match(/controller_visible=true probe_ok=true/g) ?? []).length >= 20, 'Actual underlying WebView2 controllers must remain visible')
  assert.ok(p95 <= 500, `Warm click-to-interactive P95 ${p95.toFixed(1)}ms exceeds 500ms`)
  await clickMain('[aria-label="添加网页"]'); await outsideClose()
  await clickMain('[aria-label="网页设置"]'); await outsideClose()
  await shell.evaluate('document.querySelector("[aria-label=首页]").click()')
  await clickMain(settings); await escape(); await closed()
  for (const label of ['浏览布局验收', '应用布局验收']) {
    await shell.evaluate(`document.querySelector('[aria-label="${label}"]').click()`)
    await until(() => shell.evaluate('Boolean(document.querySelector("[aria-label=验收设置]:not(:disabled)"))'), 'plugin ready')
    await clickMain('[aria-label="验收设置"]'); await outsideClose()
    await clickMain('[aria-label="验收前置程序"]'); await escape(); await closed()
    await clickMain('.plugin-help-button'); await outsideClose()
  }
  await clickMain(settings)
  await overlay.evaluate(`document.querySelector('.workspace-dialog textarea').value='draft';document.querySelector('.workspace-dialog textarea').dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('.preparation-entry').click()`)
  await until(() => overlay.evaluate('Boolean(document.querySelector(".preparation-panel[open]"))'), 'nested panel')
  await pointer([2, 2], [2, 2])
  await until(() => overlay.evaluate('Boolean(document.querySelector(".workspace-dialog[open]"))'), 'return to settings')
  assert.equal(await overlay.evaluate('document.querySelector(".workspace-dialog textarea").value'), 'draft')
  assert.equal(await overlay.evaluate('document.activeElement.classList.contains("preparation-entry")'), true)
  await overlay.evaluate('window.__TAURI__.window.getCurrentWindow().close()'); await closed()
  await clickMain(settings)
  await overlay.evaluate(`const s=Array.from(document.querySelectorAll('select')).find(s=>Array.from(s.options).some(o=>o.value==='dark'));s.value='dark';s.dispatchEvent(new Event('change',{bubbles:true}))`)
  await until(() => overlay.evaluate('document.documentElement.dataset.theme === "dark"'), 'dark theme')
  await composedScreenshot(`dark-${material}`)
  await overlay.evaluate(`const roots=document.querySelector('.workspace-dialog textarea');roots.value='';roots.dispatchEvent(new Event('input',{bubbles:true}));window.overlayOriginalCore=window.__TAURI__.core;window.__TAURI__.core={...window.overlayOriginalCore,invoke:(command,args)=>command==='workspace_settings'?new Promise(r=>setTimeout(r,750)).then(()=>window.overlayOriginalCore.invoke(command,args)):window.overlayOriginalCore.invoke(command,args)};document.querySelector('.workspace-dialog form').requestSubmit()`)
  await until(() => overlay.evaluate('document.querySelector(".workspace-dialog [aria-label=关闭]").disabled'), 'save guard')
  await pointer([2, 2], [2, 2]); assert.equal(await opened(), true)
  await escape(); assert.equal(await opened(), true)
  await overlay.evaluate('window.__TAURI__.window.getCurrentWindow().close()'); assert.equal(await opened(), true)
  await closed()
  await overlay.evaluate('window.__TAURI__.core=window.overlayOriginalCore;delete window.overlayOriginalCore')
  await clickMain(settings); await outsideClose()
  assert.equal((await host('overlay_status')).disabled, null, 'Material degradation must not disable the overlay')
  await assert.rejects(host('overlay_pending'), /仅悬浮窗口/)
  const lastId = (await host('overlay_status')).request.requestId
  await assert.rejects(overlay.evaluate(`window.__TAURI__.core.invoke('overlay_presented',{requestId:${lastId}})`), /过期/)
  assert.ok(nativeLogs.includes(material === 'disabled' ? 'material=shade' : 'material=acrylic-requested'), `Native material policy did not match ${material}`)
  await writeFile(path.join(data, 'result.json'), JSON.stringify({ material, timings, p95, verification: 'automated interaction and OS-composited captures; manually inspect material and run move/resize/DPI cases' }, null, 2))
  console.log(`PASS isolated warm-open P95=${p95.toFixed(1)}ms, gestures, nested draft/focus, plugin layouts, native close and actual controller visibility`)
  console.log(`OS-composited screenshots and timings: ${data}; manually verify Acrylic appearance, window move/resize/maximize and physical monitor DPI changes.`)
} finally {
  await writeFile(path.join(data, 'native.log'), nativeLogs)
  if (app.exitCode === null) {
    try { await shell?.evaluate('setTimeout(()=>window.__TAURI__.window.getCurrentWindow().close(),100)'); await until(() => app.exitCode !== null, 'app exit') } catch { app.kill() }
  }
  shell?.close(); overlay?.close(); page?.close()
  fixture.closeAllConnections(); fixture.close(); shellServer.closeAllConnections(); shellServer.close()
}
