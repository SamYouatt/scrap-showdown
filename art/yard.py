"""Scrapyard scenery (src/screens/scrap.rs). Bevy space, +Y up; each asset is
modelled in the local frame of the code-built Geo it replaces, so the game's
existing transforms carry over (see each builder's docstring for its frame).

Multi-variant assets (`fence` a/b) and rigs (crane, scavenger, dog) are split
into groups whose node translation is the group's pivot in the model frame.
"""
import math
import random

import bmesh
import bpy
import lib
from lib import (HALF_PI, TAU, Look, LOOKS, at, bolt, cable, glow_box, hazard_box, prism, rod, rx, ry, rz, srgb, torus)
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from props import (I, blob, bx, car_shell_geo, cy, flat_lump, sheet, tyre, wheel)

YARD_R = 27.0
FENCE_R = YARD_R + 1.2
FENCE_SEG = 2 * FENCE_R * math.sin(math.pi / 72)  # chord of one of 72 segments
LAMP_SPAN = 2 * 5.8 * math.sin(math.pi / 5)  # chord between neighbouring lamps
BOOM_PIVOT = (0.0, 2.4, 0.6)
BOOM_TILT = 0.45
BOOM_LEN = 10.0
BOOM_TIP = (0.0, 2.4 + math.sin(BOOM_TILT) * BOOM_LEN, 0.6 + math.cos(BOOM_TILT) * BOOM_LEN)
MAGNET_REST = (0.0, 6.5, 10.0)

LOOKS["fence_grey"] = Look("#8a8f93", chip=0.2, rust=0.55, rough=0.5, metal=0.45, streaks=0.6, bare="#b4b8bf")
LOOKS["fence_brown"] = Look("#7b6e60", chip=0.3, rust=0.65, rough=0.6, metal=0.3, streaks=0.5, bare="#9a9ca3")
LOOKS["dirt_tint"] = Look(lib.NEUTRAL, chip=0.0, rust=0.0, grime=0.55, rough=0.95, light=0.22, edge=0.0, tint=0, dust="#e0d8c8")
LOOKS["junk_dirt"] = Look("#8a6a52", chip=0.0, rust=0.0, grime=0.6, rough=0.95, light=0.2, edge=0.0)
LOOKS["stone"] = Look("#8e887e", chip=0.0, rust=0.0, grime=0.5, rough=0.9, light=0.2, edge=0.25)
LOOKS["van_cream"] = Look("#e8e0d0", chip=0.5, rust=0.35, streaks=0.4, bare="#9a9ca3")
LOOKS["cabin_yellow"] = Look("#d9b35a", chip=0.55, rust=0.45, streaks=0.5, bare="#8e9096")
LOOKS["door_brown"] = Look("#7a4a2e", chip=0.5, rust=0.3, bare="#4a3a2e")
LOOKS["roof_red"] = Look("#a84a4a", chip=0.5, rust=0.35, streaks=0.3)
LOOKS["board"] = Look(lib.NEUTRAL, chip=0.5, rust=0.3, streaks=0.4, tint=0, chip_col="#5a5652", bare="#5a5652")
LOOKS["tarp"] = Look(lib.NEUTRAL, chip=0.0, rust=0.0, grime=0.6, rough=0.7, light=0.3, edge=0.15, tint=0, dust="#d8ccb4")
LOOKS["hivis"] = Look("#ee7a32", chip=0.0, rust=0.0, grime=0.35, rough=0.8, light=0.25, edge=0.1)
LOOKS["glove"] = Look("#e8a63a", chip=0.0, rust=0.0, grime=0.5, rough=0.9, light=0.25, edge=0.1)
LOOKS["boot"] = Look("#4a3a2e", chip=0.0, rust=0.0, grime=0.4, rough=0.8, light=0.2, edge=0.25, bare="#7a6a5e")
LOOKS["mask"] = Look("#2e2d33", chip=0.35, rust=0.1, grime=0.3, rough=0.5, bare="#6b6e76")
LOOKS["visor"] = Look("#2f5a3a", chip=0.0, rust=0.0, grime=0.3, rough=0.15, metal=0.2, light=0.3, edge=0.4)
LOOKS["fur_dark"] = Look("#5a3a22", chip=0.0, rust=0.0, grime=0.3, rough=0.95, light=0.3, edge=0.15)
LOOKS["collar"] = Look("#d8453a", chip=0.3, rust=0.0, grime=0.3, rough=0.6, bare="#8a2a22")
for k, h in (("flag_red", "#d8453a"), ("flag_yellow", "#f2b632"), ("flag_blue", "#3f7fd8"), ("flag_teal", "#3fa7a3"), ("flag_cream", "#e8e0d0")):
    LOOKS[k] = Look(h, chip=0.0, rust=0.0, grime=0.25, rough=0.85, light=0.3, edge=0.1)
FLAGS = ["flag_red", "flag_yellow", "flag_blue", "flag_teal", "flag_cream"]


# ------------------------------------------------------------------ ground shader
class Nodes:
    """Tiny node-graph builder for the hand-written ground materials."""

    def __init__(self, m):
        self.nt = m.node_tree
        self.N, self.L = self.nt.nodes, self.nt.links

    def node(self, kind, **kw):
        n = self.N.new(kind)
        for k, v in kw.items():
            setattr(n, k, v)
        return n

    def _in(self, sock, v):
        if isinstance(v, (int, float, tuple)):
            sock.default_value = v
        else:
            self.L.new(v, sock)

    def m(self, op, a, b=None, clamp=False):
        n = self.node("ShaderNodeMath", operation=op, use_clamp=clamp)
        self._in(n.inputs[0], a)
        if b is not None:
            self._in(n.inputs[1], b)
        return n.outputs[0]

    def step(self, x, e0, e1):
        """Smoothstep from e0 to e1 (e0 > e1 gives a falling edge)."""
        lo, hi, a, b = (e0, e1, 0.0, 1.0) if e0 < e1 else (e1, e0, 1.0, 0.0)
        n = self.node("ShaderNodeMapRange", interpolation_type="SMOOTHSTEP", clamp=True)
        self._in(n.inputs["Value"], x)
        n.inputs["From Min"].default_value, n.inputs["From Max"].default_value = lo, hi
        n.inputs["To Min"].default_value, n.inputs["To Max"].default_value = a, b
        return n.outputs["Result"]

    def mix(self, fac, a, b, blend="MIX"):
        n = self.node("ShaderNodeMix", data_type="RGBA", blend_type=blend, clamp_result=True)
        self._in(n.inputs[0], fac)
        self._in(n.inputs[6], a)
        self._in(n.inputs[7], b)
        return n.outputs[2]

    def mixf(self, fac, a, b):
        n = self.node("ShaderNodeMix", data_type="FLOAT")
        self._in(n.inputs[0], fac)
        self._in(n.inputs[2], a)
        self._in(n.inputs[3], b)
        return n.outputs[0]

    def shade(self, fac, col, k):
        """col * lerp(1, k, fac)."""
        return self.mix(fac, col, (k, k, k, 1.0), "MULTIPLY")

    def noise(self, co, scale, detail=2.0, rough=0.5, w=0.0):
        n = self.node("ShaderNodeTexNoise", noise_dimensions="4D")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = rough
        n.inputs["W"].default_value = w
        self.L.new(co, n.inputs["Vector"])
        return n.outputs["Fac"]

    def voronoi(self, co, scale):
        n = self.node("ShaderNodeTexVoronoi", feature="F1")
        n.inputs["Scale"].default_value = scale
        self.L.new(co, n.inputs["Vector"])
        return n

    def ramp(self, fac, stops):
        n = self.node("ShaderNodeValToRGB")
        els = n.color_ramp.elements
        els[0].position, els[0].color = stops[0][0], srgb(stops[0][1])
        els[1].position, els[1].color = stops[1][0], srgb(stops[1][1])
        for p, h in stops[2:]:
            e = els.new(p)
            e.color = srgb(h)
        self._in(n.inputs[0], fac)
        return n.outputs[0]

    def finish(self, col, rough):
        out = self.node("ShaderNodeOutputMaterial")
        em = self.node("ShaderNodeEmission")
        self.L.new(col, em.inputs["Color"])
        self.L.new(em.outputs[0], out.inputs["Surface"])
        comb = self.node("ShaderNodeCombineColor")
        self._in(comb.inputs["Green"], rough)
        comb.inputs["Blue"].default_value = 0.0
        em2 = self.node("ShaderNodeEmission")
        em2.name = "MR_EMIT"
        self.L.new(comb.outputs[0], em2.inputs["Color"])


OUTSIDE = "#b48a5a"  # ground beyond the fence (matches the far ground)


def ground_material(name, outer=False):
    """The yard floor, all in one shader so it bakes into one texture.
    Blender object space at bake time: X = Bevy x, Y = -Bevy z, Z = up."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    G = Nodes(m)
    G.N.clear()
    obj = G.node("ShaderNodeTexCoord").outputs["Object"]
    sep = G.node("ShaderNodeSeparateXYZ")
    G.L.new(obj, sep.inputs[0])
    X, Y = sep.outputs["X"], sep.outputs["Y"]
    r = G.m("SQRT", G.m("ADD", G.m("MULTIPLY", X, X), G.m("MULTIPLY", Y, Y)))
    big = G.noise(obj, 0.12, 4.0, 0.55)
    mot = G.noise(obj, 0.45, 3.0, 0.6, w=4.0)
    fine = G.noise(obj, 3.0, 2.0, 0.5, w=9.0)
    if outer:
        col = G.ramp(r, [(0.0, OUTSIDE), (1.0, OUTSIDE)])
    else:
        rw = G.m("ADD", r, G.m("MULTIPLY", G.m("SUBTRACT", big, 0.5), 3.0))
        z = [(0.0, "#c49866"), (10.85, "#c49866"), (11.15, "#a97c4c"), (18.85, "#a97c4c"), (19.15, "#94683f"),
             (27.4, "#94683f"), (28.6, OUTSIDE), (40.0, OUTSIDE)]
        col = G.ramp(G.m("DIVIDE", rw, 40.0), [(p / 40.0, h) for p, h in z])
    # broad mottling + a fine grain (both ground and far ground share it)
    col = G.shade(G.m("MULTIPLY", G.step(mot, 0.5, 0.7), 0.35), col, 0.72)
    col = G.mix(G.m("MULTIPLY", G.step(mot, 0.45, 0.28), 0.4), col, srgb("#d4ae80"))
    col = G.shade(G.m("MULTIPLY", G.step(big, 0.45, 0.65), 0.25), col, 0.85)
    rough = 0.94
    if outer:
        col = G.shade(G.m("MULTIPLY", G.step(fine, 0.5, 0.7), 0.25), col, 0.8)
        G.finish(col, rough)
        return m
    col = G.shade(G.m("MULTIPLY", G.step(fine, 0.5, 0.68), 0.3), col, 0.8)
    # scattered pebbles
    pv = G.voronoi(obj, 2.6)
    pr = G.node("ShaderNodeSeparateColor")
    G.L.new(pv.outputs["Color"], pr.inputs[0])
    peb = G.m("MULTIPLY", G.step(pv.outputs["Distance"], 0.22, 0.16), G.step(pr.outputs["Red"], 0.4, 0.36))
    col = G.mix(G.m("MULTIPLY", peb, 0.75), col, G.mix(pr.outputs["Green"], srgb("#6e665c"), srgb("#cfc2aa")))

    # gravel tracks radiating from the van (scrap.rs: 6 x 1.6 m wide, r 5..27)
    edge = G.m("MULTIPLY", G.m("SUBTRACT", G.noise(obj, 1.4, 2.0, 0.5, w=2.0), 0.5), 0.5)
    track = rut = None
    for i in range(6):
        a = i / 6 * TAU + 0.3
        ux, uy = math.sin(a), -math.cos(a)
        along = G.m("ADD", G.m("MULTIPLY", X, ux), G.m("MULTIPLY", Y, uy))
        perp = G.m("ABSOLUTE", G.m("SUBTRACT", G.m("MULTIPLY", X, uy), G.m("MULTIPLY", Y, ux)))
        ends = G.m("MULTIPLY", G.step(along, 4.6, 5.6), G.step(along, 27.6, 26.6))
        t = G.m("MULTIPLY", G.step(G.m("ADD", perp, edge), 0.85, 0.7), ends)
        rr = G.m("MULTIPLY", G.step(G.m("ABSOLUTE", G.m("SUBTRACT", perp, 0.45)), 0.16, 0.07), ends)
        track = t if track is None else G.m("MAXIMUM", track, t)
        rut = rr if rut is None else G.m("MAXIMUM", rut, rr)
    gv = G.voronoi(obj, 5.0)
    gc = G.node("ShaderNodeSeparateColor")
    G.L.new(gv.outputs["Color"], gc.inputs[0])
    gravel = G.mix(gc.outputs["Red"], srgb("#857560"), srgb("#b2a48c"))
    gravel = G.shade(G.step(gv.outputs["Distance"], 0.25, 0.45), gravel, 0.75)
    col = G.mix(track, col, gravel)
    col = G.shade(G.m("MULTIPLY", rut, 0.35), col, 0.72)

    # oil stains, puddles and dirt patches (r 2.7..27.5)
    sv = G.voronoi(obj, 0.22)
    sc = G.node("ShaderNodeSeparateColor")
    G.L.new(sv.outputs["Color"], sc.inputs[0])
    rad = G.m("ADD", G.m("MULTIPLY", sc.outputs["Green"], 1.8), 0.4)
    dm = G.m("ADD", G.m("DIVIDE", sv.outputs["Distance"], 0.22), G.m("MULTIPLY", G.m("SUBTRACT", G.noise(obj, 0.9, 2.0, 0.5, w=6.0), 0.5), 1.4))
    inside = G.m("SUBTRACT", rad, dm)  # > 0 inside the stain
    stain = G.m("MULTIPLY", G.step(inside, 0.0, 0.18), G.step(sc.outputs["Blue"], 0.5, 0.46))
    stain = G.m("MULTIPLY", stain, G.m("MULTIPLY", G.step(r, 2.7, 3.1), G.step(r, 27.5, 27.0)))
    kind = sc.outputs["Red"]
    oil = G.m("MULTIPLY", stain, G.step(kind, 0.21, 0.19))
    pud = G.m("MULTIPLY", stain, G.m("MULTIPLY", G.step(kind, 0.19, 0.21), G.step(kind, 0.31, 0.29)))
    dirt = G.m("MULTIPLY", stain, G.step(kind, 0.29, 0.31))
    col = G.mix(G.m("MULTIPLY", dirt, 0.75), col, srgb("#b08254"))
    col = G.mix(G.m("MULTIPLY", oil, 0.9), col, G.mix(G.step(inside, 0.1, 0.8), srgb("#5a4a3e"), srgb("#3a302a")))
    pcol = G.mix(G.step(inside, 0.05, 0.3), srgb("#4e5e5c"), srgb("#6f8f9a"))
    col = G.mix(pud, col, pcol)
    rough = G.mixf(oil, rough, 0.35)
    rough = G.mixf(pud, rough, 0.08)

    # concrete apron (r 5.5) with slab joints and a kerb
    conc = G.step(r, 5.52, 5.44)
    ccol = G.shade(G.m("MULTIPLY", G.step(mot, 0.45, 0.7), 0.4), (*srgb("#a8a096")[:3], 1.0), 0.8)
    ccol = G.shade(G.m("MULTIPLY", G.step(fine, 0.5, 0.65), 0.3), ccol, 0.85)
    jx = G.m("ABSOLUTE", G.m("SUBTRACT", G.m("FRACT", G.m("DIVIDE", X, 1.8)), 0.5))
    jy = G.m("ABSOLUTE", G.m("SUBTRACT", G.m("FRACT", G.m("DIVIDE", Y, 1.8)), 0.5))
    joint = G.m("MAXIMUM", G.step(jx, 0.475, 0.49), G.step(jy, 0.475, 0.49))
    ccol = G.shade(G.m("MULTIPLY", joint, 0.6), ccol, 0.6)
    col = G.mix(conc, col, ccol)
    kerb = G.m("MULTIPLY", G.step(r, 5.4, 5.48), G.step(r, 5.75, 5.67))
    col = G.mix(kerb, col, G.shade(G.step(fine, 0.4, 0.7), (*srgb("#8f887d")[:3], 1.0), 0.8))
    rough = G.mixf(conc, rough, 0.85)

    # hazard-striped drop mat at the van (r 2.4, 20 teal/cream wedges)
    ang = G.m("ARCTAN2", X, G.m("MULTIPLY", Y, -1.0))
    odd = G.m("GREATER_THAN", G.m("FRACT", G.m("DIVIDE", ang, TAU / 10)), 0.5)
    mcol = G.mix(odd, srgb("#3fa7a3"), srgb("#e8e0d0"))
    mcol = G.shade(G.m("MULTIPLY", G.step(mot, 0.5, 0.75), 0.3), mcol, 0.8)
    worn = G.step(G.noise(obj, 2.5, 4.0, 0.6, w=1.0), 0.6, 0.66)
    mat = G.m("MULTIPLY", G.step(r, 2.42, 2.36), G.m("SUBTRACT", 1.0, G.m("MULTIPLY", worn, 0.85)))
    col = G.mix(mat, col, mcol)
    rim = G.m("MULTIPLY", G.step(r, 2.38, 2.43), G.step(r, 2.6, 2.55))
    col = G.mix(rim, col, srgb("#2a2830"))
    col = G.mix(G.step(r, 0.18, 0.14), col, srgb("#5e626c"))
    rough = G.mixf(mat, rough, 0.6)

    # grime along the foot of the fence
    col = G.shade(G.m("MULTIPLY", G.step(G.m("ABSOLUTE", G.m("SUBTRACT", r, FENCE_R)), 0.9, 0.15), 0.4), col, 0.6)
    G.finish(col, rough)
    return m


def disc(radii, seg, name):
    """Flat disc (or annulus if radii[0] > 0) in XZ facing +Y, as rings of quads."""
    bm = bmesh.new()
    rings = []
    for r in radii:
        if r == 0:
            rings.append(None)
            c = bm.verts.new((0, 0, 0))
            continue
        rings.append([bm.verts.new((math.cos(i / seg * TAU) * r, 0, math.sin(i / seg * TAU) * r)) for i in range(seg)])
    for k in range(1, len(rings)):
        A, B = rings[k - 1], rings[k]
        for i in range(seg):
            j = (i + 1) % seg
            if A is None:
                f = bm.faces.new((c, B[j], B[i]))
            else:
                f = bm.faces.new((A[i], A[j], B[j], B[i]))
            f.normal_update()
            if f.normal.y < 0:
                f.normal_flip()
    return lib._new(name, bm)


def ground_obj(radii, seg, key, outer):
    LOOKS[key] = Look("#b48a5a")  # untinted placeholder (the real material is hand-built)
    lib.MATS[key] = ground_material(key, outer)
    o = disc(radii, seg, key)
    return lib._finish(o, key, I, 0.0, smooth=False)


def yard_ground():
    """The yard floor: flat disc r 30 at y = 0, origin at the yard centre (VAN).
    Concrete apron r 5.5, dirt bands at 11 / 19, darker deep-pile dirt to the
    fence (28.2), 6 gravel tracks, stains and the striped drop mat (r 2.4)."""
    return dict(groups={"ground": [ground_obj([0, 3, 6, 10, 14, 18, 22, 26, 30], 96, "ground", False)]}, tex=2048)


def yard_outer():
    """Far ground: annulus r 29.6..80 at y = -0.02 (tucks under yard_ground)."""
    o = ground_obj([29.6, 36, 48, 62, 80], 64, "outer", True)
    o.data.transform(Matrix.Translation((0, -0.02, 0)))
    return dict(groups={"outer": [o]}, tex=512)


# ------------------------------------------------------------------ fence, mounds
def fence_geo(look, xf, dent=0.0):
    """One segment: post at x = 0, corrugated panel along +X to the next post."""
    L = FENCE_SEG
    B = [bx(0.15, 2.4, 0.15, "steel_dark", xf @ at(0, 1.2, 0), bevel=0.02, name="post")]
    B.append(bx(0.2, 0.06, 0.2, "steel_dark", xf @ at(0, 2.42, 0), bevel=0.015))
    B.append(bx(0.26, 0.08, 0.26, "concrete", xf @ at(0, 0.03, 0), bevel=0.02))

    def wave(x, z):
        y = 0.022 * math.sin(x / 0.2 * TAU)
        u = (x + L / 2) / L
        y += dent * math.exp(-((u - 0.62) ** 2 + ((z + 0.35) / 2.0) ** 2) * 18)
        return (x, y, z)

    B.append(sheet(L - 0.04, 2.0, 0.02, look, xf @ at(L / 2, 1.1, 0.0) @ rx(-HALF_PI), nx=50, nz=4, warp=wave, name="panel"))
    for y in (0.4, 1.8):
        B.append(bx(L, 0.06, 0.05, "steel_dark", xf @ at(L / 2, y, -0.06), bevel=0.01, name="rail"))
        for x in (0.3, L / 2, L - 0.3):
            B.append(bolt(0.016, "steel", xf @ at(x, y, -0.09, rx(-HALF_PI))))
    sag = [Vector((L * t, 2.34 - 0.05 * math.sin(t * math.pi), 0.0)) for t in (0.0, 0.5, 1.0)]
    B.append(cable([xf @ p for p in sag], 0.012, "ink", res=6))
    for t in (0.2, 0.45, 0.7, 0.9):
        p = xf @ Vector((L * t, 2.34 - 0.05 * math.sin(t * math.pi), 0.0))
        B.append(bx(0.05, 0.05, 0.05, "steel_dark", Matrix.Translation(p) @ rz(0.8) @ rx(0.6), bevel=0.0))
    return B


def fence():
    """One of the 72 perimeter segments at r 28.2 (chord 2.46 m). Origin at the
    foot of its post (= segment start p0); the panel runs along local +X to the
    next post; local -Z faces the yard. Groups a (galvanised) / b (rusty brown)."""
    return dict(groups={"a": fence_geo("fence_grey", I), "b": fence_geo("fence_brown", at(0, 0, 1.4), dent=0.08)},
                origins={"a": (0, 0, 0), "b": (0, 0, 1.4)}, tex=512)


def surface(objs):
    """Raycaster onto the given objects from above: hit(x, z) -> (point, normal)."""
    verts, polys = [], []
    for o in objs:
        off = len(verts)
        verts += [v.co.copy() for v in o.data.vertices]
        polys += [[off + i for i in p.vertices] for p in o.data.polygons]
    tree = BVHTree.FromPolygons(verts, polys)

    def hit(x, z):
        loc, nrm, _, _ = tree.ray_cast(Vector((x, 50.0, z)), Vector((0, -1, 0)))
        return loc, nrm
    return hit


def scatter(hit, rng, n, rmax, kinds, scale=1.0):
    """Half-buried junk bits on a surface: kinds are (weight, maker(xf, rng))."""
    out = []
    tot = sum(w for w, _ in kinds)
    for _ in range(n):
        a, d = rng.random() * TAU, math.sqrt(rng.random()) * rmax
        loc, nrm = hit(math.cos(a) * d, math.sin(a) * d)
        if loc is None:
            continue
        up = Vector((0, 1, 0)).rotation_difference(nrm.lerp(Vector((0, 1, 0)), 0.3).normalized()).to_matrix().to_4x4()
        tilt = rx((rng.random() - 0.5) * 1.2) @ rz((rng.random() - 0.5) * 1.2)
        xf = Matrix.Translation(loc) @ up @ ry(rng.random() * TAU) @ tilt @ lib.scl(scale, scale, scale)
        k = rng.random() * tot
        for w, make in kinds:
            k -= w
            if k <= 0:
                out += make(xf, rng)
                break
    return out


def mound():
    """Dirt mound under a scrap pile: radius 1, ~0.45 tall, origin on the ground
    at its centre. Tint 0 = dirt colour. Scale by the pile radius."""
    D = [flat_lump(1.0, 0.45, "dirt_tint", at(0, -0.04, 0), seed=1, rough=0.2, floor=0.0, name="mound")]
    for k, (a, d, s) in enumerate(((0.5, 0.55, 0.6), (2.6, 0.5, 0.55), (4.4, 0.6, 0.5))):
        D.append(flat_lump(s, s * 0.55, "dirt_tint", at(math.cos(a) * d, -0.04, math.sin(a) * d), seed=5 + k, rough=0.25, subdiv=2, floor=0.0, name="side"))
    for k in range(10):
        a = k * 2.4
        D.append(blob(0.14, "dirt_tint", at(math.cos(a) * 0.98, 0.0, math.sin(a) * 0.98), seed=30 + k, rough=0.2, subdiv=2, squash=0.35, floor=0.0, name="clod"))
    rng = random.Random(7)
    kinds = [
        (3, lambda xf, r: [blob(0.06 + r.random() * 0.05, "stone", xf, seed=r.randint(0, 99), rough=0.35, subdiv=1, squash=0.6)]),
        (2, lambda xf, r: [bx(0.2, 0.025, 0.1, r.choice(["steel_dark", "rusty_steel", "red_dark", "teal"]), xf @ at(0, 0.0, 0), bevel=0.005)]),
        (2, lambda xf, r: [rod(xf @ Vector((-0.15, 0.02, 0)), xf @ Vector((0.15, 0.0, 0)), 0.018, "rusty_steel", 6)]),
        (1, lambda xf, r: [cy(0.04, 0.04, 0.03, 6, "steel", xf, bevel=0.006)]),
    ]
    B = D + scatter(surface(D), rng, 26, 1.05, kinds)
    return dict(groups={"body": B}, tex=512)


def mountain():
    """Distant junk mountain: radius 1, ~0.55 tall, origin on the ground at its
    centre; scale uniformly by 6..12 (bits are baked in, so don't squash Y)."""
    D = [flat_lump(1.0, 0.55, "junk_dirt", I, seed=2, rough=0.22, subdiv=3, floor=-0.02, name="hill")]
    D.append(flat_lump(0.55, 0.42, "junk_dirt", at(0.35, 0, -0.3), seed=9, rough=0.3, subdiv=2, floor=-0.02, name="hump"))
    paints = ["red_dark", "teal", "olive", "cream", "orange", "white_goods", "steel_dark", "girder"]
    kinds = [
        (4, lambda xf, r: [bx(0.3, 0.07, 0.15, r.choice(paints), xf @ at(0, 0.01, 0), bevel=0.012)]),
        (3, lambda xf, r: [bx(0.18, 0.012, 0.14, r.choice(paints + ["steel"]), xf @ rx(0.6), bevel=0.0)]),
        (2, lambda xf, r: [torus(0.05, 0.025, "tyre", xf @ rx(1.1), seg=10, ring=6)]),
        (2, lambda xf, r: [cy(0.035, 0.035, 0.1, 8, r.choice(["red", "teal", "yellow", "steel"]), xf @ rx(1.3), bevel=0.005)]),
        (1, lambda xf, r: [rod(xf @ Vector((-0.2, 0.0, 0)), xf @ Vector((0.2, 0.06, 0)), 0.012, r.choice(["girder", "rusty_steel"]), 5)]),
        (1, lambda xf, r: [bx(0.08, 0.12, 0.08, "white_goods", xf, bevel=0.01)]),
    ]
    B = D + scatter(surface(D), random.Random(3), 75, 1.0, kinds)
    return dict(groups={"body": B}, tex=512)


def car_stack():
    """One squashed car slab, 3.2 (X) x 0.55 x 1.6, centred. Tint 0 = paint."""
    B = [bx(3.2, 0.42, 1.6, "junk_paint", at(0, -0.06, 0), bevel=0.06, name="slab")]
    B.append(prism([(-0.65, 0.14), (0.65, 0.14), (0.45, 0.27), (-0.5, 0.27)], 1.5, "junk_paint", ry(HALF_PI) @ at(0, 0, -0.1), bevel=0.03, name="roof"))
    for sz in (-1, 1):
        B.append(bx(1.1, 0.07, 0.03, "glass_dirty", at(-0.1, 0.2, sz * 0.74), bevel=0.01))
        for x in (-1.0, 1.05):
            B.append(bx(0.55, 0.15, 0.04, "ink", at(x, -0.19, sz * 0.795), bevel=0.0))
        for x in (-0.6, 0.3, 0.9):
            B.append(bx(0.05, 0.36, 0.03, "junk_paint_dark", at(x, -0.06, sz * 0.8, rz(0.25 * sz)), bevel=0.008))
    B.append(cy(0.2, 0.2, 0.12, 10, "tyre", at(1.0, -0.06, 0.8, rx(HALF_PI) @ rz(0.3)), bevel=0.03))
    for x, look in ((1.62, "steel"), (-1.62, "steel_dark")):
        B.append(bx(0.1, 0.12, 1.5, look, at(x, -0.16, 0, ry(0.08) @ rx(0.06)), bevel=0.03, name="bumper"))
    for sz in (-0.55, 0.55):
        B.append(cy(0.08, 0.08, 0.04, 8, "lamp_lens", at(1.6, 0.0, sz, rz(HALF_PI)), bevel=0.008))
    return dict(groups={"body": B}, tex=512)


def zone_board():
    """Zone sign: 3.3 x 0.7 board at y 1.8 on two posts (x +-1.3), origin on the
    ground between the posts; faces -Z (towards the van). Tint 0 = board colour."""
    B = [bx(3.3, 0.7, 0.06, "board", at(0, 1.8, 0), bevel=0.02, name="board")]
    B.append(bx(2.6, 0.12, 0.07, "cream", at(0, 1.8, 0), bevel=0.01, name="stripe"))
    for y in (1.47, 2.13):
        B.append(bx(3.34, 0.05, 0.08, "steel_dark", at(0, y, 0), bevel=0.01, name="trim"))
    for x, h in ((-1.3, 2.1), (1.3, 2.1)):
        B.append(bx(0.12, h, 0.12, "steel_dark", at(x, h / 2, 0.09), bevel=0.02, name="post"))
        B.append(bx(0.3, 0.08, 0.3, "concrete", at(x, 0.03, 0.09), bevel=0.02))
    for x in (-1.55, 1.55):
        for y in (1.55, 2.05):
            B.append(bolt(0.02, "steel", at(x, y, -0.035, rx(-HALF_PI))))
    return dict(groups={"body": B}, tex=512)


# ------------------------------------------------------------------ buildings
def van():
    """The scavenger's van in its own frame (scrap.rs build_world): origin on the
    ground under the body centre, nose to -Z, logo on +X. Place at VAN_BODY with
    ry(PI/2) as today."""
    B = []
    B.append(bx(2.4, 1.6, 4.6, "van_cream", at(0, 1.2, 0), bevel=0.12, name="body"))
    B.append(bx(2.42, 0.5, 4.62, "teal", at(0, 0.6, 0), bevel=0.05, name="band"))
    B.append(bx(2.2, 0.7, 1.2, "van_cream", at(0, 0.95, -2.7), bevel=0.1, name="nose"))
    B.append(bx(2.22, 0.3, 1.22, "teal", at(0, 0.6, -2.7), bevel=0.05))
    B.append(bx(2.0, 0.55, 0.05, "glass", at(0, 1.75, -2.31), bevel=0.02, name="screen"))
    for sx in (-1, 1):
        B.append(bx(0.04, 0.5, 0.75, "glass", at(sx * 1.205, 1.68, -1.75), bevel=0.015, name="sidewin"))
        B.append(bx(0.05, 0.16, 0.12, "ink", at(sx * 1.32, 1.6, -2.2), bevel=0.02, name="mirror"))
        B.append(rod((sx * 1.2, 1.6, -2.2), (sx * 1.3, 1.6, -2.2), 0.015, "ink", 6))
        B.append(bx(0.02, 1.4, 0.03, "ink", at(sx * 1.205, 1.15, -1.25), bevel=0.0))
        for z in (-1.8, 1.5):
            B.append(cy(0.5, 0.5, 0.04, 12, "ink", at(sx * 1.2, 0.42, z, rz(HALF_PI)), bevel=0.0, name="arch"))
            B += wheel(0.42, 0.3, at(sx * 1.15, 0.42, z))
    # sliding door + tail doors
    B.append(bx(0.02, 1.4, 0.03, "ink", at(1.205, 1.15, 0.0), bevel=0.0))
    B.append(bx(0.04, 0.05, 0.25, "steel", at(1.22, 1.2, -0.2), bevel=0.01))
    B.append(bx(0.03, 1.4, 0.02, "ink", at(0, 1.15, 2.305), bevel=0.0))
    for sx in (-0.55, 0.55):
        B.append(bx(0.8, 0.45, 0.04, "glass", at(sx, 1.6, 2.3), bevel=0.015))
        B.append(bx(0.22, 0.3, 0.05, "tail_lens", at(sx * 1.85, 0.9, 2.31), bevel=0.01))
    # nose: grille, lights, bumpers
    B.append(bx(1.2, 0.3, 0.05, "ink", at(0, 0.95, -3.31), bevel=0.02, name="grille"))
    for k in range(4):
        B.append(bx(1.1, 0.03, 0.04, "steel", at(0, 0.84 + k * 0.075, -3.33), bevel=0.0, name="slat"))
    for sx in (-0.8, 0.8):
        B.append(cy(0.13, 0.13, 0.06, 12, "lamp_lens", at(sx, 0.98, -3.31, rx(HALF_PI)), bevel=0.01))
    B.append(bx(2.3, 0.18, 0.16, "steel", at(0, 0.45, -3.35), bevel=0.05, name="bumper"))
    B.append(bx(2.3, 0.18, 0.16, "steel_dark", at(0, 0.45, 2.36), bevel=0.05))
    B.append(rod((0.7, 0.3, 2.2), (0.7, 0.28, 2.45), 0.05, "steel_dark", 8))
    # roof rack
    for sx in (-1.08, 1.08):
        B.append(bx(0.06, 0.12, 3.6, "steel_dark", at(sx, 2.12, 0.2), bevel=0.01, name="rail"))
        for z in (-1.5, 0.2, 1.9):
            B.append(bx(0.06, 0.14, 0.06, "steel_dark", at(sx, 2.05, z), bevel=0.0))
    for sx in (-0.8, 0.0, 0.8):
        B.append(bx(0.06, 0.05, 3.6, "steel_dark", at(sx, 2.07, 0.2), bevel=0.0))
    for z in (-1.5, -0.6, 0.3, 1.2, 1.9):
        B.append(bx(2.2, 0.05, 0.06, "steel_dark", at(0, 2.08, z), bevel=0.0))
    B.append(bx(0.7, 0.4, 0.6, "wood", at(-0.5, 2.3, 0.8), bevel=0.03, name="crate"))
    for y in (2.18, 2.42):
        B.append(bx(0.72, 0.05, 0.62, "wood_dark", at(-0.5, y, 0.8), bevel=0.0))
    B += tyre(0.336, 0.16, at(0.5, 2.25, -0.4), lugs=10, seg=16, ring=8)
    B.append(cable([Vector((-1.05, 2.15, 0.55)), Vector((-0.5, 2.56, 0.8)), Vector((0.05, 2.15, 1.05))], 0.02, "rope", res=6))
    # red logo (cog + spanner) on the +X side
    B.append(bx(0.06, 0.9, 1.2, "red", at(1.22, 1.3, 0.4), bevel=0.03, name="logo"))
    lx = at(1.26, 1.3, 0.4, rz(-HALF_PI))
    B.append(cy(0.26, 0.26, 0.03, 16, "cream", lx, bevel=0.0))
    B.append(cy(0.12, 0.12, 0.035, 12, "red", lx, bevel=0.0))
    for i in range(8):
        a = i / 8 * TAU
        B.append(bx(0.1, 0.03, 0.1, "cream", lx @ at(math.cos(a) * 0.29, 0, math.sin(a) * 0.29, ry(-a)), bevel=0.0))
    return dict(groups={"body": B}, tex=768)


def cabin():
    """Portacabin, origin on the ground at its centre, front (window + door) at -Z.
    Place at CABIN with ry(-0.5) as today."""
    B = [bx(5.0, 2.6, 2.6, "cabin_yellow", at(0, 1.5, 0), bevel=0.06, name="box")]
    B.append(bx(5.3, 0.15, 2.9, "steel_dark", at(0, 2.85, 0), bevel=0.03, name="roof"))
    for sx in (-1, 1):
        for sz in (-1, 1):
            B.append(bx(0.14, 2.62, 0.14, "steel_dark", at(sx * 2.47, 1.5, sz * 1.27), bevel=0.02))
            B.append(bx(0.5, 0.2, 0.5, "concrete", at(sx * 2.1, 0.1, sz * 0.9), bevel=0.03))
    # wall ribs (back + ends + front where clear of window/door)
    for i in range(9):
        x = -2.0 + i * 0.5
        B.append(bx(0.06, 2.3, 0.04, "cabin_yellow", at(x, 1.5, 1.31), bevel=0.01))
        if not (-1.9 < x < -0.5 or 0.7 < x < 1.7):
            B.append(bx(0.06, 2.3, 0.04, "cabin_yellow", at(x, 1.5, -1.31), bevel=0.01))
    for sx in (-1, 1):
        for z in (-0.75, -0.25, 0.25, 0.75):
            B.append(bx(0.04, 2.3, 0.06, "cabin_yellow", at(sx * 2.51, 1.5, z), bevel=0.01))
    # window, door, steps, sign
    B.append(bx(1.36, 0.96, 0.06, "steel_dark", at(-1.2, 1.7, -1.31), bevel=0.02, name="frame"))
    B.append(bx(1.2, 0.8, 0.05, "glass", at(-1.2, 1.7, -1.33), bevel=0.01, name="window"))
    B.append(bx(0.04, 0.8, 0.06, "steel_dark", at(-1.2, 1.7, -1.35), bevel=0.0))
    for k in range(4):
        B.append(rod((-1.8 + k * 0.4, 1.28, -1.39), (-1.8 + k * 0.4, 2.12, -1.39), 0.015, "ink", 6))
    B.append(bx(0.92, 1.9, 0.05, "steel_dark", at(1.2, 1.15, -1.31), bevel=0.02))
    B.append(bx(0.8, 1.8, 0.05, "door_brown", at(1.2, 1.1, -1.33), bevel=0.02, name="door"))
    B.append(bx(0.06, 0.04, 0.1, "steel", at(0.9, 1.1, -1.38), bevel=0.01))
    B.append(bx(0.5, 0.12, 0.03, "cream", at(1.2, 1.7, -1.37), bevel=0.0))
    for y, z in ((0.07, -1.85), (0.16, -1.6)):
        B.append(bx(1.0, 0.1, 0.32, "steel_dark", at(1.2, y, z), bevel=0.02, name="step"))
    B.append(bx(1.6, 0.4, 0.05, "cream", at(-0.1, 2.55, -1.33), bevel=0.02, name="sign"))
    for x in (-0.65, -0.35, -0.05, 0.25, 0.5):
        B.append(bx(0.18, 0.2, 0.02, "ink", at(x, 2.55, -1.36), bevel=0.0))
    # AC unit, stove pipe, gas bottle
    B.append(bx(0.5, 0.45, 0.35, "steel_clean", at(2.7, 1.9, 0.3), bevel=0.03, name="ac"))
    B.append(cy(0.16, 0.16, 0.04, 12, "ink", at(2.95, 1.9, 0.3, rz(HALF_PI)), bevel=0.0))
    B.append(cy(0.08, 0.08, 0.8, 10, "steel_dark", at(1.6, 3.3, 0.6), bevel=0.01, name="flue"))
    B.append(cy(0.13, 0.08, 0.1, 10, "ink", at(1.6, 3.74, 0.6), bevel=0.01))
    B.append(cy(0.16, 0.16, 0.7, 12, "red", at(-2.75, 0.4, 0.8), bevel=0.05, name="gas"))
    B.append(cy(0.05, 0.06, 0.12, 8, "steel", at(-2.75, 0.81, 0.8), bevel=0.01))
    return dict(groups={"body": B}, tex=1024)


def lamp_post():
    """Lamp post, origin on the ground at the post; head at y 4; the glow strip
    glow_lamp (origin at its centre, y 3.92) sits under the head. Bunting hooks
    on at y 3.7."""
    B = [bx(0.15, 4.0, 0.15, "steel_dark", at(0, 2.0, 0), bevel=0.02, name="post")]
    B.append(bx(0.34, 0.12, 0.34, "concrete", at(0, 0.06, 0), bevel=0.03))
    B.append(bx(0.22, 0.3, 0.22, "steel_dark", at(0, 0.3, 0), bevel=0.02))
    B.append(bx(0.6, 0.15, 0.3, "ink", at(0, 4.0, 0), bevel=0.04, name="head"))
    B.append(bx(0.5, 0.06, 0.26, "steel_dark", at(0, 4.09, 0), bevel=0.02))
    B.append(bx(0.16, 0.24, 0.1, "yellow", at(0, 1.3, -0.12), bevel=0.02, name="box"))
    B.append(torus(0.07, 0.015, "steel", at(0, 3.7, 0), seg=12, ring=5))
    B.append(cable([Vector((0.03, 1.42, -0.12)), Vector((0.05, 2.6, -0.1)), Vector((0.03, 3.9, -0.08))], 0.012, "ink", res=4))
    g = glow_box(0.4, 0.06, 0.2, "glow_lamp", at(0, 3.92, 0))
    return dict(groups={"body": B}, glows=[g], origins={"glow_lamp": (0, 3.92, 0)}, tex=512)


def bunting():
    """One festoon span from lamp to lamp along +X: x 0 -> 6.818 at y 3.7,
    sagging 0.6 at mid-span, 8 flags. Origin on the ground under the first lamp;
    rotate so +X points at the next lamp."""
    L = LAMP_SPAN

    def yat(u):
        return 3.7 - math.sin(u * math.pi) * 0.6

    pts = [Vector((L * k / 12, yat(k / 12), 0.0)) for k in range(13)]
    B = [cable(pts, 0.012, "ink", res=3)]
    for k in range(1, 9):
        u = k / 9
        slope = math.atan(-0.6 * math.pi / L * math.cos(u * math.pi))
        flap = 0.35 * math.sin(k * 2.1)
        f = at(L * u, yat(u), 0.0, rz(slope) @ rx(flap))
        tri = prism([(-0.12, 0.0), (0.12, 0.0), (0.0, -0.27)], 0.012, FLAGS[k % 5], f @ ry(HALF_PI), bevel=0.0, name="flag")
        B.append(tri)
    return dict(groups={"body": B}, tex=256)


def kennel():
    """Dog kennel, origin on the ground at its centre, door at -Z (place at
    KENNEL with ry(atan2(x, z)) as today so the door faces the van)."""
    B = [bx(1.4, 1.0, 1.4, "wood", at(0, 0.5, 0), bevel=0.05, name="box")]
    for i in range(4):
        y = 0.12 + i * 0.25
        for ang in (0.0, HALF_PI, math.pi, -HALF_PI):
            B.append(bx(1.3, 0.02, 0.02, "wood_dark", ry(ang) @ at(0, y, 0.705), bevel=0.0))
    # gable roof, ridge along Z
    B.append(prism([(-0.7, 1.0), (0.7, 1.0), (0.0, 1.55)], 1.4, "wood", ry(HALF_PI), bevel=0.02, name="gable"))
    for sx in (-1, 1):
        a = math.atan2(0.55, 0.7)
        B.append(bx(1.05, 0.07, 1.7, "roof_red", at(sx * 0.4, 1.3, 0, rz(-sx * a)), bevel=0.02, name="roof"))
    B.append(bx(0.1, 0.08, 1.74, "roof_red", at(0, 1.6, 0), bevel=0.02))
    # arched door
    B.append(bx(0.6, 0.5, 0.05, "ink", at(0, 0.3, -0.71), bevel=0.0, name="door"))
    B.append(cy(0.3, 0.3, 0.05, 14, "ink", at(0, 0.55, -0.71, rx(HALF_PI)), bevel=0.0))
    B.append(bx(0.42, 0.13, 0.03, "cream", at(0, 1.18, -0.72), bevel=0.01, name="plate"))
    for x in (-0.12, 0.0, 0.12):
        B.append(bx(0.06, 0.07, 0.01, "ink", at(x, 1.18, -0.74), bevel=0.0))
    # bowl, bone, chain
    B.append(cy(0.25, 0.2, 0.12, 14, "red", at(0.9, 0.06, -0.6), bevel=0.02, name="bowl"))
    B.append(cy(0.19, 0.19, 0.02, 14, "wood_dark", at(0.9, 0.11, -0.6), bevel=0.0))
    B.append(rod((-0.75, 0.04, -0.85), (-0.45, 0.04, -0.95), 0.03, "cream", 6))
    for x, z in ((-0.75, -0.85), (-0.45, -0.95)):
        for d in (-0.035, 0.035):
            B.append(cy(0.035, 0.035, 0.06, 8, "cream", at(x, 0.04, z + d, rx(HALF_PI)), bevel=0.0))
    B.append(cable([Vector((0.7, 0.3, -0.4)), Vector((0.9, 0.05, -0.9)), Vector((0.5, 0.03, -1.3))], 0.02, "steel_dark", res=5))
    return dict(groups={"body": B}, tex=512)


def warning_sign():
    """'Beware of the dog' board 1.2 x 0.5 at y 1.2 on a post, origin on the
    ground at the post; the painted face is -Z (place with ry(atan2(x, z)))."""
    B = [bx(1.2, 0.5, 0.05, "yellow", at(0, 1.2, 0), bevel=0.015, name="board")]
    B.append(bx(0.08, 1.45, 0.08, "steel_dark", at(0, 0.725, 0.065), bevel=0.015, name="post"))
    z = -0.03
    for y, w, h in ((1.425, 1.1, 0.03), (0.975, 1.1, 0.03)):
        B.append(bx(w, h, 0.01, "ink", at(0, y, z), bevel=0.0))
    for x in (-0.565, 0.565):
        B.append(bx(0.03, 0.45, 0.01, "ink", at(x, 1.2, z), bevel=0.0))
    # "!" on the left, paw print on the right
    B.append(bx(0.07, 0.22, 0.01, "ink", at(-0.32, 1.25, z), bevel=0.0))
    B.append(cy(0.04, 0.04, 0.01, 8, "ink", at(-0.32, 1.08, z, rx(HALF_PI)), bevel=0.0))
    B.append(cy(0.1, 0.1, 0.01, 12, "ink", at(0.22, 1.12, z, rx(HALF_PI) @ lib.scl(1.2, 1, 0.9)), bevel=0.0))
    for dx, dy in ((-0.13, 1.26), (-0.045, 1.31), (0.045, 1.31), (0.13, 1.26)):
        B.append(cy(0.04, 0.04, 0.01, 10, "ink", at(0.22 + dx, dy, z, rx(HALF_PI)), bevel=0.0))
    for x in (-0.4, 0.4):
        B.append(bolt(0.018, "steel", at(x, 1.2, -0.03, rx(-HALF_PI))))
    return dict(groups={"body": B}, tex=256)


# ------------------------------------------------------------------ crane
def crane():
    """Crawler crane, origin on the ground at its turning centre (CRANE).
    base   - tracks 3 x 0.8 x 3.4, origin (0, 0, 0)
    cab    - house + operator cab + lattice boom; origin (0, 0.8, 0) on top of
             the base; rotate about Y for yaw (boom points +Z at yaw 0)
    magnet - origin at its top attach point, modelled at rest (0, 6.5, 10)
    cable  - 1 m along -Y from its origin (at the boom tip); scale Y to length"""
    yel = "yellow"
    Bs = []
    for sx in (-1, 1):
        x = sx * 1.125
        Bs.append(bx(0.75, 0.62, 2.6, "tyre", at(x, 0.4, 0), bevel=0.06, name="track"))
        for sz in (-1, 1):
            Bs.append(cy(0.36, 0.36, 0.75, 16, "tyre", at(x, 0.4, sz * 1.3, rz(HALF_PI)), bevel=0.05))
            Bs.append(cy(0.24, 0.24, 0.79, 12, "steel_dark", at(x, 0.4, sz * 1.3, rz(HALF_PI)), bevel=0.02))
        for k in range(5):
            Bs.append(cy(0.15, 0.15, 0.8, 8, "steel_dark", at(x, 0.27, -0.9 + k * 0.45, rz(HALF_PI)), bevel=0.0))
        for k in range(12):
            Bs.append(bx(0.78, 0.05, 0.1, "steel_dark", at(x, 0.74, -1.3 + k * 0.236), bevel=0.0))
        Bs.append(bx(0.05, 0.22, 2.5, yel, at(sx * 1.51, 0.48, 0), bevel=0.02))
    Bs.append(bx(1.5, 0.45, 2.4, "steel_dark", at(0, 0.45, 0), bevel=0.04, name="frame"))
    Bs.append(cy(1.0, 1.0, 0.12, 24, "steel", at(0, 0.74, 0), bevel=0.02, name="slew"))
    # house
    C = [bx(2.4, 1.6, 2.6, yel, at(0, 1.6, -0.3), bevel=0.08, name="house")]
    C.append(hazard_box(2.42, 0.6, 0.62, at(0, 1.25, -1.82), bevel=0.04))
    C.append(bx(2.42, 0.25, 0.64, "ink", at(0, 1.7, -1.82), bevel=0.03))
    C.append(bx(0.03, 0.6, 1.0, "ink", at(-1.21, 1.7, -0.9), bevel=0.0, name="vent"))
    for k in range(3):
        C.append(bx(0.04, 0.05, 1.0, "steel_dark", at(-1.225, 1.55 + k * 0.15, -0.9), bevel=0.0))
    C.append(cy(0.07, 0.07, 0.8, 10, "ink", at(-0.8, 2.7, -1.2), bevel=0.01, name="stack"))
    C.append(cy(0.1, 0.07, 0.08, 10, "steel_dark", at(-0.8, 3.12, -1.2), bevel=0.01))
    # operator cab: frame + glass
    C.append(bx(1.2, 1.0, 1.0, yel, at(0.5, 2.6, 0.6), bevel=0.05, name="cab"))
    C.append(bx(1.0, 0.65, 0.04, "glass", at(0.5, 2.68, 1.105), bevel=0.01))
    C.append(bx(0.04, 0.6, 0.75, "glass", at(1.105, 2.68, 0.6), bevel=0.01))
    C.append(bx(0.04, 0.6, 0.75, "glass", at(-0.105, 2.68, 0.6), bevel=0.01))
    C.append(bx(1.3, 0.06, 1.1, "steel_dark", at(0.5, 3.13, 0.6), bevel=0.02))
    # A-frame and pendant lines to the tip
    for sx in (-0.5, 0.5):
        C.append(rod((sx, 2.4, -1.4), (0.0, 3.6, -1.0), 0.05, yel, 8))
    bt = Matrix.Translation(BOOM_PIVOT) @ rx(-BOOM_TILT)
    tipL = bt @ Vector((0, 0.3, BOOM_LEN - 0.1))
    for sx in (-0.25, 0.25):
        C.append(rod((sx, 3.6, -1.0), (tipL.x + sx, tipL.y, tipL.z), 0.018, "ink", 4))
    # lattice boom (boom frame: along +Z from the foot pin)
    n = 9
    seg = BOOM_LEN / n
    for sx in (-0.3, 0.3):
        for sy in (-0.3, 0.3):
            C.append(rod(bt @ Vector((sx, sy, 0.05)), bt @ Vector((sx, sy, BOOM_LEN)), 0.045, yel, 6, bevel=0.0))
    for i in range(n):
        z0, z1 = i * seg, (i + 1) * seg
        f = 1 if i % 2 else -1
        for sx in (-0.3, 0.3):
            C.append(rod(bt @ Vector((sx, -0.3 * f, z0)), bt @ Vector((sx, 0.3 * f, z1)), 0.025, yel, 5, bevel=0.0))
        for sy in (-0.3, 0.3):
            C.append(rod(bt @ Vector((-0.3 * f, sy, z0)), bt @ Vector((0.3 * f, sy, z1)), 0.025, yel, 5, bevel=0.0))
    C.append(cy(0.12, 0.12, 0.8, 12, "steel_dark", bt @ rz(HALF_PI), bevel=0.02, name="pin"))
    C.append(cy(0.3, 0.3, 0.14, 16, "steel_dark", bt @ at(0, 0, BOOM_LEN) @ rz(HALF_PI), bevel=0.02, name="sheave"))
    C.append(bx(0.7, 0.7, 0.12, yel, bt @ at(0, 0, BOOM_LEN - 0.05), bevel=0.02))
    C.append(rod(Vector((0.0, 2.45, -0.6)), bt @ Vector((0, 0.32, BOOM_LEN)), 0.02, "ink", 4))
    # magnet, hanging from its top eye
    mp = at(*MAGNET_REST)
    M = [torus(0.1, 0.03, "steel", mp @ at(0, -0.1, 0) @ rx(HALF_PI), seg=12, ring=6)]
    M.append(bx(0.4, 0.22, 0.4, "red", mp @ at(0, -0.3, 0), bevel=0.04, name="box"))
    M.append(cy(0.9, 0.9, 0.3, 24, "steel_dark", mp @ at(0, -0.6, 0), bevel=0.04, name="disc"))
    M.append(cy(0.92, 0.92, 0.1, 24, "yellow", mp @ at(0, -0.52, 0), bevel=0.02))
    M.append(cy(0.8, 0.8, 0.03, 24, "ink", mp @ at(0, -0.76, 0), bevel=0.0))
    for i in range(6):
        a = i / 6 * TAU
        M.append(bx(0.5, 0.1, 0.08, "steel_dark", mp @ at(math.cos(a) * 0.55, -0.42, math.sin(a) * 0.55, ry(-a)), bevel=0.02))
    M.append(cable([mp @ Vector((0.18, -0.32, 0.0)), mp @ Vector((0.4, -0.25, 0.1)), mp @ Vector((0.5, -0.42, 0.2))], 0.025, "ink", res=4))
    tip = Vector(BOOM_TIP)
    K = [cy(0.03, 0.03, 1.0, 6, "ink", at(tip.x, tip.y - 0.5, tip.z), bevel=0.0, name="cable")]
    return dict(groups={"base": Bs, "cab": C, "magnet": M, "cable": K},
                origins={"base": (0, 0, 0), "cab": (0, 0.8, 0), "magnet": MAGNET_REST, "cable": BOOM_TIP}, tex=768)


# ------------------------------------------------------------------ characters
def scavenger():
    """The player. Groups (origins = scrap.rs rig offsets):
    body (0, 0, 0) feet; leg (0.13, 0.6, 0) hip, hangs down; arm (0.33, 1.08, 0)
    shoulder, hangs down; sack (0, 1.0, -0.32) centre. Faces +Z."""
    B = [bx(0.5, 0.55, 0.32, "denim", at(0, 0.85, 0), bevel=0.06, name="torso")]
    B.append(bx(0.44, 0.14, 0.28, "denim", at(0, 0.58, 0), bevel=0.04, name="hips"))
    B.append(bx(0.52, 0.12, 0.34, "hivis", at(0, 1.0, 0), bevel=0.03, name="band"))
    B.append(bx(0.22, 0.18, 0.04, "denim", at(0, 0.82, 0.165), bevel=0.02, name="bib"))
    B.append(bx(0.12, 0.06, 0.02, "steel", at(0, 0.88, 0.19), bevel=0.0))
    for sx in (-0.13, 0.13):
        B.append(bx(0.07, 0.05, 0.36, "denim", at(sx, 1.12, 0), bevel=0.01))
        B.append(cy(0.025, 0.025, 0.02, 8, "steel", at(sx, 0.95, 0.17, rx(HALF_PI)), bevel=0.0))
    B.append(bx(0.16, 0.08, 0.16, "skin", at(0, 1.14, 0), bevel=0.03))
    B.append(bx(0.34, 0.32, 0.32, "skin", at(0, 1.32, 0), bevel=0.08, name="head"))
    B.append(bx(0.07, 0.07, 0.06, "skin", at(0, 1.3, 0.17), bevel=0.02, name="nose"))
    for sx in (-0.07, 0.07):
        B.append(bx(0.05, 0.07, 0.02, "ink", at(sx, 1.35, 0.165), bevel=0.01))
        B.append(bx(0.04, 0.08, 0.06, "skin", at(sx * 2.5, 1.32, 0), bevel=0.02))
    # yellow cap, welding mask flipped up on it
    B.append(bx(0.38, 0.1, 0.36, "yellow", at(0, 1.5, -0.02), bevel=0.04, name="cap"))
    B.append(bx(0.3, 0.03, 0.12, "yellow", at(0, 1.47, 0.2), bevel=0.01))
    mask = at(0, 1.5, 0.12, rx(-0.9))
    B.append(bx(0.38, 0.3, 0.1, "mask", mask, bevel=0.03, name="mask"))
    B.append(bx(0.22, 0.08, 0.02, "visor", mask @ at(0, 0.03, 0.055), bevel=0.0))
    for sx in (-0.2, 0.2):
        B.append(cy(0.04, 0.04, 0.04, 10, "steel_dark", at(sx, 1.45, 0.03, rz(HALF_PI)), bevel=0.0))
    hip = Vector((0.13, 0.6, 0.0))
    lg = Matrix.Translation(hip)
    Lg = [bx(0.18, 0.5, 0.2, "denim", lg @ at(0, -0.25, 0), bevel=0.04, name="leg")]
    Lg.append(bx(0.19, 0.06, 0.21, "denim", lg @ at(0, -0.41, 0), bevel=0.02))
    Lg.append(bx(0.13, 0.12, 0.03, "denim", lg @ at(0, -0.2, 0.1), bevel=0.01))
    Lg.append(bx(0.2, 0.12, 0.3, "boot", lg @ at(0, -0.52, 0.05), bevel=0.04, name="boot"))
    Lg.append(bx(0.21, 0.04, 0.31, "ink", lg @ at(0, -0.57, 0.05), bevel=0.01))
    sh = Vector((0.33, 1.08, 0.0))
    ag = Matrix.Translation(sh)
    A = [bx(0.14, 0.45, 0.16, "denim", ag @ at(0, -0.2, 0), bevel=0.04, name="arm")]
    A.append(bx(0.16, 0.14, 0.18, "denim", ag @ at(0, -0.02, 0), bevel=0.04))
    A.append(bx(0.15, 0.06, 0.17, "denim", ag @ at(0, -0.36, 0), bevel=0.02))
    A.append(bx(0.15, 0.13, 0.17, "glove", ag @ at(0, -0.45, 0), bevel=0.04, name="glove"))
    sk = Vector((0.0, 1.0, -0.32))
    S = [blob(0.3, "sack", at(*sk), seed=4, rough=0.15, subdiv=2, squash=0.95, name="sack")]
    S.append(cy(0.08, 0.12, 0.12, 10, "sack", at(sk.x, sk.y + 0.3, sk.z), bevel=0.02))
    S.append(torus(0.09, 0.02, "rope", at(sk.x, sk.y + 0.3, sk.z), seg=10, ring=5))
    S.append(bx(0.14, 0.12, 0.02, "cloth", at(sk.x + 0.1, sk.y - 0.05, sk.z - 0.29, ry(0.3)), bevel=0.0))
    S.append(rod((sk.x - 0.05, sk.y + 0.3, sk.z), (sk.x - 0.15, sk.y + 0.62, sk.z - 0.05), 0.025, "rusty_steel", 6))
    S.append(cable([Vector((-0.2, 1.22, -0.12)), Vector((0.0, 1.3, -0.25)), Vector((0.2, 1.22, -0.12))], 0.02, "rope", res=4))
    return dict(groups={"body": B, "leg": Lg, "arm": A, "sack": S},
                origins={"leg": tuple(hip), "arm": tuple(sh), "sack": tuple(sk)}, tex=512)


def dog():
    """The yard dog, facing +Z. body: origin on the ground (as `dg`); leg: one
    leg hanging from its hip at (0.15, 0.4, 0.3) (as `dl` at each DogLeg spot);
    glow_eye_l / glow_eye_r at their centres (-+0.096, 0.91, 0.685)."""
    hd = Vector((0.0, 0.85, 0.5))
    B = [bx(0.45, 0.4, 0.9, "fur", at(0, 0.55, 0), bevel=0.12, name="body")]
    B.append(bx(0.36, 0.12, 0.6, "fur_light", at(0, 0.38, 0.05), bevel=0.05))
    B.append(bx(0.4, 0.2, 0.3, "fur_dark", at(0, 0.72, -0.15), bevel=0.06))
    B.append(bx(0.36, 0.34, 0.36, "fur", at(*hd), bevel=0.09, name="head"))
    B.append(bx(0.22, 0.18, 0.22, "fur_light", at(0, hd.y - 0.06, hd.z + 0.24), bevel=0.05, name="snout"))
    B.append(bx(0.1, 0.08, 0.06, "ink", at(0, hd.y, hd.z + 0.36), bevel=0.02, name="nose"))
    B.append(bx(0.12, 0.03, 0.02, "ink", at(0, hd.y - 0.12, hd.z + 0.355), bevel=0.0))
    B.append(bx(0.3, 0.05, 0.05, "fur_dark", at(0, hd.y + 0.12, hd.z + 0.17, rx(-0.2)), bevel=0.015, name="brow"))
    for sx in (-0.12, 0.12):
        B.append(bx(0.08, 0.2, 0.06, "fur_dark", at(sx, hd.y + 0.22, hd.z - 0.05, rz(-sx * 1.2)), bevel=0.02, name="ear"))
        B.append(bx(0.07, 0.07, 0.02, "ink", at(sx * 0.8, hd.y + 0.06, hd.z + 0.18), bevel=0.0))
    B.append(bx(0.4, 0.08, 0.4, "collar", at(0, hd.y - 0.2, hd.z - 0.05), bevel=0.02, name="collar"))
    for i in range(6):
        a = i / 6 * TAU
        B.append(cy(0.0, 0.03, 0.05, 6, "steel", at(math.cos(a) * 0.205, hd.y - 0.2, hd.z - 0.05 + math.sin(a) * 0.205, ry(-a) @ rz(-HALF_PI)), bevel=0.0))
    B.append(cy(0.05, 0.05, 0.015, 10, "yellow", at(0, hd.y - 0.3, hd.z + 0.16, rx(HALF_PI)), bevel=0.0))
    B.append(bx(0.08, 0.08, 0.35, "fur", at(0, 0.8, -0.55, rx(0.7)), bevel=0.03, name="tail"))
    B.append(bx(0.09, 0.09, 0.08, "fur_light", at(0, 0.92, -0.67, rx(0.7)), bevel=0.03))
    eyes = {f"glow_eye_{s}": (sx * 0.8, hd.y + 0.06, hd.z + 0.185) for s, sx in (("l", -0.12), ("r", 0.12))}
    glows = [glow_box(0.06, 0.06, 0.02, n, at(*p)) for n, p in eyes.items()]
    hip = Vector((0.15, 0.4, 0.3))
    lf = Matrix.Translation(hip)
    Lg = [bx(0.12, 0.4, 0.12, "fur", lf @ at(0, -0.2, 0), bevel=0.03, name="leg")]
    Lg.append(bx(0.14, 0.07, 0.18, "fur_light", lf @ at(0, -0.365, 0.03), bevel=0.025, name="paw"))
    return dict(groups={"body": B, "leg": Lg}, glows=glows, origins={"leg": tuple(hip), **eyes}, tex=512)


def tarp():
    """Blue tarp draped over a deep-pile heap: 1.1 (X) x 0.9 (Z) footprint,
    centred; scale by the pile radius. Tint 0 = tarp colour."""
    def drape(x, z):
        u, v = x / 0.55, z / 0.45
        d = min(1.0, math.hypot(u, v))
        y = 0.12 * (1 - d * d) - 0.06 * d ** 4
        y += 0.03 * math.sin(x * 9 + z * 4) + 0.02 * math.sin(z * 13 - x * 3)
        y += 0.05 * math.exp(-((u - 0.3) ** 2 + (v + 0.2) ** 2) * 6)
        return (x, y, z)

    B = [sheet(1.1, 0.9, 0.012, "tarp", I, nx=16, nz=14, warp=drape, name="tarp")]
    for x, z in ((-0.52, -0.42), (0.52, -0.42), (-0.52, 0.42), (0.52, 0.42), (0.0, -0.43), (0.0, 0.43)):
        p = drape(x, z)
        B.append(torus(0.018, 0.006, "steel", at(p[0], p[1] + 0.008, p[2]), seg=8, ring=4))
    B.append(cable([Vector((-0.5, -0.04, -0.2)), Vector((-0.1, 0.17, 0.0)), Vector((0.5, -0.04, 0.25))], 0.01, "rope", res=6))
    return dict(groups={"body": B}, tex=256)


ASSETS = {
    "yard_ground": yard_ground, "yard_outer": yard_outer, "fence": fence, "mound": mound, "mountain": mountain,
    "car_stack": car_stack, "zone_board": zone_board, "van": van, "cabin": cabin, "lamp_post": lamp_post,
    "bunting": bunting, "kennel": kennel, "warning_sign": warning_sign, "crane": crane, "scavenger": scavenger,
    "dog": dog, "tarp": tarp,
}
