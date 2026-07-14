import { fmtSimTime, useOps } from '../state/store'
import { describeEvent } from './lib'

export function TimelinePanel() {
  const events = useOps((s) => s.events)
  return (
    <div className="timeline">
      <div className="panel-title timeline-title">EVENT TIMELINE</div>
      <div className="timeline-list">
        {events.length === 0 && <div className="row-sub empty">no events yet</div>}
        {[...events].reverse().map((e) => {
          const { text, severity } = describeEvent(e)
          return (
            <div className={`tl-row ${severity}`} key={e.seq}>
              <span className="tl-time">{fmtSimTime(e.sim_time_ms)}</span>
              <span className="tl-type">{e.type.replace(/_/g, ' ')}</span>
              <span className="tl-text">{text}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
