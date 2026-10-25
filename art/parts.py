"""Generate textured robot parts as glTF for the look-dev scene.

Run with ./art/build.sh (headless Blender). Geometry is modelled in Bevy's
part-local space (+Y up, +Z out of the attach face) and rotated to Blender's
Z-up just before baking, so the glTF exporter hands Bevy back the same axes.

The look comes from procedural Cycles materials (edge chips, cavity rust and
grime, a soft painted top light), baked to one colour texture and one
roughness/metal texture per part, so the game only needs a standard PBR
material and no runtime shader.
"""
import math
import os
import sys

import bpy
import bmesh
from mathutils import Matrix, Vector

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models")
TEX = int(os.environ.get("ART_TEX", "512"))
SAMPLES = int(os.environ.get("ART_SAMPLES", "16"))
HALF_PI = math.pi / 2


# ------------------------------------------------------------------ scene
def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    s = bpy.context.scene
    s.render.engine = "CYCLES"
    s.cycles.device = "CPU"
    s.cycles.samples = SAMPLES
    s.render.bake.margin = 8


def srgb(h):
    h = h.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c) + (1.0,)


def rx(a):
    return Matrix.Rotation(a, 4, "X")


def ry(a):
    return Matrix.Rotation(a, 4, "Y")


def rz(a):
    return Matrix.Rotation(a, 4, "Z")


def at(x, y, z, rot=None):
    m = Matrix.Translation((x, y, z))
    return m @ rot if rot is not None else m


# ------------------------------------------------------------------ materials
class Look:
    """Procedural stylised-wear parameters for one material."""

    def __init__(self, base, chip=0.5, rust=0.3, grime=0.6, rough=0.55, metal=0.0,
                 bare="#cfccc4", chip_scale=13.0, streaks=0.0, light=0.18, rings=0.0):
        self.base, self.chip, self.rust, self.grime = base, chip, rust, grime
        self.rough, self.metal, self.bare = rough, metal, bare
        self.chip_scale, self.streaks, self.light = chip_scale, streaks, light
        self.rings = rings  # concentric lathe/brush marks about the part's local Z


LOOKS = {
    "red": Look("#d8402e", chip=0.65, rust=0.35, streaks=0.4),
    "red_dark": Look("#8e2a22", chip=0.55, rust=0.5),
    "steel": Look("#a9adb5", chip=0.0, rust=0.35, rough=0.4, metal=0.6, streaks=0.3),
    "steel_dark": Look("#5e626c", chip=0.3, rust=0.45, rough=0.45, metal=0.5, bare="#9da2aa"),
    "ink": Look("#2d2b33", chip=0.35, rust=0.0, grime=0.3, rough=0.6, bare="#6b6e76"),
    "yellow": Look("#f2b02e", chip=0.7, rust=0.3, streaks=0.3),
    "blue_cable": Look("#2f62b0", chip=0.0, rust=0.0, grime=0.5, rough=0.45, light=0.25),
    "tyre": Look("#30343c", chip=0.3, rust=0.0, grime=0.12, rough=0.85, light=0.3, bare="#6e6c6a"),
    "blade": Look("#d2d0cb", chip=0.0, rust=0.25, grime=0.5, rough=0.3, metal=0.55, streaks=0.2, light=0.3, rings=0.35),
    "orange": Look("#d0652c", chip=0.6, rust=0.4),
}


def socket(node, name, out=True):
    socks = node.outputs if out else node.inputs
    return socks[name]


def build_material(name, look):
    """A Cycles material whose Emission is the final stylised colour; a
    custom property holds the roughness/metal graph used for the second bake."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    N.clear()

    def node(kind, **kw):
        n = N.new(kind)
        for k, v in kw.items():
            setattr(n, k, v)
        return n

    def math_op(op, a, b=None, clamp=False):
        n = node("ShaderNodeMath", operation=op, use_clamp=clamp)
        for i, v in enumerate((a, b)):
            if v is None:
                continue
            if isinstance(v, (int, float)):
                n.inputs[i].default_value = v
            else:
                L.new(v, n.inputs[i])
        return n.outputs[0]

    def ramp(fac, stops, constant=False):
        n = node("ShaderNodeValToRGB")
        if constant:
            n.color_ramp.interpolation = "CONSTANT"
        els = n.color_ramp.elements
        els[0].position, els[0].color = stops[0][0], (stops[0][1],) * 3 + (1,)
        els[1].position, els[1].color = stops[1][0], (stops[1][1],) * 3 + (1,)
        for p, v in stops[2:]:
            e = els.new(p)
            e.color = (v,) * 3 + (1,)
        L.new(fac, n.inputs[0])
        return n.outputs[0]

    def mix(fac, a, b):
        n = node("ShaderNodeMix", data_type="RGBA", blend_type="MIX")
        for sock, v in ((0, fac), (6, a), (7, b)):
            if isinstance(v, tuple):
                n.inputs[sock].default_value = v
            elif isinstance(v, (int, float)):
                n.inputs[sock].default_value = v
            else:
                L.new(v, n.inputs[sock])
        return n.outputs[2]

    def mul_col(a, b):
        n = node("ShaderNodeMix", data_type="RGBA", blend_type="MULTIPLY")
        n.inputs[0].default_value = 1.0
        for sock, v in ((6, a), (7, b)):
            if isinstance(v, tuple):
                n.inputs[sock].default_value = v
            else:
                L.new(v, n.inputs[sock])
        return n.outputs[2]

    def noise(scale, detail=2.0, rough=0.5, distortion=0.0, coords=None, w=0.0):
        n = node("ShaderNodeTexNoise", noise_dimensions="4D")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = rough
        n.inputs["Distortion"].default_value = distortion
        n.inputs["W"].default_value = w
        L.new(coords, n.inputs["Vector"])
        return n.outputs["Fac"]

    tc = node("ShaderNodeTexCoord")
    obj = tc.outputs["Object"]
    geo = node("ShaderNodeNewGeometry")

    # edge mask: how far a softly bevelled normal departs from the true one
    bev = node("ShaderNodeBevel", samples=8)
    bev.inputs["Radius"].default_value = 0.06
    dot = node("ShaderNodeVectorMath", operation="DOT_PRODUCT")
    L.new(bev.outputs[0], dot.inputs[0])
    L.new(geo.outputs["Normal"], dot.inputs[1])
    edge = math_op("MULTIPLY", math_op("SUBTRACT", 1.0, dot.outputs["Value"]), 5.0, clamp=True)

    # cavity mask from local ambient occlusion
    ao = node("ShaderNodeAmbientOcclusion", samples=16, only_local=True)
    ao.inputs["Distance"].default_value = 0.06
    cavity = math_op("SUBTRACT", 1.0, ao.outputs["AO"], clamp=True)

    # noise sits near 0.5; stretch it to a usable 0..1
    big = ramp(noise(2.2, 3.0, 0.55, coords=obj, w=hash(name) % 97), [(0.4, 0.0), (0.6, 1.0)])
    mid = ramp(noise(look.chip_scale, 8.0, 0.75, distortion=0.1, coords=obj, w=hash(name) % 31), [(0.38, 0.0), (0.62, 1.0)])

    # value variation in the paint: broad, soft, like hand-painted blotches
    base = srgb(look.base)
    lighter = tuple(min(1.0, c * 1.3) for c in base[:3]) + (1.0,)
    darker = tuple(c * 0.72 for c in base[:3]) + (1.0,)
    col = mix(big, darker, lighter)
    col = mix(0.3, col, base)

    # painted top light: up-facing surfaces a touch warmer/lighter
    sep = node("ShaderNodeSeparateXYZ")
    L.new(geo.outputs["Normal"], sep.inputs[0])
    up = math_op("MULTIPLY", math_op("ADD", sep.outputs["Z"], 1.0), 0.5)
    sun = tuple(min(1.0, c * 1.25 + 0.04) for c in base[:3]) + (1.0,)
    col = mix(math_op("MULTIPLY", ramp(up, [(0.45, 0.0), (1.0, 1.0)]), look.light), col, sun)

    if look.rings > 0:
        wv = node("ShaderNodeTexWave", wave_type="RINGS", rings_direction="Y")
        wv.inputs["Scale"].default_value = 9.0
        wv.inputs["Distortion"].default_value = 3.0
        wv.inputs["Detail"].default_value = 3.0
        L.new(obj, wv.inputs["Vector"])
        r = math_op("MULTIPLY", ramp(wv.outputs["Fac"], [(0.2, 0.0), (0.8, 1.0)]), look.rings)
        col = mix(r, col, tuple(c * 0.7 for c in base[:3]) + (1.0,))
        # bright ground edge where the light catches it
        col = mix(math_op("MULTIPLY", ramp(edge, [(0.2, 0.0), (0.6, 1.0)]), 0.8), col, (1.0, 1.0, 0.97, 1.0))

    # vertical drip streaks under rust
    if look.streaks > 0:
        sc = node("ShaderNodeMapping")
        sc.inputs["Scale"].default_value = (14.0, 14.0, 1.2)
        L.new(obj, sc.inputs["Vector"])
        st = noise(1.0, 2.0, 0.5, coords=sc.outputs[0], w=3.0)
        streak = math_op("MULTIPLY", ramp(st, [(0.55, 0.0), (0.75, 1.0)]), look.streaks * 0.5)
        col = mix(streak, col, srgb("#7a3a1e"))

    # bright worn edges (light catching the bevels)
    col = mix(math_op("MULTIPLY", ramp(edge, [(0.1, 0.0), (0.5, 1.0)]), 0.2), col, lighter)

    rough_val = look.rough
    rough_sock = None
    metal_sock = None
    chipmask = None
    if look.chip > 0:
        wear = math_op("ADD", math_op("MULTIPLY", edge, 0.6), math_op("MULTIPLY", mid, 0.7))
        thr = 1.15 - look.chip * 0.45
        outline = ramp(wear, [(0.0, 0.0), (thr - 0.05, 1.0)], constant=True)
        chipmask = ramp(wear, [(0.0, 0.0), (thr, 1.0)], constant=True)
        col = mix(outline, col, tuple(c * 0.5 for c in base[:3]) + (1.0,))
        bare = srgb(look.bare)
        bare_hi = tuple(min(1.0, c * 1.2) for c in bare[:3]) + (1.0,)
        col = mix(chipmask, col, mix(ramp(mid, [(0.4, 0.0), (0.7, 1.0)]), bare, bare_hi))

    rustmask = None
    if look.rust > 0:
        rn = ramp(noise(5.0, 5.0, 0.65, distortion=0.4, coords=obj, w=7.0 + hash(name) % 13), [(0.4, 0.0), (0.6, 1.0)])
        rr = math_op("ADD", math_op("MULTIPLY", cavity, 1.2), math_op("MULTIPLY", rn, 0.6))
        rr = math_op("ADD", rr, math_op("MULTIPLY", edge, 0.2))
        t = 1.0 - look.rust * 0.5
        # crisp two-tone blotches: an orange stain with a dark pitted core
        rustmask = ramp(rr, [(t - 0.07, 0.0), (t - 0.04, 1.0)])
        core = ramp(rr, [(t - 0.005, 0.0), (t + 0.015, 1.0)])
        col = mix(rustmask, col, mix(ramp(rn, [(0.3, 0.0), (0.7, 1.0)]), srgb("#b0532a"), srgb("#c8682e")))
        col = mix(core, col, mix(ramp(mid, [(0.3, 0.0), (0.7, 1.0)]), srgb("#5e2814"), srgb("#8a3c1c")))

    # grime and baked occlusion in the crevices (no SSAO on WebGL2)
    if look.grime > 0:
        # sandy dust settles in shallow creases, dark oily grime in deep ones
        dust = math_op("MULTIPLY", ramp(math_op("ADD", cavity, math_op("MULTIPLY", big, 0.1)), [(0.15, 0.0), (0.35, 1.0)]), look.grime * 0.45)
        col = mix(dust, col, srgb("#b89a72"))
        g = math_op("MULTIPLY", ramp(cavity, [(0.2, 0.0), (0.55, 1.0)]), look.grime)
        col = mix(g, col, mul_col(col, srgb("#3a2a24")))
    occ = ramp(ao.outputs["AO"], [(0.0, 0.45), (0.85, 1.0)])
    col = mul_col(col, occ)

    dbg = os.environ.get("ART_MASK")
    if dbg:
        col = {"edge": edge, "cavity": cavity, "mid": mid, "big": big}[dbg]
    out = node("ShaderNodeOutputMaterial")
    em = node("ShaderNodeEmission")
    L.new(col, em.inputs["Color"])
    L.new(em.outputs[0], out.inputs["Surface"])

    # roughness/metal graph (glTF packing: G = roughness, B = metallic)
    r = rough_val
    rgh = node("ShaderNodeValue")
    rgh.outputs[0].default_value = r
    rough_sock = rgh.outputs[0]
    met = node("ShaderNodeValue")
    met.outputs[0].default_value = look.metal
    metal_sock = met.outputs[0]
    if chipmask is not None:
        n = node("ShaderNodeMix", data_type="FLOAT")
        n.inputs["B"].default_value = 0.32
        L.new(chipmask, n.inputs[0])
        L.new(rough_sock, n.inputs["A"])
        rough_sock = n.outputs[0]
        n2 = node("ShaderNodeMix", data_type="FLOAT")
        n2.inputs["B"].default_value = 0.3
        L.new(chipmask, n2.inputs[0])
        L.new(metal_sock, n2.inputs["A"])
        metal_sock = n2.outputs[0]
    if rustmask is not None:
        n = node("ShaderNodeMix", data_type="FLOAT")
        n.inputs["B"].default_value = 0.95
        L.new(rustmask, n.inputs[0])
        L.new(rough_sock, n.inputs["A"])
        rough_sock = n.outputs[0]
        n2 = node("ShaderNodeMix", data_type="FLOAT")
        n2.inputs["B"].default_value = 0.0
        L.new(rustmask, n2.inputs[0])
        L.new(metal_sock, n2.inputs["A"])
        metal_sock = n2.outputs[0]
    comb = node("ShaderNodeCombineColor")
    L.new(rough_sock, comb.inputs["Green"])
    L.new(metal_sock, comb.inputs["Blue"])
    em2 = node("ShaderNodeEmission")
    em2.name = "MR_EMIT"
    L.new(comb.outputs[0], em2.inputs["Color"])
    m["em_color"] = em.name
    return m


MATS = {}


def mat(key):
    if key not in MATS:
        MATS[key] = build_material(key, LOOKS[key])
    return MATS[key]


def glow_material():
    m = bpy.data.materials.new("glow")
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = srgb("#ffd25a")
    b.inputs["Emission Color"].default_value = srgb("#ffd25a")
    b.inputs["Emission Strength"].default_value = 1.0
    return m


# ------------------------------------------------------------------ geometry
def _finish(obj, m, xf, bevel, segs=3, smooth=True):
    obj.data.materials.append(m)
    obj.matrix_world = xf @ obj.matrix_world
    bpy.context.view_layer.update()
    # bake in the transform so bevel widths are in real units
    obj.data.transform(obj.matrix_world)
    obj.matrix_world = Matrix.Identity(4)
    if bevel > 0:
        b = obj.modifiers.new("bevel", "BEVEL")
        b.width = bevel
        b.segments = segs
        b.limit_method = "ANGLE"
        b.angle_limit = math.radians(35)
        b.harden_normals = False
        b.profile = 0.6
    if smooth:
        for p in obj.data.polygons:
            p.use_smooth = True
        wn = obj.modifiers.new("wn", "WEIGHTED_NORMAL")
        wn.keep_sharp = True
        wn.weight = 100
    return obj


def _new(name, bm):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.collection.objects.link(o)
    return o


def box(w, h, d, look, xf, bevel=0.02, name="box"):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=(w, h, d), verts=bm.verts)
    return _finish(_new(name, bm), mat(look), xf, min(bevel, w / 2.2, h / 2.2, d / 2.2))


def cyl(rt, rb, h, seg, look, xf, bevel=0.015, name="cyl", smooth_sides=True):
    """Cylinder along +Y, centred."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=rb, radius2=rt, depth=h)
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=rx(-HALF_PI))
    o = _new(name, bm)
    return _finish(o, mat(look), xf, min(bevel, h / 2.2), smooth=smooth_sides)


def bolt(r, look, xf):
    return cyl(r, r, r * 0.8, 6, look, xf, bevel=r * 0.3, name="bolt")


def rod(a, b, r, look, seg=8, name="rod"):
    a, b = Vector(a), Vector(b)
    d = b - a
    q = Vector((0, 1, 0)).rotation_difference(d.normalized())
    return cyl(r, r, d.length, seg, look, Matrix.Translation((a + b) / 2) @ q.to_matrix().to_4x4(), bevel=r * 0.3, name=name)


def cable(pts, r, look):
    curve = bpy.data.curves.new("cable", "CURVE")
    curve.dimensions = "3D"
    sp = curve.splines.new("BEZIER")
    sp.bezier_points.add(len(pts) - 1)
    for bp, p in zip(sp.bezier_points, pts):
        bp.co = p
        bp.handle_left_type = bp.handle_right_type = "AUTO"
    curve.bevel_depth = r
    curve.bevel_resolution = 3
    curve.resolution_u = 8
    o = bpy.data.objects.new("cable", curve)
    bpy.context.collection.objects.link(o)
    bpy.context.view_layer.objects.active = o
    o.select_set(True)
    bpy.ops.object.convert(target="MESH")
    o = bpy.context.view_layer.objects.active
    o.select_set(False)
    return _finish(o, mat(look), Matrix.Identity(4), 0.0)


def vents(w, h, n, look, xf):
    out = []
    for i in range(n):
        y = -h / 2 + (i + 0.5) * (h / n)
        out.append(box(w, h / (n * 2), 0.03, look, xf @ at(0, y, 0) @ rx(-0.35), bevel=0.006, name="vent"))
    return out


def face_bolts(w, h, z, inset, r, look="steel_dark"):
    return [bolt(r, look, at(sx * (w / 2 - inset), sy * (h / 2 - inset), z, rx(HALF_PI)))
            for sx in (-1, 1) for sy in (-1, 1)]


def corners(w, d, y, inset, r, look="steel_dark"):
    return [bolt(r, look, at(sx * (w / 2 - inset), y, sz * (d / 2 - inset))) for sx in (-1, 1) for sz in (-1, 1)]


# ------------------------------------------------------------------ parts
def build_core():
    """Brain Box: the bot's head/body with glowing eyes."""
    groups = {"body": []}
    B = groups["body"]
    B.append(box(0.54, 0.4, 0.54, "red", at(0, 0, 0), bevel=0.05, name="shell"))
    B.append(box(0.565, 0.085, 0.565, "red_dark", at(0, -0.13, 0), bevel=0.02, name="band"))
    # face plate, eye sockets, grille
    B.append(box(0.44, 0.24, 0.06, "ink", at(0, 0.04, 0.28), bevel=0.025, name="face"))
    for sx in (-0.1, 0.1):
        B.append(cyl(0.078, 0.078, 0.06, 20, "steel_dark", at(sx, 0.045, 0.31, rx(HALF_PI)), bevel=0.012))
        B.append(cyl(0.06, 0.06, 0.02, 20, "ink", at(sx, 0.045, 0.335, rx(HALF_PI)), bevel=0.004))
    B += vents(0.22, 0.07, 3, "steel_dark", at(0, -0.075, 0.315))
    # top hatch, handle, hinges, corner bolts
    B.append(box(0.3, 0.045, 0.28, "red_dark", at(0, 0.215, -0.04), bevel=0.015, name="hatch"))
    B.append(box(0.13, 0.035, 0.035, "steel_dark", at(0, 0.25, 0.06), bevel=0.01))
    for sx in (-0.1, 0.1):
        B.append(cyl(0.022, 0.022, 0.06, 10, "steel_dark", at(sx, 0.225, -0.185, rz(HALF_PI)), bevel=0.005))
    B += corners(0.46, 0.46, 0.205, 0.0, 0.022)
    # rivet rows along the shell's vertical seams
    for sx in (-1, 1):
        for sz in (-1, 1):
            for y in (-0.06, 0.02, 0.1):
                B.append(bolt(0.014, "steel_dark", at(sx * 0.272, y, sz * 0.235, rz(-sx * HALF_PI))))
    # side vents and bolted cheek plates
    for sx in (-1, 1):
        B.append(box(0.03, 0.2, 0.3, "steel_dark", at(sx * 0.278, 0.02, -0.04), bevel=0.01, name="cheek"))
        B += vents(0.24, 0.15, 4, "ink", at(sx * 0.295, 0.02, -0.04, ry(sx * HALF_PI)))
    # antenna + cable
    B.append(cyl(0.035, 0.048, 0.06, 12, "steel_dark", at(-0.17, 0.225, -0.17)))
    B.append(rod((-0.17, 0.24, -0.17), (-0.17, 0.62, -0.19), 0.012, "steel_dark", 6))
    B.append(cable([Vector(p) for p in [(0.12, 0.2, -0.22), (0.18, 0.31, -0.31), (0.23, 0.12, -0.3), (0.2, 0.0, -0.275)]], 0.018, "blue_cable"))
    glow = []
    gm = glow_material()
    for sx in (-0.1, 0.1):
        bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=16, v_segments=8, radius=0.05)
        bmesh.ops.scale(bm, vec=(1, 1, 0.5), verts=bm.verts)
        o = _new("glow_eye", bm)
        o.data.materials.append(gm)
        o.data.transform(at(sx, 0.045, 0.335))
        for p in o.data.polygons:
            p.use_smooth = True
        glow.append(o)
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=2, radius=0.03)
    o = _new("glow_tip", bm)
    o.data.materials.append(glow_material())
    o.data.materials[0].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = srgb("#ff5a3c")
    o.data.transform(at(-0.17, 0.635, -0.19))
    for p in o.data.polygons:
        p.use_smooth = True
    glow.append(o)
    return groups, glow


def build_wheel():
    """Wheel: bracket + motor (body) and a spinning tyre (spin)."""
    groups = {"body": [], "spin": []}
    B, S = groups["body"], groups["spin"]
    r, w = 0.3, 0.22
    zc = 0.24 + 0.02
    B.append(box(0.2, 0.2, 0.05, "steel_dark", at(0, 0, 0.025), bevel=0.015))
    B += face_bolts(0.18, 0.18, 0.05, 0.02, 0.016)
    mlen = zc - w / 2 - 0.04
    B.append(cyl(0.075, 0.075, mlen, 16, "orange", at(0, 0, (zc - w / 2 + 0.04) / 2 + 0.01, rx(HALF_PI)), bevel=0.012))
    for i in range(3):
        B.append(cyl(0.095, 0.095, 0.014, 16, "steel_dark", at(0, 0, 0.08 + i * 0.03, rx(HALF_PI)), bevel=0.004))
    # tyre: a fat rounded drum with chunky lugs
    S.append(cyl(r - 0.02, r - 0.02, w, 32, "tyre", at(0, 0, 0, rx(HALF_PI)), bevel=0.06, name="tyre"))
    n = 12
    for i in range(n):
        a = i / n * math.tau
        S.append(box(0.075, 0.05, w * 0.84, "tyre", at(math.cos(a) * (r - 0.01), math.sin(a) * (r - 0.01), 0, rz(a - HALF_PI)), bevel=0.012, name="lug"))
    for sz in (-1, 1):
        S.append(cyl(r * 0.62, r * 0.62, 0.035, 24, "yellow", at(0, 0, sz * (w / 2 + 0.002), rx(HALF_PI)), bevel=0.012, name="rim"))
    S.append(cyl(r * 0.24, r * 0.28, 0.06, 16, "steel_dark", at(0, 0, w / 2 + 0.03, rx(HALF_PI)), bevel=0.012))
    for i in range(5):
        a = i / 5 * math.tau
        S.append(box(0.04, r * 0.48, 0.03, "yellow", at(math.cos(a) * r * 0.36, math.sin(a) * r * 0.36, w / 2 + 0.022, rz(a - HALF_PI)), bevel=0.01))
        S.append(bolt(0.017, "steel", at(math.cos(a + 0.6) * r * 0.17, math.sin(a + 0.6) * r * 0.17, w / 2 + 0.065, rx(HALF_PI))))
    # the spinning group is modelled about its own origin (Bevy spins it about local Z)
    return groups, [], {"spin": (0, 0, zc)}


# ------------------------------------------------------------------ bake + export
def join(objs, name):
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    for o in objs:
        bpy.context.view_layer.objects.active = o
        for mod in list(o.modifiers):
            bpy.ops.object.modifier_apply(modifier=mod.name)
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    o = bpy.context.view_layer.objects.active
    o.name = name
    o.data.name = name
    o.select_set(False)
    return o


def unwrap(objs):
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    for o in objs:
        o.select_set(True)
        o.data.uv_layers.new(name="UVMap") if not o.data.uv_layers else None
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(50), island_margin=0.006, scale_to_bounds=False)
    bpy.ops.uv.pack_islands(margin=0.006, rotate=True)
    bpy.ops.object.mode_set(mode="OBJECT")


def bake(objs, img, emit_node=None):
    """Bake Emit for every material on objs into img. emit_node selects which
    emission shader feeds the output ('MR_EMIT' for roughness/metal)."""
    mats = {m for o in objs for m in o.data.materials}
    saved = {}
    for m in mats:
        nt = m.node_tree
        out = next(n for n in nt.nodes if n.type == "OUTPUT_MATERIAL")
        saved[m] = out.inputs["Surface"].links[0].from_node
        if emit_node:
            nt.links.new(nt.nodes[emit_node].outputs[0], out.inputs["Surface"])
        tn = nt.nodes.get("BAKE_TARGET") or nt.nodes.new("ShaderNodeTexImage")
        tn.name = "BAKE_TARGET"
        tn.image = img
        for n in nt.nodes:
            n.select = False
        tn.select = True
        nt.nodes.active = tn
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.bake(type="EMIT", margin=8)
    for m, n in saved.items():
        m.node_tree.links.new(n.outputs[0], next(x for x in m.node_tree.nodes if x.type == "OUTPUT_MATERIAL").inputs["Surface"])


def export_part(name, builder):
    reset()
    MATS.clear()
    res = builder()
    groups, glow = res[0], res[1]
    origins = res[2] if len(res) > 2 else {}
    joined = [join(objs, gname) for gname, objs in groups.items() if objs]
    # Bevy space (Y up) -> Blender (Z up): the glTF exporter undoes this
    to_bl = rx(HALF_PI)
    for o in joined + glow:
        o.data.transform(to_bl)
    for o in joined:
        if o.name in origins:
            o.location = to_bl @ Vector(origins[o.name])
    unwrap(joined)
    col = bpy.data.images.new(f"{name}_col", TEX, TEX)
    mr = bpy.data.images.new(f"{name}_mr", TEX, TEX)
    mr.colorspace_settings.name = "Non-Color"
    bake(joined, col)
    bake(joined, mr, "MR_EMIT")
    for img in (col, mr):
        img.file_format = "PNG"
        img.pack()
    # one export material for the whole part
    em = bpy.data.materials.new(name)
    em.use_nodes = True
    nt = em.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    t1 = nt.nodes.new("ShaderNodeTexImage")
    t1.image = col
    nt.links.new(t1.outputs["Color"], bsdf.inputs["Base Color"])
    t2 = nt.nodes.new("ShaderNodeTexImage")
    t2.image = mr
    sep = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(t2.outputs["Color"], sep.inputs[0])
    nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
    nt.links.new(sep.outputs["Blue"], bsdf.inputs["Metallic"])
    for o in joined:
        o.data.materials.clear()
        o.data.materials.append(em)
    for o in bpy.context.view_layer.objects:
        o.select_set(o in joined or o in glow)
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, f"{name}.glb")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=True,
                              export_apply=True, export_yup=True, export_image_format="AUTO")
    if os.environ.get("ART_DEBUG"):
        col.save_render(os.path.join("/tmp", f"{name}_col.png"))
    print(f"wrote {path}")


def build_spinner():
    """Spinner: finned motor housing (body) and a toothed disc (spin)."""
    groups = {"body": [], "spin": []}
    B, S = groups["body"], groups["spin"]
    B.append(box(0.36, 0.36, 0.14, "steel_dark", at(0, 0, 0.07), bevel=0.03))
    for i in range(4):
        B.append(box(0.39, 0.022, 0.1, "ink", at(0, -0.12 + i * 0.08, 0.07), bevel=0.008))
    B += face_bolts(0.34, 0.34, 0.14, 0.03, 0.016)
    B.append(cyl(0.08, 0.1, 0.07, 16, "ink", at(0, 0, 0.16, rx(HALF_PI)), bevel=0.01))
    # disc in its own frame (Bevy spins it about local Z)
    S.append(cyl(0.42, 0.42, 0.05, 48, "blade", at(0, 0, 0, rx(HALF_PI)), bevel=0.01, name="disc"))
    S.append(cyl(0.15, 0.15, 0.09, 24, "red", at(0, 0, 0, rx(HALF_PI)), bevel=0.02, name="hub"))
    for i in range(6):
        a = i / 6 * math.tau
        S.append(bolt(0.018, "steel", at(math.cos(a) * 0.1, math.sin(a) * 0.1, 0.05, rx(HALF_PI))))
    for i in range(3):
        a = i / 3 * math.tau
        tooth = cyl(0.0, 0.11, 0.18, 4, "blade", Matrix.Identity(4), bevel=0.01, name="tooth")
        tooth.data.transform(at(math.cos(a) * 0.46, math.sin(a) * 0.46, 0, rz(a - HALF_PI)) @ Matrix.Diagonal((1, 1, 0.4, 1)))
        S.append(tooth)
        # painted danger slot
        S.append(box(0.14, 0.03, 0.06, "red", at(math.cos(a + 1) * 0.28, math.sin(a + 1) * 0.28, 0, rz(a + 1)), bevel=0.008))
    return groups, [], {"spin": (0, 0, 0.2)}


PARTS = {"core": build_core, "wheel": build_wheel, "spinner": build_spinner}

if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    for n in argv or PARTS:
        export_part(n, PARTS[n])
