// Raw WebGPU renderer: shadow pass -> low-res scene pass (colour + normals +
// depth) -> full-screen post pass that upscales with outlines.
import { mat4 } from 'wgpu-matrix';
import sceneSrc from './scene.wgsl?raw';
import postSrc from './post.wgsl?raw';
import { Camera } from './camera';
import { DrawList, INSTANCE_FLOATS } from './drawlist';
import { VERTEX_FLOATS } from './mesh';
import { V3, add, cross, dot, norm, scale } from '../math';

export interface RenderOptions {
  outlines: boolean;
  palette: boolean;
  shadows: boolean;
}

export const SKY: V3 = [0.55, 0.62, 0.66];
const SHADOW_SIZE = 1024;

const vertexBuffers: GPUVertexBufferLayout[] = [
  {
    arrayStride: VERTEX_FLOATS * 4,
    stepMode: 'vertex',
    attributes: [
      { shaderLocation: 0, offset: 0, format: 'float32x3' },
      { shaderLocation: 1, offset: 12, format: 'float32x3' },
      { shaderLocation: 2, offset: 24, format: 'float32x3' },
    ],
  },
  {
    arrayStride: INSTANCE_FLOATS * 4,
    stepMode: 'instance',
    attributes: [
      { shaderLocation: 3, offset: 0, format: 'float32x4' },
      { shaderLocation: 4, offset: 16, format: 'float32x4' },
      { shaderLocation: 5, offset: 32, format: 'float32x4' },
      { shaderLocation: 6, offset: 48, format: 'float32x4' },
    ],
  },
];

export class Renderer {
  private meshes: { buf: GPUBuffer; count: number }[] = [];
  private instanceBuf: GPUBuffer;
  private globals: GPUBuffer;
  private postUniform: GPUBuffer;
  private scenePipe: GPURenderPipeline;
  private shadowPipe: GPURenderPipeline;
  private postPipe: GPURenderPipeline;
  private sceneGlobalsBG: GPUBindGroup;
  private shadowGlobalsBG: GPUBindGroup;
  private shadowBG: GPUBindGroup;
  private shadowTex: GPUTexture;
  private targets?: { w: number; h: number; color: GPUTexture; normal: GPUTexture; depth: GPUTexture; postBG: GPUBindGroup };

  private constructor(
    readonly device: GPUDevice,
    private ctx: GPUCanvasContext,
    private format: GPUTextureFormat,
    readonly adapterName: string,
  ) {
    const scene = device.createShaderModule({ code: sceneSrc });
    const post = device.createShaderModule({ code: postSrc });

    this.scenePipe = device.createRenderPipeline({
      layout: 'auto',
      vertex: { module: scene, entryPoint: 'vs_main', buffers: vertexBuffers },
      fragment: { module: scene, entryPoint: 'fs_main', targets: [{ format: 'rgba8unorm' }, { format: 'rgba16float' }] },
      primitive: { topology: 'triangle-list', cullMode: 'none' },
      depthStencil: { format: 'depth32float', depthWriteEnabled: true, depthCompare: 'less' },
    });
    this.shadowPipe = device.createRenderPipeline({
      layout: 'auto',
      vertex: { module: scene, entryPoint: 'vs_shadow', buffers: vertexBuffers },
      primitive: { topology: 'triangle-list', cullMode: 'none' },
      depthStencil: { format: 'depth32float', depthWriteEnabled: true, depthCompare: 'less', depthBias: 2, depthBiasSlopeScale: 1.5 },
    });
    this.postPipe = device.createRenderPipeline({
      layout: 'auto',
      vertex: { module: post, entryPoint: 'vs' },
      fragment: { module: post, entryPoint: 'fs', targets: [{ format }] },
      primitive: { topology: 'triangle-list' },
    });

    this.globals = device.createBuffer({ size: 160, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    this.postUniform = device.createBuffer({ size: 64, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    this.instanceBuf = device.createBuffer({ size: 64 * 1024, usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST });
    this.shadowTex = device.createTexture({
      size: [SHADOW_SIZE, SHADOW_SIZE],
      format: 'depth32float',
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });

    const globalsEntry = [{ binding: 0, resource: { buffer: this.globals } }];
    this.sceneGlobalsBG = device.createBindGroup({ layout: this.scenePipe.getBindGroupLayout(0), entries: globalsEntry });
    this.shadowGlobalsBG = device.createBindGroup({ layout: this.shadowPipe.getBindGroupLayout(0), entries: globalsEntry });
    this.shadowBG = device.createBindGroup({
      layout: this.scenePipe.getBindGroupLayout(1),
      entries: [
        { binding: 0, resource: this.shadowTex.createView() },
        { binding: 1, resource: device.createSampler({ compare: 'less', magFilter: 'nearest', minFilter: 'nearest' }) },
      ],
    });
  }

  static async create(canvas: HTMLCanvasElement): Promise<Renderer> {
    if (!navigator.gpu) throw new Error('This browser does not expose WebGPU (navigator.gpu is missing).');
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
    if (!adapter) throw new Error('WebGPU is available but no GPU adapter was found.');
    const device = await adapter.requestDevice();
    device.lost.then((info) => console.error('WebGPU device lost:', info.message));
    device.addEventListener('uncapturederror', (e) => console.error('WebGPU error:', (e as GPUUncapturedErrorEvent).error.message));
    const ctx = canvas.getContext('webgpu');
    if (!ctx) throw new Error('Could not create a WebGPU canvas context.');
    const format = navigator.gpu.getPreferredCanvasFormat();
    ctx.configure({ device, format, alphaMode: 'opaque' });
    const info = adapter.info;
    const name = [info.vendor, info.architecture, info.description].filter(Boolean).join(' ') || 'unknown adapter';
    return new Renderer(device, ctx, format, name);
  }

  setMeshes(meshes: Float32Array[]) {
    for (const m of this.meshes) m.buf.destroy();
    this.meshes = meshes.map((data) => {
      const buf = this.device.createBuffer({ size: data.byteLength, usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST });
      this.device.queue.writeBuffer(buf, 0, data);
      return { buf, count: data.length / VERTEX_FLOATS };
    });
  }

  private ensureTargets(w: number, h: number) {
    if (this.targets && this.targets.w === w && this.targets.h === h) return this.targets;
    if (this.targets) for (const t of [this.targets.color, this.targets.normal, this.targets.depth]) t.destroy();
    const usage = GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING;
    const color = this.device.createTexture({ size: [w, h], format: 'rgba8unorm', usage });
    const normal = this.device.createTexture({ size: [w, h], format: 'rgba16float', usage });
    const depth = this.device.createTexture({ size: [w, h], format: 'depth32float', usage });
    const postBG = this.device.createBindGroup({
      layout: this.postPipe.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.postUniform } },
        { binding: 1, resource: color.createView() },
        { binding: 2, resource: normal.createView() },
        { binding: 3, resource: depth.createView() },
      ],
    });
    this.targets = { w, h, color, normal, depth, postBG };
    return this.targets;
  }

  /** Light-space matrix covering the visible area, snapped to shadow texels so shadows don't crawl. */
  private lightMatrix(cam: Camera, lightDir: V3) {
    const half = Math.max(cam.lowW, cam.lowH) * cam.texel * 0.6 + 3;
    const st = (half * 2) / SHADOW_SIZE;
    const f = scale(lightDir, -1);
    const r = norm(cross(f, [0, 1, 0]));
    const u = cross(r, f);
    const c = cam.target;
    const centre = add(
      add(scale(r, Math.round(dot(c, r) / st) * st), scale(u, Math.round(dot(c, u) / st) * st)),
      scale(f, dot(c, f)),
    );
    const view = mat4.lookAt(add(centre, scale(lightDir, 60)), centre, [0, 1, 0]);
    return { m: mat4.multiply(mat4.ortho(-half, half, -half, half, 1, 120), view), texel: st };
  }

  render(cam: Camera, list: DrawList, opts: RenderOptions, lightDir: V3, time: number) {
    const { device } = this;
    const { data, ranges } = list.pack();
    if (data.byteLength > this.instanceBuf.size) {
      this.instanceBuf.destroy();
      let size = this.instanceBuf.size;
      while (size < data.byteLength) size *= 2;
      this.instanceBuf = device.createBuffer({ size, usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST });
    }
    device.queue.writeBuffer(this.instanceBuf, 0, data);

    const light = this.lightMatrix(cam, lightDir);
    const g = new Float32Array(40);
    g.set(cam.viewProj, 0);
    g.set(light.m, 16);
    g.set([lightDir[0], lightDir[1], lightDir[2], opts.shadows ? 1 : 0], 32);
    g.set([time, light.texel, 0, 0], 36);
    device.queue.writeBuffer(this.globals, 0, g);

    const t = this.ensureTargets(cam.lowW, cam.lowH);
    device.queue.writeBuffer(
      this.postUniform,
      0,
      new Float32Array([
        cam.screenW, cam.screenH, cam.lowW, cam.lowH,
        cam.offset[0], cam.offset[1], cam.scale, opts.outlines ? 1 : 0,
        cam.texelOrigin[0], cam.texelOrigin[1], opts.palette ? 1 : 0, cam.texel,
        cam.far - cam.near, 0, 0, 0,
      ]),
    );

    const enc = device.createCommandEncoder();
    const drawAll = (pass: GPURenderPassEncoder) => {
      pass.setVertexBuffer(1, this.instanceBuf);
      ranges.forEach(([first, count], prim) => {
        const mesh = this.meshes[prim];
        if (!count || !mesh) return;
        pass.setVertexBuffer(0, mesh.buf);
        pass.draw(mesh.count, count, 0, first);
      });
    };

    const shadow = enc.beginRenderPass({
      colorAttachments: [],
      depthStencilAttachment: { view: this.shadowTex.createView(), depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store' },
    });
    if (opts.shadows) {
      shadow.setPipeline(this.shadowPipe);
      shadow.setBindGroup(0, this.shadowGlobalsBG);
      drawAll(shadow);
    }
    shadow.end();

    const scenePass = enc.beginRenderPass({
      colorAttachments: [
        { view: t.color.createView(), clearValue: [SKY[0], SKY[1], SKY[2], 1], loadOp: 'clear', storeOp: 'store' },
        { view: t.normal.createView(), clearValue: [0, 0, 0, 0], loadOp: 'clear', storeOp: 'store' },
      ],
      depthStencilAttachment: { view: t.depth.createView(), depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store' },
    });
    scenePass.setPipeline(this.scenePipe);
    scenePass.setBindGroup(0, this.sceneGlobalsBG);
    scenePass.setBindGroup(1, this.shadowBG);
    drawAll(scenePass);
    scenePass.end();

    const postPass = enc.beginRenderPass({
      colorAttachments: [{ view: this.ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }],
    });
    postPass.setPipeline(this.postPipe);
    postPass.setBindGroup(0, t.postBG);
    postPass.draw(3);
    postPass.end();

    device.queue.submit([enc.finish()]);
  }

  get canvasFormat() {
    return this.format;
  }
}
