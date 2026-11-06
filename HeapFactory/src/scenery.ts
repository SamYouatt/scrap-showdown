import * as THREE from 'three'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import { mat } from './entities'
import { fbm, mulberry32, smoothstep } from './noise'

/**
 * Pure decoration, no physics: the junk carpet that makes the heap read as a heap,
 * weeds pushing up through it, and a skyline of distant junk mountains and landmarks.
 */

interface Opts {
  heightAt: (x: number, z: number) => number
  /** Points (with radius) where the ground must stay clear: dock, lift, memories. */
  avoid: { p: THREE.Vector3; r: number }[]
  /** Direction from the play area towards the ship; the skyline leaves a gap for it. */
  shipYaw: number
}

const RADIUS = 124
const UP = new THREE.Vector3(0, 1, 0)

function normalAt(h: Opts['heightAt'], x: number, z: number) {
  const e = 0.6
  return new THREE.Vector3(h(x - e, z) - h(x + e, z), 2 * e, h(x, z - e) - h(x, z + e)).normalize()
}

function carGeometry() {
  const body = new THREE.BoxGeometry(3.8, 0.75, 1.7)
  body.translate(0, 0.45, 0)
  const cabin = new THREE.BoxGeometry(2.0, 0.6, 1.5)
  cabin.translate(-0.2, 1.1, 0)
  return mergeGeometries([body, cabin])!
}

function applianceGeometry() {
  const b = new THREE.BoxGeometry(0.7, 0.85, 0.65)
  const door = new THREE.CylinderGeometry(0.22, 0.22, 0.05, 12)
  door.rotateX(Math.PI / 2)
  door.translate(0, -0.05, 0.34)
  return mergeGeometries([b.toNonIndexed(), door.toNonIndexed()])!
}

interface DecorKind {
  geo: THREE.BufferGeometry
  count: number
  colors: number[]
  /** Random scale range applied per axis. */
  scale: [number, number]
  stretch?: boolean
  /** How far it sinks into the heap, as a fraction of its height. */
  bury: number
  tilt: number
  /** Approximate footprint radius, for keeping clear areas tidy. */
  size: number
}

export function buildScenery(scene: THREE.Scene, o: Opts) {
  const rng = mulberry32(4242)
  const meshes: { mesh: THREE.InstancedMesh; pos: THREE.Vector3[]; mats: THREE.Matrix4[] }[] = []

  const RUST = [0x8a4b2e, 0x9b5a34, 0x6e3b26, 0xa8693e]
  const PAINT = [0x2f5d7c, 0x9b3b2e, 0x3f6b48, 0xb8a66a, 0x5d7f7a, 0xc9a25a, 0x7a3b4a, 0x445566]
  const GREY = [0x6f747a, 0x8a8f96, 0x5a5f66, 0x9aa3ad]
  const WHITE = [0xe9e4d6, 0xd5dbe0, 0xcfc6b0, 0xbdb5a0]
  const PLASTIC = [0xe76f51, 0xf4a261, 0xe9c46a, 0x4cc9f0, 0x90be6d, 0x2a9d8f, 0xf28482]

  const kinds: DecorKind[] = [
    { geo: new THREE.BoxGeometry(1, 1, 1), count: 3200, colors: [...RUST, ...GREY, ...PAINT], scale: [0.25, 1.1], stretch: true, bury: 0.45, tilt: 0.7, size: 0.6 },
    { geo: new THREE.BoxGeometry(1.4, 0.05, 0.9), count: 1500, colors: [...RUST, ...GREY, ...PAINT], scale: [0.6, 1.6], bury: 0.2, tilt: 0.6, size: 0.9 },
    { geo: new THREE.CylinderGeometry(0.3, 0.3, 0.9, 10), count: 320, colors: [...PAINT, ...RUST, 0x2b2b2b], scale: [0.85, 1.15], bury: 0.35, tilt: 1.6, size: 0.5 },
    { geo: new THREE.TorusGeometry(0.34, 0.13, 6, 14), count: 800, colors: [0x262424, 0x2f2b2a, 0x1e1d1d], scale: [0.8, 1.3], bury: 0.3, tilt: 1.4, size: 0.5 },
    { geo: new THREE.CylinderGeometry(0.07, 0.07, 2.4, 6), count: 600, colors: [...GREY, ...RUST], scale: [0.6, 1.4], bury: 0.1, tilt: 1.5, size: 1.2 },
    { geo: applianceGeometry(), count: 220, colors: WHITE, scale: [0.9, 1.15], bury: 0.4, tilt: 0.9, size: 0.5 },
    { geo: new THREE.IcosahedronGeometry(0.22, 0), count: 1800, colors: PLASTIC, scale: [0.6, 1.5], stretch: true, bury: 0.3, tilt: 3, size: 0.3 },
    { geo: carGeometry(), count: 70, colors: [...PAINT, ...RUST], scale: [0.9, 1.05], bury: 0.35, tilt: 0.45, size: 2.2 },
  ]

  const clear = (x: number, z: number, size: number) => o.avoid.every((a) => Math.hypot(x - a.p.x, z - a.p.z) > a.r + size)
  const q = new THREE.Quaternion()
  const tilt = new THREE.Quaternion()
  const color = new THREE.Color()

  for (const k of kinds) {
    const material = new THREE.MeshStandardMaterial({ roughness: 0.9, metalness: 0.05, flatShading: true })
    const mesh = new THREE.InstancedMesh(k.geo, material, k.count)
    mesh.castShadow = true
    mesh.receiveShadow = true
    k.geo.computeBoundingBox()
    const height = k.geo.boundingBox!.max.y - k.geo.boundingBox!.min.y
    const pos: THREE.Vector3[] = []
    const mats: THREE.Matrix4[] = []
    let n = 0
    for (let tries = 0; n < k.count && tries < k.count * 8; tries++) {
      const a = rng() * Math.PI * 2
      const r = Math.sqrt(rng()) * RADIUS
      const x = Math.cos(a) * r
      const z = Math.sin(a) * r
      // clumpy: junk collects in patches, with bare runs between
      if (fbm(x * 0.05, z * 0.05, 77) * 0.5 + 0.5 < rng() * 0.6) continue
      if (!clear(x, z, k.size)) continue
      const s = k.scale[0] + rng() * (k.scale[1] - k.scale[0])
      const scale = k.stretch ? new THREE.Vector3(s * (0.5 + rng()), s * (0.4 + rng() * 0.8), s * (0.5 + rng())) : new THREE.Vector3(s, s, s)
      const n0 = normalAt(o.heightAt, x, z)
      q.setFromUnitVectors(UP, n0)
      tilt.setFromEuler(new THREE.Euler((rng() - 0.5) * k.tilt, rng() * Math.PI * 2, (rng() - 0.5) * k.tilt))
      q.multiply(tilt)
      const y = o.heightAt(x, z) + height * scale.y * (0.5 - k.bury)
      const p = new THREE.Vector3(x, y, z)
      const m = new THREE.Matrix4().compose(p, q, scale)
      mesh.setMatrixAt(n, m)
      // a little sun-bleaching and grime per piece
      color.setHex(k.colors[Math.floor(rng() * k.colors.length)]).multiplyScalar(0.75 + rng() * 0.35)
      mesh.setColorAt(n, color)
      pos.push(p)
      mats.push(m)
      n++
    }
    mesh.count = n
    mesh.instanceMatrix.needsUpdate = true
    if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true
    scene.add(mesh)
    meshes.push({ mesh, pos, mats })
  }

  const weeds = buildWeeds(scene, o, rng, clear)
  buildSkyline(scene, o, rng)

  return {
    /** Hide decoration under something the player built. */
    clearAround(p: THREE.Vector3, r: number) {
      const zero = new THREE.Matrix4().makeScale(0, 0, 0)
      for (const { mesh, pos } of [...meshes, weeds]) {
        let changed = false
        pos.forEach((q, i) => {
          if (Math.hypot(q.x - p.x, q.z - p.z) < r) {
            mesh.setMatrixAt(i, zero)
            changed = true
          }
        })
        if (changed) mesh.instanceMatrix.needsUpdate = true
      }
    },
    update(time: number) {
      weeds.uniforms.uTime.value = time
    },
  }
}

/** Grass and wildflowers growing through the junk: nature slowly coming back. */
function buildWeeds(scene: THREE.Scene, o: Opts, rng: () => number, clear: (x: number, z: number, s: number) => boolean) {
  const c = document.createElement('canvas')
  c.width = 128
  c.height = 128
  const g = c.getContext('2d')!
  for (let i = 0; i < 26; i++) {
    const x = 20 + rng() * 88
    const h = 50 + rng() * 75
    const lean = (rng() - 0.5) * 40
    g.strokeStyle = `hsl(${70 + rng() * 40}, ${35 + rng() * 25}%, ${55 + rng() * 25}%)`
    g.lineWidth = 2 + rng() * 3
    g.beginPath()
    g.moveTo(x, 128)
    g.quadraticCurveTo(x + lean * 0.3, 128 - h * 0.6, x + lean, 128 - h)
    g.stroke()
  }
  for (let i = 0; i < 5; i++) {
    g.fillStyle = rng() < 0.5 ? '#fff4d6' : '#ffd166'
    g.beginPath()
    g.arc(25 + rng() * 78, 20 + rng() * 50, 3 + rng() * 3, 0, Math.PI * 2)
    g.fill()
  }
  const tex = new THREE.CanvasTexture(c)
  tex.colorSpace = THREE.SRGBColorSpace

  const a = new THREE.PlaneGeometry(1, 1)
  a.translate(0, 0.5, 0)
  const b = a.clone().rotateY(Math.PI / 2)
  const geo = mergeGeometries([a, b])!
  const uniforms = { uTime: { value: 0 } }
  const material = new THREE.MeshStandardMaterial({ map: tex, alphaTest: 0.45, side: THREE.DoubleSide, roughness: 1 })
  material.onBeforeCompile = (shader) => {
    shader.uniforms.uTime = uniforms.uTime
    shader.vertexShader = 'uniform float uTime;\n' + shader.vertexShader.replace(
      '#include <begin_vertex>',
      `#include <begin_vertex>
      vec3 root = instanceMatrix[3].xyz;
      float sway = sin(uTime * 1.6 + root.x * 0.35 + root.z * 0.25) + 0.4 * sin(uTime * 3.1 + root.z);
      transformed.x += sway * 0.09 * uv.y * uv.y;
      transformed.z += sway * 0.05 * uv.y * uv.y;`,
    )
  }
  const COUNT = 2600
  const mesh = new THREE.InstancedMesh(geo, material, COUNT)
  mesh.receiveShadow = true
  const pos: THREE.Vector3[] = []
  const color = new THREE.Color()
  let n = 0
  for (let tries = 0; n < COUNT && tries < COUNT * 10; tries++) {
    const ang = rng() * Math.PI * 2
    const r = Math.sqrt(rng()) * RADIUS
    const x = Math.cos(ang) * r
    const z = Math.sin(ang) * r
    // weeds like sheltered, flatter ground and come in drifts
    const slope = 1 - normalAt(o.heightAt, x, z).y
    if (slope > 0.25 || fbm(x * 0.08, z * 0.08, 91) < 0.05 + rng() * 0.4) continue
    if (!clear(x, z, 0.3)) continue
    const s = 0.5 + rng() * 0.9
    const p = new THREE.Vector3(x, o.heightAt(x, z) - 0.05, z)
    mesh.setMatrixAt(n, new THREE.Matrix4().compose(p, new THREE.Quaternion().setFromAxisAngle(UP, rng() * Math.PI), new THREE.Vector3(s, s * (0.7 + rng() * 0.6), s)))
    // greener in the hollows, straw-coloured on the exposed tops
    color.setHSL(0.17 + rng() * 0.08 - slope * 0.2, 0.45, 0.42 + rng() * 0.15)
    mesh.setColorAt(n, color)
    pos.push(p)
    n++
  }
  mesh.count = n
  scene.add(mesh)
  return { mesh, pos, uniforms }
}

/** Land rolling away beyond the heap, distant junk mountains, and silhouetted landmarks. */
function buildSkyline(scene: THREE.Scene, o: Opts, rng: () => number) {
  // --- far ground: rolling junk that meets the heap's outer walls and climbs to the mountains
  const skirtH = (x: number, z: number) => {
    const r = Math.hypot(x, z)
    return smoothstep(128, 200, r) * (8 + 9 * fbm(x * 0.012, z * 0.012, 31) + (r - 128) * 0.04) - 3
  }
  const ring = new THREE.RingGeometry(120, 900, 160, 40)
  ring.rotateX(-Math.PI / 2)
  const rp = ring.attributes.position as THREE.BufferAttribute
  const cols = new Float32Array(rp.count * 3)
  const c = new THREE.Color()
  for (let i = 0; i < rp.count; i++) {
    const x = rp.getX(i)
    const z = rp.getZ(i)
    rp.setY(i, skirtH(x, z))
    c.setHex(0x7a5f4c).lerp(new THREE.Color(0x5d4f5a), fbm(x * 0.02, z * 0.02, 33) * 0.5 + 0.5)
    cols.set([c.r, c.g, c.b], i * 3)
  }
  ring.setAttribute('color', new THREE.BufferAttribute(cols, 3))
  ring.computeVertexNormals()
  scene.add(new THREE.Mesh(ring, new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 1, flatShading: true })))

  const gapOk = (a: number, width: number) => Math.abs(Math.atan2(Math.sin(a - o.shipYaw), Math.cos(a - o.shipYaw))) > width
  const polar = (r: number, a: number) => new THREE.Vector3(Math.sin(a) * r, 0, Math.cos(a) * r)

  // --- distant junk mountains, fading into the haze
  const mountMat = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 1, flatShading: true })
  for (let i = 0; i < 22; i++) {
    const a = rng() * Math.PI * 2
    if (!gapOk(a, 0.35)) continue
    const p = polar(260 + rng() * 260, a)
    const h = 35 + rng() * 90
    const geo = new THREE.ConeGeometry(50 + rng() * 80, h, 12, 6)
    const gp = geo.attributes.position as THREE.BufferAttribute
    const mc = new Float32Array(gp.count * 3)
    for (let j = 0; j < gp.count; j++) {
      const x = gp.getX(j)
      const y = gp.getY(j)
      const z = gp.getZ(j)
      const n = fbm(x * 0.05 + i, z * 0.05, 61)
      const k = 1 + n * 0.35
      gp.setXYZ(j, x * k, y + n * h * 0.12, z * k)
      c.setHex(0x6a4c3e).lerp(new THREE.Color(0x8a6a5a), (y / h + 0.5) * 0.6)
      mc.set([c.r, c.g, c.b], j * 3)
    }
    geo.setAttribute('color', new THREE.BufferAttribute(mc, 3))
    geo.computeVertexNormals()
    const m = new THREE.Mesh(geo, mountMat)
    m.position.set(p.x, skirtH(p.x, p.z) + h / 2 - 6, p.z)
    m.rotation.y = rng() * Math.PI
    scene.add(m)
  }

  // --- landmarks: each sits at a different bearing so there is always something on the horizon
  const place = (obj: THREE.Object3D, rel: number, r: number) => {
    const a = o.shipYaw + rel
    const p = polar(r, a)
    obj.position.set(p.x, skirtH(p.x, p.z) - 1, p.z)
    obj.rotation.y = a + Math.PI / 2
    obj.traverse((m) => (m.castShadow = false))
    scene.add(obj)
  }
  place(crane(), 0.9, 175)
  place(ferrisWheel(), -1.15, 200)
  place(carTower(rng), 2.1, 165)
  place(mast(), -2.3, 185)
  place(crane(), Math.PI + 0.2, 230)
  place(carTower(rng), -0.55, 240)
  // a line of old power pylons marching across the land
  for (let i = 0; i < 7; i++) place(pylon(), 1.5 + i * 0.16, 150 + i * 6)
}

function box(w: number, h: number, d: number, color: number, x = 0, y = 0, z = 0, rz = 0) {
  const m = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), mat(color))
  m.position.set(x, y, z)
  m.rotation.z = rz
  return m
}

function crane() {
  const g = new THREE.Group()
  const col = 0xc9a25a
  for (let y = 0; y < 48; y += 4) {
    g.add(box(2.4, 0.3, 0.3, col, 0, y, 1.2), box(2.4, 0.3, 0.3, col, 0, y, -1.2), box(0.3, 0.3, 2.4, col, 1.2, y, 0), box(0.3, 0.3, 2.4, col, -1.2, y, 0))
  }
  for (const [x, z] of [[-1.2, -1.2], [1.2, -1.2], [-1.2, 1.2], [1.2, 1.2]]) g.add(box(0.4, 50, 0.4, col, x, 25, z))
  g.add(box(46, 1.6, 1.6, col, 12, 50, 0), box(8, 4, 3, 0x5a5f66, -9, 48, 0), box(3, 3, 3, 0x6b4a32, 0, 52, 0))
  // the jib droops a little, and a cable still hangs from it
  g.children.at(-3)!.rotation.z = -0.04
  g.add(box(0.12, 22, 0.12, 0x222222, 30, 38, 0), box(2, 1.4, 1.4, 0x8a4b2e, 30, 27, 0))
  return g
}

function ferrisWheel() {
  const g = new THREE.Group()
  const col = 0xd9cbb0
  const wheel = new THREE.Group()
  wheel.add(new THREE.Mesh(new THREE.TorusGeometry(22, 0.5, 6, 40), mat(col)))
  for (let i = 0; i < 16; i++) {
    const a = (i / 16) * Math.PI * 2
    const spoke = box(0.3, 22, 0.3, col, Math.cos(a) * 11, Math.sin(a) * 11, 0)
    spoke.rotation.z = a + Math.PI / 2
    wheel.add(spoke)
    if (i % 2 === 0 && i !== 6) wheel.add(box(2.2, 2.4, 2.2, [0xd1495b, 0x2a9d8f, 0xe9c46a, 0x4cc9f0][i % 4], Math.cos(a) * 22, Math.sin(a) * 22 - 1.6, 0))
  }
  wheel.position.y = 26
  wheel.rotation.z = 0.2
  // it has slumped sideways, half off its stand
  wheel.rotation.x = 0.18
  g.add(wheel)
  for (const s of [-1, 1]) {
    g.add(box(0.8, 30, 0.8, col, s * 7, 13, 2, s * 0.28), box(0.8, 30, 0.8, col, s * 7, 13, -2, s * 0.28))
  }
  return g
}

function carTower(rng: () => number) {
  const g = new THREE.Group()
  let y = 0
  let lean = 0
  const colors = [0x2f5d7c, 0x9b3b2e, 0x3f6b48, 0xb8a66a, 0x5d7f7a, 0xe9e4d6, 0x7a3b4a]
  for (let i = 0; i < 14; i++) {
    lean += (rng() - 0.3) * 0.5
    const car = box(4.4, 1.5, 2, colors[Math.floor(rng() * colors.length)], lean, y + 0.75, 0)
    car.rotation.y = (rng() - 0.5) * 1.2
    car.rotation.z = (rng() - 0.5) * 0.15
    g.add(car)
    y += 1.4
  }
  g.scale.setScalar(1.6)
  return g
}

function mast() {
  const g = new THREE.Group()
  const col = 0x9b3b2e
  for (let y = 0; y < 60; y += 5) {
    const w = 3 - y * 0.04
    g.add(box(w, 0.25, 0.25, y % 10 ? 0xe9e4d6 : col, 0, y, 0))
  }
  for (const s of [-1, 1]) g.add(box(0.35, 62, 0.35, col, s * 1.6, 31, 0, s * -0.045))
  const light = new THREE.Mesh(new THREE.SphereGeometry(0.8, 8, 6), new THREE.MeshBasicMaterial({ color: 0xff5544, fog: false }))
  light.position.y = 62
  g.add(light)
  g.rotation.z = 0.08 // leaning
  return g
}

function pylon() {
  const g = new THREE.Group()
  const col = 0x4a4f57
  g.add(box(0.5, 26, 0.5, col, -2, 13, 0, -0.08), box(0.5, 26, 0.5, col, 2, 13, 0, 0.08))
  g.add(box(10, 0.5, 0.5, col, 0, 22, 0), box(7, 0.5, 0.5, col, 0, 25, 0))
  for (let y = 4; y < 22; y += 4.5) {
    const brace = box(0.25, 6, 0.25, col, 0, y, 0, 0.7)
    g.add(brace)
  }
  return g
}
