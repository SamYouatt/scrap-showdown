"""Robot parts. Part-local space (see src/parts.rs): the attach face is at z = 0
and the part extends along +Z; +Y is the part's "up" arrow.

Moving groups are modelled where they sit at rest (angle 0) and given a pivot
in `origins`; the game rotates them about it:
  spin  - wheel tyre / saw disc, spins about local Z
  arm   - hammer / flipper, swings about local X
  whip  - the antenna, squashed along Y so it bends instead of clipping
  upper / lower / foot - leg segments, each in its own frame (posed by IK)
Paint (`paint*` looks) takes the bot's colour at runtime.
Keep the numbers below in step with HAMMER / FLIP / WEDGE_PROFILE in parts.rs.
"""
import math

from lib import (HALF_PI, TAU, at, bolt, bolt_row, box, cable, corners, cyl, face_bolts, glow_sphere,
                 hazard_box, prism, rod, rx, ry, rz, scl, vents)
from mathutils import Matrix, Vector

HAMMER_PIVOT = (0.0, -0.1, 0.24)
HAMMER_LEN = 0.7
FLIP_HINGE = (0.0, -0.2, 0.15)
FLIP_LEN = 0.82
WEDGE_PROFILE = [(0.0, -0.19), (0.0, 0.19), (0.62, -0.17), (0.62, -0.21)]


def v(x, y, z):
    return Vector((x, y, z))


def core():
    """Brain Box: the bot's head/body with glowing eyes."""
    B = []
    B.append(box(0.54, 0.4, 0.54, "paint", at(0, 0, 0), bevel=0.05, name="shell"))
    B.append(box(0.565, 0.085, 0.565, "paint_dark", at(0, -0.13, 0), bevel=0.02, name="band"))
    # face plate, eye sockets, grille
    B.append(box(0.44, 0.24, 0.06, "ink", at(0, 0.04, 0.28), bevel=0.025, name="face"))
    for sx in (-0.1, 0.1):
        B.append(cyl(0.078, 0.078, 0.06, 20, "steel_dark", at(sx, 0.045, 0.31, rx(HALF_PI)), bevel=0.012))
        B.append(cyl(0.06, 0.06, 0.02, 20, "ink", at(sx, 0.045, 0.335, rx(HALF_PI)), bevel=0.004))
    B += vents(0.22, 0.07, 3, "steel_dark", at(0, -0.075, 0.315))
    # top hatch, handle, hinges, corner bolts
    B.append(box(0.3, 0.045, 0.28, "paint_dark", at(0, 0.215, -0.04), bevel=0.015, name="hatch"))
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
    B.append(cyl(0.035, 0.048, 0.06, 12, "steel_dark", at(-0.17, 0.225, -0.17)))
    B.append(cable([v(0.12, 0.2, -0.22), v(0.18, 0.31, -0.31), v(0.23, 0.12, -0.3), v(0.2, 0.0, -0.275)], 0.018, "blue_cable"))
    # springy antenna: its own group so it can bend
    W = [rod((-0.17, 0.24, -0.17), (-0.17, 0.62, -0.19), 0.012, "steel_dark", 6)]
    glows = [glow_sphere(0.05, n, (sx, 0.045, 0.335), (1, 1, 0.5)) for n, sx in (("glow_eye_l", -0.1), ("glow_eye_r", 0.1))]
    glows.append(glow_sphere(0.03, "glow_tip", (-0.17, 0.635, -0.19)))
    origins = {"whip": (-0.17, 0.24, -0.17), "glow_eye_l": (-0.1, 0.045, 0.335), "glow_eye_r": (0.1, 0.045, 0.335), "glow_tip": (-0.17, 0.635, -0.19)}
    return dict(groups={"body": B, "whip": W}, glows=glows, origins=origins)


def panel_box(w, h, d, look, panel, xf):
    """A box with a recessed darker panel + bolts on each of its 4 sides and top."""
    out = [box(w, h, d, look, xf, bevel=0.035)]
    for ang, length in ((0.0, w), (HALF_PI, d), (math.pi, w), (-HALF_PI, d)):
        depth = d if ang in (0.0, math.pi) else w
        f = xf @ ry(ang)
        out.append(box(length * 0.66, h * 0.55, 0.02, panel, f @ at(0, 0, depth / 2), bevel=0.008, name="panel"))
        for sx in (-1, 1):
            for sy in (-1, 1):
                out.append(bolt(0.018, "steel_dark", f @ at(sx * length * 0.41, sy * h * 0.36, depth / 2 + 0.005, rx(HALF_PI))))
    out.append(box(w * 0.7, 0.02, d * 0.7, panel, xf @ at(0, h / 2, 0), bevel=0.008))
    for sx in (-1, 1):
        for sz in (-1, 1):
            out.append(bolt(0.022, "steel_dark", xf @ at(sx * (w * 0.46 - 0.03), h / 2 + 0.01, sz * (d * 0.46 - 0.03))))
    return out


def block():
    B = panel_box(0.5, 0.36, 0.5, "steel", "steel_dark", at(0, 0, 0.25))
    # weld seam where it meets the parent
    B.append(box(0.42, 0.28, 0.02, "steel_dark", at(0, 0, 0.01), bevel=0.005))
    return dict(groups={"body": B})


def slab():
    l = 0.9
    B = [box(0.5, 0.14, l, "steel", at(0, 0, l / 2), bevel=0.03)]
    # lightening holes (dark recessed discs)
    for i in range(3):
        B.append(cyl(0.075, 0.075, 0.02, 20, "ink", at(0, 0.067, 0.18 + i * 0.27), bevel=0.004))
        B.append(cyl(0.09, 0.09, 0.012, 20, "steel_dark", at(0, 0.068, 0.18 + i * 0.27), bevel=0.003))
    for sx in (-1, 1):
        B.append(box(0.04, 0.05, l * 0.96, "steel_dark", at(sx * 0.23, 0.085, l / 2), bevel=0.01))
        B += bolt_row((sx * 0.17, 0.075, 0.08), (sx * 0.17, 0.075, l - 0.08), 5, 0.017)
    B.append(hazard_box(0.5, 0.06, 0.06, at(0, 0, l - 0.02)))
    return dict(groups={"body": B})


def girder():
    l, s = 0.84, 0.075
    B = []
    for sx in (-1, 1):
        for sy in (-1, 1):
            B.append(box(0.035, 0.035, l, "girder", at(sx * s, sy * s, l / 2), bevel=0.008))
    n = 5
    for i in range(n):
        z0, z1 = i / n * l, (i + 1) / n * l
        f = 1 if i % 2 == 1 else -1
        for sx in (-1, 1):
            B.append(rod((sx * s, -s * f, z0), (sx * s, s * f, z1), 0.012, "girder", 6))
        for sy in (-1, 1):
            B.append(rod((-s * f, sy * s, z0), (s * f, sy * s, z1), 0.012, "girder", 6))
    for z in (0.02, l - 0.02):
        B.append(box(0.2, 0.2, 0.04, "steel_dark", at(0, 0, z), bevel=0.01))
        B += face_bolts(0.18, 0.18, z + 0.02, 0.02, 0.016)
    return dict(groups={"body": B})


def armour():
    B = [box(0.6, 0.48, 0.08, "paint", at(0, 0, 0.04), bevel=0.025)]
    B.append(box(0.42, 0.3, 0.03, "paint_dark", at(0, 0.02, 0.085), bevel=0.01))
    B.append(hazard_box(0.3, 0.07, 0.014, at(0, -0.19, 0.088), bevel=0.004))
    for i in range(6):
        for sy in (-1, 1):
            B.append(bolt(0.016, "steel", at(-0.25 + i * 0.1, sy * 0.2, 0.085, rx(HALF_PI))))
    return dict(groups={"body": B})


def battery():
    z = 0.15
    B = [box(0.38, 0.28, 0.3, "battery", at(0, 0, z), bevel=0.025)]
    B.append(box(0.385, 0.1, 0.305, "yellow", at(0, 0.03, z), bevel=0.01))
    B.append(box(0.15, 0.04, 0.307, "ink", at(-0.07, 0.03, z), bevel=0.005))
    for sx, look in ((-0.1, "red"), (0.1, "ink")):
        B.append(cyl(0.035, 0.04, 0.06, 10, look, at(sx, 0.06, z + 0.17, rx(HALF_PI)), bevel=0.006))
        cl = "red_cable" if look == "red" else "ink"
        B.append(cable([v(sx, 0.06, z + 0.2), v(sx * 1.6, 0.13, z + 0.22), v(sx * 2.0, 0.17, z + 0.05), v(sx * 1.9, 0.15, z - 0.15)], 0.014, cl))
    B.append(rod((-0.12, -0.1, z + 0.16), (0.12, -0.1, z + 0.16), 0.012, "ink", 6))
    for sx in (-0.12, 0.12):
        B.append(rod((sx, -0.1, z + 0.16), (sx, -0.1, z + 0.13), 0.012, "ink", 6))
    return dict(groups={"body": B})


def wheel(big=False):
    """Bracket + motor (body) and a spinning tyre (spin)."""
    r, w = (0.42, 0.3) if big else (0.3, 0.22)
    zc = (0.26 if big else 0.2) + 0.02
    B, S = [], []
    B.append(box(0.2, 0.2, 0.05, "steel_dark", at(0, 0, 0.025), bevel=0.015))
    B += face_bolts(0.18, 0.18, 0.05, 0.02, 0.016)
    mlen = zc - w / 2 - 0.04
    B.append(cyl(0.075, 0.075, mlen, 16, "orange", at(0, 0, (zc - w / 2 + 0.04) / 2 + 0.01, rx(HALF_PI)), bevel=0.012))
    for i in range(3):
        B.append(cyl(0.095, 0.095, 0.014, 16, "steel_dark", at(0, 0, 0.08 + i * 0.03, rx(HALF_PI)), bevel=0.004))
    c = at(0, 0, zc)
    # tyre: a fat rounded drum with chunky lugs (lug tips = wheel_radius)
    S.append(cyl(r - 0.02, r - 0.02, w, 36, "tyre", c @ rx(HALF_PI), bevel=0.06, name="tyre"))
    if big:
        n = 14
        for i in range(n):
            for side in (-1, 1):
                a = (i + (0.5 if side > 0 else 0.0)) / n * TAU
                S.append(box(0.12, 0.07, w * 0.5, "tyre", c @ at(math.cos(a) * (r + 0.01), math.sin(a) * (r + 0.01), side * w * 0.25, rz(a - HALF_PI) @ ry(side * 0.35)), bevel=0.015, name="lug"))
    else:
        n = 12
        for i in range(n):
            a = i / n * TAU
            S.append(box(0.08, 0.05, w * 0.84, "tyre", c @ at(math.cos(a) * (r + 0.01), math.sin(a) * (r + 0.01), 0, rz(a - HALF_PI)), bevel=0.012, name="lug"))
    rim = "red" if big else "yellow"
    for sz in (-1, 1):
        S.append(cyl(r * 0.62, r * 0.62, 0.035, 28, rim, c @ at(0, 0, sz * (w / 2 + 0.002), rx(HALF_PI)), bevel=0.012, name="rim"))
    S.append(cyl(r * 0.24, r * 0.28, 0.06, 16, "steel_dark", c @ at(0, 0, w / 2 + 0.03, rx(HALF_PI)), bevel=0.012))
    for i in range(5):
        a = i / 5 * TAU
        S.append(box(0.04, r * 0.48, 0.03, rim, c @ at(math.cos(a) * r * 0.36, math.sin(a) * r * 0.36, w / 2 + 0.022, rz(a - HALF_PI)), bevel=0.01))
        S.append(bolt(0.017, "steel", c @ at(math.cos(a + 0.6) * r * 0.17, math.sin(a + 0.6) * r * 0.17, w / 2 + 0.065, rx(HALF_PI))))
    return dict(groups={"body": B, "spin": S}, origins={"spin": (0, 0, zc)})


def leg():
    """Hip actuator (body); thigh, shin and foot each in their own frame
    (origin at the joint, segment along +Z), posed by IK in the game."""
    B = [box(0.28, 0.3, 0.12, "steel_dark", at(0, 0, 0.06), bevel=0.02)]
    B += face_bolts(0.26, 0.28, 0.12, 0.03, 0.016)
    B.append(cyl(0.12, 0.12, 0.18, 20, "orange", at(0, 0, 0.19, rx(HALF_PI)), bevel=0.02))
    B.append(cyl(0.05, 0.05, 0.04, 12, "steel", at(0, 0, 0.29, rx(HALF_PI)), bevel=0.008))
    U = [box(0.13, 0.13, 0.44, "orange", at(0, 0, 0.21), bevel=0.03)]
    U.append(rod((0.08, 0.04, 0.05), (0.08, 0.04, 0.36), 0.025, "steel_clean", 10))
    U.append(box(0.06, 0.06, 0.12, "steel_dark", at(0.08, 0.04, 0.08), bevel=0.01))
    U += bolt_row((-0.066, 0.0, 0.08), (-0.066, 0.0, 0.36), 3, 0.014, up=(-1, 0, 0))
    L = [box(0.1, 0.1, 0.42, "steel", at(0, 0, 0.22), bevel=0.02)]
    L.append(cyl(0.085, 0.085, 0.17, 16, "steel_dark", rz(HALF_PI), bevel=0.012))
    L.append(cyl(0.04, 0.04, 0.19, 10, "yellow", rz(HALF_PI), bevel=0.008))
    F = [box(0.24, 0.08, 0.3, "steel_dark", at(0, -0.02, 0.04), bevel=0.02)]
    for sx in (-0.08, 0.0, 0.08):
        F.append(box(0.06, 0.05, 0.08, "orange", at(sx, -0.03, 0.22), bevel=0.01))
    F.append(cyl(0.05, 0.05, 0.1, 12, "steel", at(0, 0.02, 0.0, rz(HALF_PI)), bevel=0.01))
    return dict(groups={"body": B, "upper": U, "lower": L, "foot": F})


def spinner():
    """Finned motor housing (body) and a toothed disc (spin)."""
    B, S = [], []
    B.append(box(0.36, 0.36, 0.14, "steel_dark", at(0, 0, 0.07), bevel=0.03))
    for i in range(4):
        B.append(box(0.39, 0.022, 0.1, "ink", at(0, -0.12 + i * 0.08, 0.07), bevel=0.008))
    B += face_bolts(0.34, 0.34, 0.14, 0.03, 0.016)
    B.append(cyl(0.08, 0.1, 0.07, 16, "ink", at(0, 0, 0.16, rx(HALF_PI)), bevel=0.01))
    c = at(0, 0, 0.2)
    S.append(cyl(0.42, 0.42, 0.05, 48, "blade", c @ rx(HALF_PI), bevel=0.01, name="disc"))
    S.append(cyl(0.15, 0.15, 0.09, 24, "red", c @ rx(HALF_PI), bevel=0.02, name="hub"))
    for i in range(6):
        a = i / 6 * TAU
        S.append(bolt(0.018, "steel", c @ at(math.cos(a) * 0.1, math.sin(a) * 0.1, 0.05, rx(HALF_PI))))
    for i in range(3):
        a = i / 3 * TAU
        tooth = cyl(0.0, 0.11, 0.18, 4, "blade", Matrix.Identity(4), bevel=0.01, name="tooth")
        tooth.data.transform(c @ at(math.cos(a) * 0.46, math.sin(a) * 0.46, 0, rz(a - HALF_PI)) @ scl(1, 1, 0.4))
        S.append(tooth)
        S.append(box(0.14, 0.03, 0.06, "red", c @ at(math.cos(a + 1) * 0.28, math.sin(a + 1) * 0.28, 0, rz(a + 1)), bevel=0.008))
    return dict(groups={"body": B, "spin": S}, origins={"spin": (0, 0, 0.2)})


def hammer():
    B = [box(0.4, 0.4, 0.12, "steel_dark", at(0, 0, 0.06), bevel=0.025)]
    B += face_bolts(0.38, 0.38, 0.12, 0.04, 0.017)
    _, py, pz = HAMMER_PIVOT
    for sx in (-0.11, 0.11):
        B.append(box(0.05, 0.14, pz + 0.02, "ink", at(sx, py, (pz + 0.02) / 2 + 0.05), bevel=0.01))
    # pneumatic ram
    B.append(rod((0, 0.12, 0.12), (0, py + 0.12, pz + 0.05), 0.04, "steel_clean", 12))
    B.append(rod((0, 0.12, 0.12), (0, 0.14, 0.24), 0.05, "yellow", 12))
    # arm, modelled at rest (angle 0: shaft along +Z from the pivot, blade +Y)
    p = at(*HAMMER_PIVOT)
    l = HAMMER_LEN
    A = [cyl(0.06, 0.06, 0.26, 16, "yellow", p @ rz(HALF_PI), bevel=0.012)]
    A.append(box(0.08, 0.08, l, "steel", p @ at(0, 0, l / 2), bevel=0.015))
    A.append(rod(p @ v(0, -0.07, 0.05), p @ v(0, 0, l * 0.6), 0.016, "steel_dark", 6))
    h = p @ at(0, 0, l)
    A.append(box(0.24, 0.2, 0.22, "paint", h, bevel=0.03, name="head"))
    A.append(box(0.27, 0.08, 0.24, "ink", h, bevel=0.01))
    blade = cyl(0.0, 0.16, 0.24, 4, "blade", Matrix.Identity(4), bevel=0.012, name="blade")
    blade.data.transform(h @ at(0, 0.21, 0) @ ry(math.pi / 4) @ rz(math.pi) @ scl(1, 1, 0.35))
    A.append(blade)
    A.append(cyl(0.08, 0.09, 0.07, 16, "steel", h @ at(0, -0.13, 0) @ rx(math.pi), bevel=0.012))
    A += [bolt(0.016, "steel_dark", h @ at(sx, 0, 0.12, rx(HALF_PI))) for sx in (-0.08, 0.08)]
    return dict(groups={"body": B, "arm": A}, origins={"arm": HAMMER_PIVOT})


def flipper():
    B = [box(0.48, 0.44, 0.1, "steel_dark", at(0, 0, 0.05), bevel=0.02)]
    B += face_bolts(0.46, 0.42, 0.1, 0.04, 0.016)
    _, hy, hz = FLIP_HINGE
    for sx in (-0.18, 0.0, 0.18):
        B.append(cyl(0.035, 0.035, 0.1, 12, "steel", at(sx, hy, hz, rz(HALF_PI)), bevel=0.008))
    B.append(rod((0, 0, 0.08), (0, 0.12, 0.16), 0.05, "yellow", 12))
    p = at(*FLIP_HINGE)
    l = FLIP_LEN
    A = [box(0.5, l, 0.045, "yellow", p @ at(0, l / 2, 0.02), bevel=0.012, name="plate")]
    for i in range(4):
        A.append(box(0.5, 0.06, 0.012, "ink", p @ at(0, 0.2 + i * 0.15, 0.047), bevel=0.003))
    for sx in (-0.18, 0.18):
        A.append(box(0.03, l * 0.9, 0.05, "steel_dark", p @ at(sx, l * 0.45, -0.02), bevel=0.008))
    A.append(box(0.52, 0.05, 0.03, "steel", p @ at(0, l - 0.01, 0), bevel=0.008))
    for sx in (-0.09, 0.09):
        A.append(cyl(0.03, 0.03, 0.08, 12, "steel_dark", p @ at(sx, 0, 0, rz(HALF_PI)), bevel=0.006))
    return dict(groups={"body": B, "arm": A}, origins={"arm": FLIP_HINGE})


def spike():
    B = [cyl(0.16, 0.18, 0.08, 20, "steel_dark", at(0, 0, 0.04, rx(HALF_PI)), bevel=0.015)]
    for i in range(6):
        a = i / 6 * TAU
        B.append(bolt(0.016, "steel_dark", at(math.cos(a) * 0.13, math.sin(a) * 0.13, 0.085, rx(HALF_PI))))
    B.append(cyl(0.12, 0.13, 0.08, 20, "steel", at(0, 0, 0.12, rx(HALF_PI)), bevel=0.012))
    B.append(cyl(0.135, 0.135, 0.03, 20, "red", at(0, 0, 0.17, rx(HALF_PI)), bevel=0.006))
    B.append(cyl(0.0, 0.12, 0.56, 16, "blade", at(0, 0, 0.44, rx(HALF_PI)), bevel=0.01, name="spike"))
    for i in range(4):
        a = i / 4 * TAU + math.pi / 4
        B.append(cyl(0.0, 0.04, 0.2, 8, "steel", at(math.cos(a) * 0.1, math.sin(a) * 0.1, 0.26, rx(HALF_PI)), bevel=0.004))
    return dict(groups={"body": B})


def wedge():
    B = [prism(WEDGE_PROFILE, 0.5, "olive", Matrix.Identity(4), bevel=0.012, name="ramp")]
    tz, ty = WEDGE_PROFILE[2]
    top_y = WEDGE_PROFILE[1][1]
    B.append(hazard_box(0.52, 0.035, 0.06, at(0, ty - 0.015, tz - 0.01), bevel=0.006))
    slope = math.atan2(top_y - ty, tz)
    for sx in (-0.15, 0.15):
        B.append(box(0.03, 0.03, math.hypot(tz, top_y - ty) * 0.9, "steel_dark", at(sx, (top_y + ty) / 2 + 0.02, tz / 2, rx(slope)), bevel=0.006))
    B.append(box(0.5, 0.36, 0.03, "steel_dark", at(0, 0, 0.015), bevel=0.008))
    return dict(groups={"body": B})


ASSETS = {
    "core": core, "block": block, "slab": slab, "girder": girder, "armour": armour, "battery": battery,
    "wheel": wheel, "bigwheel": lambda: wheel(True), "leg": leg, "spinner": spinner, "hammer": hammer,
    "flipper": flipper, "spike": spike, "wedge": wedge,
}
