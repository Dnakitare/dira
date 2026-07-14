// The common operating picture. Top-down orthographic Three.js scene:
// zones, assets, tracks with motion trails and uncertainty halos. The halo
// is driven by the runtime's staleness model — when a link degrades, the
// picture visibly blooms. Pure display: all state arrives via snapshots.

import * as THREE from 'three'
import type { Asset, Track, WorldState, Zone } from '../protocol/types'
import type { Selection } from '../state/store'

export const COLORS = {
  friendly: 0x3fc1e4,
  unknown: 0xe5b84b,
  alert: 0xf0603f,
  ok: 0x57b26e,
  zone: 0x7a8ca6,
  grid: 0x141e2b,
  gridMajor: 0x1e2c3e,
  ink: 0xc6d2df,
}

const TRAIL_MAX = 160
const SYMBOL_R = 34 // meters; symbol half-size at world scale

interface TrackObj {
  group: THREE.Group
  body: THREE.LineLoop
  dot: THREE.Mesh
  halo: THREE.LineLoop
  leader: THREE.Line
  trail: THREE.Line
  trailPts: number[]
  label: THREE.Sprite
  pick: THREE.Mesh
  status: Track['status']
}

interface AssetObj {
  group: THREE.Group
  body: THREE.LineLoop
  fill: THREE.Mesh
  reach: THREE.LineLoop
  objective: THREE.Line
  label: THREE.Sprite
  pick: THREE.Mesh
}

interface ZoneObj {
  ring: THREE.LineLoop
  fill: THREE.Mesh
  label: THREE.Sprite
  kind: Zone['kind']
}

function circlePoints(radius: number, segments = 72): THREE.BufferGeometry {
  const pts: THREE.Vector3[] = []
  for (let i = 0; i < segments; i++) {
    const a = (i / segments) * Math.PI * 2
    pts.push(new THREE.Vector3(Math.cos(a) * radius, Math.sin(a) * radius, 0))
  }
  return new THREE.BufferGeometry().setFromPoints(pts)
}

function diamondGeom(r: number): THREE.BufferGeometry {
  return new THREE.BufferGeometry().setFromPoints([
    new THREE.Vector3(0, r, 0),
    new THREE.Vector3(r, 0, 0),
    new THREE.Vector3(0, -r, 0),
    new THREE.Vector3(-r, 0, 0),
  ])
}

function triangleGeom(r: number): THREE.BufferGeometry {
  return new THREE.BufferGeometry().setFromPoints([
    new THREE.Vector3(0, r * 1.2, 0),
    new THREE.Vector3(r * 0.9, -r, 0),
    new THREE.Vector3(-r * 0.9, -r, 0),
  ])
}

function textSprite(text: string, colorCss: string): THREE.Sprite {
  const pad = 8
  const font = '600 30px ui-monospace, SFMono-Regular, Menlo, monospace'
  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')!
  ctx.font = font
  const w = Math.ceil(ctx.measureText(text).width) + pad * 2
  const h = 44
  canvas.width = w
  canvas.height = h
  ctx.font = font
  ctx.fillStyle = colorCss
  ctx.textBaseline = 'middle'
  ctx.fillText(text, pad, h / 2)
  const texture = new THREE.CanvasTexture(canvas)
  texture.minFilter = THREE.LinearFilter
  const material = new THREE.SpriteMaterial({ map: texture, transparent: true, depthTest: false })
  const sprite = new THREE.Sprite(material)
  const heightMeters = 78
  sprite.scale.set((w / h) * heightMeters, heightMeters, 1)
  return sprite
}

function cssColor(hex: number): string {
  return `#${hex.toString(16).padStart(6, '0')}`
}

export class SceneManager {
  private renderer: THREE.WebGLRenderer
  private scene = new THREE.Scene()
  private camera: THREE.OrthographicCamera
  private container: HTMLElement
  private frustum = 4600
  private raf = 0
  private clock = new THREE.Clock()

  private gridGroup = new THREE.Group()
  private zoneObjs = new Map<string, ZoneObj>()
  private trackObjs = new Map<string, TrackObj>()
  private assetObjs = new Map<string, AssetObj>()
  private selectRing: THREE.LineLoop
  private scenarioKey = ''
  private lastSimTime = -1
  private world: WorldState | null = null

  private dragging = false
  private moved = 0
  private lastPointer = { x: 0, y: 0 }

  constructor(
    canvas: HTMLCanvasElement,
    private onSelect: (sel: Selection) => void,
  ) {
    this.container = canvas.parentElement!
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false })
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    this.renderer.setClearColor(0x0a0e14)
    this.camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 100)
    this.camera.position.set(0, 0, 10)
    this.scene.add(this.gridGroup)

    const ringMat = new THREE.LineBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.9 })
    this.selectRing = new THREE.LineLoop(circlePoints(SYMBOL_R * 2.2), ringMat)
    this.selectRing.visible = false
    this.selectRing.renderOrder = 10
    this.scene.add(this.selectRing)

    this.resize()
    new ResizeObserver(() => this.resize()).observe(this.container)

    canvas.addEventListener('pointerdown', this.onPointerDown)
    canvas.addEventListener('pointermove', this.onPointerMove)
    canvas.addEventListener('pointerup', this.onPointerUp)
    canvas.addEventListener('wheel', this.onWheel, { passive: false })

    const loop = () => {
      this.raf = requestAnimationFrame(loop)
      this.pulse()
      this.renderer.render(this.scene, this.camera)
    }
    loop()
  }

  dispose(): void {
    cancelAnimationFrame(this.raf)
    this.renderer.dispose()
  }

  private resize(): void {
    const w = this.container.clientWidth
    const h = this.container.clientHeight
    if (w === 0 || h === 0) return
    this.renderer.setSize(w, h, false)
    const aspect = w / h
    this.camera.left = (-this.frustum * aspect) / 2
    this.camera.right = (this.frustum * aspect) / 2
    this.camera.top = this.frustum / 2
    this.camera.bottom = -this.frustum / 2
    this.camera.updateProjectionMatrix()
  }

  // --- interaction -----------------------------------------------------

  private onPointerDown = (e: PointerEvent) => {
    this.dragging = true
    this.moved = 0
    this.lastPointer = { x: e.clientX, y: e.clientY }
  }

  private onPointerMove = (e: PointerEvent) => {
    if (!this.dragging) return
    const dx = e.clientX - this.lastPointer.x
    const dy = e.clientY - this.lastPointer.y
    this.moved += Math.abs(dx) + Math.abs(dy)
    this.lastPointer = { x: e.clientX, y: e.clientY }
    const metersPerPx =
      (this.camera.right - this.camera.left) / (this.camera.zoom * this.container.clientWidth)
    this.camera.position.x -= dx * metersPerPx
    this.camera.position.y += dy * metersPerPx
  }

  private onPointerUp = (e: PointerEvent) => {
    this.dragging = false
    if (this.moved > 6) return
    const rect = this.renderer.domElement.getBoundingClientRect()
    const ndc = new THREE.Vector2(
      ((e.clientX - rect.left) / rect.width) * 2 - 1,
      -((e.clientY - rect.top) / rect.height) * 2 + 1,
    )
    const raycaster = new THREE.Raycaster()
    raycaster.setFromCamera(ndc, this.camera)
    const picks: THREE.Object3D[] = []
    this.trackObjs.forEach((o) => picks.push(o.pick))
    this.assetObjs.forEach((o) => picks.push(o.pick))
    const hits = raycaster.intersectObjects(picks, false)
    if (hits.length > 0) {
      const { kind, id } = hits[0].object.userData as { kind: 'track' | 'asset'; id: string }
      this.onSelect({ kind, id })
    } else {
      this.onSelect(null)
    }
  }

  private onWheel = (e: WheelEvent) => {
    e.preventDefault()
    const next = this.camera.zoom * Math.exp(-e.deltaY * 0.0012)
    this.camera.zoom = Math.min(Math.max(next, 0.4), 18)
    this.camera.updateProjectionMatrix()
  }

  // --- world sync ------------------------------------------------------

  updateWorld(world: WorldState): void {
    this.world = world
    const key = `${world.scenario_id}:${world.seed}`
    if (key !== this.scenarioKey || world.sim_time_ms < this.lastSimTime) {
      if (key !== this.scenarioKey) {
        this.scenarioKey = key
        this.buildStatic(world)
      }
      this.trackObjs.forEach((o) => {
        o.trailPts = []
      })
    }
    this.lastSimTime = world.sim_time_ms

    this.syncZones(world)
    this.syncTracks(world)
    this.syncAssets(world)
  }

  setSelection(sel: Selection): void {
    if (!sel || sel.kind === 'zone') {
      this.selectRing.visible = false
      return
    }
    const obj =
      sel.kind === 'track' ? this.trackObjs.get(sel.id)?.group : this.assetObjs.get(sel.id)?.group
    if (obj) {
      this.selectRing.visible = true
      this.selectRing.position.copy(obj.position)
    } else {
      this.selectRing.visible = false
    }
  }

  private buildStatic(world: WorldState): void {
    this.gridGroup.clear()
    this.zoneObjs.forEach((z) => {
      this.scene.remove(z.ring, z.fill, z.label)
    })
    this.zoneObjs.clear()
    this.trackObjs.forEach((t) => this.scene.remove(t.group))
    this.trackObjs.clear()
    this.assetObjs.forEach((a) => this.scene.remove(a.group))
    this.assetObjs.clear()

    const { width, height } = world.bounds
    this.frustum = Math.max(width, height) * 1.15
    this.resize()
    this.camera.position.set(0, 0, 10)
    this.camera.zoom = 1
    this.camera.updateProjectionMatrix()

    // Graticule: minor every 250 m, major every 1000 m.
    const minor: THREE.Vector3[] = []
    const major: THREE.Vector3[] = []
    const hw = width / 2
    const hh = height / 2
    for (let x = -hw; x <= hw; x += 250) {
      const bucket = x % 1000 === 0 ? major : minor
      bucket.push(new THREE.Vector3(x, -hh, -1), new THREE.Vector3(x, hh, -1))
    }
    for (let y = -hh; y <= hh; y += 250) {
      const bucket = y % 1000 === 0 ? major : minor
      bucket.push(new THREE.Vector3(-hw, y, -1), new THREE.Vector3(hw, y, -1))
    }
    this.gridGroup.add(
      new THREE.LineSegments(
        new THREE.BufferGeometry().setFromPoints(minor),
        new THREE.LineBasicMaterial({ color: COLORS.grid }),
      ),
      new THREE.LineSegments(
        new THREE.BufferGeometry().setFromPoints(major),
        new THREE.LineBasicMaterial({ color: COLORS.gridMajor }),
      ),
    )
    // Boundary frame.
    const frame = new THREE.LineLoop(
      new THREE.BufferGeometry().setFromPoints([
        new THREE.Vector3(-hw, -hh, -1),
        new THREE.Vector3(hw, -hh, -1),
        new THREE.Vector3(hw, hh, -1),
        new THREE.Vector3(-hw, hh, -1),
      ]),
      new THREE.LineBasicMaterial({ color: COLORS.gridMajor }),
    )
    this.gridGroup.add(frame)
  }

  private syncZones(world: WorldState): void {
    for (const zone of world.zones) {
      let obj = this.zoneObjs.get(zone.id)
      if (!obj) {
        const ring = new THREE.LineLoop(
          circlePoints(zone.radius_m, 96),
          new THREE.LineBasicMaterial({ color: COLORS.zone, transparent: true, opacity: 0.9 }),
        )
        const fill = new THREE.Mesh(
          new THREE.CircleGeometry(zone.radius_m, 96),
          new THREE.MeshBasicMaterial({ color: COLORS.zone, transparent: true, opacity: 0.05 }),
        )
        fill.position.z = -0.5
        const label = textSprite(zone.name.toUpperCase(), cssColor(COLORS.zone))
        label.position.set(zone.center.x, zone.center.y + zone.radius_m + 90, 1)
        ring.position.set(zone.center.x, zone.center.y, 0)
        obj = { ring, fill, label, kind: zone.kind }
        fill.position.x = zone.center.x
        fill.position.y = zone.center.y
        this.scene.add(ring, fill, label)
        this.zoneObjs.set(zone.id, obj)
      }
      const ringMat = obj.ring.material as THREE.LineBasicMaterial
      const fillMat = obj.fill.material as THREE.MeshBasicMaterial
      if (zone.kind === 'protected') {
        const color = zone.covered ? COLORS.ok : COLORS.alert
        ringMat.color.setHex(color)
        fillMat.color.setHex(color)
        fillMat.opacity = zone.covered ? 0.04 : 0.07
      }
    }
  }

  private syncTracks(world: WorldState): void {
    const seen = new Set<string>()
    for (const track of world.tracks) {
      seen.add(track.id)
      let obj = this.trackObjs.get(track.id)
      if (!obj) {
        obj = this.buildTrack(track)
        this.trackObjs.set(track.id, obj)
      }
      this.updateTrack(obj, track)
    }
    for (const [id, obj] of this.trackObjs) {
      if (!seen.has(id)) {
        this.scene.remove(obj.group)
        this.trackObjs.delete(id)
      }
    }
  }

  private buildTrack(track: Track): TrackObj {
    const group = new THREE.Group()
    const color = track.flagged ? COLORS.alert : COLORS.unknown
    const bodyMat = new THREE.LineBasicMaterial({ color, transparent: true })
    const body = new THREE.LineLoop(diamondGeom(SYMBOL_R), bodyMat)
    const dot = new THREE.Mesh(
      new THREE.CircleGeometry(7, 16),
      new THREE.MeshBasicMaterial({ color, transparent: true }),
    )
    const halo = new THREE.LineLoop(
      circlePoints(1, 64),
      new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.4 }),
    )
    const leader = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3()]),
      new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.7 }),
    )
    const trailGeom = new THREE.BufferGeometry()
    trailGeom.setAttribute('position', new THREE.BufferAttribute(new Float32Array(TRAIL_MAX * 3), 3))
    trailGeom.setDrawRange(0, 0)
    const trail = new THREE.Line(
      trailGeom,
      new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.3 }),
    )
    const label = textSprite(track.id.toUpperCase(), cssColor(color))
    label.position.set(SYMBOL_R * 2.4, SYMBOL_R * 2.0, 1)
    const pick = new THREE.Mesh(
      new THREE.CircleGeometry(110, 12),
      new THREE.MeshBasicMaterial({ transparent: true, opacity: 0, depthWrite: false }),
    )
    pick.userData = { kind: 'track', id: track.id }
    group.add(body, dot, halo, leader, label, pick)
    this.scene.add(group)
    this.scene.add(trail)
    return { group, body, dot, halo, leader, trail, trailPts: [], label, pick, status: track.status }
  }

  private updateTrack(obj: TrackObj, track: Track): void {
    obj.group.position.set(track.pos.x, track.pos.y, 2)
    obj.status = track.status
    const color = track.flagged ? COLORS.alert : COLORS.unknown
    ;[obj.body, obj.halo, obj.leader, obj.trail].forEach((line) => {
      ;(line.material as THREE.LineBasicMaterial).color.setHex(color)
    })
    ;(obj.dot.material as THREE.MeshBasicMaterial).color.setHex(color)

    const alpha = track.status === 'active' ? 1 : track.status === 'stale' ? 0.55 : 0.25
    ;(obj.body.material as THREE.LineBasicMaterial).opacity = alpha
    ;(obj.dot.material as THREE.MeshBasicMaterial).opacity = alpha
    ;(obj.leader.material as THREE.LineBasicMaterial).opacity = 0.7 * alpha

    // The data-age halo: radius = uncertainty.
    obj.halo.scale.setScalar(Math.max(track.uncertainty_m, 1))

    // Velocity leader: 20 s projection.
    const lead = obj.leader.geometry.getAttribute('position') as THREE.BufferAttribute
    lead.setXYZ(0, 0, 0, 0)
    lead.setXYZ(1, track.vel.x * 20, track.vel.y * 20, 0)
    lead.needsUpdate = true

    // Trail.
    obj.trailPts.push(track.pos.x, track.pos.y, 0.5)
    if (obj.trailPts.length > TRAIL_MAX * 3) {
      obj.trailPts.splice(0, obj.trailPts.length - TRAIL_MAX * 3)
    }
    const pos = obj.trail.geometry.getAttribute('position') as THREE.BufferAttribute
    for (let i = 0; i < obj.trailPts.length; i++) {
      pos.array[i] = obj.trailPts[i]
    }
    pos.needsUpdate = true
    obj.trail.geometry.setDrawRange(0, obj.trailPts.length / 3)
  }

  private syncAssets(world: WorldState): void {
    const seen = new Set<string>()
    for (const asset of world.assets) {
      seen.add(asset.id)
      let obj = this.assetObjs.get(asset.id)
      if (!obj) {
        obj = this.buildAsset(asset)
        this.assetObjs.set(asset.id, obj)
      }
      this.updateAsset(obj, asset, world)
    }
    for (const [id, obj] of this.assetObjs) {
      if (!seen.has(id)) {
        this.scene.remove(obj.group)
        this.assetObjs.delete(id)
      }
    }
  }

  private buildAsset(asset: Asset): AssetObj {
    const group = new THREE.Group()
    const body = new THREE.LineLoop(
      triangleGeom(SYMBOL_R),
      new THREE.LineBasicMaterial({ color: COLORS.friendly, transparent: true }),
    )
    const fill = new THREE.Mesh(
      new THREE.ShapeGeometry(
        new THREE.Shape([
          new THREE.Vector2(0, SYMBOL_R * 1.2),
          new THREE.Vector2(SYMBOL_R * 0.9, -SYMBOL_R),
          new THREE.Vector2(-SYMBOL_R * 0.9, -SYMBOL_R),
        ]),
      ),
      new THREE.MeshBasicMaterial({ color: COLORS.friendly, transparent: true, opacity: 0.25 }),
    )
    const reach = new THREE.LineLoop(
      circlePoints(asset.observe_radius_m, 72),
      new THREE.LineBasicMaterial({ color: COLORS.friendly, transparent: true, opacity: 0.12 }),
    )
    const objective = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3()]),
      new THREE.LineBasicMaterial({ color: COLORS.friendly, transparent: true, opacity: 0.35 }),
    )
    objective.visible = false
    const label = textSprite(asset.id.toUpperCase(), cssColor(COLORS.friendly))
    label.position.set(SYMBOL_R * 2.4, -SYMBOL_R * 2.0, 1)
    const pick = new THREE.Mesh(
      new THREE.CircleGeometry(110, 12),
      new THREE.MeshBasicMaterial({ transparent: true, opacity: 0, depthWrite: false }),
    )
    pick.userData = { kind: 'asset', id: asset.id }
    group.add(body, fill, reach, label, pick)
    this.scene.add(group)
    this.scene.add(objective)
    return { group, body, fill, reach, objective, label, pick }
  }

  private updateAsset(obj: AssetObj, asset: Asset, world: WorldState): void {
    obj.group.position.set(asset.pos.x, asset.pos.y, 3)
    const speed = Math.hypot(asset.vel.x, asset.vel.y)
    if (speed > 0.1) {
      const heading = Math.atan2(asset.vel.y, asset.vel.x) - Math.PI / 2
      obj.body.rotation.z = heading
      obj.fill.rotation.z = heading
    }
    const busy = asset.status === 'enroute' || asset.status === 'investigating'
    ;(obj.fill.material as THREE.MeshBasicMaterial).opacity =
      asset.status === 'unavailable' ? 0.08 : busy ? 0.5 : 0.25

    // Line to the current objective, if any.
    const assignment = world.assignments.find(
      (a) => a.id === asset.assignment && a.status === 'active',
    )
    let target: { x: number; y: number } | null = null
    if (assignment) {
      if (assignment.objective.type === 'observe_zone') {
        const zone = world.zones.find((z) => z.id === (assignment.objective as any).zone)
        if (zone) target = zone.center
      } else {
        const track = world.tracks.find((t) => t.id === (assignment.objective as any).track)
        if (track) target = track.pos
      }
    }
    if (target) {
      obj.objective.visible = true
      const pos = obj.objective.geometry.getAttribute('position') as THREE.BufferAttribute
      pos.setXYZ(0, asset.pos.x, asset.pos.y, 0.5)
      pos.setXYZ(1, target.x, target.y, 0.5)
      pos.needsUpdate = true
    } else {
      obj.objective.visible = false
    }
  }

  /** Subtle breathing on stale/lost halos, run from the render loop. */
  private pulse(): void {
    if (!this.world) return
    const t = this.clock.getElapsedTime()
    const beat = 0.55 + 0.35 * Math.sin(t * 2.6)
    this.trackObjs.forEach((obj) => {
      const mat = obj.halo.material as THREE.LineBasicMaterial
      mat.opacity = obj.status === 'active' ? 0.35 : beat * (obj.status === 'stale' ? 0.8 : 0.5)
    })
    if (this.selectRing.visible) {
      ;(this.selectRing.material as THREE.LineBasicMaterial).opacity = 0.5 + 0.4 * Math.sin(t * 4)
    }
  }
}
