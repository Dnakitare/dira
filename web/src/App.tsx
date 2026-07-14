import { useEffect } from 'react'
import { connect } from './net/socket'
import { useOps } from './state/store'
import { SceneView } from './scene/SceneView'
import { TopBar } from './panels/TopBar'
import { HealthPanel } from './panels/HealthPanel'
import { RecommendationsPanel } from './panels/RecommendationsPanel'
import { TimelinePanel } from './panels/TimelinePanel'
import { ReplayBar } from './panels/ReplayBar'

function Toasts() {
  const toasts = useOps((s) => s.toasts)
  const dismiss = useOps((s) => s.dismissToast)
  useEffect(() => {
    if (toasts.length === 0) return
    const t = setTimeout(() => dismiss(toasts[0].id), 6000)
    return () => clearTimeout(t)
  }, [toasts, dismiss])
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`} onClick={() => dismiss(t.id)}>
          {t.text}
        </div>
      ))}
    </div>
  )
}

export default function App() {
  useEffect(() => {
    connect()
  }, [])

  return (
    <div className="app">
      <TopBar />
      <aside className="rail rail-left">
        <HealthPanel />
      </aside>
      <main className="scene-cell">
        <SceneView />
      </main>
      <aside className="rail rail-right">
        <RecommendationsPanel />
      </aside>
      <footer className="bottom">
        <ReplayBar />
        <TimelinePanel />
      </footer>
      <Toasts />
    </div>
  )
}
