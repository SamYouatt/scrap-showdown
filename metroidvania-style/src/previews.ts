// Small pixel-art previews of each part for the parts bin. Each part is fitted to a
// bare hull, settled for a moment, and only that piece is drawn through the normal
// pixel pipeline with a transparent background, then copied out as an image.
import { Camera } from './render/camera';
import { DrawList, INSTANCE_FLOATS } from './render/drawlist';
import { primitiveMeshes } from './render/mesh';
import { Renderer } from './render/renderer';
import { Creature, CreaturePiece, Design, Hull } from './creature';
import { BODIES, BodyId, PARTS, PartDef, designFor } from './parts';
import { V3, add, dot, norm, scale } from './math';

/** Preview size in CSS pixels; rendered at twice that with two CSS pixels per texel. */
export const PREVIEW_W = 72;
export const PREVIEW_H = 52;
const DPR = 2;
const PIXEL = 2;

const HULLS: Record<BodyId, Hull> = { box: 'crate', drum: 'drum', pod: 'pod' };
const flat = { half: 50, heightAt: () => 0 };

function subject(p: PartDef): { design: Design; only: CreaturePiece[] } {
  if (p.section === 'legs') return { design: designFor({ body: 'box', slots: { legs: p.id } }), only: ['hull', 'legs'] };
  if (p.section === 'control') return { design: designFor({ body: 'box', slots: { fronttop: p.id } }), only: ['head'] };
  const slot = p.fits.includes('front') ? 'front' : p.fits[0];
  return { design: designFor({ body: 'box', slots: { [slot]: p.id } }), only: ['tools'] };
}

/** Renders every body and part; returns image URLs keyed by body or part id. */
export async function renderPreviews(): Promise<Map<string, string>> {
  const canvas = Object.assign(document.createElement('canvas'), { width: PREVIEW_W * DPR, height: PREVIEW_H * DPR });
  const renderer = await Renderer.create(canvas, true);
  renderer.setMeshes(primitiveMeshes());
  const copy = Object.assign(document.createElement('canvas'), { width: canvas.width, height: canvas.height });
  const g = copy.getContext('2d')!;
  const cam = new Camera();
  cam.yaw = 0.75;
  const list = new DrawList();
  const opts = { outlines: true, palette: false, shadows: true };
  const ly = cam.yaw - Math.PI / 4;
  const light = norm([-0.35 * Math.cos(ly) + 0.55 * Math.sin(ly), 1, 0.35 * Math.sin(ly) + 0.55 * Math.cos(ly)] as V3);

  const jobs: [string, Design, CreaturePiece[]][] = [
    ...BODIES.map((b): [string, Design, CreaturePiece[]] => [b.id, { ...designFor({ body: 'box', slots: {} }), hull: HULLS[b.id] }, ['hull']]),
    ...PARTS.map((p): [string, Design, CreaturePiece[]] => {
      const s = subject(p);
      return [p.id, s.design, s.only];
    }),
  ];
  const out = new Map<string, string>();
  for (const [key, design, only] of jobs) {
    const c = new Creature(design, flat, [0, 0, 0], 0);
    // Heads look straight ahead, so the camera sees them three-quarter on; legs and tools settle into their rest pose.
    c.lookPoint = [0, 0.8, 6];
    for (let i = 0; i < 40; i++) c.update(1 / 60, i / 60);
    list.clear();
    c.draw(list, only);
    frame(cam, list, canvas.width, canvas.height);
    renderer.render(cam, list, opts, light, 0);
    g.clearRect(0, 0, copy.width, copy.height);
    g.drawImage(canvas, 0, 0);
    out.set(key, copy.toDataURL('image/png'));
  }
  renderer.device.destroy();
  return out;
}

/** Points the camera at the middle of what's drawn and zooms so it fills the frame. */
function frame(cam: Camera, list: DrawList, w: number, h: number) {
  cam.target = [0, 0, 0];
  cam.update(w, h, DPR, PIXEL);
  const { data, ranges } = list.pack();
  const count = ranges.reduce((n, [, k]) => n + k, 0);
  let u0 = Infinity, u1 = -Infinity, v0 = Infinity, v1 = -Infinity;
  for (let i = 0; i < count; i++) {
    const o = i * INSTANCE_FLOATS;
    const p: V3 = [data[o], data[o + 1], data[o + 2]];
    const r = Math.max(data[o + 8], data[o + 9], data[o + 10]) * 0.45;
    const u = dot(p, cam.right), v = dot(p, cam.up);
    u0 = Math.min(u0, u - r); u1 = Math.max(u1, u + r);
    v0 = Math.min(v0, v - r); v1 = Math.max(v1, v + r);
  }
  const texel = Math.max((u1 - u0) / (cam.lowW - 6), (v1 - v0) / (cam.lowH - 6));
  cam.zoom = cam.scale / (texel * DPR);
  cam.target = add(scale(cam.right, (u0 + u1) / 2), scale(cam.up, (v0 + v1) / 2));
  cam.update(w, h, DPR, PIXEL);
}
