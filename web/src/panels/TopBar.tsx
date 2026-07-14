import { fmtSimTime, useOps } from '../state/store'

export function TopBar() {
  const world = useOps((s) => s.world)
  const runtime = useOps((s) => s.runtime)
  const connected = useOps((s) => s.connected)
  const scenarios = useOps((s) => s.scenarios)
  const send = useOps((s) => s.send)

  const phase = world?.phase ?? 'idle'
  const mode = runtime?.mode ?? '—'
  const isReplay = mode === 'replay'

  return (
    <header className="topbar">
      <div className="topbar-left">
        <span className="wordmark">DIRA</span>
        <span className={`chip mode-${mode}`}>{mode.toUpperCase()}</span>
        <span className="scenario-name">{world?.scenario_name ?? 'no scenario'}</span>
        {!isReplay && scenarios.length > 1 && (
          <select
            className="scenario-select"
            value={world?.scenario_id ?? ''}
            onChange={(e) => send({ type: 'select_scenario', scenario_id: e.target.value })}
          >
            {scenarios.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        )}
      </div>

      <div className="topbar-center">
        <span className="sim-clock">{world ? fmtSimTime(world.sim_time_ms) : '—'}</span>
        <span className={`chip phase-${phase}`}>{phase.toUpperCase()}</span>
      </div>

      <div className="topbar-right">
        {!isReplay && (
          <>
            {phase === 'idle' && (
              <button className="btn primary" onClick={() => send({ type: 'start' })}>
                START
              </button>
            )}
            {phase === 'running' && (
              <button className="btn" onClick={() => send({ type: 'pause' })}>
                PAUSE
              </button>
            )}
            {phase === 'paused' && (
              <button className="btn primary" onClick={() => send({ type: 'resume' })}>
                RESUME
              </button>
            )}
            <button className="btn" onClick={() => send({ type: 'reset' })}>
              RESET
            </button>
          </>
        )}
        <span className={`conn ${connected ? 'up' : 'down'}`}>
          <span className="conn-dot" />
          {connected ? 'LINK' : 'NO LINK'}
        </span>
        <span className="chip sim-badge">SIMULATION</span>
      </div>
    </header>
  )
}
