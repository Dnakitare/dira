import type { LinkState } from '../protocol/types'
import { fmtLinkState, fmtSimTime, useOps } from '../state/store'

const LINK_FAULTS: { label: string; state: LinkState }[] = [
  { label: 'NOM', state: { state: 'nominal' } },
  { label: 'DLY', state: { state: 'delayed', delay_ms: 1500 } },
  { label: 'INT', state: { state: 'intermittent', loss: 0.5 } },
  { label: 'OFF', state: { state: 'unavailable' } },
]

function linkClass(state: LinkState): string {
  switch (state.state) {
    case 'nominal':
      return 'ok'
    case 'unavailable':
      return 'alert'
    default:
      return 'warn'
  }
}

export function HealthPanel() {
  const world = useOps((s) => s.world)
  const runtime = useOps((s) => s.runtime)
  const send = useOps((s) => s.send)
  const select = useOps((s) => s.select)
  if (!world) return null
  const isReplay = runtime?.mode === 'replay'

  return (
    <div className="rail-scroll">
      <section className="panel">
        <h2 className="panel-title">LINKS</h2>
        {world.links.map((link) => (
          <div key={link.id} className="row">
            <div className="row-main">
              <span className="row-id">{link.id}</span>
              <span className={`chip ${linkClass(link.state)}`}>{fmtLinkState(link.state)}</span>
            </div>
            <div className="row-sub">
              {link.name} · since {fmtSimTime(link.since_ms)}
            </div>
            {!isReplay && (
              <div className="fault-btns">
                {LINK_FAULTS.map((f) => (
                  <button
                    key={f.label}
                    className={`btn tiny ${f.state.state === link.state.state ? 'active' : ''}`}
                    title={`set ${link.id} ${f.label}`}
                    onClick={() =>
                      send({
                        type: 'inject_fault',
                        fault: { target: 'link', link: link.id, state: f.state },
                      })
                    }
                  >
                    {f.label}
                  </button>
                ))}
              </div>
            )}
          </div>
        ))}
      </section>

      <section className="panel">
        <h2 className="panel-title">NODES</h2>
        {world.nodes.map((node) => (
          <div key={node.id} className="row">
            <div className="row-main">
              <span className="row-id">{node.id}</span>
              <span
                className={`chip ${
                  node.health === 'nominal' ? 'ok' : node.health === 'degraded' ? 'warn' : 'alert'
                }`}
              >
                {node.health.toUpperCase()}
              </span>
            </div>
            <div className="row-sub">{node.name}</div>
            {!isReplay && (
              <div className="fault-btns">
                {(['nominal', 'degraded', 'unavailable'] as const).map((h) => (
                  <button
                    key={h}
                    className={`btn tiny ${node.health === h ? 'active' : ''}`}
                    onClick={() =>
                      send({
                        type: 'inject_fault',
                        fault: { target: 'node', node: node.id, health: h },
                      })
                    }
                  >
                    {h.slice(0, 3).toUpperCase()}
                  </button>
                ))}
              </div>
            )}
          </div>
        ))}
      </section>

      <section className="panel">
        <h2 className="panel-title">ASSETS</h2>
        {world.assets.map((asset) => (
          <div
            key={asset.id}
            className="row clickable"
            onClick={() => select({ kind: 'asset', id: asset.id })}
          >
            <div className="row-main">
              <span className="row-id friendly">{asset.id}</span>
              <span className={`chip ${asset.status === 'available' ? 'ok' : 'busy'}`}>
                {asset.status.toUpperCase()}
              </span>
            </div>
            <div className="row-sub">
              {asset.kind} · {Math.round(asset.speed_mps)} m/s
              {asset.assignment ? ` · ${asset.assignment}` : ''}
            </div>
          </div>
        ))}
      </section>

      <section className="panel">
        <h2 className="panel-title">TRACKS</h2>
        {world.tracks.length === 0 && <div className="row-sub empty">no tracks in picture</div>}
        {world.tracks.map((track) => (
          <div
            key={track.id}
            className="row clickable"
            onClick={() => select({ kind: 'track', id: track.id })}
          >
            <div className="row-main">
              <span className={`row-id ${track.flagged ? 'alert' : 'unknown'}`}>{track.id}</span>
              <span
                className={`chip ${
                  track.status === 'active' ? 'ok' : track.status === 'stale' ? 'warn' : 'alert'
                }`}
              >
                {track.status.toUpperCase()}
              </span>
            </div>
            <div className="row-sub">
              {track.class} · ±{Math.round(track.uncertainty_m)} m · via {track.via_link}
              {track.flagged ? ' · FLAGGED' : ''}
            </div>
          </div>
        ))}
      </section>
    </div>
  )
}
