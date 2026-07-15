// Symbology key: shape encodes domain, color encodes identity. Mirrors the
// glyphs the SceneManager draws so the picture reads without prior training.

const CY = '#3fc1e4'
const AM = '#e5b84b'
const RD = '#f0603f'

function AirGlyph({ color }: { color: string }) {
  // Arc open at the bottom (air track frame).
  return (
    <svg width="18" height="18" viewBox="-10 -10 20 20" aria-hidden>
      <path
        d="M 8.14 3.42 A 9 9 0 1 0 -8.14 3.42"
        fill="none"
        stroke={color}
        strokeWidth="1.8"
      />
    </svg>
  )
}

function GroundGlyph({ color }: { color: string }) {
  return (
    <svg width="18" height="18" viewBox="-10 -10 20 20" aria-hidden>
      <rect x="-7" y="-7" width="14" height="14" fill="none" stroke={color} strokeWidth="1.8" />
    </svg>
  )
}

function UnknownGlyph({ color }: { color: string }) {
  return (
    <svg width="18" height="18" viewBox="-10 -10 20 20" aria-hidden>
      <path d="M 0 -8 L 8 0 L 0 8 L -8 0 Z" fill="none" stroke={color} strokeWidth="1.8" />
    </svg>
  )
}

function AssetGlyph() {
  return (
    <svg width="18" height="18" viewBox="-10 -10 20 20" aria-hidden>
      <path d="M 0 -8 L 7 7 L -7 7 Z" fill="none" stroke={CY} strokeWidth="1.8" />
    </svg>
  )
}

export function Legend() {
  return (
    <div className="legend">
      <div className="legend-row">
        <AssetGlyph />
        <span>asset</span>
      </div>
      <div className="legend-row">
        <AirGlyph color={AM} />
        <span>air track</span>
      </div>
      <div className="legend-row">
        <GroundGlyph color={AM} />
        <span>ground track</span>
      </div>
      <div className="legend-row">
        <UnknownGlyph color={AM} />
        <span>unknown</span>
      </div>
      <div className="legend-row">
        <UnknownGlyph color={RD} />
        <span>flagged</span>
      </div>
    </div>
  )
}
