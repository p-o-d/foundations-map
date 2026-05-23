# 3D Icon Set Generation — Agent Prompt

**Use:** paste the block below verbatim as the first message to a fresh AI agent
session. The agent returns 14 SVG icons for the 3D sector view.

**Output handling:** translate returned SVGs into `egui::Painter` calls in
`crates/map-app/src/renderer/icons.rs`. Stationary subtypes use the existing
`draw_station_frame` (square). Ships drop their frame entirely.

---

```
You are designing a 14-icon set for the 3D sector view of an X4 Foundations
universe-map app (Rust + egui). Icons are drawn as vector primitives — no
raster, no fonts. Theme: SPACE. Every icon must feel like sci-fi space tech,
not earthly imagery. Deliver SVG snippets I will port into egui paint calls.

================================================================================
CATEGORIES + FRAME RULES
================================================================================

A. STATIONARY (10 subtypes) → SOLID SQUARE FRAME, faction-coloured stroke.
   Frame is drawn programmatically — you design the INNER GLYPH only.

B. SHIPS (4 subtypes) → NO FRAME. Glyph alone. Faction colour goes inside the
   glyph as fill or accent.

Reason ships drop the frame: dozens per sector — frames clutter. Stationary
objects are sparser; frame helps mark "built/anchored structure".

================================================================================
THE 14 SUBTYPES (space-themed concepts)
================================================================================

STATIONARY (square frame):

 1. Factory         — orbital industry: solar panels, reactor stacks, refinery
                      tanks, antenna arrays.
 2. WharfShipyard   — open drydock: scaffolding gantry cradling a partial hull,
                      construction arms.
 3. Defense         — battle platform: turret cluster, missile silo, shield
                      emitters, gun nacelles.
 4. Trading         — commerce hub: credit chip, holo cargo manifest, trade-
                      route node. AVOID earthly coins or merchant scales.
 5. EquipmentDock   — modular docking bay: ring with attachment clamps, fuel
                      gantry, repair arm. AVOID wrench.
 6. HQ              — command spire: antenna tower, control bridge, capital
                      seat with broadcast dish.
 7. PlayerStation   — claimed orbital: sigil, signal beacon, owner mark.
                      Renders with WHITE frame, so glyph must read on white.
 8. GenericStation  — neutral hab module: ringed orbital, undefined silhouette,
                      placeholder feel.
 9. Anomaly         — warp distortion / singularity: radiating star, gravity
                      ripple, energy lattice.
10. ResourceZone    — asteroid belt or ice field: clustered irregular rocks /
                      crystalline chunks at varying sizes.

SHIPS (no frame, top-down silhouettes):

11. Capital     — dreadnought / carrier: long imposing hull, multiple gun pods
                  or hangar bays, heavy mass.
12. Medium      — corvette / frigate: mid-length armed hull, fewer modules,
                  agile profile.
13. Small       — fighter / interceptor: tiny dart, single cockpit pod, swept
                  wings or thrusters.
14. Transport   — freighter: long spine carrying detached cargo containers /
                  pods, non-combat profile.

================================================================================
TECHNICAL CONSTRAINTS
================================================================================

Render size:
  - Normal:   22 × 22 px
  - Selected: 30 × 30 px

Design grid: 16 × 16 units, coords -8..+8 centred at (0,0). Runtime applies
`s = half/8.0` (half = 11 normal, 15 selected). All coords + stroke widths
multiply by s.
  - Coord `4` → 5.5 px from centre normal, 7.5 px selected.
  - Stroke `1.6` → 2.2 px normal, 3.0 px selected.

STATIONARY inner glyph: fit within roughly ±5..±6 from centre so it doesn't
touch the frame (which sits at ±8). SHIP glyph: use the full ±8 canvas.

Colors:
  - White         #ffffff — primary glyph fill
  - Gold          #cfa84b — faction-colour placeholder (swaps at runtime to
                            real faction RGB). Use for frame strokes + any
                            faction-tinted glyph accents.
  - Grey          #8c8c8c — static/unowned things (anomaly, resource zone).
  - Background    #0c1018 — preview against this; do NOT include in output.

Glyph stroke widths in design units: 1.4..1.8.

Egui primitives (use SVG equivalents):
  <rect>          → painter.rect_filled / rect_stroke
  <circle>        → painter.circle_filled / circle_stroke
  <polygon>       → Shape::convex_polygon  (CONVEX only)
  <path d="M..Z"> → Shape::Path           (CONCAVE allowed — stars, hulls,
                                            stepped silhouettes)
  <line>          → painter.line_segment

Output concave shapes as <path>, never <polygon>.

UNSUPPORTED: gradients, blur, drop-shadows, partial opacity, text, raster,
masks, clip-paths. Pure solid-color shapes only.

================================================================================
STYLE GUIDELINES
================================================================================

1. SPACE THEMATICS. Reach for sci-fi vocabulary: antenna arrays, hull plating,
   gantries, modular pods, thrusters, beacons, scaffolding, energy ripples,
   asteroid clusters. Avoid medieval (shields, crowns, scrolls) and avoid
   earthly commerce (literal coins, scales, marketplaces).

2. DO NOT copy X4's in-game icons. The game uses generic upward chevrons (▲)
   for ships and undecorated boxes for stations. Be different.

3. Minimalist + iconic. At 22 px you have ~12 × 12 px of usable space inside
   the frame. Each icon = a unique silhouette readable at a squint.

4. All 14 must be visually distinct. Especially the 4 ships — they share no
   frame, so silhouettes must diverge clearly. Test: ships side by side at
   22 px must be unmistakable.

5. Pick a single visual language and hold it across all 14:
   - filled silhouettes vs hollow line-art
   - sharp corners vs rounded
   - symbolic vs literal

6. Ships read as VESSELS without a frame — directional or elongated hull
   silhouettes. Capital = imposing/long, Small = tiny/sharp, Transport =
   spine + detached pods, Medium = mid-scale armed.

================================================================================
OUTPUT FORMAT — EXACTLY THIS
================================================================================

For each of the 14 icons:

ICON: <subtype>
CATEGORY: stationary | ship
ONE-LINE CONCEPT: <≤10 words>
SVG:
<svg width="32" height="32" viewBox="-8 -8 16 16" xmlns="http://www.w3.org/2000/svg">
  <!-- STATIONARY ONLY: include frame flagged so I skip it at port time -->
  <rect data-frame="true" x="-8" y="-8" width="16" height="16"
        fill="none" stroke="#cfa84b" stroke-width="1.6"/>
  <!-- Then the glyph: -->
  ...your shapes (white fill / gold accent)...
</svg>
NOTES: <1-3 sentences max>

After the 14, produce:

CONTACT SHEET — one SVG, 4×4 grid (last 2 cells empty), each cell at the real
22 px render size, background #0c1018, gold frames on stationary, ships
frameless.

CONSISTENCY NOTES — 3 sentences on the visual language tying the 14 together.

================================================================================
QUALITY BAR
================================================================================

REJECT if:
  - Any two ships look similar at 22 px.
  - Any glyph depends on detail < 1.5 design units (~2 px normal).
  - Any glyph uses gradients/opacity/blur/text/raster.
  - Ships have frames.
  - Set looks like X4 in-game icons.
  - Concave shape uses <polygon> not <path>.
  - Concepts feel medieval or earthly instead of space-sci-fi.

ACCEPT when:
  - All 14 distinct at 22 px on contact sheet.
  - Each concept reads space-thematic and matches the subtype.
  - Ships form clear hierarchy: Small < Medium < Capital ≠ Transport.
  - Visual language consistent across all 14.

Begin.
```
