import * as THREE from 'three'
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js'
import { OutputPass } from 'three/examples/jsm/postprocessing/OutputPass.js'
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js'
import { ShaderPass } from 'three/examples/jsm/postprocessing/ShaderPass.js'
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js'

/**
 * Painterly finish, applied after tone mapping:
 * a small Kuwahara filter flattens texture into brush-like patches,
 * then a warm grade (violet shadows, honey highlights), a canvas grain and a vignette.
 * `low` (0..1) drains colour from the edges as Lazlo's charge runs out.
 */
const PaintShader = {
  uniforms: {
    tDiffuse: { value: null as THREE.Texture | null },
    resolution: { value: new THREE.Vector2(1, 1) },
    paint: { value: 1 },
    low: { value: 0 },
  },
  vertexShader: /* glsl */ `
    varying vec2 vUv;
    void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }
  `,
  fragmentShader: /* glsl */ `
    uniform sampler2D tDiffuse;
    uniform vec2 resolution;
    uniform float paint;
    uniform float low;
    varying vec2 vUv;

    float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

    vec3 kuwahara(vec2 uv) {
      const int R = 3;
      vec2 px = 1.0 / resolution;
      vec3 mean[4];
      vec3 sq[4];
      for (int k = 0; k < 4; k++) { mean[k] = vec3(0.0); sq[k] = vec3(0.0); }
      for (int j = 0; j <= R; j++) {
        for (int i = 0; i <= R; i++) {
          vec3 a = texture2D(tDiffuse, uv + vec2(-i, -j) * px).rgb;
          vec3 b = texture2D(tDiffuse, uv + vec2( i, -j) * px).rgb;
          vec3 c = texture2D(tDiffuse, uv + vec2(-i,  j) * px).rgb;
          vec3 d = texture2D(tDiffuse, uv + vec2( i,  j) * px).rgb;
          mean[0] += a; sq[0] += a * a;
          mean[1] += b; sq[1] += b * b;
          mean[2] += c; sq[2] += c * c;
          mean[3] += d; sq[3] += d * d;
        }
      }
      float n = float((R + 1) * (R + 1));
      float best = 1e9;
      vec3 col = vec3(0.0);
      for (int k = 0; k < 4; k++) {
        vec3 m = mean[k] / n;
        vec3 v = abs(sq[k] / n - m * m);
        float s = v.r + v.g + v.b;
        if (s < best) { best = s; col = m; }
      }
      return col;
    }

    void main() {
      vec3 src = texture2D(tDiffuse, vUv).rgb;
      vec3 col = paint > 0.5 ? kuwahara(vUv) : src;

      // grade: violet in the shadows, honey in the highlights, a touch more saturation
      float l = dot(col, vec3(0.299, 0.587, 0.114));
      col = mix(col, col * vec3(0.86, 0.84, 1.08) + vec3(0.025, 0.012, 0.05), smoothstep(0.55, 0.0, l));
      col = mix(col, col * vec3(1.06, 1.0, 0.88), smoothstep(0.5, 1.0, l));
      col = mix(vec3(l), col, 1.12);

      // canvas: static grain plus a faint weave
      vec2 p = vUv * resolution;
      float grain = hash(floor(p / 1.5)) - 0.5;
      float weave = sin(p.x * 1.3) * sin(p.y * 1.3) * 0.5;
      col *= 1.0 + (grain * 0.045 + weave * 0.012) * paint;

      // vignette, deeper and colder when charge is low
      vec2 d = vUv - 0.5;
      float vig = smoothstep(0.85, 0.25 - low * 0.15, length(d * vec2(1.0, 0.8)));
      col *= mix(0.72 - low * 0.35, 1.0, vig);
      float grey = dot(col, vec3(0.299, 0.587, 0.114));
      col = mix(col, vec3(grey) * vec3(0.8, 0.88, 1.08), low * (1.0 - vig * 0.6));

      gl_FragColor = vec4(col, 1.0);
    }
  `,
}

export function createPost(renderer: THREE.WebGLRenderer, scene: THREE.Scene, camera: THREE.Camera) {
  const composer = new EffectComposer(renderer)
  composer.addPass(new RenderPass(scene, camera))
  const bloom = new UnrealBloomPass(new THREE.Vector2(innerWidth, innerHeight), 0.35, 0.6, 0.82)
  composer.addPass(bloom)
  composer.addPass(new OutputPass())
  const paint = new ShaderPass(PaintShader)
  composer.addPass(paint)

  const resize = () => {
    const pr = Math.min(devicePixelRatio, 1.5)
    composer.setPixelRatio(pr)
    composer.setSize(innerWidth, innerHeight)
    paint.uniforms.resolution.value.set(innerWidth * pr, innerHeight * pr)
  }
  resize()
  addEventListener('resize', resize)

  return {
    render: () => composer.render(),
    set painterly(on: boolean) {
      paint.uniforms.paint.value = on ? 1 : 0
    },
    get painterly() {
      return paint.uniforms.paint.value > 0.5
    },
    set lowCharge(v: number) {
      paint.uniforms.low.value = v
    },
  }
}
