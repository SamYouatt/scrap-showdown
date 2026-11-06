import * as THREE from 'three'
import RAPIER from '@dimforge/rapier3d-compat'

const HALF_HEIGHT = 0.35
const RADIUS = 0.3
const EYE = 0.5
const GRAVITY = 18
const WALK = 4.5
const SPRINT = 7
const JUMP = 6.5

export interface MoveInput {
  forward: number
  right: number
  jump: boolean
  sprint: boolean
}

/** Lazlo: a small robot on a kinematic character controller. */
export class Player {
  readonly body: RAPIER.RigidBody
  readonly collider: RAPIER.Collider
  private controller: RAPIER.KinematicCharacterController
  yaw = 0
  pitch = 0
  private vel = new THREE.Vector3()
  grounded = false
  moving = false
  sprinting = false

  constructor(world: RAPIER.World, spawn: THREE.Vector3) {
    this.body = world.createRigidBody(RAPIER.RigidBodyDesc.kinematicPositionBased().setTranslation(spawn.x, spawn.y, spawn.z))
    this.collider = world.createCollider(RAPIER.ColliderDesc.capsule(HALF_HEIGHT, RADIUS), this.body)
    const c = world.createCharacterController(0.03)
    c.setUp({ x: 0, y: 1, z: 0 })
    c.setMaxSlopeClimbAngle(THREE.MathUtils.degToRad(50))
    c.setMinSlopeSlideAngle(THREE.MathUtils.degToRad(55))
    c.enableAutostep(0.35, 0.2, false)
    c.enableSnapToGround(0.3)
    c.setApplyImpulsesToDynamicBodies(true)
    c.setCharacterMass(40)
    this.controller = c
  }

  get position() {
    const t = this.body.translation()
    return new THREE.Vector3(t.x, t.y, t.z)
  }

  get eye() {
    return this.position.add(new THREE.Vector3(0, EYE, 0))
  }

  get feet() {
    return this.position.add(new THREE.Vector3(0, -HALF_HEIGHT - RADIUS, 0))
  }

  forward() {
    return new THREE.Vector3(0, 0, -1).applyEuler(new THREE.Euler(this.pitch, this.yaw, 0, 'YXZ'))
  }

  teleport(p: THREE.Vector3) {
    this.body.setTranslation(p, true)
    this.body.setNextKinematicTranslation(p)
    this.vel.set(0, 0, 0)
  }

  /** Returns true if a jump started this step. */
  step(dt: number, input: MoveInput, ignore: RAPIER.Collider | null) {
    const fwd = new THREE.Vector3(-Math.sin(this.yaw), 0, -Math.cos(this.yaw))
    const right = new THREE.Vector3(-fwd.z, 0, fwd.x)
    const wish = fwd.multiplyScalar(input.forward).addScaledVector(right, input.right)
    if (wish.lengthSq() > 1) wish.normalize()
    this.moving = wish.lengthSq() > 0.01
    this.sprinting = this.moving && input.sprint
    const speed = this.sprinting ? SPRINT : WALK
    const accel = this.grounded ? 14 : 3
    const k = Math.min(1, accel * dt)
    this.vel.x += (wish.x * speed - this.vel.x) * k
    this.vel.z += (wish.z * speed - this.vel.z) * k

    let jumped = false
    if (this.grounded && input.jump) {
      this.vel.y = JUMP
      jumped = true
    }
    this.vel.y -= GRAVITY * dt

    const desired = this.vel.clone().multiplyScalar(dt)
    this.controller.computeColliderMovement(this.collider, desired, RAPIER.QueryFilterFlags.EXCLUDE_SENSORS, undefined, (c) => c !== ignore)
    const m = this.controller.computedMovement()
    this.grounded = this.controller.computedGrounded()
    if (this.grounded && this.vel.y < 0) this.vel.y = -0.5
    if (this.vel.y > 0 && m.y < desired.y * 0.5) this.vel.y = 0 // bonked a ceiling
    const t = this.body.translation()
    this.body.setNextKinematicTranslation({ x: t.x + m.x, y: t.y + m.y, z: t.z + m.z })
    return jumped
  }
}
