import * as THREE from 'three'
import RAPIER from '@dimforge/rapier3d-compat'
import { box, mat, type Entities, type ItemKind, type Material } from './entities'
import { fbm, mulberry32, smoothstep } from './noise'
import { buildScenery } from './scenery'

export type HeapShape = 'basin' | 'pit' | 'mountain'

export const HEAP_SHAPES: Record<HeapShape, { name: string; blurb: string }> = {
  basin: { name: 'Wide basin', blurb: 'Broad rolling heap. Long expeditions outward from the dock.' },
  pit: { name: 'Terraced pit', blurb: 'Descend terraces linked by ramps. Deeper is older.' },
  mountain: { name: 'Junk mountain', blurb: 'A towering peak of scrap. The best salvage is near the top.' },
}

const SIZE = 260
const SEG = 260

interface ShapeDef {
  height: (x: number, z: number) => number
  dock: [number, number]
  /** Returns a random ground position for "near the dock", "mid", or "far / deep" content. */
  zone: (band: 'near' | 'mid' | 'far', rng: () => number) => [number, number]
  /** True where static junk shouldn't go (keeps routes open). */
  keepClear: (x: number, z: number) => boolean
}

const wrapAngle = (a: number) => Math.atan2(Math.sin(a), Math.cos(a))

function polar(r: number, a: number): [number, number] {
  return [Math.cos(a) * r, Math.sin(a) * r]
}

function outerWalls(x: number, z: number, r: number, from: number) {
  return 16 * smoothstep(from, from + 45, r) * (0.6 + 0.4 * fbm(x * 0.04, z * 0.04, 9))
}

function basinShape(rng: () => number): ShapeDef {
  const mounds = Array.from({ length: 10 }, () => {
    const [x, z] = polar(22 + rng() * 60, rng() * Math.PI * 2)
    return { x, z, r: 6 + rng() * 9, h: 3 + rng() * 5 }
  })
  return {
    height(x, z) {
      const r = Math.hypot(x, z)
      let h = 3.5 * fbm(x * 0.025, z * 0.025, 1) + 1.1 * fbm(x * 0.1, z * 0.1, 2) + 0.3 * fbm(x * 0.4, z * 0.4, 3)
      for (const m of mounds) h += m.h * Math.exp(-((x - m.x) ** 2 + (z - m.z) ** 2) / (m.r * m.r))
      h += outerWalls(x, z, r, 80)
      return h * smoothstep(5, 15, r)
    },
    dock: [0, 0],
    zone(band, r) {
      const [lo, hi] = band === 'near' ? [7, 16] : band === 'mid' ? [22, 45] : [55, 85]
      return polar(lo + r() * (hi - lo), r() * Math.PI * 2)
    },
    keepClear: (x, z) => Math.hypot(x, z) < 12,
  }
}

function pitShape(): ShapeDef {
  // Terraces separated by cliffs. Each cliff has one ramp, staggered around the pit,
  // so getting down is a little route-finding puzzle.
  const R = 64
  const L = 4
  const D = 24
  const step = D / L
  const ringWidth = R / L
  const rampAngle = (k: number) => Math.PI + (L - k) * 2.3
  const height = (x: number, z: number) => {
    const r = Math.hypot(x, z)
    const a = Math.atan2(z, x)
    let h = -D
    for (let k = 1; k <= L; k++) {
      const b = k * ringWidth
      const arc = Math.abs(wrapAngle(a - rampAngle(k))) * b
      const width = 4 + 16 * (1 - smoothstep(4, 8, arc))
      h += step * smoothstep(b - width / 2, b + width / 2, r)
    }
    h += 0.6 * fbm(x * 0.08, z * 0.08, 2) + 0.25 * fbm(x * 0.3, z * 0.3, 3)
    h += outerWalls(x, z, r, R + 28)
    return h
  }
  const dockA = rampAngle(L)
  return {
    height,
    dock: polar(R + 12, dockA),
    zone(band, rng) {
      if (band === 'near') {
        const a = dockA + (rng() - 0.5) * 0.5
        return polar(R + 6 + rng() * 14, a)
      }
      // mid: middle terraces, far: the pit floor
      const r = band === 'mid' ? ringWidth * (1.3 + rng() * 2.4) : rng() * ringWidth * 0.8
      return polar(r, rng() * Math.PI * 2)
    },
    keepClear(x, z) {
      const r = Math.hypot(x, z)
      const a = Math.atan2(z, x)
      for (let k = 1; k <= L; k++) {
        const b = k * ringWidth
        if (Math.abs(r - b) < 14 && Math.abs(wrapAngle(a - rampAngle(k))) * b < 12) return true
      }
      const [dx, dz] = polar(R + 12, dockA)
      return Math.hypot(x - dx, z - dz) < 12
    },
  }
}

function mountainShape(): ShapeDef {
  const dockA = Math.PI * 0.5
  return {
    height(x, z) {
      const r = Math.hypot(x, z)
      const a = Math.atan2(z, x)
      const ridges = 1 + 0.25 * Math.sin(a * 5 + 1.3) * smoothstep(5, 30, r)
      let h = 40 * Math.exp(-((r / (34 * ridges)) ** 2))
      h += 2.5 * fbm(x * 0.05, z * 0.05, 4) + 0.8 * fbm(x * 0.15, z * 0.15, 5) + 0.25 * fbm(x * 0.45, z * 0.45, 6)
      h += outerWalls(x, z, r, 95)
      const [dx, dz] = polar(74, dockA)
      const f = smoothstep(4, 12, Math.hypot(x - dx, z - dz))
      return h * f + (1 - f) * 1.2
    },
    dock: polar(74, dockA),
    zone(band, rng) {
      if (band === 'near') return polar(62 + rng() * 16, dockA + (rng() - 0.5) * 0.4)
      const r = band === 'mid' ? 26 + rng() * 14 : rng() * 10
      return polar(r, rng() * Math.PI * 2)
    },
    keepClear(x, z) {
      const [dx, dz] = polar(74, dockA)
      return Math.hypot(x - dx, z - dz) < 12
    },
  }
}

export function buildHeap(shape: HeapShape, world: RAPIER.World, scene: THREE.Scene, entities: Entities) {
  const rng = mulberry32(1337)
  const def = shape === 'basin' ? basinShape(rng) : shape === 'pit' ? pitShape() : mountainShape()
  // The service lift: the first layer project, a walk out from the dock.
  // Its pad is flattened into the heap and kept clear of static junk.
  const [ldx, ldz] = def.dock
  const fromDock = Math.hypot(ldx, ldz) > 1 ? new THREE.Vector2(-ldx, -ldz).normalize() : new THREE.Vector2(0, -1)
  fromDock.rotateAround(new THREE.Vector2(), 0.55)
  const liftXZ: [number, number] = [ldx + fromDock.x * 34, ldz + fromDock.y * 34]
  const baseHeight = def.height
  const padH = baseHeight(...liftXZ)
  const heightAt = (x: number, z: number) => {
    const f = smoothstep(4.5, 9, Math.hypot(x - liftXZ[0], z - liftXZ[1]))
    return padH * (1 - f) + baseHeight(x, z) * f
  }
  const baseClear = def.keepClear
  def.keepClear = (x, z) => baseClear(x, z) || Math.hypot(x - liftXZ[0], z - liftXZ[1]) < 11

  // --- terrain: one displaced plane, used for both rendering and the trimesh collider
  const geo = new THREE.PlaneGeometry(SIZE, SIZE, SEG, SEG)
  geo.rotateX(-Math.PI / 2)
  const pos = geo.attributes.position as THREE.BufferAttribute
  const colors = new Float32Array(pos.count * 3)
  const c = new THREE.Color()
  // dusty tan, rust, steel grey, faded teal, bleached cream, oily dark: patches of different junk
  const palette = [0xa58d6f, 0x8a4b2e, 0x6f747a, 0x5d7f7a, 0xb9ab8f, 0x7a5a48, 0x8c6a7a].map((h) => new THREE.Color(h))
  const rustC = new THREE.Color(0x6e3b26)
  const dustC = new THREE.Color(0xc9b08a)
  for (let i = 0; i < pos.count; i++) {
    const x = pos.getX(i)
    const z = pos.getZ(i)
    const y = heightAt(x, z)
    pos.setY(i, y)
    // patchy junk colours, darker in crevices
    const n = fbm(x * 0.15, z * 0.15, 21) * 0.5 + 0.5
    const idx = Math.min(palette.length - 1, Math.floor(fbm(x * 0.06, z * 0.06, 23) * 2.5 + 2.5))
    c.copy(palette[Math.max(0, idx)]).lerp(palette[(idx + 1) % palette.length], n)
    // slopes streak with rust, flat tops collect pale dust, and a fine speckle reads as small junk
    const slope = Math.min(1, Math.hypot(heightAt(x + 0.8, z) - heightAt(x - 0.8, z), heightAt(x, z + 0.8) - heightAt(x, z - 0.8)) / 1.6)
    c.lerp(rustC, smoothstep(0.25, 0.8, slope) * 0.55)
    c.lerp(dustC, (1 - smoothstep(0.05, 0.3, slope)) * 0.12)
    c.multiplyScalar(0.85 + 0.3 * (fbm(x * 0.9, z * 0.9, 25) * 0.5 + 0.5))
    const crevice = heightAt(x, z) - (heightAt(x + 1.5, z) + heightAt(x - 1.5, z) + heightAt(x, z + 1.5) + heightAt(x, z - 1.5)) / 4
    c.multiplyScalar(THREE.MathUtils.clamp(1 + crevice * 0.35, 0.6, 1.2))
    colors.set([c.r, c.g, c.b], i * 3)
  }
  geo.setAttribute('color', new THREE.BufferAttribute(colors, 3))
  geo.computeVertexNormals()
  const terrain = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.95, flatShading: true }))
  terrain.receiveShadow = true
  scene.add(terrain)

  const index = geo.index!
  world.createCollider(
    RAPIER.ColliderDesc.trimesh(new Float32Array(pos.array), new Uint32Array(index.array)).setFriction(0.9),
    world.createRigidBody(RAPIER.RigidBodyDesc.fixed()),
  )

  // --- big static junk: cars, containers, girders. Gives the heap its lumpy, climbable skyline.
  const junkKinds = [
    { size: [3.8, 1.3, 1.8], colors: [0x8e3b2e, 0x2f5d7c, 0x6b6b45, 0xb8a66a, 0x444b52] },
    { size: [6, 2.6, 2.4], colors: [0x9b4a2c, 0x2d5f7a, 0x3f6b48, 0x8a7a3a] },
    { size: [8, 0.45, 0.45], colors: [0x5a5f66, 0x6e4a3a] },
    { size: [2.2, 2.2, 2.2], colors: [0x5c534a, 0x766a5c] },
  ]
  const junkBody = world.createRigidBody(RAPIER.RigidBodyDesc.fixed())
  let placed = 0
  for (let tries = 0; placed < 90 && tries < 600; tries++) {
    const [x, z] = polar(8 + rng() * 105, rng() * Math.PI * 2)
    if (def.keepClear(x, z)) continue
    const k = junkKinds[Math.floor(rng() * junkKinds.length)]
    const [w, h, d] = k.size
    const q = new THREE.Quaternion().setFromEuler(new THREE.Euler((rng() - 0.5) * 0.5, rng() * Math.PI * 2, (rng() - 0.5) * 0.5))
    const p = new THREE.Vector3(x, heightAt(x, z) + h * 0.15, z)
    const mesh = box(w, h, d, k.colors[Math.floor(rng() * k.colors.length)])
    mesh.position.copy(p)
    mesh.quaternion.copy(q)
    scene.add(mesh)
    world.createCollider(RAPIER.ColliderDesc.cuboid(w / 2, h / 2, d / 2).setTranslation(p.x, p.y, p.z).setRotation(q).setFriction(0.8), junkBody)
    placed++
  }

  // --- loose content: specific items to hunt, and scrap piles to process
  const ground = (x: number, z: number, lift: number) => new THREE.Vector3(x, heightAt(x, z) + lift, z)
  const at = (band: 'near' | 'mid' | 'far') => def.zone(band, rng)
  const spawn = (kind: ItemKind, band: 'near' | 'mid' | 'far') => {
    const [x, z] = at(band)
    entities.spawnItem(kind, ground(x, z, 1.2))
  }
  const pile = (band: 'near' | 'mid' | 'far', count: number, steelRatio: number) => {
    const [x, z] = at(band)
    for (let i = 0; i < count; i++) {
      const m: Material = rng() < steelRatio ? 'steel' : 'plastic'
      const ox = (rng() - 0.5) * 2.5
      const oz = (rng() - 0.5) * 2.5
      entities.spawnChunk(m, ground(x + ox, z + oz, 0.6 + i * 0.12))
    }
  }

  const layout: [ItemKind, 'near' | 'mid' | 'far', number][] = [
    ['fridge', 'near', 1], ['washer', 'near', 1], ['phone', 'near', 2], ['trolley', 'near', 1],
    ['ebike', 'mid', 2], ['washer', 'mid', 1], ['tv', 'mid', 2], ['phone', 'mid', 3], ['trolley', 'mid', 2], ['fridge', 'mid', 1],
    ['ebike', 'far', 2], ['fridge', 'far', 2], ['tv', 'far', 1], ['phone', 'far', 2],
  ]
  for (const [kind, band, n] of layout) for (let i = 0; i < n; i++) spawn(kind, band)
  pile('near', 30, 0.6)
  pile('near', 24, 0.5)
  pile('mid', 20, 0.6)
  pile('mid', 20, 0.4)
  pile('mid', 20, 0.7)
  pile('far', 24, 0.6)
  pile('far', 24, 0.5)

  const [dx, dz] = def.dock
  const dockPos = new THREE.Vector3(dx, heightAt(dx, dz), dz)

  // starter bales next to the dock: enough for a first deck and compactor
  const toCentre = Math.hypot(dx, dz) > 1 ? new THREE.Vector3(-dx, 0, -dz).normalize() : new THREE.Vector3(0, 0, -1)
  const side = new THREE.Vector3(-toCentre.z, 0, toCentre.x)
  const baleBase = dockPos.clone().addScaledVector(side, 3.2)
  const starter: ('steel' | 'plastic')[] = ['steel', 'steel', 'steel', 'steel', 'plastic']
  starter.forEach((b, i) => {
    const p = baleBase.clone().addScaledVector(toCentre, (i % 3) * 0.7 - 0.7)
    p.y = heightAt(p.x, p.z) + 0.5 + Math.floor(i / 3) * 0.65
    entities.spawnBale(b, p)
  })

  const spawnPos = dockPos.clone().addScaledVector(toCentre, 3)
  spawnPos.y = heightAt(spawnPos.x, spawnPos.z) + 1.2

  // --- memories: traces of people, and of robots like Lazlo. Look at them with E.
  const memories = new Map<number, Memory>()
  const shipYaw = Math.atan2(toCentre.x, toCentre.z)
  const memory = (band: 'near' | 'mid' | 'far' | [number, number], m: Omit<Memory, 'pos'>, mesh: THREE.Object3D, half: [number, number, number], faceShip = false) => {
    let x: number, z: number
    for (let i = 0; ; i++) {
      ;[x, z] = Array.isArray(band) ? band : at(band)
      if (!def.keepClear(x, z) || Array.isArray(band) || i > 20) break
    }
    const p = new THREE.Vector3(x, heightAt(x, z) + half[1] - 0.05, z)
    mesh.position.copy(p)
    mesh.rotation.y = faceShip ? shipYaw : rng() * Math.PI * 2
    scene.add(mesh)
    const q = new THREE.Quaternion().setFromEuler(mesh.rotation)
    const c = world.createCollider(RAPIER.ColliderDesc.cuboid(...half).setTranslation(p.x, p.y, p.z).setRotation(q), junkBody)
    memories.set(c.handle, { ...m, pos: p })
  }
  const radioAt = dockPos.clone().addScaledVector(side, -4).addScaledVector(toCentre, 1.5)
  memory([radioAt.x, radioAt.z], { name: 'Radio', line: 'A radio, still playing. Its battery should have died years ago.', radio: true }, radioMesh(), [0.35, 0.22, 0.15])
  memory('mid', { name: 'Robot, powered down', line: 'A robot like you. The stencil on its chest reads L4z07. Its light is out.' }, robotHusk('L4z07', false), [0.35, 0.5, 0.3])
  memory('mid', { name: 'Photograph', line: 'Sun-bleached almost to nothing. Two small shapes waving from the ramp of something enormous.' }, photoMesh(), [0.25, 0.2, 0.08])
  memory('far', { name: 'Toy rocket', line: 'A toy rocket. Someone drew portholes on it in marker, and a little robot in the window.' }, toyRocket(), [0.15, 0.4, 0.15])
  memory('far', { name: 'Robot, sitting', line: 'Another one. L4z03. It sat down here facing the ship, and never got up.' }, robotHusk('L4z03', true), [0.35, 0.35, 0.3], true)

  // the ship sits beyond the heap, straight ahead from the dock
  buildShip(scene, shape, dockPos.clone().addScaledVector(toCentre, 380), Math.atan2(toCentre.x, toCentre.z))
  const lift = buildLift(world, scene, new THREE.Vector3(liftXZ[0], padH, liftXZ[1]), Math.atan2(dockPos.x - liftXZ[0], dockPos.z - liftXZ[1]))
  const scenery = buildScenery(scene, {
    heightAt,
    shipYaw,
    avoid: [
      { p: dockPos, r: 4 },
      { p: lift.pos, r: 6.5 },
      ...[...memories.values()].map((m) => ({ p: m.pos, r: 1.2 })),
    ],
  })
  return { dockPos, spawnPos, facing: toCentre, heightAt, memories, radioPos: radioAt, lift, scenery }
}

export type Lift = ReturnType<typeof buildLift>

/** A rusted service lift over a dark shaft: the way down to the Industrial Layer. */
function buildLift(world: RAPIER.World, scene: THREE.Scene, pos: THREE.Vector3, yaw: number) {
  const g = new THREE.Group()
  g.position.copy(pos)
  g.rotation.y = yaw
  const rust = 0x7a4a32
  // the shaft: a dark opening with a steel lip
  const hole = new THREE.Mesh(new THREE.PlaneGeometry(3, 3), new THREE.MeshBasicMaterial({ color: 0x0c0a0b }))
  hole.rotation.x = -Math.PI / 2
  hole.position.y = 0.04
  g.add(hole)
  for (const [x, z, w, d] of [[0, -1.6, 3.4, 0.2], [0, 1.6, 3.4, 0.2], [-1.6, 0, 0.2, 3.4], [1.6, 0, 0.2, 3.4]]) g.add(box(w, 0.25, d, 0x5a5f66, x, 0.1, z))
  // frame and winch
  for (const [x, z] of [[-1.6, -1.6], [1.6, -1.6], [-1.6, 1.6], [1.6, 1.6]]) g.add(box(0.22, 7, 0.22, rust, x, 3.5, z))
  g.add(box(3.6, 0.25, 0.25, rust, 0, 7, -1.6), box(3.6, 0.25, 0.25, rust, 0, 7, 1.6), box(0.25, 0.25, 3.6, rust, -1.6, 7, 0), box(0.25, 0.25, 3.6, rust, 1.6, 7, 0))
  for (const z of [-1.6, 1.6]) {
    const brace = box(0.1, 4.6, 0.1, rust, 0, 4.5, z)
    brace.rotation.z = 0.62
    g.add(brace)
  }
  g.add(box(1.4, 0.9, 1.0, 0x4f5a52, 0, 7.6, 0))
  const drum = new THREE.Mesh(new THREE.CylinderGeometry(0.35, 0.35, 1.2, 12), mat(0x2f2f2f))
  drum.rotation.z = Math.PI / 2
  drum.position.set(0, 7.3, 0.7)
  g.add(drum)
  // the cage, stuck halfway down its own shaft
  const cage = new THREE.Group()
  cage.add(box(2.7, 0.15, 2.7, 0x5c5248, 0, 0, 0))
  for (const [x, z] of [[-1.3, -1.3], [1.3, -1.3], [-1.3, 1.3], [1.3, 1.3]]) cage.add(box(0.08, 2.2, 0.08, 0x8a7a5c, x, 1.1, z))
  cage.add(box(2.7, 0.08, 0.08, 0x8a7a5c, 0, 1.1, -1.3), box(0.08, 0.08, 2.7, 0x8a7a5c, -1.3, 1.1, 0), box(0.08, 0.08, 2.7, 0x8a7a5c, 1.3, 1.1, 0))
  cage.add(box(2.7, 0.1, 2.7, 0x5c5248, 0, 2.2, 0))
  cage.position.y = -1.4
  cage.rotation.z = 0.06
  g.add(cage)
  const cable = new THREE.Mesh(new THREE.CylinderGeometry(0.03, 0.03, 1, 6), mat(0x222222))
  g.add(cable)
  // sign
  const c = document.createElement('canvas')
  c.width = 256
  c.height = 96
  const ctx = c.getContext('2d')!
  ctx.fillStyle = '#d9a441'
  ctx.fillRect(0, 0, 256, 96)
  ctx.fillStyle = '#2b2420'
  ctx.font = 'bold 30px monospace'
  ctx.textAlign = 'center'
  ctx.fillText('SERVICE LIFT', 128, 42)
  ctx.font = 'bold 22px monospace'
  ctx.fillText('LEVEL 1  ↓', 128, 76)
  const tex = new THREE.CanvasTexture(c)
  tex.colorSpace = THREE.SRGBColorSpace
  const sign = new THREE.Mesh(new THREE.PlaneGeometry(1.6, 0.6), new THREE.MeshStandardMaterial({ map: tex, roughness: 0.9 }))
  sign.position.set(0, 2.6, 1.72)
  g.add(sign)
  const lamp = new THREE.Mesh(new THREE.SphereGeometry(0.25, 10, 8), mat(0x333333))
  lamp.position.set(0, 8.25, 0)
  g.add(lamp)
  g.traverse((o) => (o.castShadow = true))
  scene.add(g)
  g.updateMatrixWorld(true)

  // colliders: posts, the lip and the winch. The shaft itself is fenced by the lip.
  const q = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), yaw)
  const body = world.createRigidBody(RAPIER.RigidBodyDesc.fixed().setTranslation(pos.x, pos.y, pos.z).setRotation(q))
  const handles = new Set<number>()
  const add = (hx: number, hy: number, hz: number, x: number, y: number, z: number) => handles.add(world.createCollider(RAPIER.ColliderDesc.cuboid(hx, hy, hz).setTranslation(x, y, z), body).handle)
  for (const [x, z] of [[-1.6, -1.6], [1.6, -1.6], [-1.6, 1.6], [1.6, 1.6]]) add(0.15, 3.5, 0.15, x, 3.5, z)
  add(1.6, 0.6, 0.1, 0, 0.6, -1.6)
  add(1.6, 0.6, 0.1, 0, 0.6, 1.6)
  add(0.1, 0.6, 1.6, -1.6, 0.6, 0)
  add(0.1, 0.6, 1.6, 1.6, 0.6, 0)
  add(0.8, 0.5, 0.6, 0, 7.6, 0)

  const setCable = () => {
    const top = 7.3
    const bottom = cage.position.y + 2.2
    cable.scale.y = top - bottom
    cable.position.set(0, (top + bottom) / 2, 0)
  }
  setCable()
  return {
    pos,
    handles,
    /** Animate the cage rising out of the shaft and the lamp coming on. 0..1 */
    setRepair(t: number) {
      cage.position.y = -1.4 + t * 1.5
      cage.rotation.z = 0.06 * (1 - t)
      lamp.material = t > 0 ? mat(0xffd38a, 0xffd38a) : mat(0x333333)
      setCable()
    },
  }
}

export interface Memory {
  name: string
  line: string
  pos: THREE.Vector3
  radio?: boolean
}

function stencil(text: string) {
  const c = document.createElement('canvas')
  c.width = 128
  c.height = 64
  const g = c.getContext('2d')!
  g.fillStyle = '#c9a25a'
  g.fillRect(0, 0, 128, 64)
  g.fillStyle = '#2b2420'
  g.font = 'bold 34px monospace'
  g.textAlign = 'center'
  g.textBaseline = 'middle'
  g.fillText(text, 64, 34)
  // wear: scratch some of the paint away
  g.globalCompositeOperation = 'destination-out'
  for (let i = 0; i < 40; i++) g.fillRect(Math.random() * 128, Math.random() * 64, 2 + Math.random() * 10, 1 + Math.random() * 2)
  const t = new THREE.CanvasTexture(c)
  t.colorSpace = THREE.SRGBColorSpace
  return new THREE.MeshStandardMaterial({ map: t, transparent: true, roughness: 0.9 })
}

/** A powered-down robot from Lazlo's series: same chassis, same stencil, dead lamp. */
function robotHusk(serial: string, sitting: boolean) {
  const g = new THREE.Group()
  const y = sitting ? -0.15 : 0
  g.add(box(0.55, 0.5, 0.45, 0xc9a25a, 0, y, 0))
  g.add(box(0.4, 0.3, 0.35, 0x8a7a5c, 0, y + 0.42, 0))
  const eye = new THREE.Mesh(new THREE.SphereGeometry(0.07, 8, 6), mat(0x2a2a2a))
  eye.position.set(0, y + 0.44, 0.18)
  g.add(eye)
  const plate = new THREE.Mesh(new THREE.PlaneGeometry(0.4, 0.2), stencil(serial))
  plate.position.set(0, y + 0.02, 0.227)
  g.add(plate)
  // arms hanging, treads half buried
  g.add(box(0.06, 0.4, 0.06, 0x6c6458, -0.32, y - 0.05, 0.05), box(0.06, 0.4, 0.06, 0x6c6458, 0.32, y - 0.1, 0.1))
  g.add(box(0.15, 0.2, 0.55, 0x3b3633, -0.25, y - 0.33, 0), box(0.15, 0.2, 0.55, 0x3b3633, 0.25, y - 0.33, 0))
  // tilt the head a little: it looks tired
  g.children[1].rotation.z = sitting ? 0.25 : -0.1
  if (sitting) g.children[1].rotation.x = 0.2
  return g
}

function radioMesh() {
  const g = new THREE.Group()
  g.add(box(0.7, 0.42, 0.28, 0x8c4a32))
  g.add(box(0.28, 0.28, 0.02, 0xd8c7a0, -0.15, 0, 0.14))
  g.add(box(0.2, 0.06, 0.02, 0xffd38a, 0.18, 0.08, 0.14))
  const knob = new THREE.Mesh(new THREE.CylinderGeometry(0.04, 0.04, 0.04, 10), mat(0x222222))
  knob.rotation.x = Math.PI / 2
  knob.position.set(0.18, -0.08, 0.15)
  g.add(knob, box(0.02, 0.5, 0.02, 0x999999, 0.3, 0.45, 0))
  return g
}

function photoMesh() {
  const g = new THREE.Group()
  const frame = box(0.5, 0.4, 0.04, 0x5b4636)
  frame.rotation.x = -0.25
  g.add(frame)
  const pic = box(0.42, 0.32, 0.01, 0xe8dcc0, 0, 0, 0.025)
  pic.rotation.x = -0.25
  g.add(pic)
  g.add(box(0.04, 0.3, 0.04, 0x5b4636, 0, -0.08, -0.12))
  return g
}

function toyRocket() {
  const g = new THREE.Group()
  const body = new THREE.Mesh(new THREE.CylinderGeometry(0.1, 0.12, 0.55, 10), mat(0xe9e4d6))
  const nose = new THREE.Mesh(new THREE.ConeGeometry(0.1, 0.22, 10), mat(0xd1495b))
  nose.position.y = 0.38
  g.add(body, nose)
  for (let i = 0; i < 3; i++) {
    const fin = box(0.02, 0.18, 0.14, 0xd1495b, Math.cos((i * Math.PI * 2) / 3) * 0.12, -0.22, Math.sin((i * Math.PI * 2) / 3) * 0.12)
    fin.rotation.y = (-i * Math.PI * 2) / 3
    g.add(fin)
  }
  const port = new THREE.Mesh(new THREE.CircleGeometry(0.045, 12), mat(0x4cc9f0))
  port.position.set(0, 0.08, 0.105)
  g.add(port)
  g.rotation.z = 0.5
  return g
}

/** The colossal buried ship: always visible on the horizon, never reachable (yet). */
function buildShip(scene: THREE.Scene, shape: HeapShape, at: THREE.Vector3, yaw: number) {
  const g = new THREE.Group()
  // drawn in its own haze colour instead of through the fog, so it always reads as a silhouette
  const hull = new THREE.MeshBasicMaterial({ color: 0x8a7891, fog: false })
  const body = new THREE.Mesh(new THREE.CapsuleGeometry(28, 150, 6, 16), hull)
  body.rotation.z = Math.PI / 2
  body.rotation.y = 0.35
  g.add(body)
  const fin = new THREE.Mesh(new THREE.BoxGeometry(50, 60, 6), hull)
  fin.position.set(55, 40, -10)
  fin.rotation.z = -0.4
  g.add(fin)
  const tower = new THREE.Mesh(new THREE.CylinderGeometry(6, 10, 70, 8), hull)
  tower.position.set(-30, 45, 5)
  g.add(tower)
  for (let i = 0; i < 6; i++) {
    const light = new THREE.Mesh(new THREE.SphereGeometry(2.2, 8, 6), new THREE.MeshBasicMaterial({ color: 0xbfe8ff, fog: false }))
    light.position.set(-60 + i * 22, 10 + Math.sin(i) * 6, 26)
    g.add(light)
  }
  const tilted = new THREE.Group()
  tilted.add(g)
  g.rotation.z = 0.12
  g.position.y = shape === 'pit' ? 14 : 4
  tilted.position.set(at.x, 0, at.z)
  tilted.rotation.y = yaw + Math.PI / 2
  scene.add(tilted)
}
