"""Workshop (robot builder screen): a cosy junk garage around the turntable.

Workshop world frame (Bevy space): origin = turntable centre at floor level
(the game's BENCH_Y), +Y up, +Z = the bot's front (towards the roller door).
Every asset except the turntable is modelled in that frame with its origin at
(0, 0, 0), so the game places it at (0, BENCH_Y, 0). The turntable's origin is
its deck's top centre (placed at BENCH_Y + 0.06).

The camera orbits the bot all the way round, at most 10 m out horizontally
(dist <= 10), so everything taller than the floor decals stays outside
r = 10.5 and can never get between the camera and the bot. Walls are the
inner faces of a 24 x 24 m shed (|x|, |z| = 11.85).
"""
import math

import bmesh
from mathutils import Matrix

import lib
from lib import (HALF_PI, TAU, Look, _finish, _new, at, box, cable, cyl, glow, glow_box, lump, plane, prism,
                 rod, rx, ry, rz, scl, torus)
from lib import sphere as sphere_
from mathutils import Vector

L = lib.LOOKS
L["ws_concrete"] = Look("#8e8274", chip=0.0, rust=0.0, grime=0.8, rough=0.9, light=0.08, edge=0.15, dust="#b3a07e")
L["ws_slab"] = Look("#8a7e70", chip=0.0, rust=0.0, grime=0.5, rough=0.9, light=0.06, edge=0.0, chip_scale=4.0)
L["ws_oil"] = Look("#2c241e", chip=0.0, rust=0.0, grime=0.2, rough=0.22, metal=0.1, light=0.04, edge=0.0)
L["ws_stain"] = Look("#76685a", chip=0.0, rust=0.0, grime=0.3, rough=0.6, light=0.04, edge=0.0)
L["ws_joint"] = Look("#4a4038", chip=0.0, rust=0.0, grime=0.3, rough=0.9, light=0.0, edge=0.0)
L["ws_line"] = Look("#e9a72c", chip=1.25, rust=0.0, grime=0.5, rough=0.7, light=0.05, edge=0.0, chip_scale=7.0,
                    chip_col="#8a7e70", bare="#8a7e70")
L["ws_line_w"] = Look("#e8dcc0", chip=1.25, rust=0.0, grime=0.5, rough=0.7, light=0.05, edge=0.0, chip_scale=7.0,
                      chip_col="#8a7e70", bare="#8a7e70")
L["ws_floor_hazard"] = Look("#efb032", chip=1.1, rust=0.0, grime=0.5, rough=0.7, light=0.05, edge=0.0, chip_scale=7.0,
                            stripes=0.5, chip_col="#8a7e70", bare="#8a7e70")
L["ws_tin"] = Look("#968a7a", chip=0.0, rust=0.55, grime=0.6, rough=0.5, metal=0.45, streaks=0.7, light=0.1, edge=0.05)
L["ws_tin_teal"] = Look("#5a7e74", chip=0.5, rust=0.45, grime=0.6, streaks=0.6, light=0.1, edge=0.05)
L["ws_tin_red"] = Look("#8a4a32", chip=0.5, rust=0.5, grime=0.6, streaks=0.6, light=0.1, edge=0.05)
L["ws_tin_rust"] = Look("#8a5a3a", chip=0.0, rust=0.8, grime=0.6, rough=0.7, metal=0.2, streaks=0.8, light=0.1, edge=0.05)
L["ws_tin_cream"] = Look("#c8b892", chip=0.5, rust=0.45, grime=0.6, streaks=0.6, light=0.1, edge=0.05, bare="#8a8478")
L["ws_door"] = Look("#c8602e", chip=0.6, rust=0.45, grime=0.6, streaks=0.6, light=0.12)
L["ws_board"] = Look("#b98a58", chip=0.0, rust=0.0, grime=0.55, rough=0.85, light=0.15, edge=0.15, dust="#d0b088")
L["ws_shadow"] = Look("#4a3424", chip=0.0, rust=0.0, grime=0.2, rough=0.85, light=0.0, edge=0.0)
L["ws_card"] = Look("#b88c58", chip=0.0, rust=0.0, grime=0.5, rough=0.95, light=0.2, edge=0.25, dust="#d8b888")
L["ws_blue"] = Look("#2f64a8", chip=0.55, rust=0.3, streaks=0.3)
L["ws_gas"] = Look("#3c7a4c", chip=0.6, rust=0.4, streaks=0.3)
L["ws_mat"] = Look("#3a3634", chip=0.0, rust=0.0, grime=0.35, rough=0.9, light=0.15, edge=0.15, rings=0.35)
L["ws_tarp"] = Look("#3d6a94", chip=0.0, rust=0.0, grime=0.6, rough=0.85, light=0.25, edge=0.2, dust="#a89a80")
L["ws_tread"] = Look("#8e9096", chip=0.0, rust=0.3, grime=0.6, rough=0.45, metal=0.55, streaks=0.0, light=0.15, edge=0.4)

L["ws_window"] = Look("#4f6e8c", chip=0.0, rust=0.0, grime=0.7, rough=0.2, metal=0.1, light=0.3, edge=0.3, dust="#9a9070")

WALL = 11.85  # inner face of the walls
DOOR_W, DOOR_H = 3.9, 4.0


def v(x, y, z):
    return Vector((x, y, z))


# ------------------------------------------------------------------ local geometry helpers
def rodf(a, b, r, look, seg=6):
    """Rod without geometric bevels (only seen from 10 m+)."""
    return rod(a, b, r, look, seg, bevel=0.0)


def corrugated(w, h, look, xf, period=0.16, amp=0.022, name="tin"):
    """Corrugated sheet w (X) by h (Y), ridges running along Y, facing +Z."""
    bm = bmesh.new()
    n = max(2, round(w / period * 4))
    cols = []
    for i in range(n + 1):
        x = -w / 2 + w * i / n
        z = amp * math.sin(i / n * (w / period) * TAU)
        cols.append((bm.verts.new((x, -h / 2, z)), bm.verts.new((x, h / 2, z))))
    for i in range(n):
        a, b = cols[i]
        c, d = cols[i + 1]
        bm.faces.new((a, c, d, b))
    return _finish(_new(name, bm), look, xf, 0.0)


def blob(r, look, xf, seed=0, n=28, wob=0.3, name="blob"):
    """Irregular flat splat in XZ facing +Y (oil stains, puddles)."""
    bm = bmesh.new()
    c = bm.verts.new((0, 0, 0))
    vs = []
    for i in range(n):
        a = i / n * TAU
        k = 1 + wob * (0.6 * math.sin(3 * a + seed * 1.7) + 0.3 * math.sin(5 * a + seed * 2.9) + 0.25 * math.sin(9 * a + seed))
        vs.append(bm.verts.new((math.cos(a) * r * k, 0, math.sin(a) * r * k)))
    for i in range(n):
        bm.faces.new((c, vs[(i + 1) % n], vs[i]))
    return _finish(_new(name, bm), look, xf, 0.0, smooth=False)


def ring(r0, r1, h, look, xf, seg=64, name="ring"):
    """Flat solid annulus r0..r1, from y=0 to y=h."""
    bm = bmesh.new()
    inner = [bm.verts.new((math.cos(i / seg * TAU) * r0, h, math.sin(i / seg * TAU) * r0)) for i in range(seg)]
    outer = [bm.verts.new((math.cos(i / seg * TAU) * r1, h, math.sin(i / seg * TAU) * r1)) for i in range(seg)]
    faces = [bm.faces.new((inner[i], inner[(i + 1) % seg], outer[(i + 1) % seg], outer[i])) for i in range(seg)]
    ret = bmesh.ops.extrude_face_region(bm, geom=faces, use_keep_orig=True)
    bmesh.ops.translate(bm, vec=(0, -h, 0), verts=[e for e in ret["geom"] if isinstance(e, bmesh.types.BMVert)])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _finish(_new(name, bm), look, xf, 0.0, smooth=False)


def arc_strip(cx, cz, rad, a0, a1, w, look, y=0.0015, seg=24, name="skid"):
    """Flat curved strip (tyre skid marks) round (cx, cz) from angle a0 to a1."""
    bm = bmesh.new()
    rows = []
    for i in range(seg + 1):
        a = a0 + (a1 - a0) * i / seg
        ww = w * (0.35 + 0.65 * math.sin(math.pi * i / seg))  # tapers at both ends
        rows.append([bm.verts.new((cx + math.cos(a) * (rad + s * ww / 2), y, cz + math.sin(a) * (rad + s * ww / 2))) for s in (-1, 1)])
    for i in range(seg):
        bm.faces.new((rows[i][0], rows[i + 1][0], rows[i + 1][1], rows[i][1]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    o = _new(name, bm)
    if o.data.polygons[0].normal.y < 0:
        o.data.flip_normals()
    return _finish(o, look, Matrix.Identity(4), 0.0, smooth=False)


def retune(key, big=None):
    """Material-local tweak: change the broad value-variation noise scale."""
    m = lib.mat(key)
    for n in m.node_tree.nodes:
        if n.type == "TEX_NOISE" and abs(n.inputs["Scale"].default_value - 2.2) < 1e-6 and big is not None:
            n.inputs["Scale"].default_value = big
    return m


def tyre(xf, r=0.33, w=0.24, rim=True):
    """A car tyre lying flat (axis along Y) centred at xf."""
    out = [torus(r - w * 0.28, w * 0.5, "tyre", xf @ scl(1, 1.0, 1), seg=16, ring=6, name="tyre")]
    if rim:
        out.append(cyl(r * 0.52, r * 0.52, w * 0.55, 12, "steel_dark", xf, bevel=0.0))
    return out


def drum(xf, look, lying=False):
    """55-gallon oil drum (r 0.29, h 0.88) standing on xf (base at its origin)."""
    f = xf @ (at(0, 0.29, 0, rx(HALF_PI)) @ at(0, 0, 0) if lying else Matrix.Identity(4))
    out = [cyl(0.29, 0.29, 0.88, 16, look, f @ at(0, 0.44, 0), bevel=0.02, name="drum")]
    for y in (0.29, 0.59):
        out.append(cyl(0.3, 0.3, 0.03, 16, look, f @ at(0, y, 0), bevel=0.0))
    out.append(cyl(0.3, 0.3, 0.03, 16, "steel_dark", f @ at(0, 0.875, 0), bevel=0.0))
    out.append(cyl(0.04, 0.04, 0.02, 6, "steel_dark", f @ at(0.17, 0.89, 0.05), bevel=0.0))
    return out


# ------------------------------------------------------------------ turntable
def turntable():
    """Deck top at y = 0 (the game puts it at BENCH_Y + 0.06), base on the floor at y = -0.06."""
    B = []
    # chunky hazard-striped base ring on the floor
    B.append(cyl(1.98, 2.04, 0.04, 48, "hazard", at(0, -0.04, 0), bevel=0.012, name="plinth"))
    # steel deck with a tread rim and a rubber mat in the middle (flat for r < 1.5)
    B.append(cyl(1.82, 1.84, 0.04, 48, "ws_tread", at(0, -0.026, 0), bevel=0.01, name="deckplate"))
    B.append(cyl(1.5, 1.5, 0.012, 48, "ws_mat", at(0, -0.006, 0), bevel=0.0, name="mat"))
    B.append(ring(1.5, 1.54, 0.008, "steel_dark", at(0, -0.006, 0), seg=48))
    # 12 tick marks every 30 degrees, every third yellow (the arrow stands in for the front one)
    for i in range(1, 12):
        a = i / 12 * TAU
        look = "yellow" if i % 3 == 0 else "steel_clean"
        B.append(box(0.045, 0.014, 0.17, look, at(math.sin(a) * 1.64, 0.0, math.cos(a) * 1.64, ry(a)), bevel=0.004, segs=1))
    # deck plate seams and bolts round the rim
    for i in range(12):
        a = (i + 0.5) / 12 * TAU
        B.append(box(0.012, 0.006, 0.26, "ink", at(math.sin(a) * 1.69, -0.004, math.cos(a) * 1.69, ry(a)), bevel=0.0))
        for da in (-0.06, 0.06):
            B.append(cyl(0.02, 0.02, 0.016, 6, "steel_dark", at(math.sin(a + da) * 1.77, -0.004, math.cos(a + da) * 1.77), bevel=0.0))
    # front arrow at +Z
    B.append(cyl(0.2, 0.2, 0.026, 3, "yellow", at(0, -0.002, 1.82, ry(-HALF_PI)), bevel=0.008, name="arrow"))
    B.append(cyl(0.1, 0.1, 0.03, 3, "ink", at(0, 0.0, 1.85, ry(-HALF_PI)), bevel=0.0))
    # lugs bolting the deck to the base, and a cable conduit running off to the side
    for i in range(8):
        a = (i + 0.5) / 8 * TAU
        f = at(math.sin(a) * 1.98, -0.03, math.cos(a) * 1.98, ry(a))
        B.append(box(0.16, 0.05, 0.16, "steel_dark", f, bevel=0.012, segs=2))
        B.append(cyl(0.026, 0.026, 0.02, 6, "steel", f @ at(0, 0.03, 0), bevel=0.0))
    B.append(box(0.18, 0.05, 0.5, "steel_dark", at(-1.4, -0.035, -1.65, ry(-0.7)), bevel=0.015, segs=2))
    glows = [glow(ring(1.505, 1.53, 0.004, "steel_dark", at(0, -0.0045, 0), seg=48), "glow_ring")]
    return dict(groups={"deck": B}, glows=glows, tex=1024)


# ------------------------------------------------------------------ floor
def floor():
    """24 x 24 m concrete slab, top at y = 0, with paint, stains and drains."""
    retune("ws_slab", big=0.45)
    retune("ws_stain", big=0.8)
    F = [plane(24.0, 24.0, "ws_slab", Matrix.Identity(4), name="slab")]
    # saw-cut joints on a 6 m grid (not under the turntable)
    for p in (-6.0, 6.0):
        F.append(box(0.03, 0.004, 24.0, "ws_joint", at(p, 0.0, 0), bevel=0.0))
        F.append(box(24.0, 0.004, 0.03, "ws_joint", at(0, 0.0, p), bevel=0.0))
    # yellow rings painted round the turntable
    F.append(ring(2.3, 2.42, 0.004, "ws_line", Matrix.Identity(4), seg=72))
    F.append(ring(2.58, 2.64, 0.004, "ws_line", Matrix.Identity(4), seg=72))
    # a painted lane from the turntable out to the roller door
    for sx in (-1, 1):
        F.append(box(0.12, 0.006, 8.0, "ws_line", at(sx * 2.4, 0.0, 7.6), bevel=0.003))
    F.append(box(4.9, 0.006, 0.12, "ws_line", at(0, 0.0, 3.6), bevel=0.003))
    # keep-clear hatching in front of the door
    F.append(box(5.0, 0.006, 1.2, "ws_floor_hazard", at(0, 0.0, 10.9), bevel=0.003))
    # bay boxes along the sides, white lines
    for sx in (-1, 1):
        x0 = sx * 7.5
        F.append(box(0.1, 0.006, 6.0, "ws_line_w", at(x0, 0.0, 0.0), bevel=0.003))
        F.append(box(3.6, 0.006, 0.1, "ws_line_w", at(x0 + sx * 1.8, 0.0, 3.0), bevel=0.003))
        F.append(box(3.6, 0.006, 0.1, "ws_line_w", at(x0 + sx * 1.8, 0.0, -3.0), bevel=0.003))
    F.append(box(6.0, 0.006, 0.1, "ws_line_w", at(0.0, 0.0, -7.6), bevel=0.003))
    # oil stains, drips and dirty patches
    stains = [
        (3.3, 2.1, 0.55, "ws_oil", 1), (-3.6, -2.4, 0.35, "ws_oil", 2), (0.7, -9.6, 0.9, "ws_oil", 3),
        (-9.4, -8.6, 0.8, "ws_oil", 4), (8.6, -4.4, 0.55, "ws_oil", 5), (-1.2, 6.8, 0.45, "ws_oil", 6),
        (5.8, 8.6, 0.7, "ws_oil", 7), (-7.2, 4.6, 0.4, "ws_oil", 8),
        (-5.0, -6.0, 2.2, "ws_stain", 9), (6.5, -1.0, 1.8, "ws_stain", 10), (-2.0, 9.0, 2.0, "ws_stain", 11),
        (8.5, 8.0, 2.4, "ws_stain", 12), (-9.0, 1.0, 1.7, "ws_stain", 13), (2.0, -4.6, 1.4, "ws_stain", 14),
    ]
    for x, z, r, look, s in stains:
        y = 0.0012 if look == "ws_stain" else 0.0024
        F.append(blob(r, look, at(x, y, z, ry(s * 1.3)), seed=s))
        if look == "ws_oil":  # a few drips around each puddle
            for j in range(3):
                a = s * 2.1 + j * 2.3
                d = r * (1.3 + 0.25 * j)
                F.append(blob(r * 0.16, look, at(x + math.cos(a) * d, y, z + math.sin(a) * d), seed=s * 3 + j, n=12))
    # skid marks: bots doing donuts round the turntable and wheelspins at the door
    for cx, cz, rad, a0, a1 in ((0.4, 0.3, 3.5, 0.3, 2.4), (0.4, 0.3, 3.85, 0.25, 2.2), (-0.5, -0.2, 4.4, 3.4, 5.0),
                                (-0.5, -0.2, 4.75, 3.5, 4.9), (5.0, 9.0, 3.2, 2.6, 3.6), (5.0, 9.0, 3.55, 2.65, 3.5)):
        F.append(arc_strip(cx, cz, rad, a0, a1, 0.2, "ws_oil"))
    # square floor drain near the turntable
    dx, dz = 3.4, -3.1
    F.append(box(0.62, 0.012, 0.62, "steel_dark", at(dx, 0.0, dz), bevel=0.004))
    F.append(box(0.5, 0.004, 0.5, "ink", at(dx, 0.006, dz), bevel=0.0))
    for i in range(6):
        F.append(box(0.035, 0.012, 0.5, "steel", at(dx - 0.21 + i * 0.084, 0.006, dz), bevel=0.0))
    F.append(blob(0.6, "ws_stain", at(dx, 0.0011, dz), seed=21))
    # trench drain across the door
    F.append(box(5.4, 0.012, 0.3, "steel_dark", at(0, 0.0, 11.65), bevel=0.004))
    F.append(box(5.3, 0.004, 0.22, "ink", at(0, 0.006, 11.65), bevel=0.0))
    for i in range(36):
        F.append(box(0.03, 0.012, 0.22, "steel", at(-2.62 + i * 0.15, 0.006, 11.65), bevel=0.0))
    return dict(groups={"floor": F}, tex=2048)


# ------------------------------------------------------------------ walls
# Each wall is modelled in a local frame where it runs along X at z = -12
# (facing +Z, into the room); SIDES rotates it into place.
SIDES = {"back": 0.0, "left": HALF_PI, "front": math.pi, "right": -HALF_PI}  # -Z, -X, +Z, +X walls
PANELS = ["ws_tin", "ws_tin", "ws_tin_red", "ws_tin", "ws_tin", "ws_tin_cream", "ws_tin", "ws_tin",
          "ws_tin_teal", "ws_tin", "ws_tin", "ws_tin_rust"]


def column(F, x, hazard=False):
    """I-beam column against the wall at local x."""
    out = []
    z = -WALL + 0.12
    for dz in (-0.1, 0.1):
        out.append(box(0.26, 5.5, 0.035, "girder", F @ at(x, 2.75, z + dz), bevel=0.01, segs=2))
    out.append(box(0.035, 5.5, 0.2, "girder", F @ at(x, 2.75, z), bevel=0.0))
    out.append(box(0.4, 0.04, 0.4, "steel_dark", F @ at(x, 0.02, z), bevel=0.01, segs=2))
    for sx in (-1, 1):
        for sz in (-1, 1):
            out.append(cyl(0.025, 0.025, 0.03, 6, "steel_dark", F @ at(x + sx * 0.15, 0.05, z + sz * 0.15), bevel=0.0))
    if hazard:
        out.append(box(0.32, 1.6, 0.3, "hazard", F @ at(x, 0.84, z), bevel=0.02, segs=2))
    return out


def strip_light(F, x, z, y, name, length=1.6):
    """Fluorescent strip hung on chains from a wall bracket; returns (meshes, glow)."""
    out = [rod(F @ v(x, y + 0.75, -WALL), F @ v(x, y + 0.75, z), 0.025, "steel_dark", 6)]
    out.append(rod(F @ v(x, y + 0.45, -WALL), F @ v(x, y + 0.75, -WALL + 0.5 * (z + WALL)), 0.018, "steel_dark", 6))
    for sx in (-1, 1):
        out.append(rod(F @ v(x + sx * 0.12, y + 0.75, z), F @ v(x + sx * length * 0.4, y + 0.06, z), 0.008, "steel_dark", 4))
    out.append(box(length, 0.07, 0.2, "cream", F @ at(x, y + 0.03, z), bevel=0.015, segs=2))
    out.append(box(length + 0.04, 0.03, 0.06, "steel_dark", F @ at(x, y + 0.075, z), bevel=0.005, segs=1))
    g = glow_box(length - 0.1, 0.05, 0.08, name, F @ at(x, y - 0.025, z))
    return out, g


def wall(side, door=False):
    F = ry(SIDES[side])
    W = []
    # concrete upstand
    for x0, x1 in ((-12.3, -2.0), (2.0, 12.3)) if door else ((-12.3, 12.3),):
        W.append(box(x1 - x0, 0.7, 0.4, "ws_concrete", F @ at((x0 + x1) / 2, 0.35, -WALL - 0.2), bevel=0.03, segs=2))
    # corrugated sheets, a patchwork of salvaged colours
    off = {"back": 0, "left": 3, "front": 6, "right": 9}[side]
    for i in range(12):
        x = -11 + i * 2
        look = PANELS[(i + off) % 12]
        if door and abs(x) < 2:
            # header sheet above the door opening
            W.append(corrugated(2.04, 1.4, look, F @ at(x, 4.7, -WALL - 0.24)))
            continue
        tilt = ((i * 7 + off) % 5 - 2) * 0.004
        W.append(corrugated(2.04, 4.9, look, F @ at(x, 2.95, -WALL - 0.24 - i % 2 * 0.012) @ rz(tilt)))
    # top plate, columns and knee braces
    W.append(box(24.4, 0.26, 0.2, "girder", F @ at(0, 5.45, -WALL + 0.0), bevel=0.02, segs=2))
    for x in (-11.75, -4.0, 4.0):
        W += column(F, x, hazard=door and abs(x) == 4.0)
    for x in (-4.0, 4.0):
        for sx in (-1, 1):
            W.append(rod(F @ v(x + sx * 0.12, 4.4, -WALL + 0.12), F @ v(x + sx * 1.0, 5.32, -WALL + 0.06), 0.035, "girder", 6))
    return F, W


def door_parts(F):
    """Roller door at local x 0 (the +Z wall), left a little open at the bottom."""
    D, C = [], []  # frame / curtain
    hw = DOOR_W / 2
    z = -WALL - 0.12
    gap = 0.32
    n = 25
    sh = (DOOR_H - gap) / n
    for i in range(n):
        y = gap + (i + 0.5) * sh
        C.append(box(DOOR_W, sh * 0.9, 0.05, "ws_door", F @ at(0, y, z) @ rx(0.06), bevel=0.012, segs=2, name="slat"))
    C.append(box(DOOR_W, 0.08, 0.09, "steel_dark", F @ at(0, gap + 0.04, z + 0.01), bevel=0.012, segs=2))
    for sx in (-0.6, 0.6):
        C.append(box(0.18, 0.05, 0.05, "steel", F @ at(sx, gap + 0.16, z + 0.06), bevel=0.012, segs=1))
    for sx in (-1, 1):
        D.append(box(0.14, DOOR_H + 0.3, 0.2, "steel_dark", F @ at(sx * (hw + 0.07), (DOOR_H + 0.3) / 2, z), bevel=0.015, segs=2))
        # hazard bollards guarding the opening
        D.append(cyl(0.12, 0.12, 1.0, 16, "hazard", F @ at(sx * (hw + 0.55), 0.5, -WALL + 0.35), bevel=0.02))
        D.append(cyl(0.13, 0.12, 0.08, 16, "ink", F @ at(sx * (hw + 0.55), 1.02, -WALL + 0.35), bevel=0.02))
    # roll drum housing and the chain
    D.append(box(DOOR_W + 0.6, 0.62, 0.55, "steel_dark", F @ at(0, DOOR_H + 0.42, -WALL + 0.1), bevel=0.03, segs=2))
    D.append(box(DOOR_W + 0.64, 0.12, 0.58, "ws_door", F @ at(0, DOOR_H + 0.2, -WALL + 0.1), bevel=0.02, segs=2))
    D.append(rod(F @ v(hw + 0.3, DOOR_H + 0.2, -WALL + 0.4), F @ v(hw + 0.3, 1.3, -WALL + 0.4), 0.015, "steel", 4))
    # a sign over the door
    D.append(box(3.2, 0.6, 0.06, "cream", F @ at(0, 5.0, -WALL + 0.42), bevel=0.02, segs=2))
    D.append(box(3.0, 0.16, 0.02, "red", F @ at(0, 5.0, -WALL + 0.46), bevel=0.005, segs=1))
    for sx in (-1, 1):
        D.append(cyl(0.03, 0.03, 0.02, 6, "steel_dark", F @ at(sx * 1.45, 5.22, -WALL + 0.46, rx(HALF_PI)), bevel=0.0))
    # daylight leaking under the door
    g = glow_box(DOOR_W - 0.1, gap, 0.02, "glow_door_gap", F @ at(0, gap / 2, z - 0.06))
    return D, C, g


def window(F, x, y, w=2.6, h=0.9):
    z = -WALL - 0.17
    out = [box(w + 0.12, h + 0.12, 0.08, "steel_dark", F @ at(x, y, z), bevel=0.02, segs=2)]
    out.append(box(w, h, 0.04, "ws_window", F @ at(x, y, z + 0.03), bevel=0.0))
    for i in range(1, 4):
        out.append(box(0.04, h, 0.06, "steel_dark", F @ at(x - w / 2 + i * w / 4, y, z + 0.05), bevel=0.0))
    out.append(box(w, 0.04, 0.06, "steel_dark", F @ at(x, y, z + 0.05), bevel=0.0))
    return out


def walls(side):
    """One wall of the shed. The front one carries the roller door as its own
    group, so the game can slide it up (along +Y) when the bot heads out."""
    F, W = wall(side, door=side == "front")
    groups, glows = {"wall": W}, []
    lights = [(-7.0, 0), (7.0, 1)] if side == "front" else [(0.0, None)]
    for x, i in lights:
        m, g = strip_light(F, x, -WALL + 0.9, 3.9, f"glow_strip_{side}" + ("" if i is None else f"_{i}"))
        W += m
        glows.append(g)
    if side == "front":
        D, C, g = door_parts(F)
        W += D
        groups["door"] = C
        glows.append(g)
    if side in ("left", "right"):
        for x in (-7.8, 7.8):
            W += window(F, x, 4.2)
    return dict(groups=groups, glows=glows, tex=1024)


# ------------------------------------------------------------------ workbench corner (back wall, -Z)
def wrench(xf, l=0.32, look="steel_clean"):
    out = [box(0.035, l, 0.012, look, xf, bevel=0.004, segs=1)]
    for sy in (-1, 1):
        out.append(cyl(0.04, 0.04, 0.014, 10, look, xf @ at(0, sy * l / 2, 0, rx(HALF_PI)), bevel=0.0))
    return out


def hammer_tool(xf, look="red"):
    return [box(0.04, 0.34, 0.03, "wood", xf, bevel=0.008, segs=1),
            box(0.16, 0.05, 0.05, "steel_dark", xf @ at(0, 0.18, 0), bevel=0.01, segs=1),
            box(0.045, 0.12, 0.035, look, xf @ at(0, -0.12, 0), bevel=0.01, segs=1)]


def screwdriver(xf, look):
    return [cyl(0.022, 0.022, 0.12, 8, look, xf @ at(0, -0.06, 0), bevel=0.0),
            cyl(0.006, 0.006, 0.16, 6, "steel_clean", xf @ at(0, 0.08, 0), bevel=0.0)]


def saw(xf):
    blade = cyl(0.34, 0.34, 0.006, 3, "blade", Matrix.Identity(4), bevel=0.0, name="saw")
    blade.data.transform(xf @ at(0.12, 0, 0) @ rx(HALF_PI) @ scl(1.0, 1.0, 0.45))
    return [blade, box(0.12, 0.16, 0.03, "wood_dark", xf @ at(-0.22, 0, 0), bevel=0.01, segs=1)]


def shadowed(tools, xf, w, h):
    """Tool hanging on the pegboard over its painted outline."""
    return [box(w, h, 0.004, "ws_shadow", xf @ at(0, 0, -0.02), bevel=0.0)] + tools


def bench():
    z0 = -WALL
    B, T = [], []  # bench furniture / tools + clutter
    # pegboard with trim and a shelf above
    pz = z0 + 0.03
    B.append(box(4.4, 1.7, 0.03, "ws_board", at(0, 1.85, pz), bevel=0.008, segs=1, name="pegboard"))
    for y in (1.0, 2.7):
        B.append(box(4.5, 0.06, 0.05, "wood_dark", at(0, y, pz + 0.02), bevel=0.01, segs=1))
    for x in (-2.22, 2.22):
        B.append(box(0.06, 1.76, 0.05, "wood_dark", at(x, 1.85, pz + 0.02), bevel=0.01, segs=1))
    B.append(box(4.4, 0.05, 0.36, "wood", at(0, 3.0, z0 + 0.2), bevel=0.01, segs=1))
    for x in (-1.8, 0.0, 1.8):
        B.append(box(0.04, 0.3, 0.03, "steel_dark", at(x, 2.84, z0 + 0.05), bevel=0.006, segs=1))
        B.append(rodf((x, 2.72, z0 + 0.05), (x, 2.97, z0 + 0.32), 0.012, "steel_dark", 6))
    # the bench: thick wooden top on a red steel frame, lower shelf, drawers
    bz = z0 + 0.45
    B.append(box(4.2, 0.08, 0.86, "wood_dark", at(0, 0.92, bz), bevel=0.02, segs=2, name="benchtop"))
    B.append(box(4.22, 0.06, 0.05, "steel_dark", at(0, 0.88, bz + 0.43), bevel=0.01, segs=1))
    for sx in (-1, 1):
        for sz in (-1, 1):
            B.append(box(0.07, 0.86, 0.07, "red_dark", at(sx * 2.0, 0.44, bz + sz * 0.36), bevel=0.012, segs=1))
        B.append(box(0.06, 0.06, 0.74, "red_dark", at(sx * 2.0, 0.2, bz), bevel=0.01, segs=1))
    B.append(box(4.0, 0.04, 0.72, "wood", at(0, 0.24, bz), bevel=0.01, segs=1))
    B.append(box(0.8, 0.62, 0.78, "red", at(1.5, 0.56, bz), bevel=0.02, segs=2, name="drawers"))
    for i in range(3):
        y = 0.38 + i * 0.19
        B.append(box(0.72, 0.15, 0.02, "red_dark", at(1.5, y, bz + 0.395), bevel=0.006, segs=1))
        B.append(box(0.3, 0.025, 0.03, "steel_clean", at(1.5, y + 0.03, bz + 0.415), bevel=0.006, segs=1))
    # stuff on the lower shelf: a crate and a coil of hose
    B.append(box(0.6, 0.3, 0.45, "wood", at(-1.2, 0.41, bz - 0.05, ry(0.1)), bevel=0.02, segs=1))
    T.append(torus(0.2, 0.05, "red_cable", at(-0.3, 0.31, bz), seg=16, ring=6))
    T.append(torus(0.17, 0.045, "red_cable", at(-0.28, 0.4, bz + 0.02), seg=16, ring=6))
    # vice on the left end
    vx, vy, vz = -1.6, 0.96, bz + 0.3
    T.append(box(0.24, 0.06, 0.3, "ws_blue", at(vx, vy + 0.03, vz), bevel=0.01, segs=1))
    T.append(box(0.2, 0.16, 0.12, "ws_blue", at(vx, vy + 0.14, vz - 0.08), bevel=0.02, segs=2))
    T.append(box(0.2, 0.16, 0.1, "ws_blue", at(vx, vy + 0.14, vz + 0.1), bevel=0.02, segs=2))
    for dz in (-0.015, 0.035):
        T.append(box(0.22, 0.05, 0.02, "steel_dark", at(vx, vy + 0.2, vz + dz), bevel=0.004, segs=1))
    T.append(rodf((vx, vy + 0.12, vz + 0.1), (vx, vy + 0.12, vz + 0.36), 0.018, "steel_clean", 8))
    T.append(rodf((vx - 0.16, vy + 0.12, vz + 0.36), (vx + 0.16, vy + 0.12, vz + 0.36), 0.012, "steel_clean", 6))
    for sx in (-0.16, 0.16):
        T.append(sphere_(0.022, "steel_clean", at(vx + sx, vy + 0.12, vz + 0.36)))
    T.append(box(0.05, 0.12, 0.2, "steel_dark", at(vx, vy + 0.24, vz + 0.02, rz(0.2)), bevel=0.008, segs=1))
    # bench clutter: toolbox, tins, oil can, rag, mug, a half-built bot head, a lamp
    T.append(box(0.56, 0.22, 0.26, "red", at(0.75, 1.07, bz - 0.15, ry(-0.12)), bevel=0.02, segs=2, name="toolbox"))
    T.append(box(0.58, 0.04, 0.27, "red_dark", at(0.75, 1.19, bz - 0.15, ry(-0.12)), bevel=0.01, segs=1))
    T.append(rodf((0.6, 1.21, bz - 0.13), (0.6, 1.3, bz - 0.13), 0.012, "ink", 6))
    T.append(rodf((0.9, 1.21, bz - 0.17), (0.9, 1.3, bz - 0.17), 0.012, "ink", 6))
    T.append(rodf((0.6, 1.3, bz - 0.13), (0.9, 1.3, bz - 0.17), 0.014, "ink", 6))
    for i, (x, zz, look, r, h) in enumerate(((1.35, -0.25, "ws_tin_teal", 0.07, 0.16), (1.5, -0.2, "yellow", 0.06, 0.12),
                                             (1.42, -0.06, "ws_tin_red", 0.075, 0.2), (-0.6, 0.15, "steel", 0.05, 0.1))):
        T.append(cyl(r, r, h, 12, look, at(x, 0.96 + h / 2, bz + zz), bevel=0.0))
        T.append(cyl(r * 1.02, r * 1.02, 0.015, 12, "steel_dark", at(x, 0.96 + h - 0.005, bz + zz), bevel=0.0))
    T.append(cyl(0.02, 0.09, 0.14, 12, "yellow", at(-0.9, 1.03, bz - 0.2), bevel=0.0))
    T.append(cyl(0.008, 0.018, 0.2, 6, "steel_dark", at(-0.86, 1.18, bz - 0.2, rz(-0.5)), bevel=0.0))
    T.append(lump(0.12, "cloth", at(-0.15, 0.99, bz + 0.15) @ scl(1.4, 0.35, 1.0), seed=0, rough=0.4, subdiv=2))  # lib.lump misplaces seed != 0
    T.append(cyl(0.045, 0.04, 0.1, 12, "cream", at(0.2, 1.01, bz + 0.25), bevel=0.0))
    T.append(torus(0.03, 0.01, "cream", at(0.25, 1.01, bz + 0.25, rx(HALF_PI)), seg=10, ring=6))
    # bot head on the bench, waiting for its eyes
    hx = -0.3
    T.append(box(0.36, 0.28, 0.36, "ws_tin_teal", at(hx, 1.1, bz - 0.12, ry(0.35)), bevel=0.03, segs=2))
    T.append(box(0.3, 0.16, 0.04, "ink", at(hx, 1.12, bz - 0.12, ry(0.35)) @ at(0, 0, 0.18), bevel=0.012, segs=1))
    for sx in (-0.07, 0.07):
        T.append(cyl(0.05, 0.05, 0.04, 12, "steel_dark", at(hx, 1.13, bz - 0.12, ry(0.35)) @ at(sx, 0, 0.2, rx(HALF_PI)), bevel=0.0))
    T.append(cable([v(hx + 0.1, 1.2, bz - 0.25), v(hx + 0.35, 1.25, bz - 0.3), v(hx + 0.5, 0.98, bz - 0.2)], 0.012, "blue_cable", res=6))
    # angle-poise lamp at the right end
    lx, lz = 1.95, bz - 0.25
    T.append(cyl(0.09, 0.1, 0.03, 12, "ink", at(lx, 0.975, lz), bevel=0.0))
    T.append(rodf((lx, 0.99, lz), (lx - 0.1, 1.4, lz + 0.05), 0.012, "ink", 6))
    T.append(rodf((lx - 0.1, 1.4, lz + 0.05), (lx - 0.38, 1.36, lz + 0.18), 0.012, "ink", 6))
    T.append(cyl(0.035, 0.11, 0.14, 12, "yellow", at(lx - 0.42, 1.31, lz + 0.2, rx(0.5) @ rz(-0.4)), bevel=0.0))
    # tools on the pegboard, each over its painted outline
    ty = 1.85
    for i, x in enumerate((-1.95, -1.85, -1.75, -1.65)):
        T += shadowed(wrench(at(x, ty + 0.3 - i * 0.04, pz + 0.04), 0.26 + i * 0.05), at(x, ty + 0.3 - i * 0.04, pz + 0.04), 0.09, 0.36 + i * 0.05)
    for i, x in enumerate((-1.3, -1.12)):
        T += shadowed(hammer_tool(at(x, ty + 0.25, pz + 0.04), "red" if i else "yellow"), at(x, ty + 0.25, pz + 0.04), 0.2, 0.42)
    for i, (x, look) in enumerate(((-0.75, "red"), (-0.66, "yellow"), (-0.57, "ws_blue"), (-0.48, "red"), (-0.39, "ws_gas"))):
        T += screwdriver(at(x, ty + 0.45, pz + 0.05), look)
    T.append(box(0.5, 0.03, 0.06, "steel_dark", at(-0.57, ty + 0.3, pz + 0.05), bevel=0.005, segs=1))
    T += shadowed(saw(at(0.0, ty + 0.45, pz + 0.04)), at(0.05, ty + 0.45, pz + 0.04), 0.7, 0.24)
    T.append(torus(0.18, 0.025, "blue_cable", at(0.65, ty + 0.25, pz + 0.07, rx(HALF_PI)), seg=16, ring=6))
    T.append(torus(0.14, 0.025, "blue_cable", at(0.66, ty + 0.22, pz + 0.1, rx(HALF_PI)), seg=16, ring=6))
    for i in range(3):  # G-clamps
        f = at(1.1 + i * 0.2, ty + 0.2, pz + 0.05)
        T += shadowed([box(0.04, 0.3, 0.03, "orange", f, bevel=0.008, segs=1),
                       box(0.14, 0.04, 0.03, "orange", f @ at(0.06, 0.15, 0), bevel=0.008, segs=1),
                       box(0.14, 0.04, 0.03, "orange", f @ at(0.06, -0.15, 0), bevel=0.008, segs=1),
                       rodf(f @ v(0.11, -0.25, 0), f @ v(0.11, 0.05, 0), 0.01, "steel_clean", 6)], f @ at(0.05, 0, 0), 0.2, 0.36)
    T += shadowed([box(0.5, 0.05, 0.02, "steel_clean", at(1.8, ty + 0.55, pz + 0.04), bevel=0.004, segs=1),
                   box(0.05, 0.3, 0.02, "steel_clean", at(1.58, ty + 0.42, pz + 0.04), bevel=0.004, segs=1)], at(1.72, ty + 0.5, pz + 0.04), 0.6, 0.4)
    for x in (-2.0, -1.4, -0.9, -0.2, 0.4, 1.0, 1.7):  # pegs
        T.append(rodf((x, ty + 0.62, pz), (x, ty + 0.64, pz + 0.08), 0.008, "steel_dark", 4))
    # shelf clutter: paint tins, jars, a box of bolts, a spare wheel
    for i, x in enumerate((-1.9, -1.7, -1.5, -0.6, -0.45, 1.3, 1.45)):
        look = ("ws_tin_red", "yellow", "ws_tin_teal", "cream", "ws_blue", "orange", "ws_tin_red")[i]
        h = 0.14 + (i * 37 % 5) * 0.025
        T.append(cyl(0.07, 0.07, h, 12, look, at(x, 3.025 + h / 2, z0 + 0.2), bevel=0.0))
    for i, x in enumerate((-1.05, -0.85)):
        T.append(box(0.32, 0.22 + i * 0.06, 0.28, "ws_card", at(x - i * 0.25 + 0.15, 3.025 + 0.11 + i * 0.03, z0 + 0.2, ry(0.1 - i * 0.2)), bevel=0.01, segs=1))
    T.append(cyl(0.2, 0.2, 0.12, 16, "tyre", at(0.4, 3.24, z0 + 0.2, rx(HALF_PI)), bevel=0.04))
    T.append(cyl(0.11, 0.11, 0.13, 12, "yellow", at(0.4, 3.24, z0 + 0.2, rx(HALF_PI)), bevel=0.0))
    T.append(box(0.36, 0.1, 0.22, "steel_dark", at(0.95, 3.075, z0 + 0.2), bevel=0.01, segs=1))
    # stool
    sx_, sz_ = 0.7, -10.75
    T.append(cyl(0.2, 0.2, 0.06, 16, "red", at(sx_, 0.7, sz_), bevel=0.02))
    for i in range(3):
        a = i / 3 * TAU + 0.3
        T.append(rodf((sx_ + math.cos(a) * 0.1, 0.68, sz_ + math.sin(a) * 0.1), (sx_ + math.cos(a) * 0.24, 0.0, sz_ + math.sin(a) * 0.24), 0.018, "steel_dark", 6))
    T.append(torus(0.17, 0.012, "steel_dark", at(sx_, 0.25, sz_), seg=16, ring=5))
    # tall tool chest on castors right of the bench
    cx = 2.85
    B.append(box(0.8, 1.15, 0.55, "red", at(cx, 0.68, z0 + 0.32), bevel=0.025, segs=2, name="chest"))
    for i in range(6):
        y = 0.25 + i * 0.165 + (0.1 if i == 5 else 0)
        B.append(box(0.72, 0.13, 0.02, "red_dark", at(cx, y + 0.05, z0 + 0.6), bevel=0.005, segs=1))
        B.append(box(0.4, 0.02, 0.03, "steel_clean", at(cx, y + 0.09, z0 + 0.615), bevel=0.005, segs=1))
    B.append(box(0.82, 0.04, 0.57, "ink", at(cx, 1.27, z0 + 0.32), bevel=0.01, segs=1))
    for sx in (-0.33, 0.33):
        for sz in (0.1, 0.52):
            B.append(cyl(0.05, 0.05, 0.04, 10, "ink", at(cx + sx, 0.05, z0 + sz, rz(HALF_PI)), bevel=0.0))
    # welding cart left of the bench (beyond the column)
    wx, wz = -5.6, z0 + 0.6
    W_ = []
    W_.append(box(0.6, 0.04, 0.5, "steel_dark", at(wx, 0.25, wz), bevel=0.01, segs=1))
    W_.append(box(0.6, 0.04, 0.5, "steel_dark", at(wx, 0.75, wz), bevel=0.01, segs=1))
    for sx in (-0.28, 0.28):
        W_.append(rodf((wx + sx, 0.12, wz + 0.22), (wx + sx, 1.1, wz + 0.22), 0.015, "steel_dark", 6))
        W_.append(rodf((wx + sx, 0.12, wz - 0.22), (wx + sx, 0.8, wz - 0.22), 0.015, "steel_dark", 6))
        W_.append(cyl(0.12, 0.12, 0.05, 14, "tyre", at(wx + sx * 1.15, 0.12, wz - 0.15, rz(HALF_PI)), bevel=0.015))
    W_.append(rodf((wx - 0.28, 1.1, wz + 0.22), (wx + 0.28, 1.1, wz + 0.22), 0.015, "steel_dark", 6))
    W_.append(box(0.46, 0.34, 0.4, "ws_blue", at(wx, 0.94, wz - 0.02), bevel=0.025, segs=2, name="welder_box"))
    W_.append(box(0.3, 0.16, 0.02, "ink", at(wx - 0.02, 0.96, wz + 0.19), bevel=0.005, segs=1))
    for dx in (-0.08, 0.06):
        W_.append(cyl(0.035, 0.035, 0.03, 10, "cream", at(wx + dx, 0.97, wz + 0.2, rx(HALF_PI)), bevel=0.0))
    W_.append(cyl(0.12, 0.12, 1.3, 16, "ws_gas", at(wx + 0.05, 0.72, wz - 0.4), bevel=0.04))
    W_.append(sphere_(0.12, "ws_gas", at(wx + 0.05, 1.37, wz - 0.4)))
    W_.append(cyl(0.04, 0.04, 0.12, 8, "yellow", at(wx + 0.05, 1.5, wz - 0.4), bevel=0.0))
    W_.append(cyl(0.04, 0.04, 0.04, 8, "steel_clean", at(wx + 0.12, 1.5, wz - 0.4, rz(HALF_PI)), bevel=0.0))
    W_.append(cable([v(wx + 0.2, 0.9, wz + 0.2), v(wx + 0.5, 0.6, wz + 0.5), v(wx + 0.3, 0.02, wz + 0.7), v(wx - 0.3, 0.02, wz + 0.55)], 0.018, "ink", res=6))
    W_.append(torus(0.16, 0.025, "ink", at(wx, 1.13, wz + 0.05), seg=14, ring=6))
    # welding mask hung on the cart handle
    W_.append(box(0.2, 0.24, 0.12, "ink", at(wx - 0.2, 0.98, wz + 0.3, rx(0.2)), bevel=0.04, segs=2))
    W_.append(box(0.1, 0.05, 0.02, "ws_window", at(wx - 0.2, 1.02, wz + 0.365, rx(0.2)), bevel=0.005, segs=1))
    glows = [glow(sphere_(0.045, "cream", at(lx - 0.44, 1.27, lz + 0.23), seg=10, rings=5), "glow_lamp")]
    glows[0].data.name = "glow_lamp"
    return dict(groups={"bench": B, "bench_clutter": T, "welder": W_}, glows=glows, tex=1024)


# ------------------------------------------------------------------ props round the other walls
def rnd(seed):
    """Tiny deterministic PRNG -> function returning floats in [0, 1)."""
    st = [seed * 2654435761 % 4294967296 or 1]

    def f():
        st[0] = (st[0] * 1103515245 + 12345) % 2147483648
        return st[0] / 2147483648
    return f


def rack(F, x0, seed):
    """Steel racking with four wooden shelves of junk, against the wall at local x0."""
    R = rnd(seed)
    out = []
    zc = -WALL + 0.36
    for sx in (-0.95, 0.95):
        for dz in (-0.27, 0.27):
            out.append(box(0.07, 2.4, 0.05, "orange", F @ at(x0 + sx, 1.2, zc + dz), bevel=0.0))
        out.append(rodf(F @ v(x0 + sx, 0.3, zc - 0.27), F @ v(x0 + sx, 1.0, zc + 0.27), 0.012, "orange", 4))
        out.append(rodf(F @ v(x0 + sx, 1.0, zc - 0.27), F @ v(x0 + sx, 1.7, zc + 0.27), 0.012, "orange", 4))
    for y in (0.16, 0.78, 1.4, 2.02):
        out.append(box(1.96, 0.04, 0.6, "wood", F @ at(x0, y, zc), bevel=0.0))
        for dz in (-0.29, 0.29):
            out.append(box(1.96, 0.06, 0.03, "orange", F @ at(x0, y - 0.03, zc + dz), bevel=0.0))
        # fill the shelf left to right
        x = x0 - 0.9
        while x < x0 + 0.75:
            k = R()
            if k < 0.45:  # cardboard box
                w, h, d = 0.25 + R() * 0.25, 0.18 + R() * 0.24, 0.3 + R() * 0.2
                out.append(box(w, h, d, "ws_card", F @ at(x + w / 2, y + 0.02 + h / 2, zc + (R() - 0.5) * 0.08, ry((R() - 0.5) * 0.3)), bevel=0.0))
                out.append(box(w * 0.3, 0.004, d + 0.01, "cream", F @ at(x + w / 2, y + 0.022 + h, zc), bevel=0.0))
            elif k < 0.7:  # a row of paint tins
                w = 0.0
                for j in range(1 + int(R() * 3)):
                    r, h = 0.07, 0.12 + R() * 0.1
                    look = ("ws_tin_red", "yellow", "ws_tin_teal", "cream", "ws_blue", "orange")[int(R() * 6)]
                    out.append(cyl(r, r, h, 8, look, F @ at(x + w + r, y + 0.02 + h / 2, zc + 0.1 - j % 2 * 0.15), bevel=0.0, smooth_sides=False))
                    w += 0.16
            elif k < 0.85:  # jerry can
                w = 0.17
                out.append(box(w, 0.36, 0.4, "red", F @ at(x + w / 2, y + 0.2, zc), bevel=0.025, segs=1))
                out.append(box(0.04, 0.05, 0.16, "red_dark", F @ at(x + w / 2, y + 0.4, zc), bevel=0.01, segs=1))
            else:  # spare wheel standing up
                w = 0.2
                out.append(cyl(0.22, 0.22, 0.14, 14, "tyre", F @ at(x + 0.1, y + 0.24, zc, rz(HALF_PI)), bevel=0.04))
                out.append(cyl(0.12, 0.12, 0.15, 10, "yellow", F @ at(x + 0.1, y + 0.24, zc, rz(HALF_PI)), bevel=0.0))
            x += w + 0.05 + R() * 0.08
    return out


def tyre_stack(xf, n, seed):
    R = rnd(seed)
    out = []
    for i in range(n):
        f = xf @ at((R() - 0.5) * 0.08, 0.12 + i * 0.235, (R() - 0.5) * 0.08, ry(R() * TAU) @ rx((R() - 0.5) * 0.06))
        out += tyre(f, rim=R() < 0.4)
    return out


def props():
    """Racking along the side walls, tyres, the bonnet and the engine on a pallet."""
    S, T = [], []  # shelves / tyres / drums / scrap
    left, right, front = ry(HALF_PI), ry(-HALF_PI), ry(math.pi)
    # racking along the left (-X) wall either side of its centre
    S += rack(left, -1.6, 3)
    S += rack(left, 1.6, 8)
    S += rack(right, -7.0, 5)
    # tyres along the right (+X) wall
    for x, n, seed in ((-2.6, 5, 1), (-1.75, 4, 2), (-2.2, 2, 4)):
        zoff = 0.0 if seed != 4 else 0.75
        T += tyre_stack(right @ at(x, 0, -WALL + 0.45 + zoff), n, seed)
    T += tyre(right @ at(-0.9, 0.38, -WALL + 0.3) @ rx(1.25) @ rz(0.1))
    T += tyre(right @ at(-0.55, 0.36, -WALL + 0.32) @ rx(1.3) @ rz(-0.15), rim=False)
    T += tyre(at(-9.8, 0.12, 11.0))
    # an old car bonnet leaning on the right wall, an engine on a pallet
    T.append(box(1.5, 0.05, 1.2, "ws_tin_teal", right @ at(1.3, 0.62, -WALL + 0.3) @ rx(1.32), bevel=0.03, segs=2))
    T.append(box(1.3, 0.02, 0.9, "steel_dark", right @ at(1.3, 0.6, -WALL + 0.27) @ rx(1.32), bevel=0.0))
    ex, ez = 6.2, -WALL + 0.75
    for i in range(5):
        T.append(box(0.12, 0.03, 1.1, "wood", right @ at(ex - 0.5 + i * 0.25, 0.13, ez), bevel=0.0))
    for dz in (-0.45, 0.0, 0.45):
        T.append(box(1.2, 0.1, 0.12, "wood_dark", right @ at(ex, 0.06, ez + dz), bevel=0.0))
    T.append(box(0.7, 0.5, 0.5, "steel_dark", right @ at(ex, 0.4, ez), bevel=0.04, segs=2))
    for i in range(4):
        T.append(cyl(0.08, 0.08, 0.14, 10, "steel", right @ at(ex - 0.24 + i * 0.16, 0.7, ez), bevel=0.0))
    T.append(cyl(0.16, 0.16, 0.06, 16, "red", right @ at(ex + 0.38, 0.4, ez, rz(HALF_PI)), bevel=0.0))
    T.append(box(0.75, 0.06, 0.3, "red_dark", right @ at(ex, 0.82, ez), bevel=0.01, segs=1))
    T += tyre_stack(at(-7.6, 0, 8.0), 3, 9)  # by the front-left crates
    return dict(groups={"shelves": S, "tyres": T}, tex=1024)


def corners():
    """Drums, the scrap pile and the corner set pieces."""
    D, J = [], []
    right = ry(-HALF_PI)
    # oil drums: back-left corner, by the door and on the right wall
    D += drum(at(-11.1, 0, -10.6), "red")
    D += drum(at(-10.5, 0, -11.2), "ws_blue")
    D += drum(at(-11.0, 0, -9.7) @ ry(0.5), "yellow", lying=True)
    D += drum(at(-6.4, 0, 11.2), "ws_gas")
    D += drum(at(-7.1, 0, 11.3), "red")
    D += drum(right @ at(3.4, 0, -WALL + 0.4), "orange")
    D.append(blob(0.5, "ws_oil", at(-10.7, 0.003, -10.4), seed=0))
    # scrap pile in the front-right corner
    R = rnd(42)
    for i in range(9):
        a = R() * HALF_PI
        r = 0.3 + R() * 1.0
        x, z = 11.2 - math.cos(a) * r, 11.2 - math.sin(a) * r
        sz = 0.35 + R() * 0.3
        J.append(lump(sz, ("steel_dark", "ws_tin_rust", "red_dark", "ws_tin")[i % 4], at(x, sz * 0.4, z, ry(R() * TAU)) @ scl(1.0, 0.65, 1.0), seed=0, rough=0.5, subdiv=2))
    for i in range(5):
        a = R() * HALF_PI
        x, z = 11.0 - math.cos(a) * 1.4, 11.0 - math.sin(a) * 1.4
        J.append(box(1.4, 0.06, 0.1, "girder", at(x, 0.35 + R() * 0.5, z, ry(R() * TAU) @ rz((R() - 0.5) * 0.8)), bevel=0.0))
    J.append(box(1.6, 0.04, 1.0, "ws_tin_red", at(10.2, 0.7, 11.2, ry(0.4) @ rx(1.1)), bevel=0.02, segs=1))
    J.append(cyl(0.3, 0.3, 0.2, 16, "tyre", at(9.6, 0.3, 10.9, rz(1.2)), bevel=0.05))
    J.append(cyl(0.18, 0.18, 0.21, 12, "yellow", at(9.6, 0.3, 10.9, rz(1.2)), bevel=0.0))
    J.append(rodf((10.6, 1.4, 11.3), (9.3, 0.05, 10.2), 0.04, "steel_dark", 8))
    # boxes and a wooden crate by the door
    J.append(box(0.8, 0.6, 0.6, "wood", at(6.3, 0.3, 11.2, ry(0.08)), bevel=0.02, segs=1))
    J.append(box(0.5, 0.4, 0.45, "ws_card", at(6.35, 0.8, 11.2, ry(-0.2)), bevel=0.012, segs=1))
    J.append(box(0.6, 0.45, 0.5, "ws_card", at(7.2, 0.22, 11.25, ry(0.3)), bevel=0.012, segs=1))
    # pallets stacked in the back-right corner
    for i in range(4):
        y = i * 0.15
        for j in range(5):
            J.append(box(1.2, 0.025, 0.12, "wood", at(10.9, y + 0.13, -10.9 + (j - 2) * 0.24, ry(0.05 * i)), bevel=0.0))
        for dx in (-0.5, 0.0, 0.5):
            J.append(box(0.1, 0.1, 1.1, "wood_dark", at(10.9 + dx, y + 0.06, -10.9, ry(0.05 * i)), bevel=0.0))
    # corner set pieces (corners are > 10.6 m out, beyond the camera's orbit)
    C = []
    # a tarp-covered old battle bot (back-right)
    bx_, bz_ = 8.7, -8.4
    C.append(box(1.4, 0.55, 1.1, "steel_dark", at(bx_, 0.4, bz_, ry(0.6)), bevel=0.05, segs=2))
    C.append(lump(0.9, "ws_tarp", at(bx_, 0.55, bz_, ry(0.6)) @ scl(1.05, 0.7, 0.85), seed=0, rough=0.25, subdiv=3))
    for sx in (-1, 1):
        C.append(cyl(0.3, 0.3, 0.2, 16, "tyre", at(bx_, 0.3, bz_, ry(0.6)) @ at(sx * 0.62, 0, 0.3, rz(HALF_PI)), bevel=0.05))
        C.append(cyl(0.16, 0.16, 0.21, 12, "red", at(bx_, 0.3, bz_, ry(0.6)) @ at(sx * 0.62, 0, 0.3, rz(HALF_PI)), bevel=0.0))
    C.append(box(0.12, 0.12, 0.9, "red", at(bx_, 0.9, bz_, ry(0.6)) @ at(0.2, 0, -0.5) @ rx(-0.6), bevel=0.02, segs=1))
    C.append(box(0.3, 0.22, 0.25, "steel", at(bx_, 0.9, bz_, ry(0.6)) @ at(0.2, 0.22, -0.92), bevel=0.03, segs=1))
    for i in range(3):  # tie-down ropes
        C.append(rodf(at(bx_, 0, bz_, ry(0.6)) @ v(-0.7 + i * 0.7, 0.02, 0.62), at(bx_, 0, bz_, ry(0.6)) @ v(-0.6 + i * 0.6, 1.0, 0.0), 0.012, "cream", 4))
    # engine hoist (back-left), boom pointing into the corner
    hx_, hz_ = -8.4, -8.6
    Hf = at(hx_, 0, hz_, ry(math.pi * 0.75 + math.pi))
    for sx in (-0.4, 0.4):
        C.append(box(0.1, 0.1, 1.5, "yellow", Hf @ at(sx, 0.12, 0.55), bevel=0.015, segs=1))
        C.append(cyl(0.06, 0.06, 0.05, 8, "ink", Hf @ at(sx, 0.06, 1.25, rz(HALF_PI)), bevel=0.0))
    C.append(box(0.9, 0.1, 0.12, "yellow", Hf @ at(0, 0.12, -0.15), bevel=0.015, segs=1))
    C.append(box(0.12, 1.7, 0.12, "yellow", Hf @ at(0, 0.98, -0.15), bevel=0.015, segs=1))
    C.append(box(0.1, 0.1, 1.7, "yellow", Hf @ at(0, 1.95, 0.45) @ rx(0.35), bevel=0.015, segs=1))
    C.append(rodf(Hf @ v(0, 0.75, -0.08), Hf @ v(0, 1.62, 0.55), 0.045, "red_dark", 8))
    C.append(rodf(Hf @ v(0, 1.68, 1.22), Hf @ v(0, 1.1, 1.22), 0.012, "steel_dark", 4))
    C.append(torus(0.06, 0.015, "steel_dark", Hf @ at(0, 1.05, 1.22, rx(HALF_PI)), seg=10, ring=5))
    # crate stack and boxes (front-left)
    cx_, cz_ = -9.0, 8.9
    for i, (dx, y, dz, r_) in enumerate(((0, 0.0, 0, 0.1), (0.95, 0.0, 0.2, -0.15), (0.3, 0.7, 0.05, 0.3))):
        f = at(cx_ + dx, y + 0.35, cz_ + dz, ry(r_))
        C.append(box(0.9, 0.7, 0.7, "wood", f, bevel=0.03, segs=1))
        for sy in (-0.25, 0.25):
            C.append(box(0.92, 0.08, 0.72, "wood_dark", f @ at(0, sy, 0), bevel=0.01, segs=1))
    C.append(box(0.55, 0.45, 0.5, "ws_card", at(cx_ - 0.9, 0.225, cz_ + 0.6, ry(0.4)), bevel=0.0))
    C.append(box(0.45, 0.35, 0.4, "ws_card", at(cx_ - 0.85, 0.625, cz_ + 0.55, ry(0.1)), bevel=0.0))
    return dict(groups={"drums": D, "scrap": J, "corners": C}, tex=1024)


ASSETS = {
    "ws_turntable": turntable,
    "ws_floor": floor,
    **{f"ws_wall_{side}": (lambda side=side: walls(side)) for side in SIDES},
    "ws_bench": bench,
    "ws_props": props,
    "ws_corners": corners,
}
