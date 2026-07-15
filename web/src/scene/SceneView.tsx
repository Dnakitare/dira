import { useEffect, useRef } from 'react'
import { SceneManager } from './SceneManager'
import { useOps } from '../state/store'
import { SelectionPanel } from '../panels/SelectionPanel'
import { Legend } from '../panels/Legend'

export function SceneView() {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const connected = useOps((s) => s.connected)
  const basemap = useOps((s) => s.world?.basemap ?? null)

  useEffect(() => {
    const mgr = new SceneManager(canvasRef.current!, (sel) => useOps.getState().select(sel))
    const world = useOps.getState().world
    if (world) mgr.updateWorld(world)
    const unsub = useOps.subscribe((state, prev) => {
      if (state.world && state.world !== prev.world) mgr.updateWorld(state.world)
      if (state.selection !== prev.selection) mgr.setSelection(state.selection)
    })
    return () => {
      unsub()
      mgr.dispose()
    }
  }, [])

  return (
    <div className="scene-wrap">
      <canvas ref={canvasRef} />
      <div className="scene-watermark">SYNTHETIC DATA — SIMULATION</div>
      {basemap && (
        <div className="scene-attribution">© OpenStreetMap contributors · Protomaps</div>
      )}
      <Legend />
      <SelectionPanel />
      {!connected && (
        <div className="link-lost">
          <div className="link-lost-title">RUNTIME LINK LOST</div>
          <div className="link-lost-sub">
            reconnecting… the edge runtime continues independently; the picture will
            resynchronize on reconnect
          </div>
        </div>
      )}
    </div>
  )
}
