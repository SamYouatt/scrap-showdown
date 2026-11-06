import * as THREE from 'three'
import RAPIER from '@dimforge/rapier3d-compat'

export type Material = 'steel' | 'plastic'
export type BaleKind = Material | 'mixed'
export type Part = 'motor' | 'compressor' | 'battery' | 'chip'
export type Stock = BaleKind | Part

export const STOCK_NAMES: Record<Stock, string> = {
  steel: 'Steel bale',
  plastic: 'Plastic bale',
  mixed: 'Junk bale',
  motor: 'Motor',
  compressor: 'Compressor',
  battery: 'Battery cell',
  chip: 'Chip',
}

interface ItemDef {
  name: string
  size: [number, number, number]
  color: number
  density: number
  /** Light enough for Lazlo to pick up. Heavy items can only be pushed or salvaged. */
  liftable: boolean
  parts: Part[]
  steel: number
  plastic: number
}

export const ITEMS = {
  fridge: { name: 'Fridge', size: [0.8, 1.7, 0.7], color: 0xe9e4d6, density: 0.25, liftable: false, parts: ['compressor'], steel: 3, plastic: 1 },
  washer: { name: 'Washing machine', size: [0.75, 0.85, 0.7], color: 0xd5dbe0, density: 0.5, liftable: false, parts: ['motor'], steel: 2, plastic: 1 },
  ebike: { name: 'E-bike', size: [1.6, 0.9, 0.25], color: 0x2a9d8f, density: 0.4, liftable: true, parts: ['motor', 'battery'], steel: 1, plastic: 1 },
  tv: { name: 'Old TV', size: [0.8, 0.6, 0.45], color: 0x3a3a3a, density: 0.4, liftable: true, parts: ['chip'], steel: 0, plastic: 2 },
  phone: { name: 'Phone', size: [0.2, 0.36, 0.06], color: 0x1d1d22, density: 2, liftable: true, parts: ['chip'], steel: 0, plastic: 1 },
  trolley: { name: 'Shopping trolley', size: [0.9, 1.0, 0.6], color: 0xa9b3bd, density: 0.15, liftable: true, parts: [], steel: 3, plastic: 0 },
} satisfies Record<string, ItemDef>

export type ItemKind = keyof typeof ITEMS

export type Entity =
  | { type: 'item'; kind: ItemKind; body: RAPIER.RigidBody; collider: RAPIER.Collider; mesh: THREE.Object3D }
  | { type: 'chunk'; material: Material; body: RAPIER.RigidBody; collider: RAPIER.Collider; mesh: THREE.Object3D }
  | { type: 'bale'; bale: BaleKind; body: RAPIER.RigidBody; collider: RAPIER.Collider; mesh: THREE.Object3D }

export function entityName(e: Entity) {
  if (e.type === 'item') return ITEMS[e.kind].name
  if (e.type === 'chunk') return e.material === 'steel' ? 'Steel scrap (ferrous)' : 'Plastic scrap'
  return STOCK_NAMES[e.bale]
}

const PLASTIC_COLORS = [0xe76f51, 0xf4a261, 0xe9c46a, 0x4cc9f0, 0x90be6d, 0xf28482]
export const BALE_COLORS: Record<BaleKind, number> = { steel: 0x6f7d8c, plastic: 0xe9a03b, mixed: 0x7a6650 }

const matCache = new Map<number, THREE.MeshStandardMaterial>()
export function mat(color: number, emissive = 0) {
  const key = color * 7 + emissive
  let m = matCache.get(key)
  if (!m) {
    m = new THREE.MeshStandardMaterial({ color, roughness: 0.85, metalness: 0.05, flatShading: true, emissive, emissiveIntensity: emissive ? 1.5 : 0 })
    matCache.set(key, m)
  }
  return m
}

export function box(w: number, h: number, d: number, color: number, x = 0, y = 0, z = 0) {
  const m = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), mat(color))
  m.position.set(x, y, z)
  m.castShadow = true
  m.receiveShadow = true
  return m
}

function cyl(r: number, h: number, color: number, x = 0, y = 0, z = 0, axis: 'x' | 'y' | 'z' = 'y') {
  const m = new THREE.Mesh(new THREE.CylinderGeometry(r, r, h, 12), mat(color))
  m.position.set(x, y, z)
  if (axis === 'x') m.rotation.z = Math.PI / 2
  if (axis === 'z') m.rotation.x = Math.PI / 2
  m.castShadow = true
  return m
}

/** Greybox meshes with recognisable silhouettes, so you can spot a fridge across a valley. */
function itemMesh(kind: ItemKind): THREE.Object3D {
  const d = ITEMS[kind]
  const [w, h, l] = d.size
  const g = new THREE.Group()
  switch (kind) {
    case 'fridge':
      g.add(box(w, h, l, d.color))
      g.add(box(w * 1.01, 0.03, l * 1.01, 0x777777, 0, h * 0.2))
      g.add(box(0.04, 0.5, 0.05, 0x888888, w * 0.35, h * 0.3, l / 2 + 0.03))
      g.add(box(0.04, 0.3, 0.05, 0x888888, w * 0.35, -h * 0.05, l / 2 + 0.03))
      break
    case 'washer':
      g.add(box(w, h, l, d.color))
      g.add(cyl(0.24, 0.05, 0x445566, 0, -0.02, l / 2 + 0.01, 'z'))
      g.add(box(w * 0.9, 0.08, 0.03, 0x99a0a8, 0, h * 0.4, l / 2 + 0.01))
      break
    case 'ebike': {
      const wheel = (x: number) => {
        const t = new THREE.Mesh(new THREE.TorusGeometry(0.3, 0.05, 6, 16), mat(0x222222))
        t.position.set(x, -0.15, 0)
        t.castShadow = true
        return t
      }
      g.add(wheel(-0.55), wheel(0.55))
      const frame = box(1.1, 0.08, 0.08, d.color, 0, 0.1, 0)
      frame.rotation.z = -0.15
      g.add(frame, box(0.3, 0.25, 0.14, 0x333333, -0.05, 0.02, 0), box(0.25, 0.05, 0.1, 0x222222, -0.3, 0.32, 0))
      g.add(box(0.06, 0.4, 0.06, 0x555555, 0.45, 0.2, 0), box(0.05, 0.05, 0.4, 0x222222, 0.45, 0.4, 0))
      break
    }
    case 'tv':
      g.add(box(w, h, l, d.color))
      g.add(box(w * 0.75, h * 0.7, 0.02, 0x5c7a72, -0.05, 0, l / 2 + 0.005))
      break
    case 'phone':
      g.add(box(w, h, l, d.color))
      g.add(box(w * 0.85, h * 0.85, 0.01, 0x334455, 0, 0, l / 2 + 0.002))
      break
    case 'trolley': {
      const basket = box(w, h * 0.5, l, d.color, 0, h * 0.15, 0)
      basket.material = new THREE.MeshStandardMaterial({ color: d.color, wireframe: true })
      g.add(basket, box(w, 0.04, l, d.color, 0, h * 0.4, 0), box(w * 0.95, 0.03, l * 0.95, 0x8a939c, 0, -h * 0.25, 0))
      for (const [x, z] of [[-0.35, -0.22], [0.35, -0.22], [-0.35, 0.22], [0.35, 0.22]]) {
        g.add(cyl(0.02, h * 0.45, 0x8a939c, x, -h * 0.3, z), cyl(0.06, 0.04, 0x222222, x, -h / 2 + 0.06, z, 'z'))
      }
      g.add(box(0.05, 0.05, l, 0x333333, -w / 2 - 0.1, h * 0.4, 0))
      break
    }
  }
  return g
}

export class Entities {
  readonly all = new Set<Entity>()
  private byHandle = new Map<number, Entity>()
  private rng = Math.random

  constructor(
    private world: RAPIER.World,
    private scene: THREE.Scene,
  ) {}

  private add(e: Entity) {
    this.all.add(e)
    this.byHandle.set(e.collider.handle, e)
    this.scene.add(e.mesh)
    this.syncOne(e)
    return e
  }

  private makeBody(pos: THREE.Vector3, yaw: number, vel?: THREE.Vector3) {
    const q = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), yaw)
    const desc = RAPIER.RigidBodyDesc.dynamic()
      .setTranslation(pos.x, pos.y, pos.z)
      .setRotation(q)
      .setLinearDamping(0.15)
      .setAngularDamping(0.4)
    if (vel) desc.setLinvel(vel.x, vel.y, vel.z)
    return this.world.createRigidBody(desc)
  }

  spawnItem(kind: ItemKind, pos: THREE.Vector3, yaw = this.rng() * Math.PI * 2) {
    const d = ITEMS[kind]
    const body = this.makeBody(pos, yaw)
    const collider = this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(d.size[0] / 2, d.size[1] / 2, d.size[2] / 2).setDensity(d.density * 1000).setFriction(0.8),
      body,
    )
    return this.add({ type: 'item', kind, body, collider, mesh: itemMesh(kind) })
  }

  spawnChunk(material: Material, pos: THREE.Vector3, vel?: THREE.Vector3) {
    const r = this.rng()
    // rods, plates and lumps: loose scrap with a bit of variety
    const size: [number, number, number] = r < 0.33 ? [0.5, 0.12, 0.12] : r < 0.66 ? [0.4, 0.07, 0.3] : [0.28, 0.26, 0.24]
    const color = material === 'steel' ? [0x8a95a5, 0x7b8794, 0x9aa3ad][Math.floor(this.rng() * 3)] : PLASTIC_COLORS[Math.floor(this.rng() * PLASTIC_COLORS.length)]
    const body = this.makeBody(pos, this.rng() * Math.PI * 2, vel)
    const collider = this.world.createCollider(
      RAPIER.ColliderDesc.cuboid(size[0] / 2, size[1] / 2, size[2] / 2).setDensity(material === 'steel' ? 2500 : 900).setFriction(0.7),
      body,
    )
    return this.add({ type: 'chunk', material, body, collider, mesh: box(size[0], size[1], size[2], color) })
  }

  spawnBale(bale: BaleKind, pos: THREE.Vector3, vel?: THREE.Vector3) {
    const s = 0.6
    const body = this.makeBody(pos, this.rng() * 0.3, vel)
    const collider = this.world.createCollider(RAPIER.ColliderDesc.cuboid(s / 2, s / 2, s / 2).setDensity(700).setFriction(0.9), body)
    const g = new THREE.Group()
    g.add(box(s, s, s, BALE_COLORS[bale]))
    // strapping bands make bales read as "processed" at a glance
    g.add(box(s + 0.02, s + 0.02, 0.05, 0x2b2b2b, 0, 0, s * 0.25), box(s + 0.02, s + 0.02, 0.05, 0x2b2b2b, 0, 0, -s * 0.25))
    return this.add({ type: 'bale', bale, body, collider, mesh: g })
  }

  fromCollider(c: RAPIER.Collider | null | undefined) {
    return c ? this.byHandle.get(c.handle) : undefined
  }

  remove(e: Entity) {
    if (!this.all.delete(e)) return
    this.byHandle.delete(e.collider.handle)
    this.scene.remove(e.mesh)
    this.world.removeRigidBody(e.body)
  }

  private syncOne(e: Entity) {
    const t = e.body.translation()
    const r = e.body.rotation()
    e.mesh.position.set(t.x, t.y, t.z)
    e.mesh.quaternion.set(r.x, r.y, r.z, r.w)
  }

  sync() {
    for (const e of this.all) {
      if (e.body.isSleeping()) continue
      if (e.body.translation().y < -120) {
        this.remove(e)
        continue
      }
      this.syncOne(e)
    }
  }
}
