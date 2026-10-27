"""Shared Blender pipeline: stylised-wear materials, modelling helpers, and the
bake + glTF export every asset goes through.

Geometry is modelled in Bevy space (+Y up). `export()` rotates it to Blender's
Z-up just before baking, so the glTF exporter hands Bevy back the same axes.

Each asset is one .glb with one baked material (colour + roughness/metal
textures) shared by all its meshes. An asset has one or more *groups*; each
group becomes one glTF node + mesh named after the group, so the game can move
groups independently (a wheel's tyre, a hammer's arm). A group's origin (its
pivot) comes from `origins`; its vertices are stored relative to it.

Tinted looks (`Look(tint=n)`) are baked in neutral grey and tagged with a
vertex-colour marker; the game multiplies in a runtime colour per tint slot
(a bot's paint, a barrel's colour, a crowd member's shirt).

Glow objects (names starting `glow_`) are exported untextured; the game swaps
in its own blooming material.
"""
import math
import os

import bpy
import bmesh
from mathutils import Matrix, Vector

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models")
TEX = int(os.environ.get("ART_TEX", "512"))
SAMPLES = int(os.environ.get("ART_SAMPLES", "16"))
HALF_PI = math.pi / 2
TAU = math.tau

# vertex-colour markers for tint slots (the game reads these back)
TINT_MARKERS = [(1.0, 0.0, 0.0, 1.0), (0.0, 1.0, 0.0, 1.0), (0.0, 0.0, 1.0, 1.0)]


# ------------------------------------------------------------------ scene
def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    s = bpy.context.scene
    s.render.engine = "CYCLES"
    s.cycles.samples = SAMPLES
    s.render.bake.margin = 8
    # Apple Silicon / NVIDIA GPUs bake much faster; fall back to the CPU.
    s.cycles.device = "CPU"
    if os.environ.get("ART_GPU", "0") == "1":
        try:
            prefs = bpy.context.preferences.addons["cycles"].preferences
            for kind in ("METAL", "OPTIX", "CUDA", "HIP"):
                try:
                    prefs.compute_device_type = kind
                except TypeError:
                    continue
                prefs.get_devices()
                gpus = [d for d in prefs.devices if d.type == kind]
                if gpus:
                    for d in prefs.devices:
                        d.use = d.type == kind
                    s.cycles.device = "GPU"
                    break
        except Exception:
            pass
    MATS.clear()


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


def scl(x, y, z):
    return Matrix.Diagonal((x, y, z, 1.0))


def at(x, y, z, rot=None):
    m = Matrix.Translation((x, y, z))
    return m @ rot if rot is not None else m


# ------------------------------------------------------------------ materials
class Look:
    """Procedural stylised-wear parameters for one material."""

    def __init__(self, base, chip=0.5, rust=0.3, grime=0.6, rough=0.55, metal=0.0,
                 bare="#cfccc4", chip_scale=13.0, streaks=0.0, light=0.18, rings=0.0,
                 tint=None, chip_col=None, edge=0.2, dust=None, stripes=0.0, stripe_col="#2a2830"):
        self.base, self.chip, self.rust, self.grime = base, chip, rust, grime
        self.rough, self.metal, self.bare = rough, metal, bare
        self.chip_scale, self.streaks, self.light = chip_scale, streaks, light
        self.rings = rings  # concentric lathe/brush marks about the part's local Z
        self.tint = tint  # runtime tint slot (0..2) or None
        self.chip_col = chip_col  # override for what chips reveal
        self.edge = edge  # bright worn-edge highlight strength
        self.dust = dust  # dust colour in shallow creases
        self.stripes = stripes  # diagonal hazard stripe period in metres (0 = none)
        self.stripe_col = stripe_col


# Painted looks with `tint` are baked neutral: the game multiplies in the colour.
# Keep them light so the multiplied result matches the chosen paint.
NEUTRAL = "#f0efec"
LOOKS = {
    "red": Look("#d8402e", chip=0.65, rust=0.35, streaks=0.4),
    "red_dark": Look("#8e2a22", chip=0.55, rust=0.5),
    "steel": Look("#a9adb5", chip=0.0, rust=0.35, rough=0.4, metal=0.6, streaks=0.3),
    "steel_clean": Look("#b4b8bf", chip=0.0, rust=0.1, rough=0.35, metal=0.65, streaks=0.1, grime=0.4),
    "steel_dark": Look("#5e626c", chip=0.3, rust=0.45, rough=0.45, metal=0.5, bare="#9da2aa"),
    "ink": Look("#2d2b33", chip=0.35, rust=0.0, grime=0.3, rough=0.6, bare="#6b6e76"),
    "yellow": Look("#f2b02e", chip=0.7, rust=0.3, streaks=0.3),
    "blue_cable": Look("#2f62b0", chip=0.0, rust=0.0, grime=0.5, rough=0.45, light=0.25),
    "red_cable": Look("#c8382c", chip=0.0, rust=0.0, grime=0.5, rough=0.45, light=0.25),
    "tyre": Look("#30343c", chip=0.3, rust=0.0, grime=0.12, rough=0.85, light=0.3, bare="#6e6c6a"),
    "blade": Look("#d2d0cb", chip=0.0, rust=0.25, grime=0.5, rough=0.3, metal=0.55, streaks=0.2, light=0.3, rings=0.35),
    "orange": Look("#d0652c", chip=0.6, rust=0.4),
    "olive": Look("#7c8a46", chip=0.65, rust=0.4, streaks=0.3),
    "teal": Look("#3fa7a3", chip=0.6, rust=0.35, streaks=0.3),
    "girder": Look("#d6a24a", chip=0.6, rust=0.45, streaks=0.3),
    "battery": Look("#33343c", chip=0.2, rust=0.0, grime=0.5, rough=0.5, bare="#5a5c64"),
    "cream": Look("#efe2c4", chip=0.5, rust=0.3, streaks=0.3, bare="#9a9ca3"),
    "white_goods": Look("#e9e5dc", chip=0.45, rust=0.45, streaks=0.5, bare="#8e9096"),
    "wood": Look("#a8763e", chip=0.0, rust=0.0, grime=0.7, rough=0.8, light=0.22, dust="#c9a57a"),
    "wood_dark": Look("#7a5530", chip=0.0, rust=0.0, grime=0.7, rough=0.8),
    "glass": Look("#7fb7c9", chip=0.0, rust=0.0, grime=0.5, rough=0.15, metal=0.1, light=0.35, edge=0.4),
    "concrete": Look("#9a958c", chip=0.0, rust=0.0, grime=0.8, rough=0.9, light=0.12, edge=0.35),
    "dirt": Look("#9a7a54", chip=0.0, rust=0.0, grime=0.5, rough=0.95, light=0.15, edge=0.0),
    "rubber_mat": Look("#3a3836", chip=0.0, rust=0.0, grime=0.4, rough=0.9, light=0.15, edge=0.1),
    "cloth": Look("#b89a6a", chip=0.0, rust=0.0, grime=0.5, rough=0.95, light=0.25, edge=0.0),
    "skin": Look("#f1c7a1", chip=0.0, rust=0.0, grime=0.1, rough=0.7, light=0.25, edge=0.1),
    "fur": Look("#7a5a3a", chip=0.0, rust=0.0, grime=0.3, rough=0.95, light=0.3, edge=0.15),
    "fur_light": Look("#c8a27a", chip=0.0, rust=0.0, grime=0.2, rough=0.95, light=0.3, edge=0.1),
    "denim": Look("#3f6fb0", chip=0.0, rust=0.0, grime=0.5, rough=0.9, light=0.25, edge=0.15),
    "hazard_y": Look("#f2b632", chip=0.55, rust=0.25, grime=0.6),
    "hazard_k": Look("#2a2830", chip=0.35, rust=0.1, grime=0.4, bare="#8a8c90"),
    "hazard": Look("#f2b632", chip=0.55, rust=0.25, grime=0.6, stripes=0.36),
    "bright": Look("#fff3dc", chip=0.0, rust=0.0, grime=0.2, rough=0.5, light=0.1, edge=0.0),
    # tintable
    "paint": Look(NEUTRAL, chip=0.55, rust=0.15, streaks=0.25, tint=0, chip_col="#5a5652", bare="#5a5652"),
    "paint_dark": Look("#9c9a96", chip=0.45, rust=0.25, tint=0, chip_col="#3e3c3a", bare="#3e3c3a"),
    "paint_clean": Look(NEUTRAL, chip=0.3, rust=0.0, grime=0.35, tint=0, chip_col="#7a7672", bare="#7a7672"),
    "paint_rusty": Look(NEUTRAL, chip=0.7, rust=0.55, streaks=0.5, tint=0, chip_col="#5a5652", bare="#5a5652"),
    "tint1": Look(NEUTRAL, chip=0.0, rust=0.0, grime=0.2, rough=0.8, light=0.25, edge=0.12, tint=1),
    "tint1_rusty": Look(NEUTRAL, chip=0.6, rust=0.5, streaks=0.4, tint=1, chip_col="#5a5652", bare="#5a5652"),
    "tint2": Look(NEUTRAL, chip=0.0, rust=0.0, grime=0.2, rough=0.8, light=0.25, edge=0.12, tint=2),
}


def build_material(name, look):
    """A Cycles material whose Emission is the final stylised colour; a second
    emission node (MR_EMIT) holds the roughness/metal graph for the second bake."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    N.clear()
    seed = sum(ord(c) * (i + 1) for i, c in enumerate(name))

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
            if isinstance(v, (tuple, int, float)):
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
    big = ramp(noise(2.2, 3.0, 0.55, coords=obj, w=seed % 97), [(0.4, 0.0), (0.6, 1.0)])
    mid = ramp(noise(look.chip_scale, 8.0, 0.75, distortion=0.1, coords=obj, w=seed % 31), [(0.38, 0.0), (0.62, 1.0)])

    # value variation in the paint: broad, soft, like hand-painted blotches
    base = srgb(look.base)
    lighter = tuple(min(1.0, c * 1.3) for c in base[:3]) + (1.0,)
    darker = tuple(c * 0.72 for c in base[:3]) + (1.0,)
    if look.tint is not None:
        # neutral paint: keep the variation subtle so the runtime colour reads true
        lighter = tuple(min(1.0, c * 1.08) for c in base[:3]) + (1.0,)
        darker = tuple(c * 0.82 for c in base[:3]) + (1.0,)
    col = mix(big, darker, lighter)
    col = mix(0.3, col, base)

    if look.stripes > 0:
        xyz = node("ShaderNodeSeparateXYZ")
        L.new(obj, xyz.inputs[0])
        diag = math_op("ADD", math_op("ADD", xyz.outputs["X"], xyz.outputs["Y"]), xyz.outputs["Z"])
        band = math_op("FRACT", math_op("DIVIDE", diag, look.stripes))
        sm = ramp(band, [(0.0, 0.0), (0.5, 1.0)], constant=True)
        sc_ = srgb(look.stripe_col)
        col = mix(sm, col, mix(big, tuple(c * 0.85 for c in sc_[:3]) + (1.0,), sc_))

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
    if look.edge > 0:
        col = mix(math_op("MULTIPLY", ramp(edge, [(0.1, 0.0), (0.5, 1.0)]), look.edge), col, lighter)

    rough_sock = None
    metal_sock = None
    chipmask = None
    if look.chip > 0:
        wear = math_op("ADD", math_op("MULTIPLY", edge, 0.6), math_op("MULTIPLY", mid, 0.7))
        thr = 1.15 - look.chip * 0.45
        outline = ramp(wear, [(0.0, 0.0), (thr - 0.05, 1.0)], constant=True)
        chipmask = ramp(wear, [(0.0, 0.0), (thr, 1.0)], constant=True)
        col = mix(outline, col, tuple(c * 0.5 for c in base[:3]) + (1.0,))
        bare = srgb(look.chip_col or look.bare)
        bare_hi = tuple(min(1.0, c * 1.2) for c in bare[:3]) + (1.0,)
        col = mix(chipmask, col, mix(ramp(mid, [(0.4, 0.0), (0.7, 1.0)]), bare, bare_hi))

    rustmask = None
    if look.rust > 0:
        rn = ramp(noise(5.0, 5.0, 0.65, distortion=0.4, coords=obj, w=7.0 + seed % 13), [(0.4, 0.0), (0.6, 1.0)])
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
        col = mix(dust, col, srgb(look.dust or "#b89a72"))
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
    rgh = node("ShaderNodeValue")
    rgh.outputs[0].default_value = look.rough
    rough_sock = rgh.outputs[0]
    met = node("ShaderNodeValue")
    met.outputs[0].default_value = look.metal
    metal_sock = met.outputs[0]
    for mask, rv, mv in ((chipmask, 0.32, 0.3), (rustmask, 0.95, 0.0)):
        if mask is None:
            continue
        n = node("ShaderNodeMix", data_type="FLOAT")
        n.inputs["B"].default_value = rv
        L.new(mask, n.inputs[0])
        L.new(rough_sock, n.inputs["A"])
        rough_sock = n.outputs[0]
        n2 = node("ShaderNodeMix", data_type="FLOAT")
        n2.inputs["B"].default_value = mv
        L.new(mask, n2.inputs[0])
        L.new(metal_sock, n2.inputs["A"])
        metal_sock = n2.outputs[0]
    comb = node("ShaderNodeCombineColor")
    L.new(rough_sock, comb.inputs["Green"])
    L.new(metal_sock, comb.inputs["Blue"])
    em2 = node("ShaderNodeEmission")
    em2.name = "MR_EMIT"
    L.new(comb.outputs[0], em2.inputs["Color"])
    return m


MATS = {}


def mat(key):
    if key not in MATS:
        MATS[key] = build_material(key, LOOKS[key])
    return MATS[key]


def glow_material():
    m = bpy.data.materials.get("glow")
    if m:
        return m
    m = bpy.data.materials.new("glow")
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = srgb("#ffd25a")
    b.inputs["Emission Color"].default_value = srgb("#ffd25a")
    b.inputs["Emission Strength"].default_value = 1.0
    return m


# ------------------------------------------------------------------ geometry
def _mark(obj, look_key):
    """Vertex-colour marker: white, or the tint slot's marker colour."""
    me = obj.data
    if "Col" not in me.color_attributes:
        me.color_attributes.new("Col", "BYTE_COLOR", "CORNER")
    t = LOOKS[look_key].tint
    c = TINT_MARKERS[t] if t is not None else (1.0, 1.0, 1.0, 1.0)
    for d in me.color_attributes["Col"].data:
        d.color = c
    me.color_attributes.active_color = me.color_attributes["Col"]


def _finish(obj, look_key, xf, bevel, segs=3, smooth=True):
    obj.data.materials.append(mat(look_key))
    _mark(obj, look_key)
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


def box(w, h, d, look, xf, bevel=0.02, name="box", segs=3):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=(w, h, d), verts=bm.verts)
    return _finish(_new(name, bm), look, xf, min(bevel, w / 2.2, h / 2.2, d / 2.2), segs)


def cyl(rt, rb, h, seg, look, xf, bevel=0.015, name="cyl", smooth_sides=True):
    """Cylinder/frustum along +Y, centred."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=rb, radius2=rt, depth=h)
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=rx(-HALF_PI))
    o = _new(name, bm)
    return _finish(o, look, xf, min(bevel, h / 2.2), smooth=smooth_sides)


def torus(major, minor, look, xf, seg=24, ring=10, name="torus"):
    """Torus lying in the XZ plane (hole along +Y)."""
    bm = bmesh.new()
    verts = []
    for i in range(seg):
        a = i / seg * TAU
        row = []
        for j in range(ring):
            b = j / ring * TAU
            r = major + minor * math.cos(b)
            row.append(bm.verts.new((math.cos(a) * r, minor * math.sin(b), math.sin(a) * r)))
        verts.append(row)
    for i in range(seg):
        for j in range(ring):
            a, b = verts[i][j], verts[(i + 1) % seg][j]
            c, d = verts[(i + 1) % seg][(j + 1) % ring], verts[i][(j + 1) % ring]
            bm.faces.new((a, d, c, b))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _finish(_new(name, bm), look, xf, 0.0)


def sphere(r, look, xf, seg=16, rings=8, name="sphere", subdiv=None):
    bm = bmesh.new()
    if subdiv is not None:
        bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=r)
    else:
        bmesh.ops.create_uvsphere(bm, u_segments=seg, v_segments=rings, radius=r)
    return _finish(_new(name, bm), look, xf, 0.0)


def lump(r, look, xf, seed=0, rough=0.25, subdiv=3, name="lump"):
    """A noisy rock / dirt mound / cloth lump: displaced icosphere."""
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=r)
    o = _new(name, bm)
    tex = bpy.data.textures.new(f"lump{seed}", "CLOUDS")
    tex.noise_scale = r * 0.6
    tex.noise_depth = 2
    d = o.modifiers.new("disp", "DISPLACE")
    d.texture = tex
    d.strength = r * rough
    d.mid_level = 0.5
    d.texture_coords = "GLOBAL"
    o.location = (seed * 13.7, seed * 7.1, 0)  # vary the noise per seed
    bpy.context.view_layer.objects.active = o
    o.select_set(True)
    bpy.ops.object.modifier_apply(modifier="disp")
    o.select_set(False)
    o.location = (0, 0, 0)
    return _finish(o, look, xf, 0.0)


def prism(profile, w, look, xf, bevel=0.01, name="prism"):
    """Extrude a convex (z, y) profile along X from -w/2..w/2."""
    bm = bmesh.new()
    left = [bm.verts.new((-w / 2, y, z)) for z, y in profile]
    right = [bm.verts.new((w / 2, y, z)) for z, y in profile]
    bm.faces.new(left)
    bm.faces.new(list(reversed(right)))
    n = len(profile)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((left[i], left[j], right[j], right[i]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _finish(_new(name, bm), look, xf, bevel)


def plane(w, d, look, xf, subdiv=0, name="plane"):
    """Flat quad in XZ facing +Y."""
    bm = bmesh.new()
    bmesh.ops.create_grid(bm, x_segments=subdiv + 1, y_segments=subdiv + 1, size=0.5)
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=rx(-HALF_PI))
    bmesh.ops.scale(bm, vec=(w, 1, d), verts=bm.verts)
    return _finish(_new(name, bm), look, xf, 0.0, smooth=False)


def bolt(r, look, xf):
    return cyl(r, r, r * 0.8, 6, look, xf, bevel=r * 0.3, name="bolt")


def rod(a, b, r, look, seg=8, name="rod", bevel=None):
    a, b = Vector(a), Vector(b)
    d = b - a
    q = Vector((0, 1, 0)).rotation_difference(d.normalized())
    return cyl(r, r, d.length, seg, look, Matrix.Translation((a + b) / 2) @ q.to_matrix().to_4x4(), bevel=r * 0.3 if bevel is None else bevel, name=name)


def cable(pts, r, look, res=8):
    curve = bpy.data.curves.new("cable", "CURVE")
    curve.dimensions = "3D"
    sp = curve.splines.new("BEZIER")
    sp.bezier_points.add(len(pts) - 1)
    for bp, p in zip(sp.bezier_points, pts):
        bp.co = p
        bp.handle_left_type = bp.handle_right_type = "AUTO"
    curve.bevel_depth = r
    curve.bevel_resolution = 3
    curve.resolution_u = res
    curve.use_fill_caps = True
    o = bpy.data.objects.new("cable", curve)
    bpy.context.collection.objects.link(o)
    bpy.context.view_layer.objects.active = o
    o.select_set(True)
    bpy.ops.object.convert(target="MESH")
    o = bpy.context.view_layer.objects.active
    o.select_set(False)
    return _finish(o, look, Matrix.Identity(4), 0.0)


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


def bolt_row(a, b, n, r, look="steel_dark", up=(0, 1, 0)):
    a, b = Vector(a), Vector(b)
    q = Vector((0, 1, 0)).rotation_difference(Vector(up).normalized()).to_matrix().to_4x4()
    out = []
    for i in range(n):
        t = 0.5 if n == 1 else i / (n - 1)
        p = a.lerp(b, t)
        out.append(bolt(r, look, Matrix.Translation(p) @ q))
    return out


def hazard_box(w, h, d, xf, bevel=0.02):
    """Box painted with diagonal yellow/black hazard stripes (baked)."""
    return box(w, h, d, "hazard", xf, bevel=min(bevel, h / 4))


def glow(obj_or_bm_fn, name):
    """Mark an object as a glow node (exported untextured; the game swaps in a
    blooming material)."""
    o = obj_or_bm_fn
    o.name = name
    o.data.materials.clear()
    o.data.materials.append(glow_material())
    for p in o.data.polygons:
        p.use_smooth = True
    return o


def glow_sphere(r, name, centre=(0, 0, 0), squash=(1, 1, 1), subdiv=2):
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=r)
    bmesh.ops.scale(bm, vec=squash, verts=bm.verts)
    o = _new(name, bm)
    o.data.transform(Matrix.Translation(centre))
    return glow(o, name)


def glow_box(w, h, d, name, xf):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=(w, h, d), verts=bm.verts)
    o = _new(name, bm)
    o.data.transform(xf)
    return glow(o, name)


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
    if len(objs) > 1:
        bpy.ops.object.join()
    o = bpy.context.view_layer.objects.active
    o.name = name
    o.data.name = name
    o.select_set(False)
    return o


def unwrap(objs, margin=0.006):
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    for o in objs:
        o.select_set(True)
        if not o.data.uv_layers:
            o.data.uv_layers.new(name="UVMap")
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(50), island_margin=margin, scale_to_bounds=False)
    bpy.ops.uv.pack_islands(margin=margin, rotate=True)
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


def export(name, groups, glows=(), origins=None, tex=None):
    """Bake and write assets/models/<name>.glb.

    groups:  {group name: [objects]} -> one node + mesh per group
    glows:   glow objects (already named glow_*)
    origins: {group or glow name: (x, y, z)} pivot in Bevy space; the node sits
             there and its vertices are stored relative to it
    """
    origins = origins or {}
    tex = tex or TEX
    joined = [join(objs, gname) for gname, objs in groups.items() if objs]
    glows = list(glows)
    # Bevy space (Y up) -> Blender (Z up): the glTF exporter undoes this
    to_bl = rx(HALF_PI)
    for o in joined + glows:
        o.data.transform(to_bl)
        if o.name in origins:
            p = to_bl @ Vector(origins[o.name])
            o.data.transform(Matrix.Translation(-p))
            o.location = p
    tinted = any(LOOKS[k].tint is not None for k in MATS)
    unwrap(joined)
    col = bpy.data.images.new(f"{name}_col", tex, tex)
    mr = bpy.data.images.new(f"{name}_mr", tex, tex)
    mr.colorspace_settings.name = "Non-Color"
    bake(joined, col)
    bake(joined, mr, "MR_EMIT")
    for img in (col, mr):
        img.file_format = "PNG"
        img.pack()
    # one export material for the whole asset
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
        if not tinted and "Col" in o.data.color_attributes:
            o.data.color_attributes.remove(o.data.color_attributes["Col"])
    for o in bpy.context.view_layer.objects:
        o.select_set(o in joined or o in glows)
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, f"{name}.glb")
    bpy.ops.export_scene.gltf(
        filepath=path, export_format="GLB", use_selection=True, export_apply=True, export_yup=True,
        export_image_format="JPEG", export_jpeg_quality=int(os.environ.get("ART_JPEG", "88")),
        export_vertex_color="ACTIVE" if tinted else "NONE", export_all_vertex_colors=False,
        export_tangents=False, export_materials="EXPORT",
    )
    if os.environ.get("ART_DEBUG"):
        col.save_render(os.path.join("/tmp", f"{name}_col.png"))
    if os.environ.get("ART_PREVIEW"):
        preview(name, joined + glows, em, tinted)
    print(f"wrote {path} ({os.path.getsize(path) // 1024} KB)")


def preview(name, objs, em, tinted):
    """Quick Eevee render of the baked asset for eyeballing, to
    $ART_PREVIEW/<name>.png (three-quarter view, framed to fit)."""
    s = bpy.context.scene
    s.render.engine = "BLENDER_EEVEE"
    s.render.resolution_x = s.render.resolution_y = 768
    s.view_settings.view_transform = "Standard"
    # textured but unlit-ish: emission from the colour texture, plus a sun
    for o in objs:
        o.hide_render = False
    lo = Vector((1e9,) * 3)
    hi = Vector((-1e9,) * 3)
    for o in objs:
        for v in o.data.vertices:
            p = o.matrix_world @ v.co
            lo = Vector(map(min, lo, p))
            hi = Vector(map(max, hi, p))
    c = (lo + hi) / 2
    r = max((hi - lo).length / 2, 0.1)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    s.collection.objects.link(cam)
    d = Vector((1.0, -1.25, 0.8)).normalized()
    cam.location = c + d * r * 2.8
    cam.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    cam.data.lens = 50
    s.camera = cam
    if tinted:
        demo_tints(em)
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.0
    sun.rotation_euler = (math.radians(50), math.radians(10), math.radians(-30))
    s.collection.objects.link(sun)
    w = bpy.data.worlds.new("w")
    w.use_nodes = True
    w.node_tree.nodes["Background"].inputs[0].default_value = (0.55, 0.7, 0.9, 1)
    w.node_tree.nodes["Background"].inputs[1].default_value = 0.8
    s.world = w
    out = os.environ["ART_PREVIEW"]
    os.makedirs(out, exist_ok=True)
    s.render.filepath = os.path.join(out, f"{name}.png")
    bpy.ops.render.render(write_still=True)


def demo_tints(em):
    """Preview only: show tint slots in demo colours (red, blue, green) the
    way the game will multiply them in."""
    nt = em.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    tex_link = bsdf.inputs["Base Color"].links[0].from_socket
    ca = nt.nodes.new("ShaderNodeVertexColor")
    ca.layer_name = "Col"
    sp = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(ca.outputs["Color"], sp.inputs[0])
    acc = None
    for ch, demo in (("Red", "#d8453a"), ("Green", "#3f7fd8"), ("Blue", "#5a8a3a")):
        mixn = nt.nodes.new("ShaderNodeMix")
        mixn.data_type = "RGBA"
        nt.links.new(sp.outputs[ch], mixn.inputs[0])
        mixn.inputs[6].default_value = (0, 0, 0, 1)
        mixn.inputs[7].default_value = srgb(demo)
        if acc is None:
            acc = mixn.outputs[2]
        else:
            add = nt.nodes.new("ShaderNodeMix")
            add.data_type = "RGBA"
            add.blend_type = "ADD"
            add.inputs[0].default_value = 1.0
            nt.links.new(acc, add.inputs[6])
            nt.links.new(mixn.outputs[2], add.inputs[7])
            acc = add.outputs[2]
    # white (untinted) -> 1, marker -> demo colour
    wmul = nt.nodes.new("ShaderNodeMath")
    wmul.operation = "MULTIPLY"
    nt.links.new(sp.outputs["Red"], wmul.inputs[0])
    nt.links.new(sp.outputs["Green"], wmul.inputs[1])
    w2 = nt.nodes.new("ShaderNodeMath")
    w2.operation = "MULTIPLY"
    nt.links.new(wmul.outputs[0], w2.inputs[0])
    nt.links.new(sp.outputs["Blue"], w2.inputs[1])
    tint = nt.nodes.new("ShaderNodeMix")
    tint.data_type = "RGBA"
    nt.links.new(w2.outputs[0], tint.inputs[0])
    nt.links.new(acc, tint.inputs[6])
    tint.inputs[7].default_value = (1, 1, 1, 1)
    mul = nt.nodes.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs[0].default_value = 1.0
    nt.links.new(tex_link, mul.inputs[6])
    nt.links.new(tint.outputs[2], mul.inputs[7])
    nt.links.new(mul.outputs[2], bsdf.inputs["Base Color"])
