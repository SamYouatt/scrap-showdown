"""Battle-arena scenery: modular pieces the game instances along the layout in
src/arena.rs (A = 8 arena half-size, PIT at x=-5, z=1.2, half-size 1.1).

Conventions (Bevy space, +Y up):
  * Every group's origin is its placement anchor: spawn the group's mesh with
    Transform::from_xyz(anchor) (plus the rotation/scale noted per asset).
    Groups are laid out side by side in model space only so previews read.
  * Pieces with a "front" face the arena along local -Z (walls, stands, back
    wall, barrier, banners).  Rotate with ry(t) so -Z points at the arena:
    north (+Z) side t=0, south t=PI, east (+X) t=+PI/2, west t=-PI/2.
  * Segments that tile along X are seam-safe: hazard stripes use a 0.4 m
    period (2 m / 5) so neighbouring segments line up.
Tint slots: 0 = shirt / banner background, 1 = skin / banner cross.
"""
import math

import bmesh
import bpy

import lib
from lib import HALF_PI, TAU, Look, NEUTRAL, at, box, cyl, hazard_box, prism, rod, rx, ry, rz, torus

A = 8.0
PIT_H = 1.1
TILE = 1.97
GROUND_TILE = 8.5  # outer ground tile; the grid lines up with the walls at +-8.5

L = lib.LOOKS
L["floor"] = Look("#8a857c", chip=0.0, rust=0.2, grime=0.7, rough=0.5, metal=0.45, light=0.12, edge=0.35, bare="#b9b6ae")
L["floor_worn"] = Look("#7d7870", chip=0.3, rust=0.3, grime=0.8, rough=0.55, metal=0.4, light=0.12, edge=0.4,
                       chip_col="#b8b5ad", chip_scale=6.0)
L["floor_rust"] = Look("#857a6c", chip=0.2, rust=0.7, grime=0.8, rough=0.6, metal=0.35, streaks=0.3, light=0.1,
                       edge=0.3, chip_col="#b1aca2")
L["weld"] = Look("#55504c", chip=0.0, rust=0.55, grime=0.7, rough=0.6, metal=0.3, edge=0.3)
_mark = dict(chip=0.6, rust=0.0, grime=0.55, rough=0.7, light=0.08, edge=0.0, chip_col="#7d7870", chip_scale=9.0)
L["mark_red"] = Look("#b0553c", **_mark)
L["mark_blue"] = Look("#4f78b8", **_mark)
L["mark_yellow"] = Look("#f2b632", **_mark)
L["mark_white"] = Look("#e6dfcf", **_mark)
L["shaft"] = Look("#2e282e", chip=0.2, rust=0.5, grime=0.9, rough=0.8, metal=0.2, bare="#5a5458", light=0.04,
                  edge=0.15, streaks=0.4)
L["hazard_tile"] = Look("#f2b632", chip=0.55, rust=0.25, grime=0.6, stripes=0.4)
L["wall"] = Look("#6d6b74", chip=0.45, rust=0.4, streaks=0.35, bare="#a3a5ab")
L["stand"] = Look("#6a5f6e", chip=0.45, rust=0.35, streaks=0.35, bare="#9a9ca3")
L["stand_conc"] = Look("#7d766f", chip=0.0, rust=0.0, grime=0.8, rough=0.9, light=0.12, edge=0.35)
L["stadium"] = Look("#7a6670", chip=0.45, rust=0.4, streaks=0.5, bare="#a09a9e")
L["barrier"] = Look("#5a5660", chip=0.0, rust=0.0, grime=0.8, rough=0.9, light=0.14, edge=0.4)
L["cone"] = Look("#ee7a32", chip=0.25, rust=0.0, grime=0.5, rough=0.6, light=0.2, bare="#c45a22")
L["reflect"] = Look("#fff3dc", chip=0.45, rust=0.0, grime=0.4, rough=0.3, metal=0.2, light=0.1, chip_col="#ee7a32")
L["shirt"] = Look(NEUTRAL, chip=0.0, rust=0.0, grime=0.25, rough=0.9, light=0.25, edge=0.1, tint=0)
L["trousers"] = Look("#3c3a44", chip=0.0, rust=0.0, grime=0.3, rough=0.9, light=0.25, edge=0.1)
L["hair"] = Look("#3a2a20", chip=0.0, rust=0.0, grime=0.2, rough=0.9, light=0.3, edge=0.1)
L["banner_bg"] = Look(NEUTRAL, chip=0.0, rust=0.0, grime=0.45, rough=0.95, light=0.2, edge=0.05, streaks=0.15, tint=0)
L["banner_fg"] = Look(NEUTRAL, chip=0.0, rust=0.0, grime=0.35, rough=0.95, light=0.2, edge=0.05, tint=1)


def bolt(r, look, xf):
    """Cheap unbevelled hex bolt (these pieces are instanced a lot)."""
    return cyl(r, r, r * 0.8, 6, look, xf, bevel=0.0, name="bolt")


def bolt_row(a, b, n, r, look="steel_dark", up=(0, 1, 0)):
    a, b = lib.Vector(a), lib.Vector(b)
    q = lib.Vector((0, 1, 0)).rotation_difference(lib.Vector(up).normalized()).to_matrix().to_4x4()
    return [bolt(r, look, lib.Matrix.Translation(a.lerp(b, 0.5 if n == 1 else i / (n - 1))) @ q) for i in range(n)]


def _bm_obj(bm, look, bevel=0.0, smooth=True, name="mesh"):
    return lib._finish(lib._new(name, bm), look, at(0, 0, 0), bevel, smooth=smooth)


def annulus(r0, r1, h, seg, look, xf, name="ring"):
    """Flat ring in XZ, bottom at y=0, top at y=h."""
    bm = bmesh.new()
    loops = []
    for y, r in ((0, r0), (0, r1), (h, r1), (h, r0)):
        loops.append([bm.verts.new((math.cos(i / seg * TAU) * r, y, math.sin(i / seg * TAU) * r)) for i in range(seg)])
    for k in range(4):
        p, q = loops[k], loops[(k + 1) % 4]
        for i in range(seg):
            j = (i + 1) % seg
            bm.faces.new((p[i], p[j], q[j], q[i]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.transform(xf)
    return _bm_obj(bm, look, 0.003, name=name)


# ------------------------------------------------------------------ floor
def tile_plate(ox, look):
    t = TILE
    B = [box(t, 0.08, t, look, at(ox, -0.04, 0), bevel=0.03, segs=2, name="plate")]
    for sx in (-1, 1):
        for sz in (-1, 1):
            B.append(bolt(0.032, "steel_dark", at(ox + sx * (t / 2 - 0.12), 0.0, sz * (t / 2 - 0.12))))
        B.append(bolt(0.028, "steel_dark", at(ox + sx * (t / 2 - 0.12), 0.0, 0)))
        B.append(bolt(0.028, "steel_dark", at(ox, 0.0, sx * (t / 2 - 0.12))))
    return B


def arena_tile():
    """Three 1.97 m floor plates; top face at the origin's y (0)."""
    a = tile_plate(-2.2, "floor")
    # welded seam down the middle and a stitch weld across one end
    a.append(box(0.05, 0.012, 1.7, "weld", at(-2.2, 0.0, 0), bevel=0.005, segs=2))
    a.append(box(0.5, 0.012, 0.05, "weld", at(-2.2 + 0.45, 0.0, -0.62, ry(0.1)), bevel=0.005, segs=2))
    b = tile_plate(0.0, "floor_worn")
    # bolted repair patch, skewed
    patch = at(0.28, 0.006, 0.3, ry(0.18))
    b.append(box(0.78, 0.014, 0.58, "steel_dark", patch, bevel=0.006, segs=2, name="patch"))
    for sx in (-1, 1):
        for sz in (-1, 1):
            b.append(bolt(0.022, "steel", patch @ at(sx * 0.33, 0.006, sz * 0.23)))
    b.append(box(0.04, 0.01, 0.9, "weld", at(-0.5, 0.0, -0.35, ry(0.6)), bevel=0.004, segs=2))
    c = tile_plate(2.2, "floor_rust")
    # round inspection cover with a bolt ring
    c.append(cyl(0.32, 0.33, 0.02, 24, "steel_dark", at(2.2 - 0.3, 0.0, -0.25), bevel=0.006))
    c.append(cyl(0.22, 0.22, 0.024, 24, "weld", at(2.2 - 0.3, 0.0, -0.25), bevel=0.004))
    for i in range(6):
        ang = i / 6 * TAU
        c.append(bolt(0.02, "steel", at(2.2 - 0.3 + math.cos(ang) * 0.27, 0.01, -0.25 + math.sin(ang) * 0.27)))
    return dict(groups={"a": a, "b": b, "c": c}, origins={"a": (-2.2, 0, 0), "b": (0, 0, 0), "c": (2.2, 0, 0)}, tex=1024)


def arena_marks():
    """Painted floor markings, modelled at their arena positions."""
    def square(z, look):
        S = [box(2.2, 0.01, 2.2, look, at(0, 0.005, z), bevel=0.003, segs=2, name="square")]
        for i in range(4):
            f = at(0, 0, z, ry(i * HALF_PI))
            S.append(box(1.76, 0.012, 0.08, "mark_white", f @ at(0, 0.006, 0.84), bevel=0.002, segs=2))
        # chevron pointing at the arena centre
        for s in (-1, 1):
            S.append(box(0.12, 0.012, 0.62, "mark_white", at(s * 0.2, 0.006, z - math.copysign(0.05, z), ry(s * math.copysign(0.75, z))),
                         bevel=0.002, segs=2))
        return S
    R = [annulus(1.6, 2.0, 0.008, 48, "mark_yellow", at(0, 0, 0))]
    return dict(groups={"start_red": square(-5.2, "mark_red"), "start_blue": square(5.2, "mark_blue"), "ring": R},
                origins={"start_red": (0, 0, -5.2), "start_blue": (0, 0, 5.2), "ring": (0, 0, 0)}, tex=1024)


def arena_pit():
    """Hazard border + dark shaft, centred on the pit centre at floor level."""
    h = PIT_H
    border = h * 2 + 0.6
    P = []
    for s in (-1, 1):
        P.append(hazard_box(border, 0.12, 0.3, at(0, -0.01, s * (h + 0.15)), bevel=0.02))
        P.append(hazard_box(h * 2, 0.12, 0.3, at(s * (h + 0.15), -0.01, 0, ry(HALF_PI)), bevel=0.02))
    # border bolts
    for s in (-1, 1):
        P += bolt_row((-border / 2 + 0.15, 0.05, s * (h + 0.15)), (border / 2 - 0.15, 0.05, s * (h + 0.15)), 7, 0.022)
        P += bolt_row((s * (h + 0.15), 0.05, -h + 0.2), (s * (h + 0.15), 0.05, h - 0.2), 4, 0.022)
    # shaft walls (inner faces flush with the pit edge), stiffeners, corner angles
    d, top, bot = 0.06, -0.07, -2.55
    hh = top - bot
    for i in range(4):
        f = ry(i * HALF_PI)
        # inward-facing sheet only: the outside is buried, so it gets no texels
        P.append(lib.plane(h * 2, hh, "shaft", f @ at(0, (top + bot) / 2, h) @ rx(-HALF_PI), subdiv=1, name="shaft"))
        for y in (-0.15, -0.9, -1.7):
            P.append(box(h * 2, 0.08, 0.05, "steel_dark", f @ at(0, y, h - 0.025), bevel=0.012, segs=2))
        for x in (-0.55, 0.55):
            P.append(box(0.07, hh - 0.1, 0.035, "shaft", f @ at(x, (top + bot) / 2, h - 0.017), bevel=0.01, segs=2))
        P.append(box(0.08, hh, 0.08, "steel_dark", f @ at(h - 0.04, (top + bot) / 2, h - 0.04), bevel=0.012, segs=2))
    return dict(groups={"pit": P}, tex=1024)


# ------------------------------------------------------------------ walls
def arena_wall():
    """seg: 2 m of perimeter wall (x -1..1); post: one post; corner: 0.5 m
    corner block.  All anchored at ground level on the wall's centre line."""
    S = [box(2.0, 1.5, 0.5, "wall", at(0, 0.45, 0), bevel=0.04, segs=2, name="wall")]
    S.append(box(2.0, 0.32, 0.52, "hazard_tile", at(0, 1.02, 0), bevel=0.012, segs=2, name="band"))
    S.append(box(2.0, 0.08, 0.08, "steel_dark", at(0, 2.66, 0), bevel=0.015, segs=2, name="rail"))
    for s in (-1, 1):
        # kick plate and a bolted inset panel on both faces
        S.append(box(2.0, 0.2, 0.03, "steel_dark", at(0, 0.06, s * 0.255), bevel=0.01, segs=2))
        S.append(box(1.5, 0.42, 0.03, "steel", at(0, 0.48, s * 0.255), bevel=0.012, segs=2, name="panel"))
        S += bolt_row((-0.68, 0.64, s * 0.272), (0.68, 0.64, s * 0.272), 5, 0.02, up=(0, 0, s))
        S += bolt_row((-0.68, 0.32, s * 0.272), (0.68, 0.32, s * 0.272), 5, 0.02, up=(0, 0, s))
    # a dent-like skewed repair strip on the arena side
    S.append(box(0.55, 0.1, 0.02, "steel_dark", at(-0.55, 0.78, -0.262, rz(0.25)), bevel=0.008, segs=2))
    P = [box(0.12, 1.6, 0.12, "steel_dark", at(-1, 1.9, 0), bevel=0.02, segs=2, name="post")]
    P.append(box(0.26, 0.04, 0.32, "steel_dark", at(-1, 1.215, 0), bevel=0.01, segs=2))
    P += [bolt(0.02, "steel", at(-1 + sx * 0.09, 1.24, sz * 0.11)) for sx in (-1, 1) for sz in (-1, 1)]
    P.append(box(0.16, 0.04, 0.16, "yellow", at(-1, 2.72, 0), bevel=0.012, segs=2))
    P.append(box(0.15, 0.12, 0.12, "steel_dark", at(-1, 2.6, 0), bevel=0.01, segs=2))
    C = [box(0.5, 1.5, 0.5, "wall", at(1.25, 0.45, 0), bevel=0.04, segs=2)]
    C.append(box(0.52, 0.32, 0.52, "hazard_tile", at(1.25, 1.02, 0), bevel=0.012, segs=2))
    C.append(box(0.56, 0.06, 0.56, "steel_dark", at(1.25, 1.22, 0), bevel=0.015, segs=2))
    C += [bolt(0.022, "steel", at(1.25 + sx * 0.2, 1.25, sz * 0.2)) for sx in (-1, 1) for sz in (-1, 1)]
    return dict(groups={"seg": S, "post": P, "corner": C}, origins={"seg": (0, 0, 0), "post": (-1, 0, 0), "corner": (1.25, 0, 0)}, tex=1024)


# ------------------------------------------------------------------ stands
def arena_stand():
    """step: the top 0.6 m of one 4 m tier (top at origin, front at -Z);
    fill: unit-height plain block below it (y -1..0) the game scales in Y."""
    S = [box(4.0, 0.6, 0.9, "stand_conc", at(0, -0.3, 0), bevel=0.025, segs=2, name="tier")]
    S.append(box(4.0, 0.52, 0.04, "stand", at(0, -0.32, -0.465), bevel=0.01, segs=2, name="fascia"))
    for i in range(8):
        S.append(box(0.07, 0.52, 0.03, "stand", at(-1.75 + i * 0.5, -0.32, -0.49), bevel=0.01, segs=2))
    S += bolt_row((-1.9, -0.1, -0.49), (1.9, -0.1, -0.49), 8, 0.016, up=(0, 0, -1))
    S.append(box(4.0, 0.03, 0.08, "yellow", at(0, 0.0, -0.4), bevel=0.008, segs=2, name="nosing"))
    # bench along the back edge
    S.append(box(4.0, 0.05, 0.2, "wood", at(0, 0.3, 0.32), bevel=0.012, segs=2, name="bench"))
    for x in (-1.5, -0.5, 0.5, 1.5):
        S.append(box(0.05, 0.28, 0.16, "steel_dark", at(x, 0.14, 0.32), bevel=0.01, segs=2))
    F = [box(4.0, 1.0, 0.9, "stand_conc", at(6.0, -0.5, 0), bevel=0.02, segs=2, name="fill")]
    F.append(box(4.0, 0.9, 0.03, "stand", at(6.0, -0.5, -0.46), bevel=0.01, segs=2))
    return dict(groups={"step": S, "fill": F}, origins={"step": (0, 0, 0), "fill": (6.0, 0, 0)}, tex=1024)


# ------------------------------------------------------------------ floodlight tower
def lamp_disc(name, xf, r=0.28, seg=16):
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r, radius2=r, depth=0.02)
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=rx(-HALF_PI))
    o = lib._new(name, bm)
    o.data.transform(xf)
    return lib.glow(o, name)


def arena_tower():
    """Floodlight tower, base at the origin, lamps facing +Z (towards the arena)."""
    T = [box(1.3, 0.45, 1.3, "concrete", at(0, -0.075, 0), bevel=0.04, segs=2, name="footing")]
    for dx in (-0.35, 0.35):
        for dz in (-0.35, 0.35):
            T.append(box(0.12, 9.0, 0.12, "steel_dark", at(dx, 4.5, dz), bevel=0.015, segs=2, name="leg"))
            T.append(box(0.26, 0.04, 0.26, "steel_dark", at(dx, 0.17, dz), bevel=0.0))
            T += [bolt(0.018, "steel", at(dx + sx * 0.09, 0.19, dz + sz * 0.09)) for sx in (-1, 1) for sz in (-1, 1)]
    y, k = 1.0, 0
    while y < 9.0:
        f = 1 if k % 2 else -1
        for s in (-0.35, 0.35):
            T.append(rod((-0.35, y - 0.45 * f, s), (0.35, y + 0.45 * f, s), 0.03, "steel_dark", 6, bevel=0.0))
            T.append(rod((s, y - 0.45 * f, -0.35), (s, y + 0.45 * f, 0.35), 0.03, "steel_dark", 6, bevel=0.0))
        for s in (-0.35, 0.35):
            T.append(box(0.7, 0.05, 0.05, "steel_dark", at(0, y + 0.6, s), bevel=0.0))
        y += 1.2
        k += 1
    # ladder up the back
    for sx in (-0.18, 0.18):
        T.append(box(0.04, 8.4, 0.04, "yellow", at(sx, 4.5, -0.55), bevel=0.0))
    for i in range(24):
        T.append(rod((-0.18, 0.6 + i * 0.35, -0.55), (0.18, 0.6 + i * 0.35, -0.55), 0.015, "yellow", 6, bevel=0.0))
    for sx in (-0.18, 0.18):
        for yy in (2.0, 5.0, 8.0):
            T.append(box(0.04, 0.04, 0.2, "steel_dark", at(sx, yy, -0.45), bevel=0.0))
    # service platform under the lamp head
    T.append(box(1.5, 0.06, 1.5, "steel", at(0, 8.55, 0), bevel=0.015, segs=2, name="platform"))
    for i in range(4):
        f = ry(i * HALF_PI)
        T.append(box(1.5, 0.04, 0.04, "yellow", f @ at(0, 9.0, 0.73), bevel=0.0))
        T.append(box(0.04, 0.45, 0.04, "yellow", f @ at(0.73, 8.8, 0.73), bevel=0.0))
    # tilted lamp head (same tilt as the code version)
    pq = rx(-0.35)
    head = at(0, 9.4, 0) @ pq
    T.append(box(2.2, 1.5, 0.3, "ink", head, bevel=0.04, segs=2, name="panel"))
    T.append(box(2.0, 1.3, 0.06, "steel_dark", head @ at(0, 0, -0.17), bevel=0.015, segs=2))
    for x in (-0.7, 0.0, 0.7):
        T.append(box(0.08, 1.4, 0.06, "steel_dark", head @ at(x, 0, -0.2), bevel=0.01, segs=2))
    T.append(box(0.4, 0.3, 0.18, "steel_dark", head @ at(0.6, -0.3, -0.28), bevel=0.02, segs=2, name="junction"))
    T.append(rod((0, 8.6, 0.0), (0, 9.15, -0.05), 0.06, "steel_dark", 10))
    G = []
    origins = {}
    for i in range(3):
        for j in range(2):
            lx, ly = -0.7 + i * 0.7, -0.35 + j * 0.7
            c = head @ at(lx, ly, 0.0)
            T.append(torus(0.29, 0.035, "steel", c @ at(0, 0, 0.17) @ rx(HALF_PI), seg=16, ring=6))
            T.append(cyl(0.31, 0.31, 0.06, 16, "steel_dark", c @ at(0, 0, 0.16) @ rx(HALF_PI), bevel=0.0))
            # visor over each lamp
            T.append(box(0.62, 0.025, 0.16, "ink", c @ at(0, 0.32, 0.25) @ rx(-0.25), bevel=0.0))
            name = f"glow_lamp{i * 2 + j}"
            G.append(lamp_disc(name, c @ at(0, 0, 0.2) @ rx(HALF_PI)))
            p = c @ at(0, 0, 0.2)
            origins[name] = tuple(p.translation)
    return dict(groups={"tower": T}, glows=G, origins=origins, tex=768)


# ------------------------------------------------------------------ banner
def wave(x, y):
    """Cloth ripple: z offset, stronger towards the free bottom edge."""
    k = 0.35 + 0.65 * (1.05 - y) / 2.1
    return 0.035 * k * math.sin(x * 4.6 + y * 0.9) + 0.012 * math.sin(x * 11.0 - y * 2.0)


def cloth(w, h, nx, ny, look, xf2d, dz, thick, name="cloth"):
    """Subdivided strip w x h in XY, placed by a 2D transform, rippled by wave()."""
    bm = bmesh.new()
    rows = []
    for j in range(ny + 1):
        row = []
        for i in range(nx + 1):
            p = xf2d @ lib.Vector((-w / 2 + w * i / nx, -h / 2 + h * j / ny, 0))
            row.append(bm.verts.new((p.x, p.y, wave(p.x, p.y) + dz)))
        rows.append(row)
    for j in range(ny):
        for i in range(nx):
            bm.faces.new((rows[j][i], rows[j][i + 1], rows[j + 1][i + 1], rows[j + 1][i]))
    ob = _bm_obj(bm, look, 0.0, name=name)
    s = ob.modifiers.new("solid", "SOLIDIFY")
    s.thickness = thick
    s.offset = 0.0
    return ob


def arena_banner():
    """cloth: banner (origin at its top-centre), rod: hanging rod. Front -Z."""
    C = [cloth(1.4, 2.1, 10, 14, "banner_bg", lib.Matrix.Identity(4), 0.0, 0.02)]
    for a, dz in ((0.7, -0.016), (-0.7, -0.024)):
        C.append(cloth(0.18, 1.3, 2, 10, "banner_fg", rz(a), dz, 0.006, name="cross"))
    # hem trim along the sides and bottom (cross colour)
    for sx in (-1, 1):
        C.append(cloth(0.06, 2.0, 1, 14, "banner_fg", lib.Matrix.Translation((sx * 0.64, -0.02, 0)), -0.014, 0.006, name="trim"))
    C.append(cloth(1.4, 0.07, 10, 1, "banner_fg", lib.Matrix.Translation((0, -1.0, 0)), -0.014, 0.006, name="trim"))
    C.append(cyl(0.05, 0.05, 1.42, 10, "banner_bg", at(0, 1.07, 0, rz(HALF_PI)), bevel=0.01))
    R = [cyl(0.04, 0.04, 1.6, 10, "steel_dark", at(0, 1.08, 0, rz(HALF_PI)), bevel=0.01, name="rod")]
    for sx in (-1, 1):
        R.append(cyl(0.065, 0.065, 0.08, 10, "steel_dark", at(sx * 0.8, 1.08, 0, rz(HALF_PI)), bevel=0.015))
        R.append(rod((sx * 0.55, 1.08, 0), (sx * 0.55, 1.6, 0.08), 0.012, "steel_dark", 6))
    return dict(groups={"cloth": C, "rod": R}, origins={"cloth": (0, 1.05, 0), "rod": (0, 1.08, 0)}, tex=512)


# ------------------------------------------------------------------ back wall
def arena_backwall():
    """2.5 m stadium wall segment, y -0.5..6.5, 0.5 thick, front at -Z."""
    W = [box(2.5, 7.0, 0.4, "stadium", at(0, 3.0, 0.05), bevel=0.03, segs=2, name="sheet")]
    W.append(box(2.5, 1.7, 0.5, "concrete", at(0, 0.35, 0), bevel=0.03, segs=2, name="plinth"))
    for i in range(10):
        W.append(box(0.09, 5.1, 0.05, "stadium", at(-1.125 + i * 0.25, 3.75, -0.17), bevel=0.012, segs=2))
    W.append(box(2.5, 0.22, 0.12, "red_dark", at(0, 4.0, -0.22), bevel=0.02, segs=2, name="band"))
    W.append(box(2.5, 0.22, 0.12, "red_dark", at(0, 1.3, -0.22), bevel=0.02, segs=2))
    W.append(box(2.5, 0.25, 0.6, "steel_dark", at(0, 6.4, 0), bevel=0.03, segs=2, name="cap"))
    # steel column in the middle of the segment
    W.append(box(0.34, 6.6, 0.1, "steel_dark", at(0, 3.25, -0.3), bevel=0.02, segs=2, name="column"))
    W.append(box(0.1, 6.6, 0.12, "steel_dark", at(0, 3.25, -0.22), bevel=0.0))
    for y in (1.3, 4.0):
        W += bolt_row((-0.12, y + 0.18, -0.355), (0.12, y + 0.18, -0.355), 2, 0.022, up=(0, 0, -1))
        W += bolt_row((-0.12, y - 0.18, -0.355), (0.12, y - 0.18, -0.355), 2, 0.022, up=(0, 0, -1))
    return dict(groups={"seg": W}, tex=1024)


# ------------------------------------------------------------------ barrier, cone
def arena_barrier():
    """2 m low barrier, base at origin, 0.8 tall; hazard cap tiles along X."""
    prof = [(-0.3, 0.0), (0.3, 0.0), (0.3, 0.1), (0.21, 0.26), (0.21, 0.58), (-0.21, 0.58), (-0.21, 0.26), (-0.3, 0.1)]
    B = [prism(prof, 2.0, "barrier", at(0, 0, 0), bevel=0.02, name="body")]
    B.append(box(2.0, 0.25, 0.56, "hazard_tile", at(0, 0.7, 0), bevel=0.012, segs=2, name="cap"))
    for s in (-1, 1):
        B += bolt_row((-0.8, 0.7, s * 0.285), (0.8, 0.7, s * 0.285), 4, 0.02, up=(0, 0, s))
        B.append(box(0.36, 0.08, 0.62, "ink", at(s * 0.55, 0.035, 0), bevel=0.0))
    return dict(groups={"seg": B}, tex=512)


def arena_cone():
    B = [box(0.52, 0.04, 0.52, "rubber_mat", at(0, 0.02, 0), bevel=0.012, segs=2, name="base")]
    B.append(cyl(0.03, 0.25, 0.66, 16, "cone", at(0, 0.37, 0), bevel=0.01, name="cone_body"))
    B.append(cyl(0.125, 0.17, 0.14, 16, "reflect", at(0, 0.37, 0), bevel=0.005, name="band"))
    return dict(groups={"cone": B}, tex=512)


# ------------------------------------------------------------------ crowd
def crowd_person():
    """body origin at the body centre, head origin 0.42 above it; faces +Z."""
    B = [box(0.36, 0.34, 0.28, "shirt", at(0, 0.1, 0), bevel=0.03, segs=1, name="torso")]
    B.append(box(0.34, 0.21, 0.26, "trousers", at(0, -0.17, 0), bevel=0.02, segs=1))
    for s in (-1, 1):
        B.append(box(0.08, 0.36, 0.12, "shirt", at(s * 0.215, 0.08, 0.0, rz(s * 0.12)), bevel=0.02, segs=1))
        B.append(box(0.07, 0.07, 0.08, "tint1", at(s * 0.235, -0.14, 0.0), bevel=0.015, segs=1))
    H = [box(0.28, 0.28, 0.28, "tint1", at(0, 0.42, 0), bevel=0.04, segs=1, name="head")]
    H.append(box(0.3, 0.07, 0.3, "hair", at(0, 0.54, -0.01), bevel=0.02, segs=1))
    H.append(box(0.3, 0.16, 0.06, "hair", at(0, 0.48, -0.13), bevel=0.015, segs=1))
    for s in (-1, 1):
        H.append(box(0.045, 0.06, 0.02, "ink", at(s * 0.06, 0.44, 0.14), bevel=0.0))
    return dict(groups={"body": B, "head": H}, origins={"body": (0, 0, 0), "head": (0, 0.42, 0)}, tex=256)


# ------------------------------------------------------------------ outer ground
def ground_material(period):
    """Sandy ground whose noise is periodic over `period` metres in X and Z
    (coordinates wrapped onto a 4D torus), so tiles meet without seams."""
    m = bpy.data.materials.new("ground")
    m.use_nodes = True
    nt = m.node_tree
    N, Lk = nt.nodes, nt.links
    N.clear()

    def node(kind, **kw):
        n = N.new(kind)
        for k, v in kw.items():
            setattr(n, k, v)
        return n

    def op(o, a, b=None):
        n = node("ShaderNodeMath", operation=o)
        for i, v in enumerate((a, b)):
            if v is None:
                continue
            if isinstance(v, (int, float)):
                n.inputs[i].default_value = v
            else:
                Lk.new(v, n.inputs[i])
        return n.outputs[0]

    def ramp(fac, stops):
        n = node("ShaderNodeValToRGB")
        els = n.color_ramp.elements
        els[0].position, els[0].color = stops[0][0], stops[0][1]
        els[1].position, els[1].color = stops[1][0], stops[1][1]
        for p, c in stops[2:]:
            els.new(p).color = c
        Lk.new(fac, n.inputs[0])
        return n.outputs[0]

    def g(v):
        return (v, v, v, 1.0)

    def mix(fac, a, b):
        n = node("ShaderNodeMix", data_type="RGBA")
        for sock, v in ((0, fac), (6, a), (7, b)):
            if isinstance(v, (tuple, int, float)):
                n.inputs[sock].default_value = v
            else:
                Lk.new(v, n.inputs[sock])
        return n.outputs[2]

    tc = node("ShaderNodeTexCoord")
    sep = node("ShaderNodeSeparateXYZ")
    Lk.new(tc.outputs["Object"], sep.inputs[0])
    R = period / TAU
    ax = op("MULTIPLY", sep.outputs["X"], TAU / period)
    ay = op("MULTIPLY", sep.outputs["Y"], TAU / period)
    cmb = node("ShaderNodeCombineXYZ")
    Lk.new(op("MULTIPLY", op("COSINE", ax), R), cmb.inputs[0])
    Lk.new(op("MULTIPLY", op("SINE", ax), R), cmb.inputs[1])
    Lk.new(op("MULTIPLY", op("COSINE", ay), R), cmb.inputs[2])
    vec, w = cmb.outputs[0], op("MULTIPLY", op("SINE", ay), R)

    def noise(scale, detail, rough, off=0.0):
        n = node("ShaderNodeTexNoise", noise_dimensions="4D")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = rough
        Lk.new(vec, n.inputs["Vector"])
        Lk.new(op("ADD", w, off), n.inputs["W"])
        return n.outputs["Fac"]

    def voronoi(scale, feature="F1", off=0.0):
        n = node("ShaderNodeTexVoronoi", voronoi_dimensions="4D", feature=feature)
        n.inputs["Scale"].default_value = scale
        Lk.new(vec, n.inputs["Vector"])
        Lk.new(op("ADD", w, off), n.inputs["W"])
        return n

    sand = lib.srgb("#bfa27a")
    dark = lib.srgb("#9c8058")
    light = lib.srgb("#d6bf96")
    big = ramp(noise(0.35, 3.0, 0.6), [(0.35, g(0.0)), (0.65, g(1.0))])
    col = mix(op("MULTIPLY", big, 1.0), dark, light)
    col = mix(0.45, col, sand)
    midn = ramp(noise(2.2, 4.0, 0.6, 5.0), [(0.3, g(0.0)), (0.7, g(1.0))])
    col = mix(op("MULTIPLY", midn, 0.35), col, lib.srgb("#a88a60"))
    # fine grain
    grain = ramp(noise(40.0, 2.0, 0.7, 9.0), [(0.3, g(0.0)), (0.7, g(1.0))])
    col = mix(op("MULTIPLY", grain, 0.25), col, lib.srgb("#8a7050"))
    # dried-mud cracks in patches
    cr = voronoi(2.2, "DISTANCE_TO_EDGE", 2.0)
    crack = ramp(cr.outputs["Distance"], [(0.0, g(1.0)), (0.035, g(0.0))])
    patch = ramp(noise(0.5, 2.0, 0.5, 20.0), [(0.55, g(0.0)), (0.65, g(1.0))])
    col = mix(op("MULTIPLY", op("MULTIPLY", crack, patch), 0.7), col, lib.srgb("#6e5638"))
    # pebbles
    pv = voronoi(5.0, "F1", 30.0)
    peb = ramp(pv.outputs["Distance"], [(0.07, g(1.0)), (0.11, g(0.0))])
    pick = ramp(noise(1.5, 1.0, 0.5, 40.0), [(0.45, g(0.0)), (0.6, g(1.0))])
    pc = mix(ramp(pv.outputs["Color"], [(0.4, g(0.0)), (0.6, g(1.0))]), lib.srgb("#7d7468"), lib.srgb("#c9c0b0"))
    col = mix(op("MULTIPLY", peb, pick), col, pc)
    # oil / scorch stains
    oil = ramp(noise(0.25, 2.0, 0.5, 60.0), [(0.66, g(0.0)), (0.74, g(1.0))])
    col = mix(op("MULTIPLY", oil, 0.55), col, lib.srgb("#4a3a2c"))

    out = node("ShaderNodeOutputMaterial")
    em = node("ShaderNodeEmission")
    Lk.new(col, em.inputs["Color"])
    Lk.new(em.outputs[0], out.inputs["Surface"])
    rgh = node("ShaderNodeCombineColor")
    rgh.inputs["Green"].default_value = 0.95
    rgh.inputs["Blue"].default_value = 0.0
    em2 = node("ShaderNodeEmission")
    em2.name = "MR_EMIT"
    Lk.new(rgh.outputs[0], em2.inputs["Color"])
    return m


def arena_ground():
    """8.5 m square sandy ground tile, top at the origin, seamless when laid
    on an unrotated 8.5 m grid."""
    o = lib.plane(GROUND_TILE, GROUND_TILE, "dirt", at(0, 0, 0), name="ground")
    o.data.materials.clear()
    o.data.materials.append(ground_material(GROUND_TILE))
    return dict(groups={"ground": [o]}, tex=int(__import__("os").environ.get("ART_GROUND_TEX", "2048")))


ASSETS = {
    "arena_tile": arena_tile, "arena_marks": arena_marks, "arena_pit": arena_pit, "arena_wall": arena_wall,
    "arena_stand": arena_stand, "arena_tower": arena_tower, "arena_banner": arena_banner,
    "arena_backwall": arena_backwall, "arena_barrier": arena_barrier, "arena_cone": arena_cone,
    "crowd_person": crowd_person, "arena_ground": arena_ground,
}

