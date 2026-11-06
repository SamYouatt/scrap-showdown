import * as THREE from 'three'
import RAPIER from '@dimforge/rapier3d-compat'
import { BALE_COLORS, Entities, ITEMS, STOCK_NAMES, box, entityName, type BaleKind, type Entity, type Stock } from './entities'
import { DOCK_SUPPLY, Factory, TOOLS, TOOL_ORDER, type Tool } from './factory'
import { HEAP_SHAPES, buildHeap, type HeapShape, type Memory } from './heap'
import { Inventory, describeCost } from './inventory'
import { Player } from './player'
import { createPost } from './post'
import { HORIZON, createAtmosphere } from './atmosphere'
import { Sound } from './audio'
import { LIFT_NEEDS, Objectives, liftRemaining, type Flag, type LiftPart } from './objectives'

await RAPIER.init()

const params = new URLSearchParams(location.search)
const shapeParam = params.get('heap') as HeapShape | null
const shape: HeapShape = shapeParam && shapeParam in HEAP_SHAPES ? shapeParam : 'basin'

// ---------- rendering ----------
const renderer = new THREE.WebGLRenderer({ antialias: true })
renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
renderer.setSize(innerWidth, innerHeight)
renderer.shadowMap.enabled = true
renderer.toneMapping = THREE.ACESFilmicToneMapping
renderer.toneMappingExposure = 1.05
document.body.prepend(renderer.domElement)

const scene = new THREE.Scene()
scene.background = HORIZON.clone()
scene.fog = new THREE.Fog(HORIZON, 40, 520)
const camera = new THREE.PerspectiveCamera(72, innerWidth / innerHeight, 0.05, 1200)
scene.add(camera)

// warm key light from a low sun, cool violet fill: shadows are coloured, never black
scene.add(new THREE.HemisphereLight(0xffd6b0, 0x5a4468, 2.0))
const sun = new THREE.DirectionalLight(0xffb978, 3.2)
const SUN_DIR = new THREE.Vector3(-0.3, 0.2, -0.93).normalize() // low, golden-hour sun just above the ship
sun.castShadow = true
sun.shadow.mapSize.set(2048, 2048)
Object.assign(sun.shadow.camera, { left: -45, right: 45, top: 45, bottom: -45, near: 1, far: 260 })
sun.shadow.bias = -0.0008
scene.add(sun, sun.target)

const post = createPost(renderer, scene, camera)
const atmosphere = createAtmosphere(scene, SUN_DIR)
const sound = new Sound()

addEventListener('resize', () => {
  camera.aspect = innerWidth / innerHeight
  camera.updateProjectionMatrix()
  renderer.setSize(innerWidth, innerHeight)
})

// ---------- world ----------
const world = new RAPIER.World({ x: 0, y: -9.81, z: 0 })
world.timestep = 1 / 60
const entities = new Entities(world, scene)
const inventory = new Inventory()
const heap = buildHeap(shape, world, scene, entities)
const factory = new Factory(world, scene, entities, inventory)
factory.addNode('dock', heap.dockPos)
factory.onCompact = () => sound.compact()
factory.onBale = () => {
  sound.bale()
  flags.add('baled')
}
factory.onDeck = (p) => heap.scenery.clearAround(p, 4.6)
const flags = new Set<Flag>()
const delivered: Record<LiftPart, number> = { steel: 0, mixed: 0, motor: 0, battery: 0 }
const objectives = new Objectives(scene, { inventory, factory, entities, lift: heap.lift, dock: heap.dockPos, delivered, flags })
objectives.onStep = (intro) => intro && setTimeout(() => caption(intro), 600)
let repairT = 0
const player = new Player(world, heap.spawnPos)
player.yaw = Math.atan2(-heap.facing.x, -heap.facing.z)

// Lazlo's arm, visible in the bottom right of the view
const arm = new THREE.Group()
arm.add(box(0.035, 0.035, 0.3, 0x8a8f96, 0, 0, 0), box(0.06, 0.05, 0.05, 0x5f646b, 0, 0, -0.16))
const clawL = box(0.012, 0.012, 0.07, 0xd9a441, -0.025, 0, -0.21)
const clawR = box(0.012, 0.012, 0.07, 0xd9a441, 0.025, 0, -0.21)
arm.add(clawL, clawR)
arm.position.set(0.24, -0.2, -0.38)
arm.rotation.set(0.15, 0.3, 0)
arm.scale.setScalar(0.6)
arm.traverse((o) => (o.castShadow = false))
camera.add(arm)

// ---------- state ----------
let started = false
let tool: Tool | null = null
let rotOffset = 0
let held: Entity | null = null
let charge = 100
let downTimer = 0
let jumpQueued = false
let stepTimer = 0
const keys = new Set<string>()
const REACH = 3.4

// ---------- HUD ----------
const $ = (s: string) => document.querySelector(s) as HTMLElement
const toast = (msg: string) => {
  const el = document.createElement('div')
  el.textContent = msg
  $('#toasts').append(el)
  setTimeout(() => el.remove(), 3200)
}

let captionTimer = 0
const caption = (msg: string) => {
  const el = $('#caption')
  el.textContent = msg
  el.style.opacity = '1'
  clearTimeout(captionTimer)
  captionTimer = window.setTimeout(() => (el.style.opacity = '0'), 5500)
}

const overlay = $('#overlay')
const shapesEl = $('#overlay .shapes')
for (const [key, s] of Object.entries(HEAP_SHAPES)) {
  const b = document.createElement('button')
  b.className = key === shape ? 'on' : ''
  b.innerHTML = `${s.name}<small>${s.blurb}</small>`
  b.onclick = () => {
    if (key !== shape) location.search = `?heap=${key}`
  }
  shapesEl.append(b)
}
let woken = false
$('#overlay .go').onclick = () => {
  started = true
  overlay.style.display = 'none'
  sound.start()
  renderer.domElement.requestPointerLock()
  if (!woken) {
    // L4z10 wakes up: fade in from black, servos spinning up
    woken = true
    const fade = $('#fade')
    fade.style.transition = 'none'
    fade.style.opacity = '1'
    fade.textContent = ''
    sound.powerUp()
    setTimeout(() => {
      fade.style.transition = 'opacity 3s'
      fade.style.opacity = '0'
      setTimeout(() => (fade.style.transition = ''), 3000)
      setTimeout(() => caption('Charge low. Signal faint, somewhere below.'), 2500)
    }, 400)
  }
}
renderer.domElement.addEventListener('click', () => {
  if (started && document.pointerLockElement !== renderer.domElement) renderer.domElement.requestPointerLock()
})
document.addEventListener('pointerlockchange', () => {
  if (!document.pointerLockElement && started) {
    started = false
    overlay.style.display = 'flex'
    $('#overlay .go').textContent = 'Click to resume'
  }
})

const toolbar = $('#toolbar')
const slots = TOOL_ORDER.map((t, i) => {
  const el = document.createElement('div')
  el.className = 'slot'
  el.innerHTML = `<span class="k">${i + 1}</span>${TOOLS[t].name}<span class="c">${describeCost(TOOLS[t].cost)}</span>`
  toolbar.append(el)
  return el
})

const STOCK_ORDER: Stock[] = ['steel', 'plastic', 'mixed', 'motor', 'compressor', 'battery', 'chip']
function renderHud(aimText: string) {
  const pct = Math.max(0, charge)
  $('#battery .fill').style.width = `${pct}%`
  $('#charge-text').textContent = `${pct.toFixed(0)}%`
  const zone = factory.zoneAt(player.feet)
  const d = factory.distanceToGrid(player.feet)
  const returnCost = zone ? 0 : (d / 4.5) * 1.2 * (1 + d / 160) * 1.15
  const low = !zone && charge < returnCost * 1.4 + 5
  $('#battery').classList.toggle('low', low)
  $('#battery .meta').textContent =
    zone === 'dock'
      ? factory.supply - factory.demand > 0.01
        ? 'Charging at dock'
        : 'Dock: grid overloaded, charging slowly'
      : zone === 'grid'
        ? 'On the grid: no drain'
        : low
          ? `Low charge. Head back (≈${returnCost.toFixed(0)}% to reach the grid)`
          : `Off grid · ${d.toFixed(0)} m out · return ≈ ${returnCost.toFixed(0)}%`

  const sat = factory.satisfaction
  $('#grid').innerHTML =
    `Grid <b>${factory.demand.toFixed(1)} / ${factory.supply.toFixed(0)}</b> power` +
    (sat < 1 ? ` <span style="color:var(--warn)">· brownout ${Math.round(sat * 100)}%</span>` : '') +
    `<br><span style="color:var(--dim);font-size:12px">${factory.machines.filter((m) => m.powered).length} machines running · dock ${DOCK_SUPPLY} + solar ${factory.supply - DOCK_SUPPLY}</span>` +
    (inventory.free ? '<br><span style="color:var(--warn);font-size:12px">FREE BUILD</span>' : '')

  $('#inv').innerHTML =
    '<h4>PACK</h4>' +
    STOCK_ORDER.map((s, i) => {
      const n = inventory.stock[s]
      const sw = i < 3 ? `<span class="sw" style="background:#${BALE_COLORS[s as BaleKind].toString(16).padStart(6, '0')}"></span>` : '<span class="sw" style="background:#bbb;border-radius:50%"></span>'
      return `${i === 3 ? '<h4 style="margin-top:8px">PARTS</h4>' : ''}<div class="row ${n ? '' : 'zero'}"><span>${sw}${STOCK_NAMES[s]}</span><span>${n}</span></div>`
    }).join('')

  slots.forEach((el, i) => {
    const t = TOOL_ORDER[i]
    el.classList.toggle('on', t === tool)
    el.classList.toggle('cant', !inventory.canAfford(TOOLS[t].cost))
  })
  $('#prompt').innerHTML = aimText
}

// ---------- aiming ----------
interface Aim {
  point: THREE.Vector3
  normal: THREE.Vector3
  dist: number
  collider: RAPIER.Collider
  entity?: Entity
  info?: ReturnType<Factory['info']>
  memory?: Memory
  lift?: boolean
}

function aim(maxDist = 8): Aim | null {
  const origin = camera.getWorldPosition(new THREE.Vector3())
  const dir = camera.getWorldDirection(new THREE.Vector3())
  const hit = world.castRayAndGetNormal(new RAPIER.Ray(origin, dir), maxDist, true, undefined, undefined, player.collider, held?.body)
  if (!hit) return null
  const point = origin.addScaledVector(dir, hit.timeOfImpact)
  const normal = new THREE.Vector3(hit.normal.x, hit.normal.y, hit.normal.z)
  return { point, normal, dist: hit.timeOfImpact, collider: hit.collider, entity: entities.fromCollider(hit.collider), info: factory.info(hit.collider), memory: heap.memories.get(hit.collider.handle), lift: heap.lift.handles.has(hit.collider.handle) }
}

// ---------- build ghost ----------
const ghostOk = new THREE.MeshBasicMaterial({ color: 0x7ff0c8, transparent: true, opacity: 0.35, depthWrite: false })
const ghostBad = new THREE.MeshBasicMaterial({ color: 0xff6b5b, transparent: true, opacity: 0.35, depthWrite: false })
const ghost = new THREE.Group()
scene.add(ghost)
function setGhost(t: Tool | null) {
  ghost.clear()
  if (!t) return
  const sizes: Record<Tool, [number, number, number, number]> = {
    deck: [6, 0.2, 6, 0],
    belt: [0.96, 0.1, 0.96, 0.15],
    magnet: [1, 1.5, 1, 0.85],
    compactor: [0.9, 0.9, 0.9, 0.55],
    pylon: [0.2, 3.2, 0.2, 1.6],
    solar: [1.4, 1, 0.9, 0.5],
  }
  const [w, h, d, y] = sizes[t]
  const m = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), ghostOk)
  m.position.y = y
  ghost.add(m)
  if (t !== 'pylon' && t !== 'solar') {
    const a = new THREE.Mesh(new THREE.ConeGeometry(0.18, 0.4, 3), ghostOk)
    a.rotation.x = -Math.PI / 2
    a.position.set(0, y + h / 2 + 0.25, -0.2)
    ghost.add(a)
  }
}
function tintGhost(ok: boolean) {
  ghost.traverse((o) => {
    if (o instanceof THREE.Mesh) o.material = ok ? ghostOk : ghostBad
  })
}

/** Works out what the current tool would do at the aim point. */
function buildPlan(a: Aim | null) {
  if (!tool) return null
  const t = TOOLS[tool]
  const fail = (reason: string) => ({ ok: false as const, reason })
  if (!a) return fail('Out of reach')
  if (a.entity) return fail('Aim at solid ground, not loose junk')
  const afford = inventory.canAfford(t.cost)
  if (tool === 'deck') {
    const tr = factory.deckTransform(a.point, a.normal, player.yaw)
    ghost.position.copy(tr.pos)
    ghost.quaternion.copy(tr.q)
    const reason = tr.reason ?? (afford ? null : `Need ${inventory.missing(t.cost).join(', ')}`)
    return reason ? fail(reason) : { ok: true as const, place: () => factory.placeDeck(tr.pos, tr.q) }
  }
  if (tool === 'pylon' || tool === 'solar') {
    ghost.position.copy(a.point)
    ghost.quaternion.identity()
    if (a.normal.angleTo(new THREE.Vector3(0, 1, 0)) > THREE.MathUtils.degToRad(40)) return fail('Too steep')
    if (!afford) return fail(`Need ${inventory.missing(t.cost).join(', ')}`)
    const linked = factory.linkPreview(a.point)
    const p = a.point.clone()
    const kind = tool
    return { ok: true as const, note: linked ? 'Links to the grid' : 'Out of link range: will not be powered', place: () => factory.addNode(kind, p) }
  }
  // machines snap to a deck's grid
  const deck = a.info?.deck
  if (!deck) return fail('Machines go on a deck')
  const cell = factory.cellAt(deck, a.point)
  if (!cell) return fail('Off the edge of the deck')
  const rot = (factory.rotFacing(deck, camera.getWorldDirection(new THREE.Vector3())) + rotOffset) % 4
  const tr = factory.cellTransform(deck, cell.ix, cell.iz, rot)
  ghost.position.copy(tr.pos)
  ghost.quaternion.copy(tr.q)
  const kind = tool
  const reason = factory.canPlaceMachine(kind, deck, cell.ix, cell.iz) ?? (afford ? null : `Need ${inventory.missing(t.cost).join(', ')}`)
  return reason ? fail(reason) : { ok: true as const, place: () => factory.placeMachine(kind, deck, cell.ix, cell.iz, rot) }
}

function selectTool(t: Tool | null) {
  tool = t === tool ? null : t
  rotOffset = 0
  setGhost(tool)
  factory.setRingsVisible(tool === 'pylon' || tool === 'solar')
}

// ---------- actions ----------
function grab(e: Entity) {
  if (e.type === 'item' && !ITEMS[e.kind].liftable) {
    toast(`${ITEMS[e.kind].name} is too heavy to lift. Push it, or salvage it (E)`)
    sound.deny()
    return
  }
  sound.grab()
  held = e
  e.body.setGravityScale(0, true)
  e.body.setAngularDamping(4)
}

function release(throwIt: boolean) {
  if (!held) return
  held.body.setGravityScale(1, true)
  held.body.setAngularDamping(0.4)
  if (throwIt) held.body.setLinvel(player.forward().multiplyScalar(9), true)
  if (throwIt) sound.throw()
  else sound.drop()
  held = null
}

function salvage(e: Entity) {
  if (e.type !== 'item') return
  const d = ITEMS[e.kind]
  const t = e.body.translation()
  const at = new THREE.Vector3(t.x, t.y, t.z)
  entities.remove(e)
  for (const p of d.parts) inventory.add(p)
  const burst = () => new THREE.Vector3((Math.random() - 0.5) * 3, 2 + Math.random() * 2, (Math.random() - 0.5) * 3)
  for (let i = 0; i < d.steel; i++) entities.spawnChunk('steel', at.clone().add(new THREE.Vector3(0, i * 0.3, 0)), burst())
  for (let i = 0; i < d.plastic; i++) entities.spawnChunk('plastic', at.clone().add(new THREE.Vector3(0.2, i * 0.3, 0)), burst())
  charge -= 1.5
  sound.salvage()
  if (d.parts.length) sound.part()
  toast(d.parts.length ? `Salvaged ${d.name}: +${d.parts.map((p) => STOCK_NAMES[p]).join(', +')}` : `Salvaged ${d.name} into scrap`)
}

function deliverToLift() {
  if (flags.has('repaired')) return
  const given: string[] = []
  for (const k of liftRemaining({ delivered })) {
    const n = Math.min(inventory.stock[k], LIFT_NEEDS[k] - delivered[k])
    if (n <= 0) continue
    inventory.stock[k] -= n
    delivered[k] += n
    given.push(`${n} ${STOCK_NAMES[k]}`)
  }
  if (!given.length) {
    sound.deny()
    toast('Nothing in your pack the lift needs yet')
    return
  }
  sound.place()
  toast(`Fitted ${given.join(', ')}`)
  if (!liftRemaining({ delivered }).length) {
    flags.add('repaired')
    repairT = 0.001
    sound.powerUp()
    caption('The winch shudders, catches, and turns. The cage rises out of the dark.')
    setTimeout(() => {
      const end = $('#ending')
      end.style.opacity = '1'
      setTimeout(() => (end.style.opacity = '0'), 20000)
    }, 6000)
  }
}

function stow(e: Entity) {
  if (e.type !== 'bale') return
  if (held === e) release(false)
  entities.remove(e)
  inventory.add(e.bale)
  flags.add('stowed')
  sound.stow()
  toast(`Stowed ${STOCK_NAMES[e.bale]}`)
}

function dropBale() {
  const kind = (['mixed', 'plastic', 'steel'] as BaleKind[]).find((k) => inventory.stock[k] > 0)
  if (!kind) return toast('No bales in your pack')
  inventory.stock[kind]--
  const p = camera.getWorldPosition(new THREE.Vector3()).addScaledVector(player.forward(), 1.3)
  entities.spawnBale(kind, p)
}

function deconstruct(a: Aim | null) {
  const info = a?.info
  if (!a || !info) return
  if (info.node) {
    if (factory.removeNode(info.node)) toast('Removed. Parts refunded')
    return
  }
  if (info.deck) {
    const cell = factory.cellAt(info.deck, a.point)
    const m = cell && factory.machineAt(info.deck, cell.ix, cell.iz)
    if (m) {
      factory.removeMachine(m)
      toast(`Removed ${TOOLS[m.type].name}. Parts refunded`)
    } else if (factory.removeDeck(info.deck)) toast('Removed deck. Bale refunded')
    else toast('Clear the deck first')
  }
}

function describeAim(a: Aim | null, plan: ReturnType<typeof buildPlan>) {
  if (tool) {
    const t = TOOLS[tool]
    const head = `<b>${t.name}</b> · ${describeCost(t.cost)} · ${t.desc}`
    if (!plan) return head
    if (!plan.ok) return `${head}\n<span class="bad">${plan.reason}</span>`
    return `${head}\n${'note' in plan && plan.note ? plan.note + ' · ' : ''}LMB place${tool === 'deck' || tool === 'pylon' || tool === 'solar' ? '' : ' · R rotate'} · Q cancel`
  }
  if (held) return `Holding ${entityName(held)}\nLMB drop · RMB throw${held.type === 'bale' ? ' · E stow' : ''}`
  if (!a || a.dist > REACH + 3) return ''
  const far = a.dist > REACH
  if (a.memory) return far ? a.memory.name : `<b>${a.memory.name}</b>\nE look`
  if (a.lift) {
    if (flags.has('repaired')) return '<b>Service lift</b> · running'
    const left = liftRemaining({ delivered }).map((k) => `${delivered[k]}/${LIFT_NEEDS[k]} ${STOCK_NAMES[k]}`).join(' · ')
    return `<b>Service lift</b> · ${left}\nE hand over parts from your pack`
  }
  if (a.entity) {
    const e = a.entity
    if (far) return entityName(e)
    if (e.type === 'item') {
      const d = ITEMS[e.kind]
      const yields = [...d.parts.map((p) => STOCK_NAMES[p]), d.steel ? `${d.steel} steel scrap` : '', d.plastic ? `${d.plastic} plastic scrap` : ''].filter(Boolean).join(', ')
      return `<b>${d.name}</b>\n${d.liftable ? 'LMB grab · ' : 'Too heavy to lift · '}E salvage → ${yields}`
    }
    if (e.type === 'bale') return `<b>${entityName(e)}</b>\nLMB grab · E stow in pack`
    return `<b>${entityName(e)}</b>\nLMB grab`
  }
  const info = a.info
  if (info?.node) {
    const n = info.node
    if (n.type === 'dock') return `<b>Charging dock</b>\nSupplies ${DOCK_SUPPLY} power. Stand on it to charge`
    return `<b>${n.type === 'pylon' ? 'Relay pylon' : 'Scrap solar'}</b> · ${n.connected ? 'linked' : '<span class="bad">not linked</span>'}\nX deconstruct`
  }
  if (info?.deck) {
    const cell = factory.cellAt(info.deck, a.point)
    const m = cell && factory.machineAt(info.deck, cell.ix, cell.iz)
    if (m) {
      const status = m.powered ? (factory.satisfaction < 1 ? 'brownout' : 'running') : '<span class="bad">no power</span>'
      const extra = m.type === 'compactor' ? ` · ${m.batch.n}/4 ${m.batch.kind ?? ''}` : ''
      return `<b>${TOOLS[m.type].name}</b> · ${status}${extra}\nX deconstruct`
    }
    return '<b>Deck</b>\nX deconstruct (if empty)'
  }
  return ''
}

// ---------- input ----------
addEventListener('keydown', (ev) => {
  if (!started) return
  keys.add(ev.code)
  if (ev.repeat) return
  const a = aim()
  switch (ev.code) {
    case 'Space':
      jumpQueued = true
      break
    case 'KeyE':
      if (held?.type === 'bale') stow(held)
      else if (a?.lift && a.dist <= REACH + 1) deliverToLift()
      else if (a?.memory && a.dist <= REACH) {
        caption(a.memory.line)
        sound.memory()
      } else if (a?.entity && a.dist <= REACH) {
        if (a.entity.type === 'item') salvage(a.entity)
        else if (a.entity.type === 'bale') stow(a.entity)
      }
      break
    case 'KeyB':
      dropBale()
      break
    case 'KeyQ':
      selectTool(null)
      break
    case 'KeyR':
      rotOffset = (rotOffset + 1) % 4
      break
    case 'KeyX':
      deconstruct(a)
      break
    case 'KeyH':
      $('#help').style.display = $('#help').style.display === 'none' ? '' : 'none'
      break
    case 'KeyP':
      post.painterly = !post.painterly
      toast(post.painterly ? 'Painterly on' : 'Painterly off')
      break
    case 'KeyM':
      sound.muted = !sound.muted
      toast(sound.muted ? 'Sound off' : 'Sound on')
      break
    case 'KeyG':
      inventory.free = !inventory.free
      toast(inventory.free ? 'Free build on' : 'Free build off')
      break
    default:
      if (/^Digit[1-6]$/.test(ev.code)) selectTool(TOOL_ORDER[Number(ev.code.slice(5)) - 1])
  }
})
addEventListener('keyup', (ev) => keys.delete(ev.code))
addEventListener('blur', () => keys.clear())
addEventListener('contextmenu', (ev) => ev.preventDefault())
addEventListener('mousemove', (ev) => {
  if (!started || document.pointerLockElement !== renderer.domElement) return
  player.yaw -= ev.movementX * 0.0022
  player.pitch = THREE.MathUtils.clamp(player.pitch - ev.movementY * 0.0022, -1.5, 1.5)
})
addEventListener('mousedown', (ev) => {
  if (!started || downTimer > 0) return
  if (ev.button === 0) {
    if (tool) {
      const plan = buildPlan(aim())
      if (plan?.ok) {
        inventory.spend(TOOLS[tool].cost)
        plan.place()
        sound.place()
      } else if (plan) {
        toast(plan.reason)
        sound.deny()
      }
    } else if (held) release(false)
    else {
      const a = aim()
      if (a?.entity && a.dist <= REACH) grab(a.entity)
    }
  } else if (ev.button === 2) {
    if (held) release(true)
    else if (tool) selectTool(null)
  }
})

// ---------- simulation ----------
const holdTarget = new THREE.Vector3()
function fixedStep(dt: number) {
  if (downTimer > 0) {
    downTimer -= dt
    if (downTimer <= 0) {
      player.teleport(heap.spawnPos)
      charge = 25
      $('#fade').style.opacity = '0'
      sound.powerUp()
      caption('A tow drone dragged you home. What you were carrying is where you fell.')
    }
    world.step()
    return
  }

  const input = {
    forward: (keys.has('KeyW') ? 1 : 0) - (keys.has('KeyS') ? 1 : 0),
    right: (keys.has('KeyD') ? 1 : 0) - (keys.has('KeyA') ? 1 : 0),
    jump: jumpQueued || keys.has('Space'),
    sprint: keys.has('ShiftLeft') || keys.has('ShiftRight'),
  }
  jumpQueued = false
  const jumped = player.step(dt, input, held?.collider ?? null)
  if (!flags.has('foundLift') && player.position.distanceTo(heap.lift.pos) < 8) flags.add('foundLift')
  if (player.moving && player.grounded) {
    stepTimer -= dt * (player.sprinting ? 1.6 : 1)
    if (stepTimer <= 0) {
      sound.step(player.sprinting)
      stepTimer = 0.42
    }
  }
  if (jumped) sound.servo()

  if (held) {
    const size = held.type === 'item' ? Math.max(...ITEMS[held.kind].size) : 0.5
    holdTarget.copy(player.eye).addScaledVector(player.forward(), 1.0 + size * 0.6)
    const t = held.body.translation()
    const v = holdTarget.clone().sub(new THREE.Vector3(t.x, t.y, t.z)).multiplyScalar(12)
    if (v.length() > 14) v.setLength(14)
    held.body.setLinvel(v, true)
    if (v.length() > 13 || !entities.all.has(held)) release(false) // snagged on something
  }

  factory.step(dt, held?.body ?? null)
  world.step()

  // battery: charge is range, and the grid is shared with your machines
  const zone = factory.zoneAt(player.feet)
  const spare = Math.max(0, factory.supply - factory.demand)
  if (zone === 'dock') charge += (0.6 + spare * 1.2) * dt
  else if (zone === 'grid') charge += spare * 0.3 * dt
  else {
    const d = factory.distanceToGrid(player.feet)
    const drain = (0.3 + (player.moving ? 0.9 : 0) + (player.sprinting ? 0.9 : 0) + (held ? 0.3 : 0)) * (1 + d / 80)
    charge -= drain * dt + (jumped ? 0.5 : 0)
  }
  charge = Math.min(100, charge)
  if (charge <= 0) {
    charge = 0
    release(false)
    downTimer = 2.5
    sound.powerDown()
    $('#fade').textContent = 'L4z10 powering down…'
    $('#fade').style.opacity = '1'
  }
}

let last = performance.now()
let acc = 0
let frames = 0
function frame(now: number) {
  const dt = Math.min(0.1, (now - last) / 1000)
  last = now
  if (started) {
    acc += dt
    while (acc >= world.timestep) {
      fixedStep(world.timestep)
      acc -= world.timestep
    }
  }
  entities.sync()
  factory.animate()

  camera.position.copy(player.eye)
  camera.quaternion.setFromEuler(new THREE.Euler(player.pitch, player.yaw, 0, 'YXZ'))
  camera.updateMatrixWorld()
  const bob = player.moving && player.grounded ? Math.sin(now * 0.012) * 0.015 : 0
  arm.position.y = -0.2 + bob
  const open = held ? 0.012 : 0.025
  clawL.position.x = -open
  clawR.position.x = open

  const p = player.position
  sun.position.copy(p).addScaledVector(SUN_DIR, 120)
  sun.target.position.copy(p)

  const a = aim()
  const plan = buildPlan(a)
  ghost.visible = !!tool && !!a && !a.entity
  if (plan) tintGhost(plan.ok)
  const objectiveHtml = started ? objectives.update(dt, p) : ''
  if (repairT > 0 && repairT < 1) {
    repairT = Math.min(1, repairT + dt / 4)
    heap.lift.setRepair(repairT)
  }
  if (frames++ % 3 === 0) {
    renderHud(describeAim(a, plan))
    const obj = $('#objective')
    obj.innerHTML = objectiveHtml
    obj.style.display = objectiveHtml ? '' : 'none'
  }
  atmosphere.update(dt, p)
  heap.scenery.update(now / 1000)
  const offGrid = !factory.zoneAt(player.feet)
  const low = offGrid ? THREE.MathUtils.clamp((30 - charge) / 30, 0, 1) : 0
  post.lowCharge = low
  sound.update(dt, { height: p.y - heap.dockPos.y, radio: THREE.MathUtils.clamp(1 - heap.radioPos.distanceTo(p) / 22, 0, 1), low })
  post.render()
  requestAnimationFrame(frame)
}
requestAnimationFrame(frame)

// debug handle for automated checks and tinkering in the console
Object.assign(window, {
  __game: {
    player, factory, entities, inventory, world, heap, camera, aim, selectTool, post, sound, objectives, flags, delivered,
    start: () => $('#overlay .go').click(),
    get charge() { return charge },
    set charge(v: number) { charge = v },
    get held() { return held },
    lookAt(x: number, y: number, z: number) {
      const d = new THREE.Vector3(x, y, z).sub(player.eye)
      player.yaw = Math.atan2(-d.x, -d.z)
      player.pitch = Math.atan2(d.y, Math.hypot(d.x, d.z))
      camera.position.copy(player.eye)
      camera.quaternion.setFromEuler(new THREE.Euler(player.pitch, player.yaw, 0, 'YXZ'))
      camera.updateMatrixWorld()
    },
    /** Advance the simulation without rendering (slow headless browsers). */
    tick(steps: number) {
      for (let i = 0; i < steps; i++) fixedStep(world.timestep)
      entities.sync()
      camera.position.copy(player.eye)
      camera.updateMatrixWorld()
    },
  },
})
