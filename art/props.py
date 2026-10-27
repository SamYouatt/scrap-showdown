"""Reusable junk props (scrapyard piles, ring walls, loot drops).

Frames match the code-built Geos in src/models.rs and src/screens/scrap.rs so
each asset drops in with the same transform: every prop's origin is its
geometric centre (like the centred `bx`/`cyl` it replaces), except `car_shell`,
whose origin is on the ground under its centre (as `models::car_shell`).
Tint slot 0 = the per-instance colour (barrel body, car paint, sheet paint,
scrap bits); everything else is baked.
"""
import math

import bmesh
import lib
from lib import (HALF_PI, TAU, Look, LOOKS, at, bolt, box, cable, cyl, prism, rod, rx, ry, rz, torus)
from mathutils import Matrix, Vector
from mathutils import noise as mnoise

I = Matrix.Identity(4)

LOOKS["fridge"] = Look("#d9e1d6", chip=0.45, rust=0.45, streaks=0.5, bare="#8e9096")
LOOKS["rusty_steel"] = Look("#7a5e4c", chip=0.0, rust=0.75, rough=0.65, metal=0.35, streaks=0.4, bare="#9da2aa")
LOOKS["porthole"] = Look("#5c8aa0", chip=0.0, rust=0.0, grime=0.6, rough=0.15, metal=0.1, light=0.35, edge=0.4)
LOOKS["lamp_lens"] = Look("#fff0c0", chip=0.0, rust=0.0, grime=0.4, rough=0.2, light=0.2, edge=0.3)
LOOKS["tail_lens"] = Look("#c8302a", chip=0.0, rust=0.0, grime=0.4, rough=0.2, light=0.2, edge=0.3)
LOOKS["sack"] = Look("#b89a6a", chip=0.0, rust=0.0, grime=0.6, rough=0.95, light=0.25, edge=0.05, dust="#d8c8a0")
LOOKS["rope"] = Look("#8a6a40", chip=0.0, rust=0.0, grime=0.4, rough=0.95, light=0.2, edge=0.0)
LOOKS["plate_steel"] = Look("#a9adb5", chip=0.0, rust=0.55, rough=0.45, metal=0.5, streaks=0.4, grime=0.7)
# tint 0, properly battered: more rust and chips than lib's paint_rusty
LOOKS["junk_paint"] = Look(lib.NEUTRAL, chip=0.8, rust=0.75, streaks=0.7, grime=0.7, tint=0, chip_col="#5a5652", bare="#5a5652", chip_scale=9.0)
LOOKS["junk_paint_dark"] = Look("#9c9a96", chip=0.6, rust=0.7, streaks=0.4, tint=0, chip_col="#3e3c3a", bare="#3e3c3a")
LOOKS["glass_dirty"] = Look("#6f9fae", chip=0.0, rust=0.0, grime=0.9, rough=0.3, metal=0.1, light=0.25, edge=0.5, dust="#a89a80")


def lathe(profile, seg, look, xf, name="lathe", closed=True):
    """Revolve a list of (radius, y) points about +Y (a ring if closed)."""
    bm = bmesh.new()
    rings = []
    for i in range(seg):
        a = i / seg * TAU
        rings.append([bm.verts.new((math.cos(a) * r, y, math.sin(a) * r)) for r, y in profile])
    n = len(profile)
    for i in range(seg):
        A, B = rings[i], rings[(i + 1) % seg]
        for j in range(n if closed else n - 1):
            k = (j + 1) % n
            bm.faces.new((A[j], A[k], B[k], B[j]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return lib._finish(lib._new(name, bm), look, xf, 0.0)


# ------------------------------------------------------------------ local helpers
def bx(w, h, d, look, xf, bevel=0.02, name="box"):
    """lib.box with 2 bevel segments (keeps the many junk instances light)."""
    return box(w, h, d, look, xf, bevel=bevel, name=name, segs=2)


def cy(rt, rb, h, seg, look, xf, bevel=0.012, name="cyl"):
    o = cyl(rt, rb, h, seg, look, xf, bevel=bevel, name=name)
    if "bevel" in o.modifiers:
        o.modifiers["bevel"].segments = 2
    return o


def sheet(w, d, t, look, xf, nx=8, nz=8, warp=None, name="sheet"):
    """A thin w x d slab in XZ (normal +Y), optionally bent by warp(x, z) ->
    (x, y, z) before transforming; thickness t via solidify."""
    bm = bmesh.new()
    vs = [[None] * (nz + 1) for _ in range(nx + 1)]
    for i in range(nx + 1):
        for j in range(nz + 1):
            x, z = (i / nx - 0.5) * w, (j / nz - 0.5) * d
            vs[i][j] = bm.verts.new(warp(x, z) if warp else (x, 0.0, z))
    for i in range(nx):
        for j in range(nz):
            bm.faces.new((vs[i][j], vs[i][j + 1], vs[i + 1][j + 1], vs[i + 1][j]))
    o = lib._new(name, bm)
    s = o.modifiers.new("solid", "SOLIDIFY")
    s.thickness = t
    s.offset = 0.0
    return lib._finish(o, look, xf, 0.0)


def blob(r, look, xf, seed=0, rough=0.25, subdiv=3, freq=1.2, squash=1.0, floor=None, name="blob"):
    """Noise-displaced icosphere (lib.lump offsets the mesh for seed != 0).
    squash scales Y; floor flattens everything below that height."""
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=1.0)
    off = Vector((seed * 3.17, seed * 1.93, seed * 2.71))
    for v in bm.verts:
        n = v.co.normalized()
        d = mnoise.noise(n * freq + off) * 0.7 + mnoise.noise(n * freq * 2.7 + off) * 0.3
        p = n * r * (1.0 + d * rough)
        p.y *= squash
        if floor is not None:
            p.y = max(p.y, floor)
        v.co = p
    return lib._finish(lib._new(name, bm), look, xf, 0.0)


def flat_lump(r, h, look, xf, seed=0, rough=0.25, subdiv=3, floor=-0.04, name="lump"):
    """A lump of radius r squashed to height h, its underside flattened just
    below y = 0 (no hidden geometry under the ground)."""
    return blob(r, look, xf, seed=seed, rough=rough, subdiv=subdiv, squash=h / r, floor=floor, name=name)


def tyre(major, minor, xf, lugs=16, seg=24, ring=10, look="tyre"):
    """A tyre lying flat (hole along +Y), centred, with chunky staggered tread."""
    M, m = major, minor
    prof = [(M - m * 0.95, -m * 0.5), (M - m * 0.75, -m * 0.88), (M, -m * 0.98), (M + m * 0.7, -m * 0.95),
            (M + m * 0.93, -m * 0.72), (M + m, -m * 0.3), (M + m, m * 0.3), (M + m * 0.93, m * 0.72),
            (M + m * 0.7, m * 0.95), (M, m * 0.98), (M - m * 0.75, m * 0.88), (M - m * 0.95, m * 0.5)]
    out = [lathe(prof, seg, look, xf, name="tyre")]
    r = M + m
    wid = TAU * r / lugs * 0.5
    for i in range(lugs):
        for row in (-1, 1):
            a = (i + (0.5 if row > 0 else 0.0)) / lugs * TAU
            out.append(bx(m * 0.2, m * 0.5, wid, look,
                          xf @ at(math.cos(a) * (r - m * 0.02), row * m * 0.33, math.sin(a) * (r - m * 0.02), ry(-a) @ rx(row * 0.5)),
                          bevel=m * 0.05, name="lug"))
    return out


def wheel(r, w, xf, look_hub="steel_dark"):
    """A car wheel standing up, axle along X, centred."""
    out = [cy(r, r, w, 16, "tyre", xf @ rz(HALF_PI), bevel=0.05, name="tyre")]
    out.append(cy(r * 0.55, r * 0.55, w + 0.02, 12, look_hub, xf @ rz(HALF_PI), bevel=0.015, name="hub"))
    out.append(cy(r * 0.2, r * 0.24, w + 0.08, 8, "steel", xf @ rz(HALF_PI), bevel=0.01))
    return out


# ------------------------------------------------------------------ props
def barrel_geo(xf=I):
    """Oil drum, centred: r 0.32, h 0.9, two rolling hoops (paint_dark = darker tint)."""
    B = [cy(0.32, 0.32, 0.9, 20, "junk_paint", xf, bevel=0.03, name="drum")]
    for y in (-0.22, 0.22):
        B.append(cy(0.335, 0.335, 0.06, 20, "junk_paint_dark", xf @ at(0, y, 0), bevel=0.02, name="hoop"))
    for y in (-0.43, 0.43):
        B.append(cy(0.327, 0.327, 0.04, 20, "junk_paint_dark", xf @ at(0, y, 0), bevel=0.012, name="rim"))
    B.append(cy(0.28, 0.28, 0.02, 20, "steel_dark", xf @ at(0, 0.442, 0), bevel=0.008, name="lid"))
    B.append(cy(0.045, 0.05, 0.04, 8, "steel", xf @ at(0.17, 0.455, 0.05), bevel=0.008))
    B.append(cy(0.03, 0.03, 0.03, 8, "steel", xf @ at(-0.18, 0.45, -0.06), bevel=0.006))
    return B


def barrel():
    return dict(groups={"body": barrel_geo()}, tex=512)


def crate():
    """Wooden crate, 0.8 cube, centred, with darker bands at y = +-0.3."""
    B = [bx(0.78, 0.78, 0.78, "wood", I, bevel=0.03, name="crate")]
    for y in (-0.3, 0.3):
        B.append(bx(0.84, 0.1, 0.84, "wood_dark", at(0, y, 0), bevel=0.02, name="band"))
    # plank seams and a diagonal brace on each side
    for ang in (0.0, HALF_PI, math.pi, -HALF_PI):
        f = ry(ang)
        for x in (-0.13, 0.13):
            B.append(bx(0.02, 0.5, 0.012, "wood_dark", f @ at(x, 0, 0.392), bevel=0.004))
        B.append(bx(0.09, 0.66, 0.03, "wood", f @ at(0, 0, 0.405, rz(0.72)), bevel=0.01, name="brace"))
        for sx in (-0.34, 0.34):
            for y in (-0.3, 0.3):
                B.append(bolt(0.014, "steel_dark", f @ at(sx, y, 0.42, rx(HALF_PI))))
    for x in (-0.2, 0.0, 0.2):
        B.append(bx(0.18, 0.02, 0.7, "wood_dark", at(x, 0.395, 0), bevel=0.006))
    B.append(bx(0.24, 0.02, 0.14, "red", at(0.12, 0.405, -0.18, ry(0.2)), bevel=0.004))
    return dict(groups={"body": B}, tex=512)


def tyre_asset():
    """Old tyre lying flat, centred: major 0.42, tube 0.2 (0.62 outer, 0.4 tall)."""
    return dict(groups={"body": tyre(0.42, 0.2, I, lugs=18)}, tex=512)


def car_shell_geo(xf=I, wheels=(True, True, True, True)):
    """Rusty car body, origin on the ground under its centre, nose to +Z."""
    B = []
    B.append(bx(1.7, 0.55, 3.6, "junk_paint", xf @ at(0, 0.45, 0), bevel=0.12, name="body"))
    # cabin: trapezoid prism (z, y) profile, windows inset on top
    prof = [(-1.1, 0.7), (0.72, 0.7), (0.35, 1.2), (-0.85, 1.2)]
    B.append(prism(prof, 1.5, "junk_paint", xf, bevel=0.05, name="cabin"))
    sl = math.atan2(0.5, 0.37)
    B.append(bx(1.3, 0.56, 0.04, "glass_dirty", xf @ at(0, 0.96, 0.555, rx(-(HALF_PI - sl))), bevel=0.01, name="screen"))
    sr = math.atan2(0.5, 0.25)
    B.append(bx(1.3, 0.48, 0.04, "glass_dirty", xf @ at(0, 0.96, -0.985, rx(HALF_PI - sr)), bevel=0.01, name="rear"))
    for sx in (-1, 1):
        B.append(bx(0.04, 0.34, 1.2, "glass_dirty", xf @ at(sx * 0.755, 0.96, -0.25), bevel=0.01, name="side"))
        B.append(bx(0.04, 0.38, 0.06, "junk_paint", xf @ at(sx * 0.76, 0.96, -0.25), bevel=0.01))
        # door seams + handles
        for z in (-0.95, 0.15, 0.85):
            B.append(bx(0.02, 0.4, 0.025, "ink", xf @ at(sx * 0.85, 0.45, z), bevel=0.004))
        B.append(bx(0.03, 0.04, 0.16, "steel", xf @ at(sx * 0.86, 0.6, 0.0), bevel=0.008))
        # wheel arches
        for z in (-1.1, 1.1):
            B.append(cy(0.38, 0.38, 0.06, 12, "ink", xf @ at(sx * 0.83, 0.3, z, rz(HALF_PI)), bevel=0.0))
    # crumpled bonnet, bumpers, lights
    B.append(bx(1.5, 0.08, 1.0, "junk_paint", xf @ at(0.05, 0.74, 1.25, rx(0.08) @ rz(0.05)), bevel=0.03, name="bonnet"))
    for z, look in ((1.84, "steel"), (-1.84, "steel_dark")):
        B.append(bx(1.76, 0.16, 0.12, look, xf @ at(0, 0.32, z, ry(0.06 if z > 0 else -0.04)), bevel=0.04, name="bumper"))
    for sx in (-0.58, 0.58):
        B.append(cy(0.11, 0.11, 0.06, 10, "lamp_lens", xf @ at(sx, 0.55, 1.8, rx(HALF_PI)), bevel=0.01))
        B.append(bx(0.26, 0.12, 0.05, "tail_lens", xf @ at(sx, 0.58, -1.8), bevel=0.01))
    B.append(bx(0.7, 0.14, 0.04, "ink", xf @ at(0, 0.48, 1.81), bevel=0.01, name="grille"))
    for (sx, sz), keep in zip(((-0.85, -1.1), (-0.85, 1.1), (0.85, -1.1), (0.85, 1.1)), wheels):
        if keep:
            B += wheel(0.33, 0.25, xf @ at(sx * 0.95, 0.3, sz))
        else:
            B.append(cy(0.16, 0.16, 0.2, 10, "rusty_steel", xf @ at(sx * 0.9, 0.3, sz, rz(HALF_PI)), bevel=0.02))
    return B


def car_shell():
    return dict(groups={"body": car_shell_geo()}, tex=512)


def washer_geo(xf=I):
    """Washing machine, centred: 0.8 x 0.9 x 0.8, porthole on +Z."""
    B = [bx(0.8, 0.9, 0.8, "white_goods", xf, bevel=0.05, name="washer")]
    B.append(cy(0.26, 0.26, 0.06, 20, "steel_dark", xf @ at(0, -0.05, 0.41, rx(HALF_PI)), bevel=0.02, name="ring"))
    B.append(cy(0.18, 0.2, 0.07, 20, "porthole", xf @ at(0, -0.05, 0.42, rx(HALF_PI)), bevel=0.02, name="glass"))
    B.append(bx(0.76, 0.16, 0.03, "steel_clean", xf @ at(0, 0.34, 0.405), bevel=0.01, name="panel"))
    B.append(cy(0.045, 0.045, 0.04, 12, "ink", xf @ at(0.24, 0.34, 0.43, rx(HALF_PI)), bevel=0.008))
    B.append(bx(0.22, 0.07, 0.02, "ink", xf @ at(-0.18, 0.34, 0.425), bevel=0.005))
    B.append(bx(0.06, 0.12, 0.04, "steel_dark", xf @ at(0.27, -0.05, 0.44), bevel=0.008))
    B.append(bx(0.72, 0.05, 0.03, "steel_dark", xf @ at(0, -0.42, 0.4), bevel=0.01))
    return B


def washer():
    return dict(groups={"body": washer_geo()}, tex=512)


def fridge_geo(xf=I):
    """Old fridge, centred: 0.8 x 1.7 x 0.75, doors on +Z, handle at (0.3, 0.3)."""
    B = [bx(0.8, 1.7, 0.75, "fridge", xf, bevel=0.07, name="fridge")]
    B.append(bx(0.76, 0.42, 0.04, "fridge", xf @ at(0, 0.6, 0.38), bevel=0.03, name="freezer"))
    B.append(bx(0.76, 1.18, 0.04, "fridge", xf @ at(0, -0.22, 0.38, ry(0.03)), bevel=0.03, name="door"))
    B.append(bx(0.05, 0.5, 0.05, "steel_dark", xf @ at(0.3, 0.3, 0.42), bevel=0.015, name="handle"))
    B.append(bx(0.05, 0.22, 0.05, "steel_dark", xf @ at(0.3, 0.62, 0.42), bevel=0.015))
    for y in (0.75, 0.42, -0.7):
        B.append(bx(0.06, 0.1, 0.06, "steel", xf @ at(-0.38, y, 0.36), bevel=0.01))
    B.append(bx(0.6, 0.1, 0.03, "steel_dark", xf @ at(0, -0.8, 0.39), bevel=0.01, name="kick"))
    B.append(bx(0.16, 0.1, 0.02, "ink", xf @ at(-0.2, 0.65, 0.405), bevel=0.004))
    return B


def fridge():
    return dict(groups={"body": fridge_geo()}, tex=512)


def engine_geo(xf=I):
    """Engine block, centred: 0.7 x 0.45 x 0.5 block, flywheel on +X."""
    B = [bx(0.7, 0.45, 0.5, "steel_dark", xf, bevel=0.04, name="block")]
    B.append(bx(0.62, 0.1, 0.32, "red_dark", xf @ at(-0.02, 0.27, 0), bevel=0.03, name="cover"))
    for k in range(3):
        x = -0.2 + k * 0.2
        B.append(cy(0.045, 0.045, 0.12, 8, "steel", xf @ at(x, 0.36, 0.05), bevel=0.01))
        B.append(cy(0.03, 0.03, 0.06, 6, "ink", xf @ at(x, 0.44, 0.05), bevel=0.006))
    B.append(cy(0.17, 0.17, 0.06, 18, "rusty_steel", xf @ at(0.38, -0.02, 0, rz(HALF_PI)), bevel=0.015, name="flywheel"))
    B.append(cy(0.06, 0.06, 0.06, 10, "steel", xf @ at(0.42, -0.02, 0, rz(HALF_PI)), bevel=0.01))
    B.append(cy(0.09, 0.09, 0.05, 12, "ink", xf @ at(-0.38, 0.05, 0.08, rz(HALF_PI)), bevel=0.01, name="pulley"))
    # exhaust manifold on -Z, fins on +Z, sump below
    for k in range(3):
        x = -0.2 + k * 0.2
        B.append(rod((x, 0.12, -0.25), (x, 0.0, -0.33), 0.04, "rusty_steel", 8))
    B.append(rod((-0.28, -0.02, -0.34), (0.3, -0.02, -0.34), 0.05, "rusty_steel", 8))
    for k in range(5):
        B.append(bx(0.5, 0.02, 0.05, "steel_dark", xf @ at(0, -0.12 + k * 0.06, 0.26), bevel=0.005))
    B.append(bx(0.56, 0.1, 0.4, "steel", xf @ at(0, -0.26, 0), bevel=0.03, name="sump"))
    B.append(cable([Vector((-0.2, 0.44, 0.05)), Vector((0.0, 0.5, 0.18)), Vector((0.2, 0.44, 0.05))], 0.012, "red_cable"))
    return B


def engine():
    return dict(groups={"body": engine_geo()}, tex=512)


def pipe():
    """Bent rusty pipe, centred: (-0.7, 0, 0) -> (0.7, 0.1, 0), r 0.07, flanged ends."""
    a, b = Vector((-0.7, 0.0, 0.0)), Vector((0.7, 0.1, 0.0))
    pts = [a, Vector((-0.25, 0.02, 0.0)), Vector((0.25, 0.1, 0.12)), b]
    B = [cable(pts, 0.07, "rusty_steel", res=6)]
    for p, q in ((a, pts[1]), (b, pts[2])):
        d = (q - p).normalized()
        rot = Vector((0, 1, 0)).rotation_difference(d).to_matrix().to_4x4()
        B.append(cy(0.11, 0.11, 0.04, 12, "steel_dark", Matrix.Translation(p + d * 0.02) @ rot, bevel=0.01, name="flange"))
        B.append(cy(0.05, 0.05, 0.05, 10, "ink", Matrix.Translation(p) @ rot, bevel=0.0, name="bore"))
        for i in range(4):
            ang = i / 4 * TAU + 0.4
            off = rot @ Vector((math.cos(ang) * 0.085, 0.0, math.sin(ang) * 0.085))
            B.append(bolt(0.014, "steel", Matrix.Translation(p + d * 0.045 + off) @ rot))
    return dict(groups={"body": B}, tex=512)


def bent(x, z, w, d):
    """Corrugated, sagging sheet with one corner curled up."""
    y = 0.012 * math.sin(x / w * TAU * 4.5)
    y += -0.04 * math.cos(x / w * math.pi) + 0.03 * math.sin(z / d * math.pi + 0.6)
    u, v = x / w + 0.5, z / d + 0.5
    curl = max(0.0, u + v - 1.3)
    y += curl * curl * 0.45
    return (x, y, z)


def sheet_asset():
    """Bent corrugated metal sheet (tint 0), centred: 0.9 x 0.05 x 0.7."""
    B = [sheet(0.9, 0.7, 0.025, "junk_paint", I, nx=18, nz=6, warp=lambda x, z: bent(x, z, 0.9, 0.7))]
    for x in (-0.38, 0.38):
        for z in (-0.3, 0.3):
            if x > 0 and z > 0:
                continue
            p = bent(x, z, 0.9, 0.7)
            B.append(bolt(0.014, "steel_dark", at(p[0], p[1] + 0.014, p[2])))
    return dict(groups={"body": B}, tex=512)


def girder_bit():
    """Short lattice girder, centred: 1.2 long (X), 0.2 tall, 0.1 deep."""
    B = []
    for sy in (-0.08, 0.08):
        for sz in (-0.035, 0.035):
            B.append(bx(1.2, 0.035, 0.03, "girder", at(0, sy, sz), bevel=0.008, name="chord"))
    for k in range(5):
        x = -0.5 + k * 0.25
        B.append(bx(0.03, 0.16, 0.1, "girder", at(x, 0, 0), bevel=0.006, name="post"))
        if k < 4:
            f = 1 if k % 2 else -1
            for sz in (-0.035, 0.035):
                B.append(rod((x, -0.08 * f, sz), (x + 0.25, 0.08 * f, sz), 0.012, "girder", 6))
    for x in (-0.6, 0.6):
        B.append(bx(0.03, 0.22, 0.12, "steel_dark", at(x, 0, 0), bevel=0.008))
        for sy in (-0.07, 0.07):
            B.append(bolt(0.012, "steel", at(x + (0.02 if x > 0 else -0.02), sy, 0, rz(-HALF_PI if x > 0 else HALF_PI))))
    return dict(groups={"body": B}, tex=512)


def plate():
    """Loot: steel plate, centred: 0.6 x 0.06 x 0.5."""
    B = [bx(0.6, 0.06, 0.5, "plate_steel", I, bevel=0.015, name="plate")]
    for sx in (-0.24, 0.24):
        for sz in (-0.19, 0.19):
            B.append(cy(0.025, 0.025, 0.062, 8, "ink", at(sx, 0, sz), bevel=0.0))
    B.append(bx(0.44, 0.012, 0.03, "steel_dark", at(0, 0.032, 0.05, ry(0.1)), bevel=0.004, name="weld"))
    B.append(bx(0.16, 0.01, 0.1, "yellow", at(-0.15, 0.032, -0.12), bevel=0.003))
    return dict(groups={"body": B}, tex=256)


def bolts():
    """Loot: a tied sack of bolts, centred on the sack (r 0.22), bolts poking out the top."""
    B = [flat_lump(0.24, 0.2, "sack", at(0, -0.02, 0), seed=3, rough=0.18, subdiv=2, floor=-0.17, name="sack")]
    B.append(cy(0.07, 0.1, 0.1, 10, "sack", at(0, 0.18, 0), bevel=0.02, name="neck"))
    B.append(torus(0.075, 0.018, "rope", at(0, 0.17, 0), seg=10, ring=5))
    for i, (x, z, tx, tz) in enumerate(((0.12, 0.06, 0.4, 0.2), (-0.1, 0.1, -0.3, 0.5), (0.05, -0.13, 0.2, -0.5), (-0.13, -0.06, -0.5, -0.2))):
        rot = rx(tz) @ rz(-tx)
        f = at(x, 0.16, z) @ rot
        B.append(rod(f @ Vector((0, -0.06, 0)), f @ Vector((0, 0.06, 0)), 0.018, "steel", 6))
        B.append(cy(0.04, 0.04, 0.03, 6, "steel", f @ at(0, 0.075, 0), bevel=0.008))
    for x, z in ((0.22, 0.05), (-0.18, 0.17)):
        B.append(cy(0.035, 0.035, 0.025, 6, "steel_dark", at(x, -0.18, z), bevel=0.006))
    return dict(groups={"body": B}, tex=256)


def bits():
    """Scatter junk for the 260 'little bits'. Each group is centred on its own
    origin; groups are laid out along X only so the preview reads."""
    G, O = {}, {}

    def place(name, x):
        O[name] = (x, 0, 0)
        return at(x, 0, 0)

    # scrap_a: a bent, chipped plate offcut (tint 0), 0.5 x 0.04 x 0.25
    f = place("scrap_a", 0.0)
    G["scrap_a"] = [sheet(0.5, 0.25, 0.03, "junk_paint", f, nx=6, nz=2,
                          warp=lambda x, z: (x, 0.05 * math.sin(x / 0.5 * math.pi + 1.0) - 0.02, z))]
    # scrap_b: a 1 m bar, slightly bent, along X (scale X for 0.6..1.6 m)
    f = place("scrap_b", 1.2)
    G["scrap_b"] = [cable([f @ Vector((-0.5, 0, 0)), f @ Vector((0.05, 0.02, 0.04)), f @ Vector((0.5, 0, -0.02))], 0.03, "rusty_steel", res=4)]
    # nut: hex nut + stub of bolt, r 0.06
    f = place("nut", 2.0)
    G["nut"] = [cy(0.06, 0.06, 0.05, 6, "steel", f, bevel=0.01), rod(f @ Vector((0, 0, 0)), f @ Vector((0.0, 0.0, 0.14)), 0.025, "steel_dark", 6)]
    # can: a crushed paint tin (tint 0) lying on its side, ~0.18 x 0.12 x 0.14
    f = place("can", 2.6)
    can = cy(0.07, 0.065, 0.16, 12, "junk_paint", f @ rz(HALF_PI) @ rx(0.15), bevel=0.01, name="can")
    can.data.transform(Matrix.Translation(f.translation) @ lib.scl(1, 0.8, 1) @ Matrix.Translation(-f.translation))
    G["can"] = [can, cy(0.072, 0.072, 0.02, 12, "steel", f @ at(0.075, 0, 0) @ rz(HALF_PI), bevel=0.005)]
    # small_tyre: half-scale tyre lying flat
    f = place("small_tyre", 3.4)
    G["small_tyre"] = tyre(0.21, 0.1, f, lugs=12, seg=16, ring=8)
    return dict(groups=G, origins=O, tex=256)


ASSETS = {
    "barrel": barrel, "crate": crate, "tyre": tyre_asset, "car_shell": car_shell, "washer": washer,
    "fridge": fridge, "engine": engine, "pipe": pipe, "sheet": sheet_asset, "girder_bit": girder_bit,
    "plate": plate, "bolts": bolts, "bits": bits,
}
