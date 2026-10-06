#!/usr/bin/env python3
"""Reference re-implementation of the PS2LOGO streak/outline animation (see notes/ps2logo.md).

Reads the keyframe tables straight from the unpacked PS2LOGO image (extracted/ps2logo_100000.bin,
loaded at 0x100000) and renders the vector layers of one field `t` the way FUN_00104320 /
FUN_00104478 / FUN_001047c8 do (additive blend, no blur / feedback passes).

usage: logo_anim.py [--pal] [--out dir] t [t ...]      -> dir/logo_<t>.png (640x512)
       logo_anim.py --dump                               -> analysis/ps2logo/anim_tables.txt
"""
import math
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IMG = ROOT / "extracted" / "ps2logo_100000.bin"
BASE = 0x100000
data = IMG.read_bytes()


def u32(a):
    return struct.unpack_from("<I", data, a - BASE)[0]


def i32(a):
    return struct.unpack_from("<i", data, a - BASE)[0]


def f32(a):
    return struct.unpack_from("<f", data, a - BASE)[0]


# ----------------------------------------------------------------------------- tables
SHAPE_TABLE = {"NTSC": 0x12AFF8, "PAL": 0x12AFD8}     # 4 x {keyframes*, count}
COLOUR_TABLE = {"NTSC": 0x12B538, "PAL": 0x12B518}    # 4 x {keyframes*, count}
TYPE_TABLE = 0x121770       # per object, reversed: entry i -> object 3-i
LAYER_A = 0x121790          # drawn before the full-screen blur (objects 3 and 1)
LAYER_B = 0x1217B0          # drawn after it (objects 2 and 0)
RIBBON_MULT = [f32(0x12F228 + 4 * i) for i in range(5)]      # 0, .3, 1, .3, 0
RIBBON_DT = {0: f32(0x130A28), 1: f32(0x130A24), 2: f32(0x130A24)}   # 3.8 / 2.8 / 2.8
RIBBON_RATE = {"NTSC": f32(0x130A34), "PAL": f32(0x130A38)}          # 0.84 / 0.7
PAL_YSCALE = f32(0x130A2C) / f32(0x130A30)                           # 1.0926/0.9091 = 1.2018
SUBDIV_CHORD_RATIO = f32(0x12B04C)   # 3.5
SUBDIV_MAX_CHORD = f32(0x12B050)     # 2000
SUBDIV_N = f32(0x12B054)             # 4
EMIT_MIN_LEN = f32(0x12B044)         # 1.0


def load_shape(shape_ptr, n_polylines):
    """A shape = list of polylines; a polyline = list of (type, floats[6]) nodes.
    Applies the one-time init fix-up of FUN_00102e00 (x -= 128, y += 128)."""
    out = []
    for j in range(n_polylines):
        a = u32(shape_ptr + 4 * j)
        nodes = []
        while True:
            ty = i32(a + 0x20)
            f = [f32(a + 4 * i) for i in range(6)]
            if ty in (0, 1):
                f[0] -= 128.0
                f[1] += 128.0
            elif ty == 2:
                for i in range(0, 6, 2):
                    f[i] -= 128.0
                    f[i + 1] += 128.0
            nodes.append((ty, f))
            a += 0x30
            if ty == 3:
                break
        out.append(nodes)
    return out


def load_objects(region):
    objs = []
    base = SHAPE_TABLE[region]
    cbase = COLOUR_TABLE[region]
    for obj in range(4):
        kp, kn = u32(base + obj * 8), u32(base + obj * 8 + 4)
        keys = []
        for k in range(kn):
            t, sp, npl = i32(kp + k * 12), u32(kp + k * 12 + 4), i32(kp + k * 12 + 8)
            keys.append((t, load_shape(sp, npl)))
        cp, cn = u32(cbase + obj * 8), u32(cbase + obj * 8 + 4)
        colours = [tuple(i32(cp + k * 20 + 4 * i) for i in range(5)) for k in range(cn)]
        objs.append({"keys": keys, "colours": colours, "type": u32(TYPE_TABLE + 4 * (3 - obj)),
                     "layerA": u32(LAYER_A + 4 * (3 - obj)), "layerB": u32(LAYER_B + 4 * (3 - obj))})
    return objs


# ----------------------------------------------------------------------------- evaluation
def key_pair(keys, t):
    """(index of key <= t, index of next key, weight of the first) as FUN_00103020 does."""
    i = 0
    for k, key in enumerate(keys):
        if key[0] <= t:
            i = k
    if i >= len(keys) - 1:
        return i, i, 1.0, False
    t0, t1 = keys[i][0], keys[i + 1][0]
    return i, i + 1, (t1 - t) / (t1 - t0), True


def lerp_nodes(a, b, w):
    """Morph two shapes (same node structure) with weight w on the first (FUN_001035b8)."""
    return [[(ty, [x * w + y * (1 - w) for x, y in zip(fa, fb)]) for (ty, fa), (_, fb) in zip(pa, pb)]
            for pa, pb in zip(a, b)]


def subdiv_count(p0, node):
    """FUN_00104088: 4 steps normally, 1 (straight line) for very bent curves."""
    c1 = node[0:2]
    c2 = node[2:4]
    p3 = node[4:6]
    chord = math.hypot(p0[0] - p3[0], p0[1] - p3[1])
    if chord == 0.0:
        return 0
    # control polygon length after one de Casteljau split (FUN_001041c0), truncated to int
    m01 = ((p0[0] + c1[0]) / 2, (p0[1] + c1[1]) / 2)
    m12 = ((c1[0] + c2[0]) / 2, (c1[1] + c2[1]) / 2)
    m23 = ((c2[0] + p3[0]) / 2, (c2[1] + p3[1]) / 2)
    m012 = ((m01[0] + m12[0]) / 2, (m01[1] + m12[1]) / 2)
    m123 = ((m12[0] + m23[0]) / 2, (m12[1] + m23[1]) / 2)
    mid = ((m012[0] + m123[0]) / 2, (m012[1] + m123[1]) / 2)
    pts = [p0, m01, m012, mid, m123, m23, p3]
    L = int(sum(math.hypot(pts[i][0] - pts[i + 1][0], pts[i][1] - pts[i + 1][1]) for i in range(6)))
    n = 1
    if (chord * SUBDIV_CHORD_RATIO < L) or (chord > SUBDIV_MAX_CHORD):
        n = 1
    else:
        n = int(SUBDIV_N + 0.5)
    return max(n, 1)


def bezier_points(p0, node, n, emit_min=None):
    """Interior samples s = 1/n .. (n-1)/n of the cubic (p0, c1, c2, p3)."""
    c1 = node[0:2]
    c2 = node[2:4]
    p3 = node[4:6]
    out = []
    prev = p0
    acc = 0.0
    for i in range(1, n):
        s = i / n
        u = 1 - s
        x = u * u * u * p0[0] + 3 * u * u * s * c1[0] + 3 * u * s * s * c2[0] + s * s * s * p3[0]
        y = u * u * u * p0[1] + 3 * u * u * s * c1[1] + 3 * u * s * s * c2[1] + s * s * s * p3[1]
        if emit_min is None:
            out.append((x, y))
        else:
            acc += math.hypot(x - prev[0], y - prev[1])
            if acc > emit_min:
                out.append((x, y))
                acc = 0.0
        prev = (x, y)
    return out


def flatten(shape, counts=None, emit_min=EMIT_MIN_LEN):
    """Polyline nodes -> point lists, following FUN_00103460 (type 0 objects) or
    FUN_00103768 with precomputed counts (ribbons)."""
    out = []
    ci = 0
    for nodes in shape:
        pts = []
        prev = None
        for ty, f in nodes:
            if ty == 0:
                prev = (f[0], f[1])
                pts.append(prev)
            elif ty == 1:
                pts.append(prev)
                prev = (f[0], f[1])
            elif ty == 2:
                pts.append(prev)
                if counts is None:
                    n = subdiv_count(prev, f)
                    pts += bezier_points(prev, f, n, emit_min)
                else:
                    n = counts[ci]
                    ci += 1
                    pts += bezier_points(prev, f, n, None)
                prev = (f[4], f[5])
            elif ty == 3:
                pts.append(prev)
        out.append(pts)
    return out


def shape_at(obj, t, counts=None):
    keys = obj["keys"]
    i, j, w, running = key_pair(keys, max(t, 0))
    nodes = lerp_nodes(keys[i][1], keys[j][1], w) if i != j else keys[i][1]
    return [n for n in nodes], running


def segment_counts(obj, t):
    """FUN_00103318: subdivision count per bezier segment from the shape at time t."""
    nodes, _ = shape_at(obj, t)
    counts = []
    for pl in nodes:
        prev = None
        for ty, f in pl:
            if ty in (0, 1):
                prev = (f[0], f[1])
            elif ty == 2:
                counts.append(subdiv_count(prev, f))
                prev = (f[4], f[5])
    return counts


def colour_at(obj, t):
    """FUN_00105a70: rgb * a/128, clamped to 255."""
    cols = obj["colours"]
    i, j, w, _ = key_pair(cols, t)
    r, g, b, a = [cols[i][k + 1] * w + cols[j][k + 1] * (1 - w) for k in range(4)]
    a = min(a, 255.0)
    return tuple(min(c * a / 128.0, 255.0) for c in (r, g, b))


def to_screen(p, region):
    y = p[1] * PAL_YSCALE if region == "PAL" else p[1]
    return (p[0] + 320.0, 512.0 - y - 256.0)


# ----------------------------------------------------------------------------- render
def render(objs, t, region, size=(640, 512), layers=("A", "B")):
    import numpy as np
    from PIL import Image, ImageDraw

    acc = np.zeros((size[1], size[0], 3), dtype=np.float32)
    for idx in (3, 2, 1, 0):
        obj = objs[idx]
        if not any(obj["layer" + L] for L in layers):
            continue
        if obj["type"] == 0:
            nodes, _ = shape_at(obj, t)
            col = colour_at(obj, t)
            img = Image.new("RGB", size, (0, 0, 0))
            dr = ImageDraw.Draw(img)
            for pl in flatten(nodes):
                pts = [to_screen(p, region) for p in pl]
                if len(pts) >= 2:
                    dr.line(pts, fill=tuple(int(c) for c in col), width=1)
            acc += np.asarray(img, dtype=np.float32)
        else:
            rate = RIBBON_RATE[region]
            dt = RIBBON_DT[idx]
            counts = segment_counts(obj, t)
            times = [t - int(dt * 4 * rate)] + [t - int(dt * (4 - k) * rate) for k in range(1, 5)]
            base = colour_at(obj, t)
            curves = []
            for tt in times:
                nodes, _ = shape_at(obj, tt)
                curves.append([[to_screen(p, region) for p in pl] for pl in flatten(nodes, counts)])
            for k in range(1, 5):
                ca = tuple(int(c * RIBBON_MULT[k - 1]) for c in base)
                cb = tuple(int(c * RIBBON_MULT[k]) for c in base)
                img = Image.new("RGB", size, (0, 0, 0))
                dr = ImageDraw.Draw(img)
                for pa, pb in zip(curves[k - 1], curves[k]):
                    for i in range(min(len(pa), len(pb)) - 1):
                        # two triangles of the strip, flat-shaded with the average colour
                        cm = tuple((x + y) // 2 for x, y in zip(ca, cb))
                        dr.polygon([pa[i], pb[i], pb[i + 1], pa[i + 1]], fill=cm)
                acc += np.asarray(img, dtype=np.float32)
    return Image.fromarray(np.clip(acc, 0, 255).astype(np.uint8))


def dump(path):
    with open(path, "w") as f:
        for region in ("NTSC", "PAL"):
            objs = load_objects(region)
            f.write(f"=== {region}\n")
            for i, obj in enumerate(objs):
                f.write(f"object {i}: type {obj['type']} layerA={obj['layerA']} layerB={obj['layerB']}\n")
                f.write("  colour keys (t, r, g, b, a): " + ", ".join(map(str, obj["colours"])) + "\n")
                for t, shape in obj["keys"]:
                    f.write(f"  shape key t={t}: {len(shape)} polylines\n")
                    for j, pl in enumerate(shape):
                        f.write(f"    polyline {j}:\n")
                        for ty, fl in pl:
                            f.write(f"      {ty} " + " ".join(f"{v:9.3f}" for v in fl) + "\n")


if __name__ == "__main__":
    args = sys.argv[1:]
    if "--dump" in args:
        out = ROOT / "analysis" / "ps2logo" / "anim_tables.txt"
        dump(out)
        print("wrote", out)
        sys.exit()
    region = "PAL" if "--pal" in args else "NTSC"
    outdir = Path(args[args.index("--out") + 1]) if "--out" in args else ROOT / "analysis" / "ps2logo" / "frames"
    outdir.mkdir(parents=True, exist_ok=True)
    objs = load_objects(region)
    for a in args:
        if a.lstrip("-").isdigit() and not a.startswith("--"):
            t = int(a)
            img = render(objs, t, region)
            img.save(outdir / f"logo_{region}_{t:02d}.png")
            print("wrote", outdir / f"logo_{region}_{t:02d}.png")
