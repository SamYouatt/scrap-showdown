// Geometry pass. Renders instanced primitives into the low-res colour, normal and
// depth targets with banded "toon" lighting and hard pixel shadows. The post pass
// then adds outlines and palette effects on top.

struct Globals {
  viewProj: mat4x4f,
  lightViewProj: mat4x4f,
  lightDir: vec4f,   // xyz: direction towards the light, w: shadows on/off
  params: vec4f,     // x: time, y: shadow texel size (world)
};

@group(0) @binding(0) var<uniform> G: Globals;
@group(1) @binding(0) var shadowMap: texture_depth_2d;
@group(1) @binding(1) var shadowSampler: sampler_comparison;

struct VIn {
  @location(0) pos: vec3f,
  @location(1) nrm: vec3f,
  @location(2) col: vec3f,
  @location(3) ipos: vec4f,   // xyz position, w material
  @location(4) iquat: vec4f,
  @location(5) iscale: vec4f,
  @location(6) icol: vec4f,
};

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) nrm: vec3f,
  @location(2) col: vec3f,
  @location(3) @interpolate(flat) mat: u32,
};

fn qrot(q: vec4f, v: vec3f) -> vec3f {
  let t = 2.0 * cross(q.xyz, v);
  return v + q.w * t + cross(q.xyz, t);
}

fn worldPos(v: VIn) -> vec3f {
  return qrot(v.iquat, v.pos * v.iscale.xyz) + v.ipos.xyz;
}

@vertex
fn vs_main(v: VIn) -> VOut {
  var o: VOut;
  let w = worldPos(v);
  o.clip = G.viewProj * vec4f(w, 1.0);
  o.world = w;
  // Inverse-scale keeps normals correct under non-uniform scaling.
  o.nrm = qrot(v.iquat, v.nrm / v.iscale.xyz);
  o.col = v.col * v.icol.rgb;
  o.mat = u32(v.ipos.w + 0.5);
  return o;
}

@vertex
fn vs_shadow(v: VIn) -> @builtin(position) vec4f {
  return G.lightViewProj * vec4f(worldPos(v), 1.0);
}

fn hash2(p: vec2f) -> f32 {
  let h = dot(p, vec2f(127.1, 311.7));
  return fract(sin(h) * 43758.5453);
}

fn shadowAt(world: vec3f, n: vec3f) -> f32 {
  if (G.lightDir.w < 0.5) { return 1.0; }
  let p = G.lightViewProj * vec4f(world + n * G.params.y * 1.5, 1.0);
  let uv = p.xy * vec2f(0.5, -0.5) + 0.5;
  if (any(uv < vec2f(0.0)) || any(uv > vec2f(1.0)) || p.z > 1.0) { return 1.0; }
  return textureSampleCompareLevel(shadowMap, shadowSampler, uv, p.z - 0.002);
}

struct FOut {
  @location(0) color: vec4f,
  @location(1) normal: vec4f,
};

@fragment
fn fs_main(v: VOut) -> FOut {
  var o: FOut;
  let n = normalize(v.nrm);
  var albedo = v.col;
  o.normal = vec4f(n, 1.0);

  if (v.mat == 1u) {
    o.color = vec4f(albedo, 1.0);
    return o;
  }

  if (v.mat == 2u) {
    // World-locked speckle so the ground reads as grit and tufts, not flat colour.
    let cell = floor(v.world.xz * 7.0);
    let h = hash2(cell);
    let blob = hash2(floor(v.world.xz * 1.3));
    if (h > 0.9) { albedo *= 1.13; }
    else if (h < 0.07) { albedo *= 0.86; }
    if (blob > 0.86 && h > 0.5) { albedo = mix(albedo, vec3f(0.42, 0.47, 0.25), 0.55); }
  }

  let L = normalize(G.lightDir.xyz);
  let ndl = dot(n, L);
  let lit = clamp(ndl * 1.6, 0.0, 1.0) * shadowAt(v.world, n);

  // Three hard light bands, with cool shadows and warm highlights.
  var tone = 0.0;
  if (lit > 0.6) { tone = 1.0; } else if (lit > 0.15) { tone = 0.55; }
  let shadowTint = vec3f(0.36, 0.38, 0.55);
  let midTint = vec3f(0.72, 0.70, 0.74);
  let lightTint = vec3f(1.08, 1.0, 0.88);
  var shade = mix(shadowTint, midTint, clamp(tone / 0.55, 0.0, 1.0));
  if (tone > 0.9) { shade = lightTint; }
  // A little sky bounce on upward faces keeps shadows from going muddy.
  shade += vec3f(0.04, 0.06, 0.1) * clamp(n.y, 0.0, 1.0);

  o.color = vec4f(clamp(albedo * shade, vec3f(0.0), vec3f(1.0)), 1.0);
  return o;
}
