import * as THREE from 'three'
import type { Entities, Entity } from './entities'
import type { Factory } from './factory'
import type { Lift } from './heap'
import type { Inventory } from './inventory'

/** What the service lift needs before it will run again. */
export const LIFT_NEEDS = { steel: 4, mixed: 2, motor: 2, battery: 1 } as const
export type LiftPart = keyof typeof LIFT_NEEDS
const LIFT_LABELS: Record<LiftPart, string> = { steel: 'steel bales', mixed: 'junk bales (counterweight)', motor: 'motors', battery: 'battery cell' }

export type Flag = 'stowed' | 'baled' | 'foundLift' | 'repaired'

interface Ctx {
  inventory: Inventory
  factory: Factory
  entities: Entities
  lift: Lift
  dock: THREE.Vector3
  delivered: Record<LiftPart, number>
  flags: Set<Flag>
}

interface Step {
  title: string
  hint: (c: Ctx) => string
  done: (c: Ctx) => boolean
  target: (c: Ctx, from: THREE.Vector3) => THREE.Vector3 | null
  /** Spoken as a caption when the step begins. */
  intro?: string
}

const nearest = (c: Ctx, from: THREE.Vector3, pred: (e: Entity) => boolean) => {
  let best: THREE.Vector3 | null = null
  let bestD = Infinity
  for (const e of c.entities.all) {
    if (!pred(e)) continue
    const t = e.body.translation()
    const p = new THREE.Vector3(t.x, t.y, t.z)
    const d = p.distanceTo(from)
    if (d < bestD) {
      bestD = d
      best = p
    }
  }
  return best
}

const machine = (c: Ctx, type: string) => c.factory.machines.find((m) => m.type === type)
const machinePos = (c: Ctx, type: string) => {
  const m = machine(c, type)
  return m ? c.factory.cellTransform(m.deck, m.ix, m.iz, m.rot).pos : null
}
export const liftRemaining = (c: Pick<Ctx, 'delivered'>) =>
  (Object.keys(LIFT_NEEDS) as LiftPart[]).filter((k) => c.delivered[k] < LIFT_NEEDS[k])

const STEPS: Step[] = [
  {
    title: 'Pick up the bales by the dock',
    hint: () => 'Look at a bale and press <kbd>E</kbd> to stow it. Your pack is top right.',
    done: (c) => c.flags.has('stowed'),
    target: (c, from) => nearest(c, from, (e) => e.type === 'bale'),
  },
  {
    title: 'Salvage a fridge for its compressor',
    intro: 'A compactor needs a compressor. Fridges have them.',
    hint: () => 'Walk up to the fridge and press <kbd>E</kbd>. It breaks into a part plus loose scrap.',
    done: (c) => c.inventory.stock.compressor > 0 || !!machine(c, 'compactor'),
    target: (c, from) => nearest(c, from, (e) => e.type === 'item' && e.kind === 'fridge'),
  },
  {
    title: 'Build a deck next to the dock',
    hint: () => 'Press <kbd>1</kbd>, aim at flat ground inside the glowing power ring, then click. Machines only work on a deck.',
    done: (c) => c.factory.decks.length > 0,
    target: (c) => c.dock,
  },
  {
    title: 'Put a compactor on the deck',
    hint: () => 'Press <kbd>4</kbd> and click a cell on the deck. It squashes 4 pieces of scrap into a bale.',
    done: (c) => !!machine(c, 'compactor'),
    target: (c) => (c.factory.decks[0] ? c.factory.decks[0].group.position : c.dock),
  },
  {
    title: 'Make a bale',
    intro: 'Bales are what everything is built from.',
    hint: () => 'Grab loose scrap (click), carry it over the compactor and click to drop it in. 4 pieces make a bale.',
    done: (c) => c.flags.has('baled'),
    target: (c) => machinePos(c, 'compactor'),
  },
  {
    title: 'Find the old service lift',
    intro: 'The signal comes from below. There was a lift out there, once.',
    hint: () => 'Follow the beacon. Off the power grid your charge drains, so keep an eye on it.',
    done: (c) => c.flags.has('foundLift'),
    target: (c) => c.lift.pos,
  },
  {
    title: 'Repair the service lift',
    intro: 'The winch is seized and the counterweight is gone. It could run again.',
    hint: (c) => {
      const left = liftRemaining(c)
      const list = left.map((k) => `${LIFT_NEEDS[k] - c.delivered[k]} ${LIFT_LABELS[k]}`).join(' · ')
      const tip = left.includes('steel')
        ? 'Steel bales need pure steel: put a magnet sorter (<kbd>3</kbd>) over a belt (<kbd>2</kbd>) to pull steel aside into the compactor.'
        : left.includes('battery')
          ? 'Battery cells come from e-bikes, further out. A relay pylon (<kbd>5</kbd>) extends your range.'
          : left.includes('motor')
            ? 'Motors come from washing machines and e-bikes.'
            : 'Mixed scrap makes junk bales. Any scrap will do.'
      return `Still needs: ${list}.<br>Bring them in your pack and press <kbd>E</kbd> at the lift.<br><span class="tip">${tip}</span>`
    },
    done: (c) => c.flags.has('repaired'),
    target: (c) => c.lift.pos,
  },
]

export class Objectives {
  private index = -1
  private beacon: THREE.Mesh
  private time = 0
  onStep?: (intro: string | undefined) => void

  constructor(
    scene: THREE.Scene,
    private ctx: Ctx,
  ) {
    this.beacon = new THREE.Mesh(
      new THREE.CylinderGeometry(0.18, 0.18, 60, 8, 1, true),
      new THREE.MeshBasicMaterial({ color: 0x7ff0c8, transparent: true, opacity: 0.35, depthWrite: false, fog: false, blending: THREE.AdditiveBlending }),
    )
    this.beacon.visible = false
    scene.add(this.beacon)
  }

  get complete() {
    return this.index >= STEPS.length
  }

  /** Advances past every finished step and returns HTML for the objective panel. */
  update(dt: number, player: THREE.Vector3) {
    this.time += dt
    const start = this.index
    if (this.index < 0) this.index = 0
    if (this.ctx.flags.has('repaired')) this.index = STEPS.length
    while (this.index < STEPS.length && STEPS[this.index].done(this.ctx)) this.index++
    if (this.index !== start && this.index < STEPS.length) this.onStep?.(STEPS[this.index].intro)

    if (this.complete) {
      this.beacon.visible = false
      return ''
    }
    const step = STEPS[this.index]
    const target = step.target(this.ctx, player)
    let dist = ''
    if (target) {
      const d = target.distanceTo(player)
      this.beacon.visible = d > 9
      this.beacon.position.set(target.x, target.y + 30, target.z)
      const m = this.beacon.material as THREE.MeshBasicMaterial
      m.opacity = 0.25 + Math.sin(this.time * 2.5) * 0.1
      dist = d > 9 ? ` <span class="dist">${d.toFixed(0)} m</span>` : ''
    } else this.beacon.visible = false
    return `<h4>OBJECTIVE ${this.index + 1}/${STEPS.length}</h4><div class="title">${step.title}${dist}</div><div class="hint">${step.hint(this.ctx)}</div>`
  }
}
