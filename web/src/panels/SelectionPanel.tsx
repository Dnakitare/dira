import { fmtSimTime, useOps } from '../state/store'

export function SelectionPanel() {
  const selection = useOps((s) => s.selection)
  const world = useOps((s) => s.world)
  const select = useOps((s) => s.select)
  if (!selection || !world) return null

  let rows: [string, string][] = []
  let title = ''
  let flavor = ''

  if (selection.kind === 'track') {
    const t = world.tracks.find((x) => x.id === selection.id)
    if (!t) return null
    title = t.id.toUpperCase()
    flavor = t.flagged ? 'alert' : 'unknown'
    rows = [
      ['class', t.class],
      ['status', t.status],
      ['speed', `${Math.round(Math.hypot(t.vel.x, t.vel.y))} m/s`],
      ['uncertainty', `±${Math.round(t.uncertainty_m)} m`],
      ['last seen', fmtSimTime(t.last_seen_ms)],
      ['via link', t.via_link],
      ['flagged', t.flagged ? 'yes' : 'no'],
    ]
  } else if (selection.kind === 'asset') {
    const a = world.assets.find((x) => x.id === selection.id)
    if (!a) return null
    title = a.id.toUpperCase()
    flavor = 'friendly'
    rows = [
      ['kind', a.kind],
      ['status', a.status],
      ['speed', `${Math.round(Math.hypot(a.vel.x, a.vel.y))} / ${Math.round(a.speed_mps)} m/s`],
      ['observe radius', `${Math.round(a.observe_radius_m)} m`],
      ['assignment', a.assignment ?? '—'],
    ]
  }

  return (
    <div className={`selection-card ${flavor}`}>
      <div className="selection-head">
        <span>{title}</span>
        <button className="btn tiny" onClick={() => select(null)}>
          ✕
        </button>
      </div>
      {rows.map(([k, v]) => (
        <div className="metric-row" key={k}>
          <span>{k}</span>
          <span className="metric-val">{v}</span>
        </div>
      ))}
    </div>
  )
}
