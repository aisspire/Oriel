import { appendFileSync, writeFileSync } from 'node:fs'
import { spawn } from 'node:child_process'
const [scenario, marker, ...args] = process.argv.slice(2)
const result = value => process.stdout.write(JSON.stringify({ type: 'result', ...value }) + '\n')
if (marker) appendFileSync(marker, 'start\n')
switch (scenario) {
  case 'once': result({ success: true, url: 'http://127.0.0.1:18080/' }); break
  case 'static': result({ success: true }); break
  case 'page-env': result({ success: true, url: process.env.ORIEL_PAGE_URL || undefined }); break
  case 'file-env':
    writeFileSync(marker + '.file', process.env.ORIEL_RESULT_FILE || '');
    writeFileSync(process.env.ORIEL_RESULT_FILE, JSON.stringify({ type: 'result', success: true, url: 'http://127.0.0.1:18080/' }));
    break
  case 'file-with-stdout-pollution':
    writeFileSync(marker + '.file', process.env.ORIEL_RESULT_FILE || '');
    console.log('stdout header line 1');
    console.log('stdout header line 2');
    writeFileSync(process.env.ORIEL_RESULT_FILE, JSON.stringify({ type: 'result', success: true, url: 'http://127.0.0.1:18080/' }));
    console.log('stdout trailing line 3');
    break
  case 'file-held-stdout': {
    writeFileSync(marker + '.file', process.env.ORIEL_RESULT_FILE);
    const child = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: ['ignore', 'inherit', 'ignore'] });
    child.unref();
    writeFileSync(process.env.ORIEL_RESULT_FILE, JSON.stringify({ type: 'result', success: true, url: 'http://127.0.0.1:18080/' }));
    break
  }
  case 'file-partial':
  case 'file-large':
  case 'file-persistent':
    writeFileSync(marker + '.file', process.env.ORIEL_RESULT_FILE);
    if (scenario !== 'file-partial') process.stdout.write('x'.repeat(180000) + '\n');
    writeFileSync(process.env.ORIEL_RESULT_FILE, JSON.stringify({ type: 'result', success: true, url: 'http://127.0.0.1:18080/' }));
    process.stdout.write('tail-fragment');
    if (scenario === 'file-persistent') setInterval(() => console.log('still-running'), 100);
    break
  case 'large-then-result':
    console.log('x'.repeat(180000)); result({ success: true }); break
  case 'file-failure':
    writeFileSync(process.env.ORIEL_RESULT_FILE, JSON.stringify({ type: 'result', success: false, error: 'fixture file failed' }));
    break
  case 'args': result({ success: args[0] === '中文 & % ! ^ spaces' && args[1] === '"quoted"', error: undefined }); break
  case 'failure': result({ success: false, error: 'fixture failed' }); break
  case 'invalid': console.log('plain output'); break
  case 'internal': result({ success: true, url: 'http://tauri.localhost/' }); break
  case 'partial': process.stdout.write('{"type":"result","success":true}'); break
  case 'duplicate': result({ success: true }); result({ success: true }); break
  case 'long': console.log('a'.repeat(65537)); break
  case 'utf8': process.stdout.write(Buffer.from([0xff, 10])); break
  case 'empty': break
  case 'nonzero': result({ success: true }); process.exitCode = 9; break
  case 'delayed': setTimeout(() => result({ success: true }), 250); break
  case 'hang': setInterval(() => {}, 1000); break
  case 'once-hang': result({ success: true }); setInterval(() => {}, 1000); break
  case 'persistent': result({ success: true }); setInterval(() => {}, 1000); break
  case 'exit-later': result({ success: true }); setTimeout(() => process.exit(0), 300); break
  case 'flood':
    for (let i = 0; i < 2000; i++) process.stderr.write(`${i}: ${'文'.repeat(2000)}\n`)
    result({ success: true }); break
  case 'child': {
    const child = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore' })
    writeFileSync(marker + '.pid', String(child.pid));
    result({ success: true }); setInterval(() => {}, 1000); break
  }
  default: throw new Error(`Unknown fixture: ${scenario}`)
}
