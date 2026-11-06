import * as THREE from 'three'
import RAPIER from '@dimforge/rapier3d-compat'
import { BALE_COLORS, box, mat, type BaleKind, type Entities, type Entity } from './entities'
import type { Cost, Inventory } from './inventory'

export type Tool = 'deck' | 'belt' | 'magnet' | 'compactor' | 'pylon' | 'solar'
type MachineType = 'belt' | 'magnet' | 'compactor'

export const TOOLS: Record<Tool, { name: string; cost: Cost; desc: string; power?: number }> = {
  deck: { name: 'Deck', cost: { steel: 1 }, desc: 'Foundation you place freely. Brings its own grid.' },
  belt: { name: 'Belt', cost: {}, desc: 'Moves loose scrap. Snaps to a deck.', power: 0.25 },
  magnet: { name: 'Magnet sorter', cost: { steel: 1, motor: 1 }, desc: 'Place over a belt: lifts ferrous scrap one cell sideways.', power: 1 },
  compactor: { name: 'Compactor', cost: { steel: 2, compressor: 1 }, desc: '4 scrap → 1 bale. Mixed scrap makes a junk bale.', power: 1.5 },
  pylon: { name: 'Relay pylon', cost: { steel: 1, battery: 1 }, desc: 'Extends the grid and your range.' },
  solar: { name: 'Scrap solar', cost: { plastic: 1, chip: 1 }, desc: '+2 grid supply.' },
}
export const TOOL_ORDER: Tool[] = ['deck', 'belt', 'magnet', 'compactor', 'pylon', 'solar']

const DECK_CELLS = 6
const DECK_HALF = DECK_CELLS / 2
const DECK_TOP = 0.1
// Belts have no collider of their own: scrap slides on the deck's single flat surface,
// so there are no tile seams to snag on. A belt is a force zone plus a flush visual.
const BELT_TOP = DECK_TOP + 0.012
const BELT_SPEED = 1.4
const COMPACT_COUNT = 4
export const DOCK_SUPPLY = 6
const SOLAR_SUPPLY = 2
export const DOCK_CHARGE_RADIUS = 4
export const ZONE_RADIUS = 12
const LINK_RANGE = 16

export interface Deck {
  body: RAPIER.RigidBody
  collider: RAPIER.Collider
  group: THREE.Group
  cells: Map<number, Machine[]>
}

export interface Machine {
  type: MachineType
  deck: Deck
  ix: number
  iz: number
  rot: number
  obj: THREE.Object3D
  colliders: RAPIER.Collider[]
  powered: boolean
  batch: { kind: BaleKind | null; n: number }
  squash: number
}

export interface PowerNode {
  type: 'dock' | 'pylon' | 'solar'
  pos: THREE.Vector3
  obj: THREE.Object3D
  body: RAPIER.RigidBody
  lamp?: THREE.Mesh
  ring?: THREE.Mesh
  connected: boolean
}

const UP = new THREE.Vector3(0, 1, 0)
const LOCAL_DIRS = [new THREE.Vector3(0, 0, -1), new THREE.Vector3(1, 0, 0), new THREE.Vector3(0, 0, 1), new THREE.Vector3(-1, 0, 0)]
const cellKey = (ix: number, iz: number) => ix * 16 + iz
const rotQuat = (rot: number) => new THREE.Quaternion().setFromAxisAngle(UP, -rot * (Math.PI / 2))

// ---------- meshes (all built facing local -z) ----------

function arrow(color: number, size = 0.3) {
  const s = new THREE.Shape()
  s.moveTo(0, size)
  s.lineTo(size * 0.8, -size * 0.4)
  s.lineTo(0, -size * 0.1)
  s.lineTo(-size * 0.8, -size * 0.4)
  s.closePath()
  const m = new THREE.Mesh(new THREE.ShapeGeometry(s), mat(color, color))
  m.rotation.x = -Math.PI / 2
  return m
}

function deckMesh() {
  const g = new THREE.Group()
  g.add(box(DECK_CELLS, 0.2, DECK_CELLS, 0x8b7a63))
  for (let i = 0; i < DECK_CELLS; i++) g.add(box(0.06, 0.205, DECK_CELLS, 0x6a5b48, i - DECK_HALF + 0.5, 0, 0))
  const pts: number[] = []
  for (let i = 0; i <= DECK_CELLS; i++) {
    const p = i - DECK_HALF
    pts.push(p, DECK_TOP + 0.005, -DECK_HALF, p, DECK_TOP + 0.005, DECK_HALF, -DECK_HALF, DECK_TOP + 0.005, p, DECK_HALF, DECK_TOP + 0.005, p)
  }
  const grid = new THREE.LineSegments(
    new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute(pts, 3)),
    new THREE.LineBasicMaterial({ color: 0xf3e3c3, transparent: true, opacity: 0.35 }),
  )
  g.add(grid)
  return g
}

function machineMesh(type: MachineType) {
  const g = new THREE.Group()
  if (type === 'belt') {
    g.add(box(0.96, 0.02, 1.0, 0x2e2e33, 0, DECK_TOP + 0.006, 0))
    g.add(box(0.05, 0.04, 1.0, 0x6c7078, -0.47, DECK_TOP + 0.02, 0), box(0.05, 0.04, 1.0, 0x6c7078, 0.47, DECK_TOP + 0.02, 0))
    for (let i = 0; i < 2; i++) {
      const a = arrow(0xf2c14e, 0.18)
      a.position.y = BELT_TOP + 0.005
      a.name = 'chevron'
      g.add(a)
    }
  } else if (type === 'magnet') {
    for (const x of [-0.46, 0.46]) g.add(box(0.08, 1.5, 0.08, 0x8f3b2b, x, DECK_TOP + 0.75, 0.42))
    g.add(box(1.0, 0.1, 0.1, 0x8f3b2b, 0, DECK_TOP + 1.5, 0.42))
    g.add(box(0.12, 0.1, 0.9, 0x8f3b2b, 0, DECK_TOP + 1.5, 0))
    const head = box(0.7, 0.22, 0.7, 0xc0392b, 0, DECK_TOP + 1.3, -0.1)
    head.name = 'head'
    g.add(head)
    const a = arrow(0xff6b5b, 0.25)
    a.position.set(0, DECK_TOP + 1.56, -0.1)
    g.add(a)
  } else {
    const body = box(0.9, 0.9, 0.9, 0xd9a441, 0, DECK_TOP + 0.45, 0)
    body.name = 'body'
    g.add(body)
    const hopper = new THREE.Mesh(new THREE.CylinderGeometry(0.62, 0.38, 0.35, 4, 1, true), mat(0x9c7a35))
    hopper.rotation.y = Math.PI / 4
    hopper.position.y = DECK_TOP + 1.05
    g.add(hopper)
    g.add(box(0.6, 0.12, 0.35, 0x333333, 0, DECK_TOP + 0.25, -0.5))
    const lamp = new THREE.Mesh(new THREE.SphereGeometry(0.07, 8, 6), mat(0x555555))
    lamp.position.set(0.35, DECK_TOP + 0.95, -0.46)
    lamp.name = 'lamp'
    g.add(lamp)
  }
  return g
}

function nodeMesh(type: PowerNode['type']) {
  const g = new THREE.Group()
  let lamp: THREE.Mesh | undefined
  if (type === 'dock') {
    const pad = new THREE.Mesh(new THREE.CylinderGeometry(1.6, 1.8, 0.3, 16), mat(0x4a4f57))
    pad.position.y = 0.15
    pad.receiveShadow = true
    g.add(pad)
    const ring = new THREE.Mesh(new THREE.TorusGeometry(1.3, 0.05, 6, 32), mat(0x7ff0c8, 0x7ff0c8))
    ring.rotation.x = Math.PI / 2
    ring.position.y = 0.32
    g.add(ring)
    g.add(box(0.25, 4, 0.25, 0x5a6068, -1.5, 2, 0), box(1.2, 0.2, 0.2, 0x5a6068, -1, 3.9, 0))
    lamp = new THREE.Mesh(new THREE.SphereGeometry(0.22, 10, 8), mat(0x7ff0c8, 0x7ff0c8))
    lamp.position.set(-1.5, 4.25, 0)
  } else if (type === 'pylon') {
    g.add(box(0.18, 3.2, 0.18, 0x6b5844, 0, 1.6, 0), box(1.0, 0.12, 0.12, 0x6b5844, 0, 3.0, 0))
    lamp = new THREE.Mesh(new THREE.SphereGeometry(0.14, 10, 8), mat(0x555555))
    lamp.position.set(0, 3.35, 0)
  } else {
    g.add(box(0.12, 1.0, 0.12, 0x555a60, 0, 0.5, 0))
    const panel = box(1.4, 0.05, 0.9, 0x1f3a5f, 0, 1.05, 0)
    panel.rotation.x = -0.5
    g.add(panel)
    lamp = new THREE.Mesh(new THREE.SphereGeometry(0.08, 8, 6), mat(0x555555))
    lamp.position.set(0.6, 0.9, 0.3)
  }
  g.add(lamp)
  return { g, lamp }
}

// ---------- factory ----------

export class Factory {
  readonly decks: Deck[] = []
  readonly machines: Machine[] = []
  readonly nodes: PowerNode[] = []
  private byCollider = new Map<number, { deck?: Deck; machine?: Machine; node?: PowerNode }>()
  private cables = new THREE.Group()
  private time = 0

  onCompact?: () => void
  onBale?: () => void
  onDeck?: (pos: THREE.Vector3) => void
  supply = DOCK_SUPPLY
  demand = 0
  /** Fraction of demand the grid can meet (brownout when < 1). */
  satisfaction = 1

  constructor(
    private world: RAPIER.World,
    private scene: THREE.Scene,
    private entities: Entities,
    private inventory: Inventory,
  ) {
    scene.add(this.cables)
  }

  info(c: RAPIER.Collider | null | undefined) {
    return c ? this.byCollider.get(c.handle) : undefined
  }

  // ----- power -----

  addNode(type: PowerNode['type'], pos: THREE.Vector3) {
    const { g, lamp } = nodeMesh(type)
    g.position.copy(pos)
    this.scene.add(g)
    const body = this.world.createRigidBody(RAPIER.RigidBodyDesc.fixed().setTranslation(pos.x, pos.y, pos.z))
    const node: PowerNode = { type, pos: pos.clone(), obj: g, body, lamp, connected: type === 'dock' }
    const colliders =
      type === 'dock'
        ? [RAPIER.ColliderDesc.cylinder(0.15, 1.7).setTranslation(0, 0.15, 0), RAPIER.ColliderDesc.cuboid(0.13, 2, 0.13).setTranslation(-1.5, 2, 0)]
        : type === 'pylon'
          ? [RAPIER.ColliderDesc.cuboid(0.1, 1.6, 0.1).setTranslation(0, 1.6, 0)]
          : [RAPIER.ColliderDesc.cuboid(0.7, 0.5, 0.45).setTranslation(0, 0.7, 0)]
    for (const d of colliders) this.byCollider.set(this.world.createCollider(d, body).handle, { node })
    if (type !== 'solar') {
      const ring = new THREE.Mesh(
        new THREE.RingGeometry(ZONE_RADIUS - 0.15, ZONE_RADIUS, 64),
        new THREE.MeshBasicMaterial({ color: 0x7ff0c8, transparent: true, opacity: 0.5, side: THREE.DoubleSide, depthWrite: false }),
      )
      ring.rotation.x = -Math.PI / 2
      ring.position.copy(pos).y += 0.3
      ring.visible = false
      this.scene.add(ring)
      node.ring = ring
    }
    this.nodes.push(node)
    this.refreshGrid()
    return node
  }

  /** Flood-fill from the dock through nodes within link range. */
  refreshGrid() {
    for (const n of this.nodes) n.connected = n.type === 'dock'
    let changed = true
    while (changed) {
      changed = false
      for (const n of this.nodes) {
        if (n.connected) continue
        if (this.nodes.some((m) => m.connected && m.pos.distanceTo(n.pos) <= LINK_RANGE)) {
          n.connected = true
          changed = true
        }
      }
    }
    for (const n of this.nodes) if (n.lamp) n.lamp.material = n.connected ? mat(0x7ff0c8, 0x7ff0c8) : mat(0x555555)
    this.supply = DOCK_SUPPLY + SOLAR_SUPPLY * this.nodes.filter((n) => n.type === 'solar' && n.connected).length

    // sagging cables between linked nodes
    this.cables.clear()
    const lineMat = new THREE.LineBasicMaterial({ color: 0x222222 })
    const top = (n: PowerNode) => n.pos.clone().add(new THREE.Vector3(0, n.type === 'dock' ? 3.9 : n.type === 'pylon' ? 3.0 : 1.0, 0))
    for (let i = 0; i < this.nodes.length; i++) {
      for (let j = i + 1; j < this.nodes.length; j++) {
        const a = this.nodes[i]
        const b = this.nodes[j]
        if (!a.connected || !b.connected || a.pos.distanceTo(b.pos) > LINK_RANGE) continue
        const pa = top(a)
        const pb = top(b)
        const pts = Array.from({ length: 12 }, (_, k) => {
          const t = k / 11
          return pa.clone().lerp(pb, t).add(new THREE.Vector3(0, -Math.sin(t * Math.PI) * 0.8, 0))
        })
        this.cables.add(new THREE.Line(new THREE.BufferGeometry().setFromPoints(pts), lineMat))
      }
    }
  }

  /** What kind of powered zone a point is in: right at the dock, inside a grid zone, or outside. */
  zoneAt(p: THREE.Vector3): 'dock' | 'grid' | null {
    let best: 'dock' | 'grid' | null = null
    for (const n of this.nodes) {
      if (!n.connected || n.type === 'solar') continue
      const d = n.pos.distanceTo(p)
      if (n.type === 'dock' && d <= DOCK_CHARGE_RADIUS) return 'dock'
      if (d <= ZONE_RADIUS) best = 'grid'
    }
    return best
  }

  /** Distance from p to the nearest edge of a powered zone (0 inside). */
  distanceToGrid(p: THREE.Vector3) {
    let d = Infinity
    for (const n of this.nodes) if (n.connected && n.type !== 'solar') d = Math.min(d, Math.max(0, n.pos.distanceTo(p) - ZONE_RADIUS))
    return d
  }

  nearestChargePoint(p: THREE.Vector3) {
    return this.nodes[0].pos.distanceTo(p)
  }

  linkPreview(p: THREE.Vector3) {
    return this.nodes.some((n) => n.connected && n.pos.distanceTo(p) <= LINK_RANGE)
  }

  setRingsVisible(v: boolean) {
    for (const n of this.nodes) if (n.ring) n.ring.visible = v && n.connected
  }

  // ----- decks -----

  /** Freeform deck transform from an aimed surface. Tilt follows the ground, up to a limit. */
  deckTransform(point: THREE.Vector3, normal: THREE.Vector3, yaw: number) {
    const tilt = normal.angleTo(UP)
    const tooSteep = tilt > THREE.MathUtils.degToRad(25)
    if (tooSteep) normal = UP // keep the (red) preview level instead of standing it up the slope
    const q = new THREE.Quaternion().setFromUnitVectors(UP, normal).multiply(new THREE.Quaternion().setFromAxisAngle(UP, yaw))
    // Lift the deck clear of any junk poking up under its footprint; legs fill the gap.
    const down = UP.clone().applyQuaternion(q).negate()
    let rise = 0
    for (let x = -DECK_HALF; x <= DECK_HALF; x += 1) {
      for (let z = -DECK_HALF; z <= DECK_HALF; z += 1) {
        const from = new THREE.Vector3(x, 4, z).applyQuaternion(q).add(point)
        const hit = this.world.castRay(new RAPIER.Ray(from, down), 8, true, RAPIER.QueryFilterFlags.EXCLUDE_DYNAMIC | RAPIER.QueryFilterFlags.EXCLUDE_KINEMATIC, undefined, undefined, undefined, (c) => !this.byCollider.has(c.handle))
        if (hit) rise = Math.max(rise, 4 - hit.timeOfImpact)
      }
    }
    const pos = point.clone().addScaledVector(normal, rise + 0.12)
    let reason: string | null = tooSteep ? 'Too steep for a deck' : rise > 2 ? 'Ground too uneven here' : null
    if (!reason) {
      this.world.intersectionsWithShape(
        { x: pos.x + normal.x * 0.4, y: pos.y + normal.y * 0.4, z: pos.z + normal.z * 0.4 },
        q,
        new RAPIER.Cuboid(DECK_HALF - 0.1, 0.25, DECK_HALF - 0.1),
        (c) => {
          const info = this.byCollider.get(c.handle)
          if (info) {
            reason = 'Overlaps something you built'
            return false
          }
          return true
        },
      )
    }
    return { pos, q, reason }
  }

  placeDeck(pos: THREE.Vector3, q: THREE.Quaternion) {
    const group = deckMesh()
    group.position.copy(pos)
    group.quaternion.copy(q)
    this.scene.add(group)
    group.updateMatrixWorld(true)
    // wooden legs down to whatever is underneath, so decks look wedged into the heap
    for (const [x, z] of [[-2.8, -2.8], [2.8, -2.8], [-2.8, 2.8], [2.8, 2.8], [0, -2.8], [0, 2.8]]) {
      const top = new THREE.Vector3(x, -0.1, z).applyMatrix4(group.matrixWorld)
      const hit = this.world.castRay(new RAPIER.Ray(top, { x: 0, y: -1, z: 0 }), 6, true)
      const len = hit ? hit.timeOfImpact : 0
      if (len < 0.15) continue
      const leg = box(0.14, len, 0.14, 0x6a5b48)
      leg.position.copy(top).y -= len / 2
      this.scene.add(leg)
      group.userData.legs = [...(group.userData.legs ?? []), leg]
    }
    const body = this.world.createRigidBody(RAPIER.RigidBodyDesc.fixed().setTranslation(pos.x, pos.y, pos.z).setRotation(q))
    const collider = this.world.createCollider(RAPIER.ColliderDesc.cuboid(DECK_HALF, 0.1, DECK_HALF).setFriction(0.9), body)
    const deck: Deck = { body, collider, group, cells: new Map() }
    this.byCollider.set(collider.handle, { deck })
    this.decks.push(deck)
    this.onDeck?.(pos)
    return deck
  }

  /** Map an aimed point to the deck cell it falls in. */
  cellAt(deck: Deck, worldPoint: THREE.Vector3) {
    const local = deck.group.worldToLocal(worldPoint.clone())
    const ix = Math.floor(local.x + DECK_HALF)
    const iz = Math.floor(local.z + DECK_HALF)
    if (ix < 0 || iz < 0 || ix >= DECK_CELLS || iz >= DECK_CELLS) return null
    return { ix, iz }
  }

  /** Rotation (0..3) closest to a world direction, in the deck's own frame. */
  rotFacing(deck: Deck, worldDir: THREE.Vector3) {
    const local = worldDir.clone().applyQuaternion(deck.group.quaternion.clone().invert())
    let best = 0
    let bestDot = -Infinity
    LOCAL_DIRS.forEach((d, i) => {
      const dot = d.x * local.x + d.z * local.z
      if (dot > bestDot) {
        bestDot = dot
        best = i
      }
    })
    return best
  }

  cellTransform(deck: Deck, ix: number, iz: number, rot: number) {
    const pos = new THREE.Vector3(ix - DECK_HALF + 0.5, 0, iz - DECK_HALF + 0.5).applyMatrix4(deck.group.matrixWorld)
    const q = deck.group.quaternion.clone().multiply(rotQuat(rot))
    return { pos, q }
  }

  canPlaceMachine(type: MachineType, deck: Deck, ix: number, iz: number) {
    const here = deck.cells.get(cellKey(ix, iz)) ?? []
    if (here.some((m) => m.type === 'compactor')) return 'Cell has a compactor'
    if (type === 'compactor' && here.length) return 'Cell is not empty'
    if (here.some((m) => m.type === type)) return `Cell already has a ${TOOLS[type].name.toLowerCase()}`
    return null
  }

  placeMachine(type: MachineType, deck: Deck, ix: number, iz: number, rot: number) {
    const obj = machineMesh(type)
    const local = new THREE.Vector3(ix - DECK_HALF + 0.5, 0, iz - DECK_HALF + 0.5)
    obj.position.copy(local)
    obj.quaternion.copy(rotQuat(rot))
    deck.group.add(obj)
    const m: Machine = { type, deck, ix, iz, rot, obj, colliders: [], powered: false, batch: { kind: null, n: 0 }, squash: 0 }
    const descs =
      type === 'compactor'
        ? [RAPIER.ColliderDesc.cuboid(0.45, 0.45, 0.45).setTranslation(local.x, DECK_TOP + 0.45, local.z)]
        : []
    for (const d of descs) {
      const c = this.world.createCollider(d.setFriction(0.6), deck.body)
      m.colliders.push(c)
      this.byCollider.set(c.handle, { deck, machine: m })
    }
    const key = cellKey(ix, iz)
    deck.cells.set(key, [...(deck.cells.get(key) ?? []), m])
    this.machines.push(m)
    return m
  }

  /** Top-most machine in a cell (magnets sit over belts). */
  machineAt(deck: Deck, ix: number, iz: number) {
    const list = deck.cells.get(cellKey(ix, iz)) ?? []
    return list.find((m) => m.type === 'magnet') ?? list[0]
  }

  removeMachine(m: Machine) {
    for (const c of m.colliders) {
      this.byCollider.delete(c.handle)
      this.world.removeCollider(c, false)
    }
    m.deck.group.remove(m.obj)
    const key = cellKey(m.ix, m.iz)
    const rest = (m.deck.cells.get(key) ?? []).filter((x) => x !== m)
    if (rest.length) m.deck.cells.set(key, rest)
    else m.deck.cells.delete(key)
    this.machines.splice(this.machines.indexOf(m), 1)
    this.inventory.refund(TOOLS[m.type].cost)
  }

  removeDeck(d: Deck) {
    if (d.cells.size) return false
    this.byCollider.delete(d.collider.handle)
    this.world.removeRigidBody(d.body)
    this.scene.remove(d.group)
    for (const leg of d.group.userData.legs ?? []) this.scene.remove(leg)
    this.decks.splice(this.decks.indexOf(d), 1)
    this.inventory.refund(TOOLS.deck.cost)
    return true
  }

  removeNode(n: PowerNode) {
    if (n.type === 'dock') return false
    this.world.removeRigidBody(n.body)
    for (const [h, info] of this.byCollider) if (info.node === n) this.byCollider.delete(h)
    this.scene.remove(n.obj)
    if (n.ring) this.scene.remove(n.ring)
    this.nodes.splice(this.nodes.indexOf(n), 1)
    this.inventory.refund(TOOLS[n.type as Tool].cost)
    this.refreshGrid()
    return true
  }

  // ----- simulation -----

  private queryBox(center: THREE.Vector3, q: THREE.Quaternion, half: THREE.Vector3, skip: RAPIER.RigidBody | null) {
    const found: Entity[] = []
    this.world.intersectionsWithShape(center, q, new RAPIER.Cuboid(half.x, half.y, half.z), (c) => {
      const e = this.entities.fromCollider(c)
      if (e && e.body !== skip) found.push(e)
      return true
    })
    return found
  }

  step(dt: number, held: RAPIER.RigidBody | null) {
    this.time += dt
    // power budget
    this.demand = 0
    for (const m of this.machines) {
      const { pos } = this.cellTransform(m.deck, m.ix, m.iz, m.rot)
      m.powered = this.zoneAt(pos) !== null
      if (m.powered) this.demand += TOOLS[m.type].power ?? 0
    }
    this.satisfaction = this.demand > 0 ? Math.min(1, this.supply / this.demand) : 1
    const f = this.satisfaction

    const v = new THREE.Vector3()
    const rel = new THREE.Vector3()
    for (const m of this.machines) {
      if (!m.powered) continue
      const { pos, q } = this.cellTransform(m.deck, m.ix, m.iz, m.rot)
      const up = UP.clone().applyQuaternion(q)
      const dir = LOCAL_DIRS[0].clone().applyQuaternion(q)

      if (m.type === 'belt') {
        const centre = pos.clone().addScaledVector(up, BELT_TOP + 0.3)
        for (const e of this.queryBox(centre, q, new THREE.Vector3(0.5, 0.3, 0.5), held)) {
          const lv = e.body.linvel()
          v.set(lv.x, lv.y, lv.z)
          const t = e.body.translation()
          rel.set(t.x - pos.x, t.y - pos.y, t.z - pos.z)
          const along = v.dot(dir)
          const vertical = v.dot(up)
          const lateralV = v.clone().addScaledVector(dir, -along).addScaledVector(up, -vertical)
          const lateralOff = rel.clone().addScaledVector(dir, -rel.dot(dir)).addScaledVector(up, -rel.dot(up))
          // ease toward belt speed and toward the centre line, so flow is calm and readable
          const targetLat = lateralOff.multiplyScalar(-2)
          lateralV.lerp(targetLat, 0.2)
          const newAlong = along + (BELT_SPEED * f - along) * 0.2
          v.copy(dir).multiplyScalar(newAlong).add(lateralV).addScaledVector(up, vertical)
          e.body.setLinvel(v, true)
        }
      } else if (m.type === 'magnet') {
        // Zone spans this cell and the neighbour it drops onto. Ferrous scrap is lifted,
        // carried to above the neighbour's centre, then let go.
        const side = new THREE.Vector3(1, 0, 0).applyQuaternion(q)
        const centre = pos.clone().addScaledVector(up, BELT_TOP + 0.65).addScaledVector(dir, 0.5)
        for (const e of this.queryBox(centre, q, new THREE.Vector3(0.5, 0.65, 1.0), held)) {
          if (e.type !== 'chunk' || e.material !== 'steel') continue
          const t = e.body.translation()
          rel.set(t.x - pos.x, t.y - pos.y, t.z - pos.z)
          const across = rel.dot(dir)
          if (across >= 0.9) continue // over the drop point: falls freely
          if (Math.abs(rel.dot(side)) > 0.45) continue // only grab what's over this cell
          const h = rel.dot(up)
          if (h < 0.5 && across > 0.5) continue // already dropped onto the neighbour
          const lift = h < 0.95 ? 3 : (1.05 - h) * 6
          const carry = h > 0.7 ? THREE.MathUtils.clamp((1.0 - across) * 4, 0.4, 2.2) : 0
          v.copy(dir).multiplyScalar(carry * f).addScaledVector(up, lift * f)
          e.body.setLinvel(v, true)
          e.body.setAngvel({ x: 0, y: 2, z: 0 }, true)
        }
        const head = m.obj.getObjectByName('head')
        if (head) head.position.y = DECK_TOP + 1.3 + Math.sin(this.time * 20) * 0.01
      } else {
        const centre = pos.clone().addScaledVector(up, DECK_TOP + 0.5)
        const absorbed = this.queryBox(centre, q, new THREE.Vector3(0.62, 0.62, 0.62), held).filter((e) => e.type === 'chunk')
        if (f < 0.3) continue
        for (const e of absorbed) {
          if (e.type !== 'chunk') continue
          m.batch.kind = m.batch.kind === null ? e.material : m.batch.kind === e.material ? m.batch.kind : 'mixed'
          m.batch.n++
          this.entities.remove(e)
          m.squash = 1
          this.onCompact?.()
        }
        if (m.batch.n >= COMPACT_COUNT && m.batch.kind) {
          const out = pos.clone().addScaledVector(up, DECK_TOP + 0.45).addScaledVector(dir, 1.0)
          this.entities.spawnBale(m.batch.kind, out, dir.clone().multiplyScalar(1.5))
          m.batch = { kind: null, n: 0 }
          this.onBale?.()
        }
        m.squash = Math.max(0, m.squash - dt * 4)
        const body = m.obj.getObjectByName('body')
        if (body) body.scale.y = 1 - m.squash * 0.15
        const lamp = m.obj.getObjectByName('lamp') as THREE.Mesh | undefined
        if (lamp) lamp.material = m.batch.kind ? mat(BALE_COLORS[m.batch.kind], BALE_COLORS[m.batch.kind]) : mat(0x555555)
      }
    }
  }

  /** Visual-only animation: belt chevrons crawl along when powered. */
  animate() {
    for (const m of this.machines) {
      if (m.type !== 'belt') continue
      let i = 0
      for (const c of m.obj.children) {
        if (c.name !== 'chevron') continue
        const phase = m.powered ? (this.time * BELT_SPEED * this.satisfaction * 0.8 + i * 0.5) % 1 : i * 0.5
        c.position.z = 0.45 - phase * 0.9
        i++
      }
    }
  }
}
