// Windows-only isolated acceptance. Never reads personal scripts or SSH configuration.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { createServer } from 'node:http'
import { copyFile, mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { Cdp } from './lib/cdp.mjs'
if (process.platform !== 'win32') throw new Error('Run with Windows Node after building the frontend and Windows debug app.')
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
await mkdir(path.join(root, 'target'), { recursive: true })
const data = await mkdtemp(path.join(root, 'target', 'preparation-check-'))
const scripts = path.join(data, 'scripts 中文 & ! % space')
await mkdir(scripts)
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
async function until(fn, label, timeout = 25000) {
  const end = Date.now() + timeout
  while (Date.now() < end) { const value = await fn(); if (value) return value; await delay(75) }
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
async function profile(directory, predicate) {
  return until(async () => {
    try {
      const port = await activePort(directory)
      if (!port) return
      const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
      const target = targets.find(t => t.type === 'page' && predicate(t))
      if (target) return new Cdp().connect(target.webSocketDebuggerUrl)
    } catch { return undefined }
  }, 'profile')
}
const fixture = createServer((_req, res) => {
  res.setHeader('Content-Type', 'text/html; charset=utf-8')
  res.end('<title>Preparation fixture</title><script>sessionStorage.loads=Number(sessionStorage.loads||0)+1</script><h1>Ready</h1>')
})
const origin = `http://127.0.0.1:${await listen(fixture)}`
const counter = path.join(data, 'starts.txt')
const once = path.join(scripts, 'prepare once.mjs')
await writeFile(once, `import {appendFileSync} from 'node:fs';appendFileSync(${JSON.stringify(counter)},${JSON.stringify('start\n')});setTimeout(()=>console.log(JSON.stringify({type:'result',success:true,url:${JSON.stringify(origin)}})),200)`)
const persistent = path.join(scripts, 'persistent.mjs')
await writeFile(persistent, `import {createServer} from 'node:http';import {spawn} from 'node:child_process';import {writeFileSync} from 'node:fs';
const child=spawn(process.execPath,['-e',"require('node:http').createServer((q,s)=>s.end('child')).listen(0,'127.0.0.1',function(){console.log(this.address().port)})"],{stdio:['ignore','pipe','ignore']});
child.stdout.once('data',b=>{writeFileSync(${JSON.stringify(path.join(data, 'child-port.txt'))},String(b).trim());const server=createServer((q,s)=>s.end('<title>Owned service</title>'));server.listen(0,'127.0.0.1',()=>console.log(JSON.stringify({type:'result',success:true,url:'http://127.0.0.1:'+server.address().port})))})`)
const batch = path.join(scripts, 'prepare.cmd')
const batchArgs = path.join(data, 'batch-args.txt')
const batchChecker = path.join(data, 'check-batch-args.mjs')
await writeFile(batchChecker, `import {writeFileSync} from 'node:fs';writeFileSync(${JSON.stringify(batchArgs)},process.argv[2]);console.log(JSON.stringify({type:'result',success:process.argv[2]==='中文 & % ! ^ spaces',url:${JSON.stringify(origin)}}))`)
// The wrapper itself stays ASCII. A child supplies UTF-8 output without needing
// an attached console or changing CMD's code page in a CREATE_NO_WINDOW process.
await writeFile(batch, '@echo off\r\n"%~2" "%~3" "%~1"\r\nexit /b %errorlevel%\r\n')
const ps = path.join(scripts, 'prepare.ps1')
await writeFile(ps, '\ufeff' + `param([string]$Value)\n[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)\nif ($Value -ne '中文 & % ! ^ spaces') { [Console]::Out.WriteLine('{"type":"result","success":false,"error":"argument mismatch"}'); exit 1 }\n[Console]::Out.WriteLine('{"type":"result","success":true,"url":"${origin}"}')\nexit 0\n`)
await writeFile(path.join(data, 'workspace.json'), JSON.stringify({ version: 3, theme: 'light', webEntries: [], defaultProxy: { mode: 'direct' } }))
const shellServer = createServer(async (req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname
  const file = path.join(root, 'dist', pathname === '/' ? 'index.html' : pathname)
  try { const bytes = await readFile(file); res.setHeader('Content-Type', file.endsWith('.js') ? 'text/javascript' : file.endsWith('.css') ? 'text/css' : 'text/html'); res.end(bytes) }
  catch { res.writeHead(404); res.end() }
})
await listen(shellServer, 1420)
const app = spawn(path.join(root, 'target/windows/debug/oriel.exe'), [], { env: { ...process.env, ORIEL_DEV_DATA_DIR: data, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=0' }, stdio: ['ignore', 'pipe', 'pipe'] })
app.stdout.resume()
let nativeLogs = ''; app.stderr.on('data', b => { nativeLogs = (nativeLogs + b).slice(-65536) })
let shell, page
const results = []
const pass = name => { results.push(name); console.log('PASS ' + name) }
const host = (command, args = {}) => shell.evaluate(`window.__TAURI__.core.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`)
const prep = (file, overrides = {}) => ({ path: file, runner: 'custom', args: [], interpreter: { program: process.execPath, args: [] }, mode: 'once', timeoutSeconds: 5, ...overrides })
async function save(title, preparation, url = '', id = null) {
  const snapshot = await host('workspace_save_entry', { id, title, preparation, replacePreparation: true, url, proxy: { mode: 'direct' } })
  return snapshot.entries.find(e => e.title === title).id
}
const show = id => host('web_show', { entryId: id, bounds: { x: 310, y: 90, width: 660, height: 530 }, visible: true })
const starts = async () => (await readFile(counter, 'utf8')).trim().split('\n').length
const unreachable = async url => { try { await fetch(url, { signal: AbortSignal.timeout(1000) }); return false } catch { return true } }
try {
  shell = await profile(path.join(data, 'shell'), t => t.url.startsWith('http://127.0.0.1:1420'))
  const one = await save('准备网页', prep(once))
  await Promise.all([host('web_prepare', { entryId: one, retry: false }), host('web_prepare', { entryId: one, retry: false })])
  assert.equal(await starts(), 1)
  await show(one)
  page = await profile(path.join(data, 'web-profiles', one), t => t.url.startsWith(origin))
  await until(() => page.evaluate('sessionStorage.loads'), 'first load')
  pass('concurrent preparation launches once and loads a dynamic URL')
  const persisted = JSON.parse(await readFile(path.join(data, 'workspace.json'), 'utf8'))
  assert.equal(persisted.version, 4); assert.equal(persisted.theme, 'light'); assert.equal(persisted.webEntries[0].url, null)
  await host('web_action', { entryId: one, action: 'reload' })
  await until(async () => Number(await page.evaluate('sessionStorage.loads')) >= 2, 'ordinary refresh')
  assert.equal(await starts(), 1)
  const before = Number(await page.evaluate('sessionStorage.loads'))
  await host('web_prepare', { entryId: one, retry: true })
  await until(async () => Number(await page.evaluate('sessionStorage.loads')) > before, 'same URL deep reload')
  assert.equal(await starts(), 2)
  pass('migration preserves settings; refresh skips the program; deep reload reruns it')
  const two = await save('第二页', prep(once))
  await host('web_prepare', { entryId: two, retry: false }); assert.equal(await starts(), 3)
  await show(two); await show(one); assert.equal(await starts(), 3)
  pass('pages execute independently and switching does not restart them')
  for (const [title, file, runner] of [['CMD', batch, 'cmd'], ['PowerShell', ps, 'powershell']]) {
    const args = ['中文 & % ! ^ spaces', ...(runner === 'cmd' ? [process.execPath, batchChecker] : [])]
    const id = await save(title, prep(file, { runner, interpreter: null, args }))
    await host('web_prepare', { entryId: id, retry: false })
    assert.equal((await show(id)).url, origin + '/')
  }
  assert.equal(await readFile(batchArgs, 'utf8'), '中文 & % ! ^ spaces')
  pass('CMD and PowerShell handle spaces, Unicode and literal shell characters')
  const fixtureProgram = path.join(root, 'src-tauri/crates/oriel-core/tests/fixtures/preparation.mjs')
  for (const scenario of ['failure', 'invalid', 'partial', 'duplicate', 'long', 'utf8', 'empty', 'nonzero', 'hang']) {
    const id = await save('失败-' + scenario, prep(fixtureProgram, { args: [scenario, path.join(data, scenario + '.txt')], timeoutSeconds: 1 }), origin)
    await host('web_prepare', { entryId: id, retry: false })
    assert.equal((await show(id)).url, origin + '/')
    const failed = (await host('preparation_states')).find(r => r.entryId === id)
    assert.equal(failed.state, 'failed'); assert.ok(failed.error)
    const noAddress = await save('无登记地址-' + scenario, prep(fixtureProgram, { args: [scenario, path.join(data, 'no-url-' + scenario + '.txt')], timeoutSeconds: 1 }))
    await assert.rejects(host('web_prepare', { entryId: noAddress, retry: false }))
  }
  const staticPage = await save('使用登记地址', prep(fixtureProgram, { args: ['static', path.join(data, 'static.txt')] }), origin)
  await host('web_prepare', { entryId: staticPage, retry: false })
  assert.equal((await show(staticPage)).url, origin + '/')
  const staticRun = (await host('preparation_states')).find(r => r.entryId === staticPage)
  assert.equal(staticRun.url, null); assert.equal(staticRun.error, null)
  const contextPage = await save('地址上下文', prep(fixtureProgram, { args: ['page-env', path.join(data, 'page-env.txt')] }), origin)
  await host('web_prepare', { entryId: contextPage, retry: false })
  assert.equal((await host('preparation_states')).find(r => r.entryId === contextPage).url, origin + '/')
  pass('preparation failures retain diagnostics but registered pages open; omitted URLs and page context work')
  // Exercise the actual PowerShell wrapper without making an SSH connection:
  // a renamed Node executable stands in for ssh.exe and owns its own listener.
  const fakeSsh = path.join(scripts, 'ssh.exe')
  const fakeForward = path.join(scripts, 'fake-forward.mjs')
  const forwardScript = path.join(scripts, 'ssh-port-forward.ps1')
  await copyFile(process.execPath, fakeSsh)
  await writeFile(fakeForward, `import {createServer} from 'node:http';const port=Number(new URL(process.env.ORIEL_PAGE_URL).port);const server=createServer((q,s)=>s.end('<title>Fake SSH forward</title>'));server.on('error',e=>{console.error(e.message);process.exit(1)});server.listen(port,'127.0.0.1',()=>console.log('fixture listener ready'))`)
  const wrapper = await readFile(path.join(root, 'plugins/builtin/browser/script/ssh-port-forward.ps1'), 'utf8')
  assert.match(wrapper, /\$PageUrl = ''/)
  const command = `"${fakeSsh}" "${fakeForward}"`
  await writeFile(forwardScript, wrapper.replace(/(\$SshCommand = @'\r?\n)[^\r\n]+/, (_match, prefix) => prefix + command))
  const free = createServer()
  const freePort = await listen(free)
  await new Promise(resolve => free.close(resolve))
  const registeredForward = `http://127.0.0.1:${freePort}/`
  const wrapped = await save('SSH 模板无地址', prep(forwardScript, { runner: 'powershell', interpreter: null, mode: 'persistent', timeoutSeconds: 30 }), registeredForward)
  await host('web_prepare', { entryId: wrapped, retry: false })
  assert.equal((await show(wrapped)).url, registeredForward)
  const wrappedRun = (await host('preparation_states')).find(r => r.entryId === wrapped)
  assert.equal(wrappedRun.state, 'ready'); assert.equal(wrappedRun.url, null)
  await host('preparation_stop', { entryId: wrapped })
  await until(() => unreachable(registeredForward), 'wrapper owned listener cleanup')
  const occupied = await save('端口已有外部程序', prep(forwardScript, { runner: 'powershell', interpreter: null, mode: 'persistent', timeoutSeconds: 30 }), origin)
  await host('web_prepare', { entryId: occupied, retry: false })
  assert.equal((await show(occupied)).url, origin + '/')
  assert.equal((await host('preparation_states')).find(r => r.entryId === occupied).state, 'failed')
  assert.equal((await fetch(origin)).ok, true)
  const occupiedGeneration = (await host('preparation_states')).find(r => r.entryId === occupied).generation
  await save('端口已有外部程序', prep(forwardScript, { runner: 'powershell', interpreter: null, mode: 'persistent', timeoutSeconds: 30 }), origin + '/updated', occupied)
  assert.equal((await show(occupied)).url, origin + '/updated')
  assert.equal((await host('preparation_states')).find(r => r.entryId === occupied).generation, occupiedGeneration)
  pass('SSH wrapper omits URL and uses page context; occupied external port does not block the page or transfer ownership')

  const service = await save('持续服务', prep(persistent, { mode: 'persistent' }))
  await host('web_prepare', { entryId: service, retry: false })
  const serviceState = (await host('preparation_states')).find(r => r.entryId === service)
  const childUrl = `http://127.0.0.1:${await readFile(path.join(data, 'child-port.txt'), 'utf8')}`
  assert.equal((await fetch(serviceState.url)).ok, true); assert.equal((await fetch(childUrl)).ok, true)
  await host('preparation_stop', { entryId: service })
  await until(async () => await unreachable(serviceState.url) && await unreachable(childUrl), 'owned tree cleanup')
  assert.equal((await fetch(origin)).ok, true)
  pass('stop cleans the owned server and child while preserving the external fixture')
  await host('workspace_save_entry', { id: two, title: '第二页', preparation: prep(once, { args: ['changed'] }), replacePreparation: true, url: '', proxy: { mode: 'direct' } })
  assert.equal((await host('preparation_states')).find(r => r.entryId === two).configurationChanged, true)
  assert.equal(await starts(), 3)
  pass('saving changes neither executes nor replaces the old running command')
  await page.evaluate(`window.boundary = typeof window.__TAURI__;`)
  assert.equal(await page.evaluate('window.boundary'), 'undefined')
  pass('external pages receive no host execution interface')
  // Exercise the actual panel, both themes and a narrow window; keep screenshots isolated.
  await shell.evaluate(`document.querySelector('[aria-label="重新加载接入"]').click()`)
  await until(() => shell.evaluate(`!document.querySelector('[aria-label="重新加载接入"]').disabled`), 'workspace reload')
  await shell.evaluate(`document.querySelector('[aria-label="前置程序"]').click()`)
  await until(() => shell.evaluate('Boolean(document.querySelector(".preparation-panel[open]"))'), 'program panel')
  for (const [name, width, height] of [['light', 1100, 760], ['narrow', 520, 680]]) {
    await shell.command('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
    assert.equal(await shell.evaluate('document.documentElement.scrollWidth <= innerWidth'), true)
    await writeFile(path.join(data, name + '.png'), Buffer.from((await shell.command('Page.captureScreenshot', { format: 'png' })).data, 'base64'))
  }
  await shell.evaluate(`document.querySelector('[aria-label="关闭前置程序"]').click();document.querySelector('[aria-label="切换为深色外观"]').click();document.querySelector('[aria-label="前置程序"]').click()`)
  await writeFile(path.join(data, 'dark.png'), Buffer.from((await shell.command('Page.captureScreenshot', { format: 'png' })).data, 'base64'))
  pass('program panel opens, fits narrow layout and captures both themes')
  await host('web_prepare', { entryId: service, retry: true })
  const active = (await host('preparation_states')).find(r => r.entryId === service).url
  await shell.evaluate(`setTimeout(()=>window.__TAURI__.window.getCurrentWindow().close(),100)`)
  await until(() => app.exitCode !== null, 'complete exit')
  assert.equal(await unreachable(active), true); assert.equal((await fetch(origin)).ok, true)
  pass('complete exit cleans owned programs and preserves external services')
  console.log(JSON.stringify({ data, passed: results.length, results }, null, 2))
} finally {
  shell?.close(); page?.close(); if (app.exitCode === null) app.kill()
  fixture.close(); shellServer.close(); if (nativeLogs) console.log(nativeLogs)
}
