// Hand-maintained mirror of crates/protocol (see docs/protocol.md).
// Protocol version must match the runtime; the runtime rejects mismatches.

export const PROTOCOL_VERSION = 1

export interface Vec2 {
  x: number
  y: number
}

export type SimPhase = 'idle' | 'running' | 'paused' | 'complete'
export type TrackClass = 'unknown' | 'ground' | 'air'
export type TrackStatus = 'active' | 'stale' | 'lost'
export type AssetKind = 'patrol' | 'observer'
export type AssetStatus = 'available' | 'enroute' | 'observing' | 'investigating' | 'unavailable'
export type ZoneKind = 'protected' | 'observation'
export type NodeHealth = 'nominal' | 'degraded' | 'unavailable'
export type RecStatus = 'pending' | 'acknowledged' | 'approved' | 'declined' | 'expired'

export type LinkState =
  | { state: 'nominal' }
  | { state: 'delayed'; delay_ms: number }
  | { state: 'intermittent'; loss: number }
  | { state: 'unavailable' }

export interface Track {
  id: string
  class: TrackClass
  pos: Vec2
  vel: Vec2
  status: TrackStatus
  uncertainty_m: number
  last_seen_ms: number
  via_link: string
  flagged: boolean
}

export interface Asset {
  id: string
  kind: AssetKind
  pos: Vec2
  vel: Vec2
  speed_mps: number
  observe_radius_m: number
  status: AssetStatus
  assignment: string | null
}

export interface Zone {
  id: string
  name: string
  kind: ZoneKind
  center: Vec2
  radius_m: number
  priority: number
  covered: boolean
}

export interface Link {
  id: string
  name: string
  state: LinkState
  since_ms: number
}

export interface NodeInfo {
  id: string
  name: string
  health: NodeHealth
}

export type RecKind =
  | { type: 'assign_observation'; asset: string; zone: string }
  | { type: 'investigate_track'; asset: string; track: string }
  | { type: 'flag_track'; track: string; zone: string }
  | { type: 'comms_check'; link: string }

export interface Recommendation {
  id: string
  policy_id: string
  kind: RecKind
  reason: string
  confidence: number
  created_ms: number
  status: RecStatus
  status_changed_ms: number
  expires_ms: number
}

export type Objective =
  | { type: 'observe_zone'; zone: string }
  | { type: 'investigate_track'; track: string }

export interface Assignment {
  id: string
  asset: string
  objective: Objective
  created_ms: number
  status: 'active' | 'completed' | 'aborted'
  from_recommendation: string
}

export interface Basemap {
  url: string
  extent_m: number
  origin: [number, number]
}

export interface WorldState {
  scenario_id: string
  scenario_name: string
  seed: number
  phase: SimPhase
  sim_time_ms: number
  duration_ms: number
  bounds: { width: number; height: number }
  zones: Zone[]
  assets: Asset[]
  tracks: Track[]
  links: Link[]
  nodes: NodeInfo[]
  recommendations: Recommendation[]
  assignments: Assignment[]
  basemap: Basemap | null
}

export interface RunMetrics {
  scenario_id: string
  seed: number
  duration_ms: number
  coverage_continuity: number
  degraded_link_time_pct: number
  incursions: number
  mean_time_to_flag_ms: number | null
  recommendations_issued: number
  recommendations_approved: number
  mean_response_latency_ms: number | null
  tracks_lost: number
}

export type FaultSpec =
  | { target: 'link'; link: string; state: LinkState }
  | { target: 'node'; node: string; health: NodeHealth }

// Audit event: envelope fields + flattened DomainEvent body.
export type DomainEventBody =
  | { type: 'scenario_loaded'; scenario_id: string; name: string; seed: number }
  | { type: 'phase_changed'; phase: SimPhase }
  | { type: 'track_appeared'; track: string; class: TrackClass }
  | { type: 'track_status_changed'; track: string; status: TrackStatus }
  | { type: 'track_dropped'; track: string }
  | { type: 'zone_coverage_changed'; zone: string; covered: boolean }
  | { type: 'link_state_changed'; link: string; state: LinkState }
  | { type: 'node_health_changed'; node: string; health: NodeHealth }
  | { type: 'fault_injected'; by: string; fault: FaultSpec }
  | { type: 'recommendation_issued'; recommendation: Recommendation }
  | { type: 'recommendation_status_changed'; id: string; status: RecStatus; by: string }
  | { type: 'assignment_created'; assignment: Assignment }
  | { type: 'assignment_completed'; id: string; outcome: string }
  | { type: 'scenario_completed'; metrics: RunMetrics }

export type AuditEvent = { seq: number; sim_time_ms: number } & DomainEventBody

export interface RuntimeInfo {
  name: string
  version: string
  mode: 'simulate' | 'replay' | 'edge'
  protocol_version: number
}

export interface ScenarioInfo {
  id: string
  name: string
}

export interface RunInfo {
  run_id: number
  scenario_id: string
  seed: number
  started_at_ms: number
  completed: boolean
}

export interface ReplayStatus {
  run_id: number
  speed: number
  position_ms: number
  duration_ms: number
  playing: boolean
}

export type ServerMsg =
  | {
      kind: 'hello'
      runtime: RuntimeInfo
      scenarios: ScenarioInfo[]
      runs: RunInfo[]
      world: WorldState
      recent_events: AuditEvent[]
    }
  | { kind: 'snapshot'; world: WorldState }
  | { kind: 'event'; event: AuditEvent }
  | { kind: 'command_ack'; command_id: number }
  | { kind: 'error'; command_id: number | null; code: string; message: string }
  | ({ kind: 'replay_status' } & ReplayStatus)

export type ServerEnvelope = { v: number; seq: number } & ServerMsg

export type Command =
  | { type: 'start' }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'reset' }
  | { type: 'select_scenario'; scenario_id: string }
  | { type: 'ack_recommendation'; id: string }
  | { type: 'approve_recommendation'; id: string }
  | { type: 'decline_recommendation'; id: string }
  | { type: 'inject_fault'; fault: FaultSpec }
  | { type: 'set_speed'; multiplier: number }

export interface ClientEnvelope {
  v: number
  id: number
  command: Command
}
