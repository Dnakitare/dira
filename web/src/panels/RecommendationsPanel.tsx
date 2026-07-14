import type { Recommendation, RunMetrics } from '../protocol/types'
import { fmtSimTime, useOps } from '../state/store'
import { describeRecKind, fmtPct } from './lib'

function RecCard({ rec, simTime }: { rec: Recommendation; simTime: number }) {
  const send = useOps((s) => s.send)
  const runtime = useOps((s) => s.runtime)
  const isReplay = runtime?.mode === 'replay'
  const open = rec.status === 'pending' || rec.status === 'acknowledged'
  const ttlTotal = rec.expires_ms - rec.created_ms
  const ttlLeft = Math.max(rec.expires_ms - simTime, 0)

  return (
    <div className={`rec-card ${open ? 'open' : 'closed'} ${rec.kind.type}`}>
      <div className="rec-head">
        <span className="rec-id">{rec.id}</span>
        <span className="chip policy">{rec.policy_id}</span>
        <span className="rec-conf">{fmtPct(rec.confidence)}</span>
      </div>
      <div className="rec-action">{describeRecKind(rec.kind)}</div>
      <div className="rec-reason">{rec.reason}</div>
      {open ? (
        <>
          <div className="ttl-bar">
            <div className="ttl-fill" style={{ width: `${(ttlLeft / ttlTotal) * 100}%` }} />
          </div>
          {!isReplay && (
            <div className="rec-btns">
              {rec.status === 'pending' && (
                <button
                  className="btn tiny"
                  onClick={() => send({ type: 'ack_recommendation', id: rec.id })}
                >
                  ACK
                </button>
              )}
              <button
                className="btn tiny approve"
                onClick={() => send({ type: 'approve_recommendation', id: rec.id })}
              >
                APPROVE
              </button>
              <button
                className="btn tiny decline"
                onClick={() => send({ type: 'decline_recommendation', id: rec.id })}
              >
                DECLINE
              </button>
            </div>
          )}
          {rec.status === 'acknowledged' && <div className="rec-status">acknowledged</div>}
        </>
      ) : (
        <div className={`rec-status ${rec.status}`}>
          {rec.status} at {fmtSimTime(rec.status_changed_ms)}
        </div>
      )}
    </div>
  )
}

function MetricsCard({ metrics }: { metrics: RunMetrics }) {
  const rows: [string, string][] = [
    ['coverage continuity', fmtPct(metrics.coverage_continuity)],
    ['degraded link time', fmtPct(metrics.degraded_link_time_pct)],
    ['incursions', String(metrics.incursions)],
    [
      'mean time to flag',
      metrics.mean_time_to_flag_ms != null
        ? `${(metrics.mean_time_to_flag_ms / 1000).toFixed(1)} s`
        : '—',
    ],
    [
      'recommendations',
      `${metrics.recommendations_approved}/${metrics.recommendations_issued} approved`,
    ],
    [
      'mean response',
      metrics.mean_response_latency_ms != null
        ? `${(metrics.mean_response_latency_ms / 1000).toFixed(1)} s`
        : '—',
    ],
    ['tracks lost', String(metrics.tracks_lost)],
  ]
  return (
    <section className="panel metrics">
      <h2 className="panel-title">RUN METRICS</h2>
      {rows.map(([k, v]) => (
        <div className="metric-row" key={k}>
          <span>{k}</span>
          <span className="metric-val">{v}</span>
        </div>
      ))}
    </section>
  )
}

export function RecommendationsPanel() {
  const world = useOps((s) => s.world)
  const events = useOps((s) => s.events)
  if (!world) return null

  const open = world.recommendations.filter(
    (r) => r.status === 'pending' || r.status === 'acknowledged',
  )
  const closed = world.recommendations
    .filter((r) => r.status !== 'pending' && r.status !== 'acknowledged')
    .slice(-6)
    .reverse()

  const completed = [...events].reverse().find((e) => e.type === 'scenario_completed')
  const metrics =
    world.phase === 'complete' && completed?.type === 'scenario_completed'
      ? completed.metrics
      : null

  const active = world.assignments.filter((a) => a.status === 'active')

  return (
    <div className="rail-scroll">
      {metrics && <MetricsCard metrics={metrics} />}
      <section className="panel">
        <h2 className="panel-title">
          RECOMMENDATIONS{open.length > 0 ? ` (${open.length})` : ''}
        </h2>
        {open.length === 0 && <div className="row-sub empty">nothing awaiting review</div>}
        {open.map((rec) => (
          <RecCard key={rec.id} rec={rec} simTime={world.sim_time_ms} />
        ))}
        {closed.length > 0 && <div className="rec-divider">RESOLVED</div>}
        {closed.map((rec) => (
          <RecCard key={rec.id} rec={rec} simTime={world.sim_time_ms} />
        ))}
      </section>

      <section className="panel">
        <h2 className="panel-title">ASSIGNMENTS</h2>
        {active.length === 0 && <div className="row-sub empty">no active assignments</div>}
        {active.map((a) => (
          <div className="row" key={a.id}>
            <div className="row-main">
              <span className="row-id friendly">{a.id}</span>
              <span className="chip busy">ACTIVE</span>
            </div>
            <div className="row-sub">
              {a.asset} ·{' '}
              {a.objective.type === 'observe_zone'
                ? `observe ${a.objective.zone}`
                : `investigate ${a.objective.track}`}{' '}
              · from {a.from_recommendation}
            </div>
          </div>
        ))}
      </section>
    </div>
  )
}
