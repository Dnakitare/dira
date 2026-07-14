// Static demo player: drives the console from an exported run file
// (`dira export`) instead of a live WebSocket. Used by the GitHub Pages
// demo; the console itself cannot tell the difference — it renders
// snapshots and events exactly as in live replay mode.

import {
  PROTOCOL_VERSION,
  type AuditEvent,
  type Command,
  type RunInfo,
  type WorldState,
} from '../protocol/types'
import { useOps } from '../state/store'

interface RunExport {
  format: string
  info: RunInfo
  events: AuditEvent[]
  snapshots: { sim_time_ms: number; world: WorldState }[]
}

const TICK_MS = 100
const SPEEDS = { min: 0.1, max: 64 }

export async function startDemo(url: string): Promise<void> {
  const store = useOps.getState()
  let doc: RunExport
  try {
    const res = await fetch(url)
    if (!res.ok) throw new Error(`${res.status}`)
    doc = await res.json()
  } catch (err) {
    store.toast('error', `demo run failed to load (${err}); check the demo file URL`)
    return
  }
  if (doc.format !== 'dira-run-export/1' || doc.snapshots.length === 0) {
    store.toast('error', 'demo file is not a dira run export')
    return
  }

  const duration = Math.max(
    doc.snapshots[doc.snapshots.length - 1].sim_time_ms,
    doc.events.length > 0 ? doc.events[doc.events.length - 1].sim_time_ms : 0,
  )
  let position = 0
  let speed = 2
  let playing = true
  let nextEvent = 0
  let nextSnapshot = 0
  let ticks = 0

  const publishStatus = () =>
    useOps.getState().applyReplay({
      run_id: doc.info.run_id,
      speed,
      position_ms: position,
      duration_ms: duration,
      playing,
    })

  const restart = () => {
    position = 0
    nextEvent = 0
    nextSnapshot = 0
    useOps.getState().applySnapshot(doc.snapshots[0].world)
  }

  store.applyHello(
    {
      name: 'dira',
      version: 'static demo',
      mode: 'replay',
      protocol_version: PROTOCOL_VERSION,
    },
    [],
    [doc.info],
    doc.snapshots[0].world,
    [],
  )
  store.setConnected(true)
  store.setSend((command: Command) => {
    switch (command.type) {
      case 'pause':
        playing = false
        break
      case 'resume':
      case 'start':
        if (position >= duration) restart()
        playing = true
        break
      case 'reset':
        restart()
        playing = true
        break
      case 'set_speed':
        speed = Math.min(Math.max(command.multiplier, SPEEDS.min), SPEEDS.max)
        break
      default:
        useOps
          .getState()
          .toast('info', 'static demo replay: only transport controls are available here')
        return
    }
    publishStatus()
  })
  publishStatus()

  setInterval(() => {
    ticks++
    if (playing) {
      position = Math.min(position + TICK_MS * speed, duration)
      const s = useOps.getState()
      while (nextEvent < doc.events.length && doc.events[nextEvent].sim_time_ms <= position) {
        s.applyEvent(doc.events[nextEvent])
        nextEvent++
      }
      let latest = -1
      while (
        nextSnapshot < doc.snapshots.length &&
        doc.snapshots[nextSnapshot].sim_time_ms <= position
      ) {
        latest = nextSnapshot
        nextSnapshot++
      }
      if (latest >= 0) s.applySnapshot(doc.snapshots[latest].world)
      if (position >= duration && playing) {
        playing = false
        publishStatus()
      }
    }
    if (ticks % 10 === 0) publishStatus()
  }, TICK_MS)
}
