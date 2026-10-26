// Main and overlay share a profile and origin. URL ordering cannot identify them.
export async function connectHostTarget(targets, label, Cdp) {
  for (const target of targets.filter(t => t.type === 'page' && t.url.startsWith('http://127.0.0.1:1420'))) {
    const connection = await new Cdp().connect(target.webSocketDebuggerUrl)
    try {
      if (await connection.evaluate('window.__TAURI__?.window.getCurrentWindow().label') === label) return connection
    } catch { /* The global API may not have initialized yet. */ }
    connection.close()
  }
}
