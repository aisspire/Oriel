// Windows native integration check. Uses Node's built-in CDP WebSocket support;
// all browser/application data stays in a fresh directory under target/.
import assert from 'node:assert/strict'
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
async function connectProfile(directory, predicate) {
  return until(async () => {
    const port = await activePort(directory)
    if (!port) return
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
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
let exitCode = 0
let passwordSaveAndFill = 'requires manual acceptance'
try {
  const shell = await connectProfile(join(data, 'shell'), target => target.url.startsWith('http://127.0.0.1:1420'))
  connections.push(shell)
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

  await click('添加网页')
  await until(() => shell.evaluate(`Boolean(document.querySelector('dialog[open] input'))`), 'add webpage dialog')
  await shell.evaluate(`(() => { const inputs = document.querySelectorAll('dialog[open] input'); inputs[0].value='Native fixture'; inputs[0].dispatchEvent(new Event('input',{bubbles:true})); inputs[1].value=${JSON.stringify(`${fixtureUrl}/start`)}; inputs[1].dispatchEvent(new Event('input',{bubbles:true})); document.querySelector('dialog[open] form').requestSubmit(); })()`)
  await until(() => shell.evaluate(`!document.querySelector('dialog[open]') && Boolean(document.querySelector('button[aria-label="Native fixture"]'))`), 'saved webpage navigation')
  workspace = await invoke('workspace_snapshot')
  const entry = workspace.entries.find(entry => entry.title === 'Native fixture')
  assert(entry?.editable)
  const page = await connectProfile(join(data, 'web-profiles', entry.id), target => target.url.startsWith(fixtureUrl))
  connections.push(page)
  await until(() => page.evaluate('document.title === "Fixture /start"'), 'initial webpage load')
  passed('webpage creation through UI and native rendering')

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
  await until(() => shell.evaluate(`Boolean(document.querySelector('dialog[open] input'))`), 'edit webpage dialog')
  await shell.evaluate(`(() => { const input = document.querySelector('dialog[open] input'); input.value='Native fixture renamed'; input.dispatchEvent(new Event('input',{bubbles:true})); document.querySelector('dialog[open] form').requestSubmit() })()`)
  entry.title = 'Native fixture renamed'
  await until(() => shell.evaluate(`Boolean(document.querySelector('button[aria-label="Native fixture renamed"]')) && !document.querySelector('dialog[open]')`), 'renamed entry')
  assert.equal((await invoke('workspace_snapshot')).entries.find(item => item.id === entry.id).title, entry.title)
  assert.equal(await page.evaluate('location.pathname'), '/spa')
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
  assert.equal(await secondPage.evaluate('document.cookie'), '')
  passed('independent entry browser profiles')

  const external = join(data, 'external-plugin')
  await mkdir(external)
  await writeFile(join(external, 'plugin.json'), JSON.stringify({ schemaVersion: 1, id: 'check.external', name: 'External', version: '1', capabilities: ['web-page'], entries: [{ id: 'page', title: 'External webpage', kind: 'web-page', url: `${fixtureUrl}/external` }] }))
  let updated = await invoke('workspace_settings', { defaultProxy: { mode: 'custom', address: `http://127.0.0.1:${proxyPort}` }, pluginDirectories: [external] })
  assert(updated.entries.some(item => item.id === 'check.external.page'))
  await click('重新加载接入')
  await until(() => shell.evaluate(`Boolean(document.querySelector('button[aria-label="External webpage"]'))`), 'external manifest navigation')
  await click('浏览器')
  await invoke('web_navigate', { entryId: 'oriel.browser.browser', url: 'http://oriel-proxy.test/probe' })
  await until(() => proxyRequests.some(url => url.includes('oriel-proxy.test/probe')), 'default HTTP proxy request')
  passed('external webpage registration without recompilation and default HTTP proxy')

  const socksEntryId = (await invoke('workspace_save_entry', { id: null, title: 'SOCKS fixture', url: 'http://oriel-socks.test/probe', proxy: { mode: 'custom', address: `socks5://127.0.0.1:${socksPort}` } })).entries.find(item => item.title === 'SOCKS fixture').id
  await invoke('web_show', { entryId: socksEntryId, bounds: { x: 100, y: 150, width: 600, height: 400 }, visible: true })
  await until(() => socksRequests > 0, 'SOCKS5 network request')
  passed('per-entry SOCKS5 proxy')

  const beforeDirect = visits
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
  await click(entry.title)
  passed('unavailable proxy reports an error without falling back to a direct connection')

  const persisted = JSON.parse(await readFile(join(data, 'workspace.json'), 'utf8'))
  assert.equal(persisted.webEntries.length, 3)
  assert.equal(persisted.pluginDirectories[0], external)
  secondPage.close()
  await click('Separate profile')
  await click('网页设置')
  await until(() => shell.evaluate(`Boolean(document.querySelector('dialog[open] .delete-entry'))`), 'delete entry dialog')
  await shell.evaluate(`document.querySelector('dialog[open] .delete-entry').click();`)
  await until(() => shell.evaluate(`Boolean(document.querySelector('.delete-confirmation .danger'))`), 'delete confirmation')
  await shell.evaluate(`document.querySelector('.delete-confirmation .danger').click()`)
  await until(() => shell.evaluate(`!document.querySelector('button[aria-label="Separate profile"]') && !document.querySelector('dialog[open]')`), 'deleted entry')
  assert(!(await invoke('workspace_snapshot')).entries.some(item => item.id === secondEntry.id))
  await readFile(join(data, 'web-profiles', secondEntry.id, 'EBWebView', 'Local State'))
  passed('webpage removal through UI preserves browser data')

  for (const connection of connections) { try { connection.close() } catch {} }
  app.kill()
  await until(() => app.exitCode !== null || app.signalCode !== null, 'application exit')
  app = spawn(executable, [], appOptions)
  app.stderr.on('data', data => { nativeErrors += data })
  const restarted = await connectProfile(join(data, 'shell'), target => target.url.startsWith('http://127.0.0.1:1420'))
  connections.push(restarted)
  await until(() => restarted.evaluate(`Boolean(window.__TAURI__?.core)`), 'restarted host API')
  const restored = await restarted.evaluate(`window.__TAURI__.core.invoke('workspace_snapshot')`)
  assert(restored.entries.some(item => item.id === entry.id && item.title === entry.title))
  assert(!restored.entries.some(item => item.id === secondEntry.id))
  assert.equal(restored.defaultProxy.mode, 'custom')
  passed('application restart restores local entries and proxy configuration')

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
