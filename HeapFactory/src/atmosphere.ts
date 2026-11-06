import * as THREE from 'three'

function softDot() {
  const c = document.createElement('canvas')
  c.width = c.height = 32
  const g = c.getContext('2d')!
  const grad = g.createRadialGradient(16, 16, 0, 16, 16, 16)
  grad.addColorStop(0, 'rgba(255,255,255,1)')
  grad.addColorStop(1, 'rgba(255,255,255,0)')
  g.fillStyle = grad
  g.fillRect(0, 0, 32, 32)
  return new THREE.CanvasTexture(c)
}

export const HORIZON = new THREE.Color(0xf2b98e)

/** Golden-hour sky, drifting dust in the light, and gulls wheeling over the heap. */
export function createAtmosphere(scene: THREE.Scene, sunDir: THREE.Vector3) {
  // --- sky dome: dusty violet overhead, peach at the horizon, a soft low sun
  const sky = new THREE.Mesh(
    new THREE.SphereGeometry(950, 32, 16),
    new THREE.ShaderMaterial({
      side: THREE.BackSide,
      depthWrite: false,
      fog: false,
      uniforms: {
        sunDir: { value: sunDir },
        zenith: { value: new THREE.Color(0x5f6188) },
        mid: { value: new THREE.Color(0xd99a86) },
        horizon: { value: HORIZON },
        sun: { value: new THREE.Color(0xfff0c8) },
      },
      vertexShader: /* glsl */ `
        varying vec3 vDir;
        void main() {
          vDir = normalize(position);
          vec4 p = modelViewMatrix * vec4(position, 1.0);
          gl_Position = projectionMatrix * p;
          gl_Position.z = gl_Position.w; // always behind everything
        }
      `,
      fragmentShader: /* glsl */ `
        uniform vec3 sunDir, zenith, mid, horizon, sun;
        varying vec3 vDir;
        void main() {
          float h = clamp(vDir.y, -0.2, 1.0);
          vec3 col = mix(horizon, mid, smoothstep(0.0, 0.18, h));
          col = mix(col, zenith, smoothstep(0.15, 0.75, h));
          float d = max(dot(normalize(vDir), normalize(sunDir)), 0.0);
          col += sun * (smoothstep(0.9988, 0.9994, d) * 1.4 + pow(d, 60.0) * 0.45 + pow(d, 6.0) * 0.22);
          // a band of haze sitting on the horizon
          col = mix(col, horizon * 1.05, smoothstep(0.08, 0.0, abs(h - 0.01)) * 0.6);
          gl_FragColor = vec4(col, 1.0);
        }
      `,
    }),
  )
  sky.frustumCulled = false
  scene.add(sky)

  // --- dust motes catching the light, wrapped around the player
  const MOTES = 500
  const BOX = 26
  const motePos = new Float32Array(MOTES * 3)
  const seeds = new Float32Array(MOTES)
  for (let i = 0; i < MOTES; i++) {
    motePos.set([(Math.random() - 0.5) * BOX, Math.random() * 8, (Math.random() - 0.5) * BOX], i * 3)
    seeds[i] = Math.random() * 100
  }
  const moteGeo = new THREE.BufferGeometry().setAttribute('position', new THREE.BufferAttribute(motePos, 3))
  const motes = new THREE.Points(
    moteGeo,
    new THREE.PointsMaterial({ color: 0xffe2b0, size: 0.035, map: softDot(), transparent: true, opacity: 0.6, depthWrite: false, blending: THREE.AdditiveBlending }),
  )
  motes.frustumCulled = false
  scene.add(motes)

  // --- gulls: little flapping silhouettes circling high up
  const wingGeo = new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute([0, 0, -0.25, 0, 0, 0.25, 1.1, 0.1, 0], 3))
  const gullMat = new THREE.MeshBasicMaterial({ color: 0x3b3340, side: THREE.DoubleSide, fog: true })
  const gulls = Array.from({ length: 7 }, (_, i) => {
    const g = new THREE.Group()
    const l = new THREE.Mesh(wingGeo, gullMat)
    const r = new THREE.Mesh(wingGeo, gullMat)
    r.scale.x = -1
    g.add(l, r)
    g.scale.setScalar(1.4)
    scene.add(g)
    return { g, l, r, radius: 25 + Math.random() * 40, height: 28 + Math.random() * 20, speed: 0.08 + Math.random() * 0.06, phase: i * 0.9, cx: (Math.random() - 0.5) * 120, cz: (Math.random() - 0.5) * 120 }
  })

  let t = 0
  return {
    update(dt: number, player: THREE.Vector3) {
      t += dt
      for (let i = 0; i < MOTES; i++) {
        const k = i * 3
        const s = seeds[i]
        let x = motePos[k] + Math.sin(t * 0.3 + s) * 0.004 + 0.006
        let y = motePos[k + 1] + Math.sin(t * 0.5 + s * 1.3) * 0.003
        let z = motePos[k + 2] + Math.cos(t * 0.25 + s) * 0.004
        // wrap into the box around the player
        const wx = x + motes.position.x - player.x
        const wz = z + motes.position.z - player.z
        if (wx > BOX / 2) x -= BOX
        if (wx < -BOX / 2) x += BOX
        if (wz > BOX / 2) z -= BOX
        if (wz < -BOX / 2) z += BOX
        if (y > 8) y = 0
        motePos[k] = x
        motePos[k + 1] = y
        motePos[k + 2] = z
      }
      moteGeo.attributes.position.needsUpdate = true
      motes.position.set(0, player.y - 2, 0)

      for (const b of gulls) {
        const a = t * b.speed + b.phase
        b.g.position.set(b.cx + Math.cos(a) * b.radius, b.height + Math.sin(a * 2.3) * 2, b.cz + Math.sin(a) * b.radius)
        b.g.rotation.set(0, -a, Math.sin(a) * 0.2)
        const flap = Math.sin(t * 6 + b.phase * 3) * 0.6 * (Math.sin(t * 0.4 + b.phase) > 0.2 ? 1 : 0.15)
        b.l.rotation.z = flap
        b.r.rotation.z = -flap
      }
    },
  }
}
