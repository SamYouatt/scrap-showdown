// Upscale pass. Each screen pixel looks up one low-res texel (nearest), shifted
// by the camera's sub-texel offset, then adds pixel-art outlines:
//  - silhouette texels (a neighbour is much further away) are darkened
//  - convex creases (normals diverge at similar depth) get a one-texel highlight
// Optionally the colour is quantised with a world-locked Bayer dither.

struct Post {
  a: vec4f, // screen.xy (device px), low.xy (texels)
  b: vec4f, // offset.xy (texels), scale (device px per texel), outlines on/off
  c: vec4f, // texelOrigin.xy, palette on/off, texel size (world units)
  d: vec4f, // x: depth range (far - near)
};

@group(0) @binding(0) var<uniform> P: Post;
@group(0) @binding(1) var colorTex: texture_2d<f32>;
@group(0) @binding(2) var normalTex: texture_2d<f32>;
@group(0) @binding(3) var depthTex: texture_depth_2d;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
  let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}

fn clampT(t: vec2i) -> vec2i {
  return clamp(t, vec2i(0), vec2i(P.a.zw) - vec2i(1));
}
fn depthAt(t: vec2i) -> f32 {
  return textureLoad(depthTex, clampT(t), 0) * P.d.x;
}
fn normalAt(t: vec2i) -> vec4f {
  return textureLoad(normalTex, clampT(t), 0);
}

fn bayer4(p: vec2i) -> f32 {
  let m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
  let q = ((p % 4) + 4) % 4;
  return (m[q.y * 4 + q.x] + 0.5) / 16.0;
}

@fragment
fn fs(@builtin(position) frag: vec4f) -> @location(0) vec4f {
  let low = (frag.xy - P.a.xy * 0.5) / P.b.z + P.a.zw * 0.5 + P.b.xy;
  let t = clampT(vec2i(floor(low)));
  var col = textureLoad(colorTex, t, 0).rgb;
  let n4 = normalAt(t);

  if (P.b.w > 0.5 && n4.w > 0.5) {
    let d = depthAt(t);
    let n = n4.xyz;
    let threshold = P.c.w * 6.0;
    var silhouette = false;
    var crease = 0.0;
    let offs = array<vec2i, 4>(vec2i(1, 0), vec2i(-1, 0), vec2i(0, 1), vec2i(0, -1));
    for (var i = 0; i < 4; i++) {
      let tt = t + offs[i];
      let nn = normalAt(tt);
      let dn = depthAt(tt);
      if (nn.w < 0.5 || dn - d > threshold) {
        silhouette = true;
      } else if (dn - d > -threshold) {
        // Only light the side of the crease that faces up-left, so it reads as a bevel.
        let bias = dot(n - nn.xyz, vec3f(1.0, 1.0, 1.0));
        if (bias > 0.0) { crease += 1.0 - dot(n, nn.xyz); }
      }
    }
    if (silhouette) {
      col *= vec3f(0.42, 0.40, 0.48);
    } else if (crease > 0.25) {
      col = min(col * 1.28 + vec3f(0.03), vec3f(1.0));
    }
  }

  if (P.c.z > 0.5) {
    let levels = 7.0;
    let thr = bayer4(t + vec2i(P.c.xy)) - 0.5;
    col = clamp(floor(col * (levels - 1.0) + 0.5 + thr * 0.9) / (levels - 1.0), vec3f(0.0), vec3f(1.0));
  }
  return vec4f(col, 1.0);
}
