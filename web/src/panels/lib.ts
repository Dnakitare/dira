import type { AuditEvent, RecKind } from '../protocol/types'
import { fmtLinkState } from '../state/store'

export function describeRecKind(kind: RecKind): string {
  switch (kind.type) {
    case 'assign_observation':
      return `assign ${kind.asset} to observe ${kind.zone}`
    case 'investigate_track':
      return `send ${kind.asset} to investigate ${kind.track}`
    case 'flag_track':
      return `flag ${kind.track} near ${kind.zone}`
    case 'comms_check':
      return `check link ${kind.link}`
  }
}

export type Severity = 'alert' | 'ok' | 'neutral'

export function describeEvent(e: AuditEvent): { text: string; severity: Severity } {
  switch (e.type) {
    case 'scenario_loaded':
      return { text: `scenario ${e.scenario_id} loaded, seed ${e.seed}`, severity: 'neutral' }
    case 'phase_changed':
      return { text: `phase → ${e.phase}`, severity: 'neutral' }
    case 'track_appeared':
      return { text: `${e.track} acquired (${e.class})`, severity: 'neutral' }
    case 'track_status_changed':
      return {
        text: `${e.track} → ${e.status}`,
        severity: e.status === 'active' ? 'ok' : 'alert',
      }
    case 'track_dropped':
      return { text: `${e.track} dropped from picture`, severity: 'alert' }
    case 'zone_coverage_changed':
      return {
        text: `${e.zone} coverage ${e.covered ? 'restored' : 'lost'}`,
        severity: e.covered ? 'ok' : 'alert',
      }
    case 'link_state_changed':
      return {
        text: `${e.link} → ${fmtLinkState(e.state)}`,
        severity: e.state.state === 'nominal' ? 'ok' : 'alert',
      }
    case 'node_health_changed':
      return {
        text: `${e.node} health ${e.health}`,
        severity: e.health === 'nominal' ? 'ok' : 'alert',
      }
    case 'fault_injected': {
      const what =
        e.fault.target === 'link'
          ? `${e.fault.link} ${fmtLinkState(e.fault.state)}`
          : `${e.fault.node} ${e.fault.health}`
      return { text: `fault (${e.by}): ${what}`, severity: 'alert' }
    }
    case 'recommendation_issued':
      return {
        text: `${e.recommendation.id} ${describeRecKind(e.recommendation.kind)}`,
        severity: e.recommendation.kind.type === 'flag_track' ? 'alert' : 'neutral',
      }
    case 'recommendation_status_changed':
      return {
        text: `${e.id} ${e.status} (${e.by})`,
        severity: e.status === 'approved' ? 'ok' : 'neutral',
      }
    case 'assignment_created':
      return {
        text: `${e.assignment.id}: ${e.assignment.asset} tasked`,
        severity: 'ok',
      }
    case 'assignment_completed':
      return { text: `${e.id} complete: ${e.outcome}`, severity: 'ok' }
    case 'scenario_completed':
      return { text: `scenario complete`, severity: 'neutral' }
  }
}

export function fmtPct(x: number): string {
  return `${(x * 100).toFixed(1)}%`
}

export function fmtDateTime(ms: number): string {
  const d = new Date(ms)
  return d.toLocaleString(undefined, {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}
