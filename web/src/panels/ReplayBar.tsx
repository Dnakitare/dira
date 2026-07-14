import { fmtSimTime, useOps } from '../state/store'

const SPEEDS = [0.5, 1, 2, 4, 8]

export function ReplayBar() {
  const replay = useOps((s) => s.replay)
  const runtime = useOps((s) => s.runtime)
  const send = useOps((s) => s.send)
  if (runtime?.mode !== 'replay' || !replay) return null

  const progress = replay.duration_ms > 0 ? replay.position_ms / replay.duration_ms : 0

  return (
    <div className="replay-bar">
      <span className="chip mode-replay">REPLAY RUN {replay.run_id}</span>
      {replay.playing ? (
        <button className="btn" onClick={() => send({ type: 'pause' })}>
          PAUSE
        </button>
      ) : (
        <button className="btn primary" onClick={() => send({ type: 'resume' })}>
          PLAY
        </button>
      )}
      <button className="btn" onClick={() => send({ type: 'reset' })}>
        RESTART
      </button>
      <div className="replay-progress">
        <div className="replay-fill" style={{ width: `${progress * 100}%` }} />
      </div>
      <span className="replay-time">
        {fmtSimTime(replay.position_ms)} / {fmtSimTime(replay.duration_ms)}
      </span>
      <div className="speed-btns">
        {SPEEDS.map((s) => (
          <button
            key={s}
            className={`btn tiny ${replay.speed === s ? 'active' : ''}`}
            onClick={() => send({ type: 'set_speed', multiplier: s })}
          >
            {s}×
          </button>
        ))}
      </div>
    </div>
  )
}
