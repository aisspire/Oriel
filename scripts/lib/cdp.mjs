export class Cdp {
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
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.exception?.value ?? result.exceptionDetails.text)
    return result.result.value
  }
  close() { this.socket.close() }
}
