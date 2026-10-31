// Orthographic "isometric" camera that renders to a low-resolution pixel grid.
//
// The camera centre is snapped to whole texels in view space so static geometry
// never shimmers as the camera follows the creature. The leftover sub-texel
// offset is applied when the low-res image is upscaled, so the camera still
// glides smoothly (the t3ssel8r trick).
import { mat4, Mat4 } from 'wgpu-matrix';
import { V3, add, cross, dot, norm, scale } from '../math';

export class Camera {
  yaw = Math.PI / 4;
  targetYaw = Math.PI / 4;
  /** 30° elevation gives the classic 2:1 pixel-art dimetric slope. */
  pitch = (30 * Math.PI) / 180;
  target: V3 = [0, 0, 0];
  /** CSS pixels per world unit. Fixed: zooming would change the pixel density. */
  readonly zoom = 80;

  // Derived each frame by update().
  right: V3 = [1, 0, 0];
  up: V3 = [0, 1, 0];
  fwd: V3 = [0, 0, -1];
  centre: V3 = [0, 0, 0];
  texel = 0.05;
  /** Device pixels per low-res texel. */
  scale = 3;
  lowW = 1;
  lowH = 1;
  screenW = 1;
  screenH = 1;
  offset: [number, number] = [0, 0];
  /** Snapped camera position in whole texels, used to lock dithering to the world. */
  texelOrigin: [number, number] = [0, 0];
  viewProj: Mat4 = mat4.identity();
  readonly near = 0.1;
  readonly far = 160;

  update(screenW: number, screenH: number, dpr: number, pixelSize: number) {
    this.screenW = screenW;
    this.screenH = screenH;
    this.scale = Math.max(1, Math.round(pixelSize * dpr));
    this.lowW = Math.ceil(screenW / this.scale) + 2;
    this.lowH = Math.ceil(screenH / this.scale) + 2;
    this.texel = this.scale / (this.zoom * dpr);

    const cp = Math.cos(this.pitch);
    const towardsEye: V3 = [cp * Math.sin(this.yaw), Math.sin(this.pitch), cp * Math.cos(this.yaw)];
    this.fwd = scale(towardsEye, -1);
    this.right = norm(cross(this.fwd, [0, 1, 0]));
    this.up = cross(this.right, this.fwd);

    const tx = dot(this.target, this.right) / this.texel;
    const ty = dot(this.target, this.up) / this.texel;
    const sx = Math.round(tx), sy = Math.round(ty);
    this.offset = [tx - sx, -(ty - sy)];
    this.texelOrigin = [sx, -sy];
    this.centre = add(
      add(scale(this.right, sx * this.texel), scale(this.up, sy * this.texel)),
      scale(this.fwd, dot(this.target, this.fwd)),
    );

    const eye = add(this.centre, scale(this.fwd, -80));
    const view = mat4.lookAt(eye, this.centre, [0, 1, 0]);
    const hw = (this.lowW * this.texel) / 2, hh = (this.lowH * this.texel) / 2;
    const proj = mat4.ortho(-hw, hw, -hh, hh, this.near, this.far);
    this.viewProj = mat4.multiply(proj, view);
  }

  /** Low-res texel coordinate of a device pixel (matches post.wgsl). */
  private toLow(px: number, py: number): [number, number] {
    return [
      (px - this.screenW / 2) / this.scale + this.lowW / 2 + this.offset[0],
      (py - this.screenH / 2) / this.scale + this.lowH / 2 + this.offset[1],
    ];
  }

  /** World-space ray through a device pixel: origin on the eye plane, direction = fwd. */
  ray(px: number, py: number): { origin: V3; dir: V3 } {
    const [lx, ly] = this.toLow(px, py);
    const a = (lx - this.lowW / 2) * this.texel;
    const b = (this.lowH / 2 - ly) * this.texel;
    const origin = add(add(this.centre, scale(this.right, a)), add(scale(this.up, b), scale(this.fwd, -80)));
    return { origin, dir: this.fwd };
  }

  /** Device-pixel position of a world point (for the debug overlay). */
  project(p: V3): [number, number] {
    const rel: V3 = [p[0] - this.centre[0], p[1] - this.centre[1], p[2] - this.centre[2]];
    const lx = dot(rel, this.right) / this.texel + this.lowW / 2;
    const ly = this.lowH / 2 - dot(rel, this.up) / this.texel;
    return [
      (lx - this.lowW / 2 - this.offset[0]) * this.scale + this.screenW / 2,
      (ly - this.lowH / 2 - this.offset[1]) * this.scale + this.screenH / 2,
    ];
  }
}
