// Windows native integration check. Uses Node's built-in CDP WebSocket support;
// all browser/application data stays in a fresh directory under target/.
import assert from 'node:assert/strict'
import { connectHostTarget } from './lib/host-target.mjs'
import { execFile, spawn } from 'node:child_process'
import { promisify } from 'node:util'
import { createServer } from 'node:http'
import { createServer as createTcpServer } from 'node:net'
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import { dirname, extname, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

if (process.platform !== 'win32') throw new Error('Run with Windows Node after building the Windows debug application and frontend.')
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const data = join(root, 'target', 'browser-check', `${Date.now()}`)
await mkdir(data, { recursive: true })
await mkdir(join(data, 'screenshots'), { recursive: true })
// Exercise the legacy format without touching any real workspace configuration.
const legacyConfig = JSON.stringify({ version: 1, webEntries: [], pluginDirectories: [], defaultProxy: { mode: 'system' } })
await writeFile(join(data, 'workspace.json'), legacyConfig)
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
async function until(check, description, timeout = 25000) {
  const deadline = Date.now() + timeout
  let last
  while (Date.now() < deadline) {
    try { const result = await check(); if (result) return result } catch (error) { last = error }
    await delay(100)
  }
  throw new Error(`Timeout: ${description}${last ? ` (${last.message})` : ''}`)
}
async function listen(server, port = 0) {
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port, '127.0.0.1', resolve) })
  return server.address().port
}
async function activePort(directory) {
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, item.name)
    if (item.name === 'DevToolsActivePort') return Number((await readFile(path, 'utf8')).split('\n')[0])
    if (item.isDirectory()) { const found = await activePort(path); if (found) return found }
  }
}
class Cdp {
  next = 0
  pending = new Map()
  async connect(url) {
    this.socket = new WebSocket(url)
    this.socket.addEventListener('message', event => {
      const message = JSON.parse(String(event.data))
      const deferred = this.pending.get(message.id)
      if (deferred) {
        clearTimeout(deferred.timer); this.pending.delete(message.id)
        if (message.error) deferred.reject(new Error(message.error.message))
        else deferred.resolve(message.result)
      }
    })
    await new Promise((resolve, reject) => { this.socket.addEventListener('open', resolve, { once: true }); this.socket.addEventListener('error', reject, { once: true }) })
    return this
  }
  command(method, params = {}) {
    const id = ++this.next
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`CDP timeout: ${method}`)) }, 30000)
      this.pending.set(id, { resolve, reject, timer }); this.socket.send(JSON.stringify({ id, method, params }))
    })
  }
  async evaluate(expression) {
    const result = await this.command('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, userGesture: true })
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text)
    return result.result.value
  }
  close() { this.socket.close() }
}
async function connectProfile(directory, predicate, label) {
  return until(async () => {
    const port = await activePort(directory)
    if (!port) return
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
    if (label) return connectHostTarget(targets, label, Cdp)
    const target = targets.find(target => target.type === 'page' && predicate(target))
    if (target) return new Cdp().connect(target.webSocketDebuggerUrl)
  }, `debug target ${directory}`)
}

let visits = 0
const fixtureRequests = []
const fixture = createServer((request, response) => {
  visits++
  fixtureRequests.push(request.url)
  if (request.url === '/signed-in') { response.writeHead(200, { 'content-type': 'text/html' }); response.end('<title>Signed in</title>Signed in'); return }
  if (request.url === '/authenticate') { request.resume(); response.writeHead(303, { location: '/signed-in' }); response.end(); return }
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' })
  response.end(`<!doctype html><title>Fixture ${request.url}</title><h1>Oriel browser fixture</h1><a id="next" href="/next">Next</a><button id="popup" onclick="window.open('/popup','login')">Popup</button><form action="/authenticate" method="post"><label>User<input name="username" autocomplete="username"></label><label>Password<input name="password" type="password" autocomplete="current-password"></label><button>Sign in</button></form>`)
})
const fixturePort = await listen(fixture)
const fixtureUrl = `http://127.0.0.1:${fixturePort}`
const proxyRequests = []
const proxy = createServer((request, response) => {
  proxyRequests.push(request.url)
  response.writeHead(200, { 'content-type': 'text/html' }); response.end('<title>HTTP proxy reached</title>HTTP proxy reached')
})
const proxyPort = await listen(proxy)
let socksRequests = 0
const socks = createTcpServer(socket => {
  let phase = 0
  let buffer = Buffer.alloc(0)
  socket.on('error', () => {})
  socket.on('data', chunk => {
    buffer = Buffer.concat([buffer, chunk])
    if (phase === 0 && buffer.length >= 2 + buffer[1]) { buffer = buffer.subarray(2 + buffer[1]); socket.write(Buffer.from([5, 0])); phase = 1 }
    if (phase === 1 && buffer.length >= 5) {
      const length = buffer[3] === 3 ? 7 + buffer[4] : buffer[3] === 1 ? 10 : 22
      if (buffer.length < length) return
      buffer = buffer.subarray(length); socket.write(Buffer.from([5, 0, 0, 1, 127, 0, 0, 1, 0, 0])); phase = 2
    }
    if (phase === 2 && buffer.includes(Buffer.from('\r\n\r\n'))) {
      socksRequests++; phase = 3
      const body = '<title>SOCKS proxy reached</title>SOCKS proxy reached'
      socket.end(`HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: ${Buffer.byteLength(body)}\r\nConnection: close\r\n\r\n${body}`)
    }
  })
})
const socksPort = await listen(socks)
const shellServer = createServer(async (request, response) => {
  const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname)
  const path = resolve(root, 'dist', pathname === '/' ? 'index.html' : `.${pathname}`)
  if (!path.startsWith(join(root, 'dist') + sep)) { response.writeHead(403); response.end(); return }
  try {
    const contents = await readFile(path)
    response.setHeader('content-type', ({ '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml' })[extname(path)] ?? 'application/octet-stream')
    response.end(contents)
  } catch { response.writeHead(404); response.end() }
})
await listen(shellServer, 1420)
const appOptions = {
  env: { ...process.env, ORIEL_DEV_DATA_DIR: data, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=0 --host-resolver-rules="MAP oriel-no-fallback.test 127.0.0.1"' },
  stdio: ['ignore', 'pipe', 'pipe'],
}
const executable = resolve(root, process.argv[2] ?? 'target/windows/debug/oriel.exe')
let app = spawn(executable, [], appOptions)
let nativeErrors = ''
app.stderr.on('data', data => { nativeErrors += data })
const connections = []
const results = []
function passed(name) { results.push(name); process.stdout.write(`PASS ${name}\n`) }

async function closeForWindowState(connection) {
  const port = await activePort(join(data, 'shell'))
  // Let CDP return before destroying its own WebView; force-kill does not exercise close handlers.
  await connection.evaluate('setTimeout(() => { void window.__TAURI__.window.getCurrentWindow().close() }, 100)')
  await until(() => app.exitCode !== null || app.signalCode !== null, 'normal application close')
  connection.close()
  // WebView2 can outlive the host briefly; do not reconnect to a retiring profile.
  if (port) await until(async () => {
    try { await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1000) }); return false }
    catch { return true }
  }, 'shell browser shutdown')
  return JSON.parse(await readFile(join(data, 'window-state.json'), 'utf8'))
}

async function restartForWindowState() {
  app = spawn(executable, [], appOptions)
  app.stderr.on('data', data => { nativeErrors += data })
  const connection = await connectProfile(join(data, 'shell'), undefined, 'main')
  connections.push(connection)
  await until(() => connection.evaluate(`Boolean(document.querySelector('button[aria-label="最大化"],button[aria-label="还原窗口"]'))`), 'restored window controls')
  return connection
}

let exitCode = 0
let passwordSaveAndFill = 'requires manual acceptance'
try {
  const shell = await connectProfile(join(data, 'shell'), undefined, 'main')
  connections.push(shell)
  const overlay = await connectProfile(join(data, 'shell'), undefined, 'overlay')
  connections.push(overlay)
  const invoke = (command, args = {}) => shell.evaluate(`window.__TAURI__.core.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`)
  const click = async label => {
    await until(() => shell.evaluate(`(() => { const button = document.querySelector('button[aria-label=${JSON.stringify(label)}]'); return button && !button.disabled })()`), `enabled button ${label}`)
    return shell.evaluate(`document.querySelector('button[aria-label=${JSON.stringify(label)}]').click()`)
  }
  await until(() => shell.evaluate(`Boolean(document.querySelector('button[aria-label="浏览器"]'))`), 'browser navigation')
  let workspace = await invoke('workspace_snapshot')
  assert.equal(workspace.browserSupported, true)
  assert.equal(workspace.configPath, join(data, 'workspace.json'))
  passed('isolated native startup and builtin registration')
  const initialWindow = await shell.evaluate('({ width: innerWidth, height: innerHeight, availableHeight: screen.availHeight })')
  assert(initialWindow.width <= 1100 && initialWindow.height <= 760)
  assert(initialWindow.height <= initialWindow.availableHeight)
  passed('smaller first window fits the screen work area')

  await click('浏览器')
  await until(() => shell.evaluate(`Boolean(document.querySelector('.fixed-browser-empty'))`), 'empty fixed-page workspace')
  assert.equal(await readFile(join(data, 'workspace.json'), 'utf8'), legacyConfig)
  assert.equal(await shell.evaluate(`document.querySelectorAll('.web-surface').length`), 0)
  passed('browser starts with a fixed-page empty state and preserves the legacy file until saving')
  await click('添加网页')
  await until(() => overlay.evaluate(`Boolean(document.querySelector('dialog[open] input'))`), 'add webpage dialog')
  await overlay.evaluate(`(() => { const inputs = document.querySelectorAll('dialog[open] input'); inputs[0].value='Native fixture'; inputs[0].dispatchEvent(new Event('input',{bubbles:true})); inputs[1].value=${JSON.stringify(`${fixtureUrl}/start`)}; inputs[1].dispatchEvent(new Event('input',{bubbles:true})); document.querySelector('dialog[open] form').requestSubmit(); })()`)
  await until(() => shell.evaluate(`Boolean(document.querySelector('button[aria-label="Native fixture"]'))`), 'saved webpage navigation')
  workspace = await invoke('workspace_snapshot')
  const entry = workspace.entries.find(entry => entry.title === 'Native fixture')
  assert(entry?.editable)
  const page = await connectProfile(join(data, 'web-profiles', entry.id), target => target.url.startsWith(fixtureUrl))
  connections.push(page)
  await until(() => page.evaluate('document.title === "Fixture /start"'), 'initial webpage load')
  assert.equal(JSON.parse(await readFile(join(data, 'workspace.json'), 'utf8')).version, 4)
  assert.equal(await shell.evaluate(`document.querySelectorAll('.primary-nav .site-monogram').length`), 0)
  assert.equal(await shell.evaluate(`document.querySelectorAll('.fixed-page').length`), 1)
  await until(() => overlay.evaluate('!document.querySelector("dialog[open]")'), 'add overlay closed')
  passed('webpage creation through UI and native rendering')

  const instanceToken = await page.evaluate('window.layoutInstanceToken = Math.random().toString(36)')
  await click('收窄网页侧栏')
  await until(async () => (await invoke('workspace_snapshot')).browserSidebar.collapsed, 'collapsed preference persisted')
  assert.equal(await shell.evaluate(`document.querySelector('.browser-sidebar').getBoundingClientRect().width`), 52)
  await click('展开网页侧栏')
  await until(async () => !(await invoke('workspace_snapshot')).browserSidebar.collapsed, 'expanded preference persisted')
  await shell.evaluate(`document.querySelector('[role="separator"]').focus()`)
  await shell.command('Input.dispatchKeyEvent', { type: 'keyDown', key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39 })
  await shell.command('Input.dispatchKeyEvent', { type: 'keyUp', key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39 })
  await until(async () => (await invoke('workspace_snapshot')).browserSidebar.width === 250, 'keyboard sidebar resize persisted')
  assert.equal(await page.evaluate('window.layoutInstanceToken'), instanceToken)
  await click('首页')
  assert.equal(await shell.evaluate(`document.querySelector('.browser-workspace')`), null)
  await click('浏览器')
  await until(() => shell.evaluate(`document.querySelector('.browser-sidebar')?.getBoundingClientRect().width === 250`), 'sidebar width restored after plugin switch')
  assert.equal(await page.evaluate('window.layoutInstanceToken'), instanceToken)
  passed('collapse, keyboard resize and plugin switching preserve the native webpage instance')

  const resizeGrip = await shell.evaluate(`(() => { const bounds = document.querySelector('[role="separator"]').getBoundingClientRect(); return { x: bounds.x + bounds.width / 2, y: bounds.y + 80 } })()`)
  await shell.evaluate(`window.resizeEvents=[];for(const type of ['pointerdown','pointermove','pointerup'])document.addEventListener(type,event=>window.resizeEvents.push({type,x:event.clientX,y:event.clientY,buttons:event.buttons}),true)`)
  await shell.command('Input.dispatchMouseEvent', { type: 'mouseMoved', ...resizeGrip })
  await shell.command('Input.dispatchMouseEvent', { type: 'mousePressed', button: 'left', buttons: 1, clickCount: 1, ...resizeGrip })
  await until(() => page.evaluate('document.visibilityState === "hidden"'), 'native webpage hidden before resizing across its bounds')
  await shell.command('Input.dispatchMouseEvent', { type: 'mouseMoved', buttons: 1, x: resizeGrip.x + 50, y: resizeGrip.y })
  await shell.command('Input.dispatchMouseEvent', { type: 'mouseReleased', button: 'left', buttons: 0, clickCount: 1, x: resizeGrip.x + 50, y: resizeGrip.y })
  try { await until(async () => (await invoke('workspace_snapshot')).browserSidebar.width === 300, 'pointer sidebar resize persisted') }
  catch (error) { throw new Error(error.message + ' ' + JSON.stringify({grip:resizeGrip,events:await shell.evaluate('window.resizeEvents')})) }
  assert.equal(await page.evaluate('window.layoutInstanceToken'), instanceToken)
  passed('pointer resizing across the native webpage preserves the page instance')

  await page.evaluate(`document.querySelector('#next').click()`)
  await until(() => shell.evaluate(`document.querySelector('input[aria-label="网页地址"]').value.endsWith('/next') && !document.querySelector('button[aria-label="上一页"]').disabled`), 'history state')
  await click('上一页')
  await until(() => page.evaluate('location.pathname === "/start"'), 'back navigation')
  await click('下一页')
  await until(() => page.evaluate('location.pathname === "/next"'), 'forward navigation')
  await shell.evaluate(`(() => { const input=document.querySelector('input[aria-label="网页地址"]'); input.value='unsent.example'; input.dispatchEvent(new Event('input',{bubbles:true})) })()`)
  await click('上一页')
  await until(() => shell.evaluate(`document.querySelector('input[aria-label="网页地址"]').value.endsWith('/start')`), 'navigation replaces an unsubmitted address')
  await click('下一页')
  await until(() => page.evaluate('location.pathname === "/next"'), 'forward after pending address edit')
  await page.evaluate(`history.pushState({}, '', '/spa'); document.title='SPA title'`)
  await until(() => shell.evaluate(`document.querySelector('input[aria-label="网页地址"]').value.endsWith('/spa')`), 'SPA URL update')
  const beforeRefresh = visits
  await click('刷新')
  await until(() => visits > beforeRefresh, 'refresh network request')
  passed('native back, forward, reload and SPA address synchronization')

  await click('网页设置')
  await until(() => overlay.evaluate(`Boolean(document.querySelector('dialog[open] input'))`), 'edit webpage dialog')
  await overlay.evaluate(`(() => { const input = document.querySelector('dialog[open] input'); input.value='Native fixture renamed'; input.dispatchEvent(new Event('input',{bubbles:true})); document.querySelector('dialog[open] form').requestSubmit() })()`)
  entry.title = 'Native fixture renamed'
  await until(() => shell.evaluate(`Boolean(document.querySelector('button[aria-label="Native fixture renamed"]'))`), 'renamed entry')
  assert.equal((await invoke('workspace_snapshot')).entries.find(item => item.id === entry.id).title, entry.title)
  assert.equal(await page.evaluate('location.pathname'), '/spa')
  await until(() => overlay.evaluate('!document.querySelector("dialog[open]")'), 'edit overlay closed')
  passed('webpage editing preserves identity and the current page')

  await invoke('web_navigate', { entryId: entry.id, url: `${fixtureUrl}/login` })
  await until(() => page.evaluate(`Boolean(document.querySelector('input[name="password"]'))`), 'test login form')
  await page.evaluate(`document.querySelector('input[name="username"]').focus()`)
  await page.command('Input.insertText', { text: 'oriel-smoke@example.test' })
  await page.evaluate(`document.querySelector('input[name="password"]').focus()`)
  await page.command('Input.insertText', { text: 'Smoke-Only-123' })
  await page.evaluate(`document.querySelector('form').requestSubmit()`)
  await until(() => page.evaluate('location.pathname === "/signed-in"'), 'test login redirect')
  try {
    await delay(500)
    // Only inspect/invoke controls inside the isolated app's own window.
    const script = `Add-Type -AssemblyName UIAutomationClient; Add-Type -AssemblyName UIAutomationTypes; $p = Get-Process -Id ${app.pid}; $root = [System.Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle); $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button); $buttons = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition); foreach ($button in $buttons) { if ($button.Current.Name -match '^(保存|Save|保存密码|Save password)$') { $pattern = $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern); $pattern.Invoke(); Write-Output saved; break } }`
    const { stdout } = await promisify(execFile)('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { timeout: 8000 })
    if (stdout.trim() === 'saved') {
      await invoke('web_navigate', { entryId: entry.id, url: `${fixtureUrl}/login` })
      await until(() => page.evaluate(`document.querySelector('input[name="password"]')?.value === 'Smoke-Only-123'`), 'native saved-password filling', 8000)
      passwordSaveAndFill = 'passed with isolated test account'
      passed('native password saving and login filling with a test account')
    } else process.stdout.write('NOTE Native save prompt requires manual acceptance.\n')
  } catch { process.stdout.write('NOTE Native save/fill requires manual acceptance.\n') }

  await invoke('web_navigate', { entryId: entry.id, url: `${fixtureUrl}/start` })
  await until(() => page.evaluate(`Boolean(document.querySelector('#popup'))`), 'fixture restored after login check')
  await page.evaluate(`document.cookie='entry_cookie=first; path=/'`)
  await click('账号密码')
  await until(() => shell.evaluate(`Boolean(document.querySelector('dialog[open]'))`), 'password dialog')
  await until(() => page.evaluate('document.visibilityState === "hidden"'), 'native view hidden behind local dialog')
  await shell.evaluate(`document.querySelector('.password-dialog .danger').click()`)
  await until(() => shell.evaluate(`Boolean(document.querySelector('.password-dialog .form-success'))`), 'native password clear completion')
  await shell.evaluate(`document.querySelector('.password-dialog .primary-button').click()`)
  await until(() => page.evaluate(`document.cookie.includes('entry_cookie=first')`), 'cookies preserved after password clear')
  passed('native password clearing preserves existing site cookies')

  const denied = await page.evaluate(`Promise.race([window.__TAURI__.core.invoke('workspace_delete_entry', {id:${JSON.stringify(entry.id)}}).then(()=> 'allowed',()=> 'denied'),new Promise(resolve=>setTimeout(()=>resolve('blocked'),800))])`)
  assert.notEqual(denied, 'allowed')
  assert((await invoke('workspace_snapshot')).entries.some(item => item.id === entry.id))
  passed('remote webpage cannot mutate host configuration')

  await page.evaluate(`document.querySelector('#popup').click()`)
  const popup = await connectProfile(join(data, 'web-profiles', entry.id), target => target.url.endsWith('/popup'))
  connections.push(popup)
  assert((await popup.evaluate('document.cookie')).includes('entry_cookie=first'))
  await popup.evaluate('window.close()')
  passed('website popup shares its originating profile')

  const second = await invoke('workspace_save_entry', { id: null, title: 'Separate profile', url: `${fixtureUrl}/second`, proxy: { mode: 'direct' } })
  await click('重新加载接入')
  await until(() => shell.evaluate(`Boolean(document.querySelector('button[aria-label="Separate profile"]'))`), 'second sidebar entry')
  await click('Separate profile')
  const secondEntry = second.entries.find(item => item.title === 'Separate profile')
  const secondPage = await connectProfile(join(data, 'web-profiles', secondEntry.id), target => target.url.startsWith(fixtureUrl))
  connections.push(secondPage)
  await until(() => secondPage.evaluate('location.pathname === "/second" && document.readyState === "complete"'), 'second webpage loaded before reading its cookies')
  assert.equal(await secondPage.evaluate('document.cookie'), '')
  passed('independent entry browser profiles')

  await click(entry.title)
  await until(() => shell.evaluate(`document.querySelector('input[aria-label="网页地址"]').value.includes('/start')`), 'first page restored before background menu')
  await click('Separate profile的菜单')
  await until(() => shell.evaluate(`Boolean(document.querySelector('.page-actions[open]'))`), 'background page action menu')
  await until(() => page.evaluate('document.visibilityState === "hidden"'), 'native page hidden for host menu')
  assert.equal(await shell.evaluate(`document.querySelector('.fixed-page.is-active .fixed-page-button').getAttribute('aria-label')`), entry.title)
  await shell.evaluate(`Array.from(document.querySelectorAll('.page-actions button')).find(button => button.textContent === '上移').click()`)
  await until(async () => (await invoke('workspace_snapshot')).entries.filter(entry => entry.editable)[0].id === secondEntry.id, 'fixed page order persisted')
  assert.equal(await shell.evaluate(`document.querySelector('.fixed-page.is-active .fixed-page-button').getAttribute('aria-label')`), entry.title)
  assert((await page.evaluate('document.cookie')).includes('entry_cookie=first'))
  passed('background page menu targets its own entry and reorders without switching or losing login data')

  const dragPoints = await shell.evaluate(`Array.from(document.querySelectorAll('.fixed-page-button')).map(button => { const bounds = button.getBoundingClientRect(); return { x: bounds.x + 24, y: bounds.y + bounds.height / 2 } })`)
  const dragSource = dragPoints[1]
  const dragDestination = { x: dragPoints[0].x, y: dragPoints[0].y - 10 }
  await shell.command('Input.dispatchMouseEvent', { type: 'mouseMoved', ...dragSource })
  await shell.command('Input.dispatchMouseEvent', { type: 'mousePressed', button: 'left', buttons: 1, clickCount: 1, ...dragSource })
  for (let step = 1; step <= 12; step++) {
    await shell.command('Input.dispatchMouseEvent', { type: 'mouseMoved', buttons: 1, x: dragSource.x, y: dragSource.y + (dragDestination.y - dragSource.y) * step / 12 })
  }
  await shell.command('Input.dispatchMouseEvent', { type: 'mouseReleased', button: 'left', buttons: 0, clickCount: 1, ...dragDestination })
  await until(async () => (await invoke('workspace_snapshot')).entries.filter(entry => entry.editable)[0].id === entry.id, 'pointer drag order persisted')
  assert.equal(await shell.evaluate(`document.querySelector('.fixed-page.is-active .fixed-page-button').getAttribute('aria-label')`), entry.title)
  assert((await page.evaluate('document.cookie')).includes('entry_cookie=first'))
  passed('dragging fixed pages persists their order without changing the selected page')

  await shell.evaluate(`document.querySelector('button[aria-label="Separate profile"]').focus()`)
  await shell.command('Input.dispatchKeyEvent', { type: 'keyDown', key: 'ContextMenu', code: 'ContextMenu', windowsVirtualKeyCode: 93 })
  await shell.command('Input.dispatchKeyEvent', { type: 'keyUp', key: 'ContextMenu', code: 'ContextMenu', windowsVirtualKeyCode: 93 })
  await until(() => shell.evaluate(`Boolean(document.querySelector('.page-actions[open]'))`), 'keyboard page action menu')
  await shell.command('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 })
  await shell.command('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 })
  await until(() => shell.evaluate(`!document.querySelector('.page-actions[open]') && document.activeElement?.getAttribute('aria-label') === 'Separate profile'`), 'keyboard menu dismissal restores focus')
  passed('keyboard page menus restore focus without switching the active webpage')

  const extraIds = []
  for (let index = 0; index < 36; index++) {
    const title = `固定网页 ${index + 1} · 用于检查较长名称与列表滚动`
    const added = await invoke('workspace_save_entry', { id: null, title, url: `${fixtureUrl}/unused-${index}`, proxy: { mode: 'direct' } })
    extraIds.push(added.entries.find(item => item.title === title).id)
  }
  await click('重新加载接入')
  await until(() => shell.evaluate(`document.querySelectorAll('.fixed-page').length === 38`), 'long fixed-page list')
  assert(await shell.evaluate(`(() => { const list = document.querySelector('.fixed-pages'); const add = document.querySelector('.add-fixed-page').getBoundingClientRect(); return list.scrollHeight > list.clientHeight && add.bottom <= innerHeight && add.top >= 0 })()`))
  const profiles = await readdir(join(data, 'web-profiles'))
  assert(extraIds.every(id => !profiles.includes(id)))
  const expandedShot = await shell.command('Page.captureScreenshot', { format: 'png' })
  await writeFile(join(data, 'screenshots', 'fixed-pages-expanded.png'), Buffer.from(expandedShot.data, 'base64'))
  passed('long fixed-page lists scroll independently and do not eagerly create native profiles')

  for (const width of [1100, 900, 800, 680, 480, 390]) {
    await shell.command('Emulation.setDeviceMetricsOverride', { width, height: 760, deviceScaleFactor: 1, mobile: false })
    await until(() => shell.evaluate(`(() => { const root = document.querySelector('.browser-workspace'); return document.documentElement.scrollWidth <= innerWidth && root.classList.contains('is-compact') === (root.getBoundingClientRect().width < 720) })()`), `fixed-page layout at ${width}px`)
    assert(await shell.evaluate(`(() => { const list = document.querySelector('.fixed-pages'); return list.scrollWidth <= list.clientWidth })()`), `fixed-page list overflow at ${width}px`)
    if (width === 480) {
      const narrowShot = await shell.command('Page.captureScreenshot', { format: 'png' })
      await writeFile(join(data, 'screenshots', 'fixed-pages-compact-480.png'), Buffer.from(narrowShot.data, 'base64'))
    }
  }
  await shell.command('Emulation.clearDeviceMetricsOverride')
  await until(() => shell.evaluate(`!document.querySelector('.browser-workspace').classList.contains('is-compact')`), 'expanded layout restored after narrow windows')
  assert.deepEqual((await invoke('workspace_snapshot')).browserSidebar, { collapsed: false, width: 300 })
  for (const id of extraIds) await invoke('workspace_delete_entry', { id })
  await click('重新加载接入')
  await until(() => shell.evaluate(`document.querySelectorAll('.fixed-page').length === 2`), 'temporary layout fixtures removed')
  passed('six viewport sizes preserve sidebar preferences without horizontal overflow')

  const external = join(data, 'external-plugin')
  await mkdir(external)
  await writeFile(join(external, 'plugin.json'), JSON.stringify({ schemaVersion: 1, id: 'check.external', name: 'External', version: '1', capabilities: ['web-page'], entries: [{ id: 'page', title: 'External webpage', kind: 'web-page', url: `${fixtureUrl}/external` }] }))
  let updated = await invoke('workspace_settings', { defaultProxy: { mode: 'custom', address: `http://127.0.0.1:${proxyPort}` }, pluginDirectories: [external] })
  assert(updated.entries.some(item => item.id === 'check.external.page'))
  await click('重新加载接入')
  await until(() => shell.evaluate(`Boolean(document.querySelector('button[aria-label="External webpage"]'))`), 'external manifest navigation')
  await click('External webpage')
  assert.equal(await shell.evaluate(`document.querySelector('.browser-sidebar')`), null)
  await invoke('web_navigate', { entryId: 'check.external.page', url: 'http://oriel-proxy.test/probe' })
  await until(() => proxyRequests.some(url => url.includes('oriel-proxy.test/probe')), 'default HTTP proxy request')
  passed('external webpage registration without recompilation and default HTTP proxy')

  const socksEntryId = (await invoke('workspace_save_entry', { id: null, title: 'SOCKS fixture', url: 'http://oriel-socks.test/probe', proxy: { mode: 'custom', address: `socks5://127.0.0.1:${socksPort}` } })).entries.find(item => item.title === 'SOCKS fixture').id
  await invoke('web_show', { entryId: socksEntryId, bounds: { x: 100, y: 150, width: 600, height: 400 }, visible: true })
  await until(() => socksRequests > 0, 'SOCKS5 network request')
  passed('per-entry SOCKS5 proxy')

  const beforeDirect = visits
  await click('浏览器')
  await click('Separate profile')
  await until(() => shell.evaluate(`document.querySelector('input[aria-label="网页地址"]').value.endsWith('/second')`), 'explicit direct entry')
  await invoke('web_navigate', { entryId: secondEntry.id, url: `${fixtureUrl}/direct-check` })
  await until(() => visits > beforeDirect, 'explicit direct request despite default proxy')
  assert(!proxyRequests.some(url => url.includes('/direct-check')))
  passed('explicit direct override bypasses the configured default proxy')

  // Closing the debug connection ensures it cannot keep the retiring browser process alive.
  page.close()
  updated = await invoke('workspace_save_entry', { id: entry.id, title: entry.title, url: entry.url, proxy: { mode: 'custom', address: `http://127.0.0.1:${proxyPort}` } })
  assert.equal(updated.diagnostics.length, 0)
  await click(entry.title)
  await invoke('web_navigate', { entryId: entry.id, url: 'http://proxy-change.test/check' })
  await until(() => proxyRequests.some(url => url.includes('proxy-change.test/check')), 'proxy reconfiguration after process teardown')
  passed('proxy changes recreate the native view and take effect')

  const probe = (await invoke('workspace_save_entry', { id: null, title: 'Proxy failure probe', url: `http://oriel-no-fallback.test:${fixturePort}/direct-precondition`, proxy: { mode: 'direct' } })).entries.find(item => item.title === 'Proxy failure probe')
  const probeBounds = { x: 100, y: 150, width: 600, height: 400 }
  await invoke('web_show', { entryId: probe.id, bounds: probeBounds, visible: true })
  await until(() => fixtureRequests.includes('/direct-precondition'), 'direct-mode precondition for proxy failure test')
  const unused = createServer()
  const unusedPort = await listen(unused)
  await new Promise(resolve => unused.close(resolve))
  await invoke('workspace_save_entry', { id: probe.id, title: probe.title, url: `http://oriel-no-fallback.test:${fixturePort}/no-fallback-check`, proxy: { mode: 'custom', address: `http://127.0.0.1:${unusedPort}` } })
  await until(async () => (await invoke('web_show', { entryId: probe.id, bounds: probeBounds, visible: true }))?.error, 'failed-proxy navigation error')
  assert(!fixtureRequests.includes('/no-fallback-check'))
  await invoke('workspace_delete_entry', { id: probe.id })
  await click('首页')
  await click('浏览器')
  await click(entry.title)
  passed('unavailable proxy reports an error without falling back to a direct connection')

  const persisted = JSON.parse(await readFile(join(data, 'workspace.json'), 'utf8'))
  assert.equal(persisted.webEntries.length, 3)
  assert.equal(persisted.pluginDirectories[0], external)
  secondPage.close()
  await click(entry.title)
  await click('Separate profile的菜单')
  await until(() => shell.evaluate(`Boolean(document.querySelector('.page-actions[open]'))`), 'background removal menu')
  await shell.evaluate(`Array.from(document.querySelectorAll('.page-actions button')).find(button => button.textContent === '移除网页').click()`)
  await until(() => overlay.evaluate(`Boolean(document.querySelector('.delete-confirmation .danger'))`), 'delete confirmation')
  assert.equal(await overlay.evaluate(`document.querySelector('dialog[open] input').value`), 'Separate profile')
  await overlay.evaluate(`document.querySelector('.delete-confirmation .danger').click()`)
  await until(() => shell.evaluate(`!document.querySelector('button[aria-label="Separate profile"]')`), 'deleted entry')
  assert(!(await invoke('workspace_snapshot')).entries.some(item => item.id === secondEntry.id))
  assert.equal(await shell.evaluate(`document.querySelector('.fixed-page.is-active .fixed-page-button').getAttribute('aria-label')`), entry.title)
  await readFile(join(data, 'web-profiles', secondEntry.id, 'EBWebView', 'Local State'))
  passed('webpage removal through UI preserves browser data')

  // Resize this isolated window through Win32, without granting extra frontend permissions.
  const resizeScript = `Add-Type @'
using System;
using System.Runtime.InteropServices;
public class OrielWindowCheck {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
}
'@
[void][OrielWindowCheck]::SetThreadDpiAwarenessContext([IntPtr](-4))
$handle = (Get-Process -Id ${app.pid}).MainWindowHandle
$outer = New-Object OrielWindowCheck+Rect
$inner = New-Object OrielWindowCheck+Rect
[void][OrielWindowCheck]::GetWindowRect($handle, [ref]$outer)
[void][OrielWindowCheck]::GetClientRect($handle, [ref]$inner)
$scale = [OrielWindowCheck]::GetDpiForWindow($handle) / 96.0
$width = [int](900 * $scale) + $outer.right - $outer.left - $inner.right
$height = [int](660 * $scale) + $outer.bottom - $outer.top - $inner.bottom
if (![OrielWindowCheck]::SetWindowPos($handle, [IntPtr]::Zero, 120, 100, $width, $height, 20)) { throw 'Could not resize test window' }
`
  await promisify(execFile)('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', resizeScript], { timeout: 10000 })
  await until(() => shell.evaluate('Math.abs(innerWidth - 900) <= 1 && Math.abs(innerHeight - 660) <= 1'), 'resized normal bounds')
  const savedWindow = await closeForWindowState(shell)
  assert(Math.abs(savedWindow.width - 900) <= 1 && Math.abs(savedWindow.height - 660) <= 1)
  assert.equal(savedWindow.x, 120)
  assert.equal(savedWindow.y, 100)
  assert.equal(savedWindow.maximized, false)
  for (const connection of connections) { try { connection.close() } catch {} }
  let restarted = await restartForWindowState()
  await until(() => restarted.evaluate(`Boolean(window.__TAURI__?.core)`), 'restarted host API')
  await until(() => restarted.evaluate('Math.abs(innerWidth - 900) <= 1 && Math.abs(innerHeight - 660) <= 1'), 'restored normal size')
  passed('window size and position survive normal close and restart')
  const restored = await restarted.evaluate(`window.__TAURI__.core.invoke('workspace_snapshot')`)
  assert(restored.entries.some(item => item.id === entry.id && item.title === entry.title))
  assert(!restored.entries.some(item => item.id === secondEntry.id))
  assert.equal(restored.defaultProxy.mode, 'custom')
  assert.equal(restored.browserSidebar.width, 300)
  assert.equal(restored.browserSidebar.collapsed, false)
  passed('application restart restores local entries and proxy configuration')

  await restarted.evaluate('window.__TAURI__.window.getCurrentWindow().toggleMaximize()')
  await until(() => restarted.evaluate('window.__TAURI__.window.getCurrentWindow().isMaximized()'), 'maximized window')
  const maximizedWindow = await closeForWindowState(restarted)
  assert.equal(maximizedWindow.maximized, true)
  assert(Math.abs(maximizedWindow.width - savedWindow.width) <= 1)
  assert(Math.abs(maximizedWindow.height - savedWindow.height) <= 1)
  restarted = await restartForWindowState()
  assert(await restarted.evaluate('window.__TAURI__.window.getCurrentWindow().isMaximized()'))
  await restarted.evaluate('window.__TAURI__.window.getCurrentWindow().toggleMaximize()')
  await until(() => restarted.evaluate('Math.abs(innerWidth - 900) <= 1 && Math.abs(innerHeight - 660) <= 1'), 'normal bounds after restoring maximized startup')
  passed('maximized startup retains the previous normal window size')

  await restarted.evaluate('window.__TAURI__.window.getCurrentWindow().minimize()')
  const minimizedWindow = await closeForWindowState(restarted)
  assert.equal(minimizedWindow.maximized, false)
  assert(Math.abs(minimizedWindow.width - 900) <= 1 && Math.abs(minimizedWindow.height - 660) <= 1)
  assert.equal(minimizedWindow.x, savedWindow.x)
  assert.equal(minimizedWindow.y, savedWindow.y)
  restarted = await restartForWindowState()
  await until(() => restarted.evaluate('Math.abs(innerWidth - 900) <= 1 && Math.abs(innerHeight - 660) <= 1'), 'normal startup after minimized close')
  passed('minimized close preserves normal bounds and reopens visibly')

  await mkdir(join(data, 'screenshots'), { recursive: true })
  await until(() => restarted.evaluate(`Boolean(document.querySelector('button[aria-label="浏览器"]'))`), 'restarted navigation')
  await restarted.evaluate(`document.querySelector('button[aria-label="浏览器"]').click()`)
  await until(() => restarted.evaluate(`Boolean(document.querySelector('.browser-view'))`), 'browser shell')
  await restarted.command('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] })
  const screenshot = await restarted.command('Page.captureScreenshot', { format: 'png' })
  await writeFile(join(data, 'screenshots', 'browser-shell.png'), Buffer.from(screenshot.data, 'base64'))
  await restarted.evaluate(`document.querySelector('button[aria-label="切换为深色外观"]').click()`)
  await until(() => restarted.evaluate(`document.documentElement.dataset.theme === 'dark'`), 'dark theme')
  await restarted.command('Emulation.setDeviceMetricsOverride', { width: 390, height: 780, deviceScaleFactor: 1, mobile: false })
  assert(await restarted.evaluate('document.documentElement.scrollWidth <= innerWidth'))
  const dark = await restarted.command('Page.captureScreenshot', { format: 'png' })
  await writeFile(join(data, 'screenshots', 'browser-dark-narrow.png'), Buffer.from(dark.data, 'base64'))
  passed('desktop and narrow dark shell layout without horizontal overflow')

  await restarted.command('Emulation.clearDeviceMetricsOverride')
  await closeForWindowState(restarted)
  await writeFile(join(data, 'window-state.json'), JSON.stringify({
    ...savedWindow, x: 100000, y: -100000, width: 10000, height: 10000,
  }))
  restarted = await restartForWindowState()
  const available = await restarted.evaluate('({ width: screen.availWidth, height: screen.availHeight, scale: devicePixelRatio })')
  const fitted = await closeForWindowState(restarted)
  assert(fitted.width <= available.width && fitted.height <= available.height)
  assert(fitted.x >= 0 && fitted.y >= 0)
  assert(fitted.x / available.scale + fitted.width <= available.width + 1)
  assert(fitted.y / available.scale + fitted.height <= available.height + 1)
  passed('offscreen and oversized saved windows return to the work area')

  await writeFile(join(data, 'window-state.json'), '{broken')
  restarted = await restartForWindowState()
  assert(await restarted.evaluate('innerWidth <= 1100 && innerHeight <= 760'))
  await restarted.evaluate('setTimeout(() => { void window.__TAURI__.window.getCurrentWindow().close() }, 100)')
  await until(() => app.exitCode !== null || app.signalCode !== null, 'close with invalid window preferences')
  assert.equal(await readFile(join(data, 'window-state.json'), 'utf8'), '{broken')
  passed('invalid window state falls back without overwriting the original file')
} catch (error) {
  exitCode = 1
  process.stderr.write(`${error.stack}\n${nativeErrors}\n`)
} finally {
  for (const connection of connections) { try { connection.close() } catch {} }
  if (app.exitCode === null && app.signalCode === null) app.kill()
  for (const server of [shellServer, fixture, proxy, socks]) { server.closeAllConnections?.(); server.close() }
  await writeFile(join(data, 'result.json'), JSON.stringify({ passed: results, nativePasswordSaveAndFill: passwordSaveAndFill, exitCode }, null, 2))
  process.stdout.write(`Artifacts: ${data}\n`)
  process.exit(exitCode)
}
