// Reconnecting WebSocket client. The runtime is authoritative; if this
// connection drops the runtime keeps going, and on reconnect the hello
// message resynchronizes the whole picture.

import { PROTOCOL_VERSION, type Command, type ServerEnvelope } from '../protocol/types'
import { useOps } from '../state/store'

const BACKOFF_MIN_MS = 500
const BACKOFF_MAX_MS = 5000

let socket: WebSocket | null = null
let backoff = BACKOFF_MIN_MS
let commandSeq = 0

function wsUrl(): string {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws'
  const token = new URLSearchParams(location.search).get('token')
  const query = token ? `?token=${encodeURIComponent(token)}` : ''
  return `${proto}://${location.host}/ws${query}`
}

export function connect(): void {
  const store = useOps.getState()
  store.setSend(send)
  open()
}

function open(): void {
  socket = new WebSocket(wsUrl())

  socket.onopen = () => {
    backoff = BACKOFF_MIN_MS
    useOps.getState().setConnected(true)
  }

  socket.onmessage = (raw) => {
    let env: ServerEnvelope
    try {
      env = JSON.parse(raw.data as string)
    } catch {
      return
    }
    const s = useOps.getState()
    switch (env.kind) {
      case 'hello':
        s.applyHello(env.runtime, env.scenarios, env.runs, env.world, env.recent_events)
        break
      case 'snapshot':
        s.applySnapshot(env.world)
        break
      case 'event':
        s.applyEvent(env.event)
        break
      case 'replay_status':
        s.applyReplay(env)
        break
      case 'command_ack':
        break
      case 'error':
        s.toast('error', `${env.code}: ${env.message}`)
        break
    }
  }

  socket.onclose = () => {
    useOps.getState().setConnected(false)
    socket = null
    setTimeout(open, backoff)
    backoff = Math.min(backoff * 2, BACKOFF_MAX_MS)
  }

  socket.onerror = () => {
    socket?.close()
  }
}

export function send(command: Command): void {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    useOps.getState().toast('error', 'not connected: command not sent')
    return
  }
  socket.send(
    JSON.stringify({
      v: PROTOCOL_VERSION,
      id: ++commandSeq,
      command,
    }),
  )
}
