// Isolated Windows integration; plugin business fixtures remain outside the host runtime.
import assert from 'node:assert/strict'
import {spawn} from 'node:child_process'
import {createServer} from 'node:http'
import {cp,copyFile,mkdir,mkdtemp,readFile,readdir,writeFile} from 'node:fs/promises'
import path from 'node:path'
import {Cdp} from './lib/cdp.mjs'
if(process.platform!=='win32')throw Error('Use Windows Node after building the host and Script Studio Windows debug executables.')
const root=path.resolve('.'),tool=path.resolve(process.env.SCRIPT_STUDIO_REPO||'plugins/external/script-studio')
await mkdir(path.join(root,'target'),{recursive:true})
const data=await mkdtemp(path.join(root,'target/script-studio-check-')),toolData=path.join(data,'tool-data'),provider=path.join(data,'plugins/script-studio')
await mkdir(provider,{recursive:true});await mkdir(toolData)
for(const file of ['plugin.json','oriel.ps1','USER_GUIDE.md'])await copyFile(path.join(tool,file),path.join(provider,file))
await copyFile(process.env.SCRIPT_STUDIO_BIN||path.join(tool,'target/windows/debug/script-studio.exe'),path.join(provider,'script-studio.exe'))
await cp(path.join(tool,'dist'),path.join(provider,'dist'),{recursive:true})
const manifest=JSON.parse(await readFile(path.join(provider,'plugin.json'),'utf8'));manifest.lifecycle.args=['-DataDir',toolData];await writeFile(path.join(provider,'plugin.json'),JSON.stringify(manifest))
await writeFile(path.join(data,'workspace.json'),JSON.stringify({version:4,pluginDirectories:[],pluginRoots:[],webEntries:[],defaultProxy:{mode:'direct'},theme:'light'}))
const scriptId=crypto.randomUUID(),fixture=path.join(data,'fixture.mjs'),marker=path.join(data,'owned.pid'),externalMarker=path.join(data,'external.pid')
await writeFile(fixture,"import fs from 'node:fs';fs.writeFileSync(process.argv[2],String(process.pid));process.stdout.write('host integration fixture\\n');setInterval(()=>{},1000)")
await writeFile(path.join(toolData,'config.json'),JSON.stringify({version:1,groups:[],scripts:[{id:scriptId,name:'隔离脚本',description:'',path:fixture,groupId:null,favorite:false,runner:'custom',program:process.execPath,interpreterArgs:[],args:[marker],cwd:null,env:{},encoding:'utf8'}]}))
const delay=ms=>new Promise(r=>setTimeout(r,ms))
async function until(fn,label,timeout=25000){const end=Date.now()+timeout;let last;while(Date.now()<end){try{const value=await fn();if(value)return value}catch(e){last=e}await delay(80)}throw Error(`Timeout ${label}: ${last?.message||''}`)}
async function activePort(dir){for(const entry of await readdir(dir,{withFileTypes:true})){const p=path.join(dir,entry.name);if(entry.name==='DevToolsActivePort')return Number((await readFile(p,'utf8')).split('\n')[0]);if(entry.isDirectory()){const port=await activePort(p);if(port)return port}}}
async function profile(dir,predicate){return until(async()=>{const port=await activePort(dir);if(!port)return;const targets=await(await fetch(`http://127.0.0.1:${port}/json/list`)).json();const target=targets.find(t=>t.type==='page'&&predicate(t));if(target)return new Cdp().connect(target.webSocketDebuggerUrl)},'native profile')}
const shellServer=createServer(async(req,res)=>{const pathname=new URL(req.url,'http://localhost').pathname;const file=path.join(root,'dist',pathname==='/'?'index.html':pathname);try{const bytes=await readFile(file);res.setHeader('Content-Type',file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html');res.end(bytes)}catch{res.writeHead(404);res.end()}})
await new Promise((resolve,reject)=>{shellServer.once('error',reject);shellServer.listen(1420,'127.0.0.1',resolve)})
const external=spawn(process.execPath,[fixture,externalMarker],{stdio:'ignore'})
const app=spawn(process.env.ORIEL_BIN||path.join(root,'target/windows/debug/oriel.exe'),[],{env:{...process.env,ORIEL_DEV_DATA_DIR:data,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=0'},stdio:['ignore','pipe','pipe']})
let diagnostics='';app.stderr.on('data',b=>diagnostics+=b)
let shell,page,desc;const results=[]
const passed=name=>{results.push(name);console.log('PASS '+name)}
const host=(command,args={})=>shell.evaluate(`window.__TAURI__.core.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`)
async function api(action,args={}){const response=await fetch(desc.origin+'/api',{method:'POST',headers:{'Content-Type':'application/json',Authorization:`Bearer ${desc.token}`},body:JSON.stringify({action,...args}),signal:AbortSignal.timeout(15000)});const result=await response.json();if(!result.ok)throw Error(result.error);return result.value}
const alive=pid=>{try{process.kill(pid,0);return true}catch{return false}}
try{
 shell=await profile(path.join(data,'shell'),t=>t.url.startsWith('http://127.0.0.1:1420'))
 await until(()=>shell.evaluate('Boolean(window.__TAURI__?.core)'),'host IPC ready')
 const snapshot=await host('workspace_snapshot');assert.deepEqual(snapshot.diagnostics,[]);const entry=snapshot.entries.find(e=>e.id==='script-studio.workspace');assert.equal(entry.presentation,'application');assert.equal(entry.service,'script-studio')
 await assert.rejects(readFile(path.join(toolData,'instance.json')));assert.match(await host('plugin_documentation',{entryId:entry.id}),/WSL 包装脚本/);await assert.rejects(readFile(path.join(toolData,'instance.json')))
 passed('default plugin directory discovers declaration and guide without starting backend')
 await until(()=>shell.evaluate(`Boolean(document.querySelector('button[aria-label="Script Studio"]'))`),'plugin navigation rendered')
 await shell.evaluate(`document.querySelector('button[aria-label="Script Studio"]').click()`)
 desc=await until(async()=>JSON.parse(await readFile(path.join(toolData,'instance.json'),'utf8')),'tool readiness')
 page=await profile(path.join(data,'web-profiles/script-studio.workspace'),t=>t.url.startsWith(desc.origin))
 await until(()=>page.evaluate(`document.body.textContent.includes('隔离脚本')`),'native tool page')
 assert.equal(await shell.evaluate(`Boolean(document.querySelector('.tool-application'))`),true);assert.equal(await shell.evaluate(`Boolean(document.querySelector('input[aria-label="网页地址"],.navigation-buttons'))`),false)
 assert.notEqual(await page.evaluate(`(async()=>{if(!window.__TAURI__?.core)return 'blocked';return Promise.race([window.__TAURI__.core.invoke('workspace_snapshot').then(()=>'allowed',()=>'denied'),new Promise(r=>setTimeout(()=>r('blocked'),500))])})()`),'allowed')
 passed('native application layout and external-page host permission boundary work without plugin-specific routing')
 await page.evaluate(`document.querySelector('[aria-label="运行 隔离脚本"]').click()`)
 const run=await until(async()=>(await api('snapshot')).runs.find(r=>r.scriptId===scriptId),'script start'),pid=await until(async()=>Number(await readFile(marker,'utf8')),'owned PID')
 await shell.evaluate(`document.querySelector('button[aria-label="首页"]').click()`);await delay(250);assert.equal((await api('snapshot')).runs.find(r=>r.id===run.id).state,'running')
 await shell.evaluate(`document.querySelector('button[aria-label="Script Studio"]').click()`);await delay(250);assert.equal(JSON.parse(await readFile(path.join(toolData,'instance.json'),'utf8')).instance,desc.instance);assert.ok(alive(pid))
 passed('switching away and reopening reuses backend and preserves active run')
 await page.evaluate(`window.scriptStudioThemeMarker=Math.random()`);const markerValue=await page.evaluate('window.scriptStudioThemeMarker')
 await host('workspace_theme',{theme:'dark'});await until(()=>page.evaluate(`matchMedia('(prefers-color-scheme: dark)').matches`),'native dark theme');assert.equal(await page.evaluate('window.scriptStudioThemeMarker'),markerValue)
 await until(()=>page.evaluate(`document.getAnimations().every(animation=>animation.playState!=='running')`),'theme transitions settle');assert.equal(await page.evaluate(`getComputedStyle(document.documentElement).colorScheme`),'dark')
 await writeFile(path.join(data,'application-dark.png'),Buffer.from((await page.command('Page.captureScreenshot',{format:'png'})).data,'base64'))
 await shell.evaluate(`document.querySelector('.plugin-help-button').focus();document.querySelector('.plugin-help-button').click()`);await until(()=>shell.evaluate(`document.querySelector('.plugin-help-dialog[open]')?.textContent.includes('WSL 包装脚本')`),'host guide')
 await shell.command('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27});await until(()=>shell.evaluate(`!document.querySelector('.plugin-help-dialog[open]')`),'guide closed');assert.equal(await shell.evaluate(`document.activeElement.classList.contains('plugin-help-button')`),true);assert.ok(alive(pid))
 passed('host theme changes native page without refresh and host guide restores focus while task continues')
 page.close();page=null;await shell.evaluate(`setTimeout(()=>void window.__TAURI__.window.getCurrentWindow().close(),100)`)
 await until(()=>app.exitCode!==null||app.signalCode!==null,'normal host exit',20000);await until(()=>!alive(pid),'owned process cleanup')
 await assert.rejects(fetch(desc.origin+'/api'));assert.equal(external.exitCode,null);assert.ok(alive(Number(await readFile(externalMarker,'utf8'))));assert.equal(JSON.parse(await readFile(path.join(toolData,'runs',run.id+'.json'),'utf8')).state,'stopped')
 passed('complete host exit stops only plugin-owned process tree and persists stopped result')
 console.log(JSON.stringify({data,passed:results.length,results},null,2))
}finally{
 shell?.close();page?.close();if(app.exitCode===null&&app.signalCode===null)app.kill();external.kill();shellServer.close()
 if(desc){try{await api('shutdown')}catch{}}
 if(diagnostics)console.log(diagnostics)
}
