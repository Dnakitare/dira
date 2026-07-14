import { create } from 'zustand'
import type {
  AuditEvent,
  Command,
  ReplayStatus,
  RunInfo,
  RuntimeInfo,
  ScenarioInfo,
  WorldState,
} from '../protocol/types'

const EVENT_CAP = 500

export interface Toast {
  id: number
  kind: 'error' | 'info'
  text: string
}

export type Selection = { kind: 'track' | 'asset' | 'zone'; id: string } | null

interface OpsState {
  connected: boolean
  runtime: RuntimeInfo | null
  scenarios: ScenarioInfo[]
  runs: RunInfo[]
  world: WorldState | null
  events: AuditEvent[]
  replay: ReplayStatus | null
  selection: Selection
  toasts: Toast[]
  /** Wall-clock ms of the last snapshot, for staleness display. */
  lastSnapshotWallMs: number
  /** Set by the socket layer; components call it to send commands. */
  send: (command: Command) => void

  applyHello: (
    runtime: RuntimeInfo,
    scenarios: ScenarioInfo[],
    runs: RunInfo[],
    world: WorldState,
    recent: AuditEvent[],
  ) => void
  applySnapshot: (world: WorldState) => void
  applyEvent: (event: AuditEvent) => void
  applyReplay: (status: ReplayStatus) => void
  setConnected: (connected: boolean) => void
  setSend: (send: (command: Command) => void) => void
  select: (selection: Selection) => void
  toast: (kind: Toast['kind'], text: string) => void
  dismissToast: (id: number) => void
}

let toastSeq = 0

export const useOps = create<OpsState>((set) => ({
  connected: false,
  runtime: null,
  scenarios: [],
  runs: [],
  world: null,
  events: [],
  replay: null,
  selection: null,
  toasts: [],
  lastSnapshotWallMs: 0,
  send: () => {},

  applyHello: (runtime, scenarios, runs, world, recent) =>
    set({
      runtime,
      scenarios,
      runs,
      world,
      events: recent.slice(-EVENT_CAP),
      lastSnapshotWallMs: Date.now(),
    }),
  applySnapshot: (world) =>
    set((s) => {
      // A reset or replay restart rewinds sim time; drop the stale timeline.
      const rewound = s.world && world.sim_time_ms < s.world.sim_time_ms
      return {
        world,
        lastSnapshotWallMs: Date.now(),
        events: rewound ? [] : s.events,
      }
    }),
  applyEvent: (event) =>
    set((s) => ({ events: [...s.events.slice(-(EVENT_CAP - 1)), event] })),
  applyReplay: (status) => set({ replay: status }),
  setConnected: (connected) => set({ connected }),
  setSend: (send) => set({ send }),
  select: (selection) => set({ selection }),
  toast: (kind, text) =>
    set((s) => ({ toasts: [...s.toasts.slice(-4), { id: ++toastSeq, kind, text }] })),
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}))

export function fmtSimTime(ms: number): string {
  const total = Math.floor(ms / 1000)
  const m = Math.floor(total / 60)
  const s = total % 60
  return `T+${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

export function fmtLinkState(state: import('../protocol/types').LinkState): string {
  switch (state.state) {
    case 'nominal':
      return 'NOMINAL'
    case 'delayed':
      return `DELAYED ${state.delay_ms}ms`
    case 'intermittent':
      return `INTERMIT ${Math.round(state.loss * 100)}%`
    case 'unavailable':
      return 'NO LINK'
  }
}
