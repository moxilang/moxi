"""Studio render of a Moxi mesh: smooth normals, soft key shadow, dome AO,
GGX highlights, fake subsurface wrap, ACES tone map. Same lights for every
model, so differences between two renders come from the geometry and the
materials, not from the renderer.

usage: studio.py world.obj scene.json out.png [--yaw DEG] [--pitch DEG] [--size W H] [--dist K]
scene.json is `moxi scene` output (for per-part roughness/metalness); may be '-'.
"""
import sys, json, math, argparse
import numpy as np
from numba import njit, prange
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument("obj"); ap.add_argument("scene"); ap.add_argument("png")
ap.add_argument("--yaw", type=float, default=28.0)
ap.add_argument("--pitch", type=float, default=10.0)
ap.add_argument("--size", type=int, nargs=2, default=[900, 1000])
ap.add_argument("--dist", type=float, default=2.6)
ap.add_argument("--ss", type=int, default=2)
ap.add_argument("--fill", type=float, default=0.84)
ap.add_argument("--exposure", type=float, default=0.62)
ap.add_argument("--sss", type=str, default="", help="comma list of material names that get skin-like wrap")
a = ap.parse_args()

# ---------------------------------------------------------------- load
mtl = {}
cur = None
for line in open(a.obj.replace(".obj", ".mtl")):
    p = line.split()
    if not p: continue
    if p[0] == "newmtl": cur = p[1]
    elif p[0] == "Kd": mtl[cur] = [float(x) for x in p[1:4]]

part_rough = {}
part_mat = {}
if a.scene != "-":
    sc = json.load(open(a.scene))
    for layer in sc["layers"]:
        for prt in layer["parts"]:
            part_rough[prt["name"]] = (prt.get("roughness", 0.5), prt.get("metalness", 0.0))
            part_mat[prt["name"]] = prt.get("material", "")
sss_mats = set(s for s in a.sss.split(",") if s)

V, F, fcol, frough, fsss = [], [], [], [], []
col, rough, sss = [0.7, 0.7, 0.7], 0.5, 0.0
for line in open(a.obj):
    p = line.split()
    if not p: continue
    if p[0] == "v": V.append([float(x) for x in p[1:4]])
    elif p[0] == "o":
        name = " ".join(p[1:])
        rough = part_rough.get(name, (0.5, 0.0))[0]
        sss = 1.0 if part_mat.get(name, "") in sss_mats else 0.0
    elif p[0] == "usemtl": col = mtl.get(p[1], col)
    elif p[0] == "f":
        idx = [int(t.split("/")[0]) - 1 for t in p[1:]]
        for k in range(1, len(idx) - 1):
            F.append([idx[0], idx[k], idx[k + 1]]); fcol.append(col); frough.append(rough); fsss.append(sss)
V = np.array(V, np.float64); F = np.array(F, np.int64)
fcol = np.array(fcol, np.float64) ** 1.0; frough = np.array(frough); fsss = np.array(fsss)
# sRGB base colors -> linear
fcol = np.where(fcol <= 0.04045, fcol / 12.92, ((fcol + 0.055) / 1.055) ** 2.4)

# smooth vertex normals (area weighted)
fn = np.cross(V[F[:, 1]] - V[F[:, 0]], V[F[:, 2]] - V[F[:, 0]])
VN = np.zeros_like(V)
for k in range(3): np.add.at(VN, F[:, k], fn)
VN /= np.linalg.norm(VN, axis=1, keepdims=True) + 1e-12

lo, hi = V.min(0), V.max(0)
center = (lo + hi) / 2; radius = np.linalg.norm(hi - lo) / 2
floor_y = lo[1]

# ground quad (a big disc of two triangles, its own material)
G = radius * 6
gv = np.array([[center[0] - G, floor_y, center[2] - G], [center[0] + G, floor_y, center[2] - G],
               [center[0] + G, floor_y, center[2] + G], [center[0] - G, floor_y, center[2] + G]])
gbase = len(V)
V = np.vstack([V, gv]); VN = np.vstack([VN, np.tile([0, 1, 0], (4, 1))])
F = np.vstack([F, [[gbase, gbase + 2, gbase + 1], [gbase, gbase + 3, gbase + 2]]])
ground_rgb = np.array([0.80, 0.78, 0.75]) ** 2.2
fcol = np.vstack([fcol, ground_rgb, ground_rgb]); frough = np.append(frough, [0.9, 0.9]); fsss = np.append(fsss, [0, 0])
GROUND = len(F) - 2

# ---------------------------------------------------------------- raster
@njit(parallel=False, cache=True)
def raster(sx, sy, sz, F, W, H, depth, tid, b1, b2):
    for t in range(F.shape[0]):
        i0, i1, i2 = F[t, 0], F[t, 1], F[t, 2]
        x0, y0, z0 = sx[i0], sy[i0], sz[i0]
        x1, y1, z1 = sx[i1], sy[i1], sz[i1]
        x2, y2, z2 = sx[i2], sy[i2], sz[i2]
        if z0 <= 0 or z1 <= 0 or z2 <= 0: continue
        area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0)
        if abs(area) < 1e-12: continue
        xmin = max(int(math.floor(min(x0, x1, x2))), 0); xmax = min(int(math.ceil(max(x0, x1, x2))), W - 1)
        ymin = max(int(math.floor(min(y0, y1, y2))), 0); ymax = min(int(math.ceil(max(y0, y1, y2))), H - 1)
        inv = 1.0 / area
        for py in range(ymin, ymax + 1):
            fy = py + 0.5
            for px in range(xmin, xmax + 1):
                fx = px + 0.5
                w0 = ((x1 - fx) * (y2 - fy) - (x2 - fx) * (y1 - fy)) * inv
                w1 = ((x2 - fx) * (y0 - fy) - (x0 - fx) * (y2 - fy)) * inv
                w2 = 1.0 - w0 - w1
                if w0 < 0 or w1 < 0 or w2 < 0: continue
                z = w0 * z0 + w1 * z1 + w2 * z2
                if z < depth[py, px]:
                    depth[py, px] = z; tid[py, px] = t; b1[py, px] = w1; b2[py, px] = w2

def basis(fwd):
    fwd = fwd / np.linalg.norm(fwd)
    up = np.array([0.0, 1.0, 0.0])
    if abs(fwd @ up) > 0.99: up = np.array([0.0, 0.0, 1.0])
    r = np.cross(fwd, up); r /= np.linalg.norm(r)
    u = np.cross(r, fwd)
    return r, u, fwd

def ortho_map(d, n):
    """depth map seen along direction d (light travels along d), n×n; returns (map, r, u, f, scale, off)"""
    r, u, f = basis(d)
    P = V - center
    x, y, z = P @ r, P @ u, P @ f
    s = (n - 2) / (2 * radius * 1.05)
    sx = x * s + n / 2; sy = -y * s + n / 2; sz = z + 10 * radius
    dm = np.full((n, n), np.inf); tid = np.full((n, n), -1, np.int64)
    raster(sx, sy, sz, F[:GROUND], n, n, dm, tid, np.zeros((n, n)), np.zeros((n, n)))
    return dm, r, u, f, s

# ---------------------------------------------------------------- camera
W, H = a.size[0] * a.ss, a.size[1] * a.ss
yaw, pitch = math.radians(a.yaw), math.radians(a.pitch)
fwd = -np.array([math.sin(yaw) * math.cos(pitch), math.sin(pitch), math.cos(yaw) * math.cos(pitch)])
eye = center - fwd * radius * a.dist
cr, cu, cf = basis(fwd)
fov = math.radians(30)
foc = (H / 2) / math.tan(fov / 2)
P = V - eye
cx, cy, cz = P @ cr, P @ cu, P @ cf
px_ = cx / np.maximum(cz, 1e-6); py_ = -cy / np.maximum(cz, 1e-6)
mm = slice(0, gbase)
xs0, xs1 = px_[mm].min(), px_[mm].max(); ys0, ys1 = py_[mm].min(), py_[mm].max()
foc = a.fill * min(W / (xs1 - xs0), H / (ys1 - ys0))
sx = (px_ - (xs0 + xs1) / 2) * foc + W / 2
sy = (py_ - (ys0 + ys1) / 2) * foc + H / 2
depth = np.full((H, W), np.inf); tid = np.full((H, W), -1, np.int64)
b1 = np.zeros((H, W)); b2 = np.zeros((H, W))
raster(sx, sy, cz, F, W, H, depth, tid, b1, b2)
# perspective-correct the barycentrics
m = tid >= 0
ti = tid[m]; w1 = b1[m]; w2 = b2[m]; w0 = 1 - w1 - w2
iz = 1 / cz
q0, q1, q2 = w0 * iz[F[ti, 0]], w1 * iz[F[ti, 1]], w2 * iz[F[ti, 2]]
qs = q0 + q1 + q2
w0, w1, w2 = q0 / qs, q1 / qs, q2 / qs
pos = w0[:, None] * V[F[ti, 0]] + w1[:, None] * V[F[ti, 1]] + w2[:, None] * V[F[ti, 2]]
nrm = w0[:, None] * VN[F[ti, 0]] + w1[:, None] * VN[F[ti, 1]] + w2[:, None] * VN[F[ti, 2]]
nrm /= np.linalg.norm(nrm, axis=1, keepdims=True) + 1e-12
view = eye - pos; view /= np.linalg.norm(view, axis=1, keepdims=True)
flip = (nrm * view).sum(1) < 0
nrm[flip] *= -1

@njit(parallel=True, cache=True)
def lookup(pos, nrm, dm, r, u, f, s, n, cen, rad, bias, pcf):
    out = np.zeros(pos.shape[0])
    for i in prange(pos.shape[0]):
        p0 = pos[i, 0] - cen[0] + nrm[i, 0] * bias
        p1 = pos[i, 1] - cen[1] + nrm[i, 1] * bias
        p2 = pos[i, 2] - cen[2] + nrm[i, 2] * bias
        x = p0 * r[0] + p1 * r[1] + p2 * r[2]
        y = p0 * u[0] + p1 * u[1] + p2 * u[2]
        z = p0 * f[0] + p1 * f[1] + p2 * f[2] + 10 * rad
        mx = x * s + n / 2; my = -y * s + n / 2
        acc = 0.0; cnt = 0.0
        for dy in range(-pcf, pcf + 1):
            for dx in range(-pcf, pcf + 1):
                ix = int(mx) + dx; iy = int(my) + dy
                cnt += 1.0
                if ix < 0 or iy < 0 or ix >= n or iy >= n: acc += 1.0; continue
                if z <= dm[iy, ix] + bias * 0.5: acc += 1.0
        out[i] = acc / cnt
    return out

key_dir = np.array([-0.55, -0.75, -0.45]); key_dir /= np.linalg.norm(key_dir)  # light travels along this
NM = 2048
dm, r_, u_, f_, s_ = ortho_map(key_dir, NM)
bias = radius * 0.004
shadow = lookup(pos, nrm, dm, r_, u_, f_, s_, NM, center, radius, bias, 2)

# dome AO: 64 directions over the upper sphere (+ some from below for the body)
rng = np.random.default_rng(3)
ao = np.zeros(len(pos)); wsum = np.zeros(len(pos))
NA = 512
golden = math.pi * (3 - math.sqrt(5))
ND = 72
for k in range(ND):
    yk = 1 - (k + 0.5) / ND * 1.15  # mostly upper hemisphere, a little below horizon
    yk = max(min(yk, 1), -1)
    rk = math.sqrt(max(0, 1 - yk * yk)); th = golden * k
    d = np.array([math.cos(th) * rk, yk, math.sin(th) * rk])  # direction TO the sky
    dmk, rk_, uk_, fk_, sk_ = ortho_map(-d, NA)
    vis = lookup(pos, nrm, dmk, rk_, uk_, fk_, sk_, NA, center, radius, radius * 0.012, 1)
    cosw = np.clip(nrm @ d, 0, None)
    ao += vis * cosw; wsum += cosw
ao = ao / np.maximum(wsum, 1e-6)
ao = np.clip(ao, 0, 1)

# ---------------------------------------------------------------- shade
base = fcol[ti]; rough = np.clip(frough[ti], 0.06, 1.0); sss = fsss[ti]
L = -key_dir
NdL = nrm @ L
wrap = 0.45 * sss
diff_key = np.clip((NdL + wrap) / (1 + wrap), 0, 1)
# subsurface: the shadow terminator warms a little on skin-like materials
term = np.clip(1 - np.abs(NdL) * 2.2, 0, 1) * sss
key_col = np.array([1.0, 0.93, 0.84]) * 2.6
sky_up = np.array([0.62, 0.70, 0.85]); sky_dn = np.array([0.35, 0.30, 0.26])
hemi = np.outer(nrm[:, 1] * 0.5 + 0.5, sky_up) + np.outer(0.5 - nrm[:, 1] * 0.5, sky_dn)
sh = shadow[:, None]
diffuse = base * (key_col * diff_key[:, None] * sh + hemi * 1.1 * ao[:, None])
diffuse += (term * sh[:, 0])[:, None] * np.array([0.35, 0.08, 0.04]) * base
# rim light from behind-right
rim_dir = np.array([0.7, 0.25, -0.65]); rim_dir /= np.linalg.norm(rim_dir)
fres = (1 - np.clip((nrm * view).sum(1), 0, 1)) ** 4
rim = np.clip(nrm @ rim_dir, 0, 1) * fres * ao
diffuse += np.outer(rim, np.array([0.9, 0.95, 1.0])) * 0.9

def ggx(n, v, l, a_):
    h = v + l; h /= np.linalg.norm(h, axis=1, keepdims=True) + 1e-12
    ndh = np.clip((n * h).sum(1), 0, 1); ndl = np.clip((n * l).sum(1), 0, 1); ndv = np.clip((n * v).sum(1), 1e-3, 1)
    a2 = a_ ** 4
    D = a2 / (math.pi * (ndh * ndh * (a2 - 1) + 1) ** 2)
    k = (a_ + 1) ** 2 / 8
    G = (ndl / (ndl * (1 - k) + k)) * (ndv / (ndv * (1 - k) + k))
    vh = np.clip((v * h).sum(1), 0, 1)
    Fr = 0.04 + 0.96 * (1 - vh) ** 5
    return D * G * Fr / (4 * ndv * np.maximum(ndl, 1e-3)) * ndl

spec = ggx(nrm, view, np.tile(L, (len(nrm), 1)), rough) * shadow
spec_col = np.outer(spec, key_col)
# environment reflection of the studio: soft box overhead-front, darker below
R = 2 * (nrm * view).sum(1)[:, None] * nrm - view
env = np.outer(np.clip(R[:, 1], 0, 1) ** 1.5, np.array([1.0, 1.0, 1.0])) * 1.2 + 0.08
softbox = np.clip((R @ np.array([-0.35, 0.55, 0.75])) - 0.75, 0, 1) * 10  # a bright card front-left
env += np.outer(softbox, [1, 1, 1])
ndv = np.clip((nrm * view).sum(1), 0, 1)
Fenv = 0.04 + 0.96 * (1 - ndv) ** 5
gloss = (1 - rough) ** 2
env_spec = env * (Fenv * gloss * ao)[:, None]
col_lin = diffuse + spec_col + env_spec

# ground: shadow catcher fading into the background
img = np.zeros((H, W, 3))
yy = np.linspace(0, 1, H)[:, None]
bg_top = np.array([0.80, 0.80, 0.82]) ** 2.2 * 1.1; bg_bot = np.array([0.80, 0.78, 0.75]) ** 2.2 * 1.0
img[:] = (bg_top * (1 - yy) + bg_bot * yy)[:, None, :] if False else (bg_top[None, None, :] * (1 - yy[:, :, None]) + bg_bot[None, None, :] * yy[:, :, None])
is_ground = ti >= GROUND
gcol = ground_rgb * (0.35 + 0.65 * shadow[is_ground])[:, None] * (0.25 + 0.75 * ao[is_ground])[:, None] * 1.25
col_lin[is_ground] = gcol
# fade the ground into the background with distance from the model
gd = np.linalg.norm((pos[is_ground] - center)[:, [0, 2]], axis=1) / (radius * 2.2)
fade = np.clip(gd, 0, 1)[:, None] ** 1.5
ys, xs = np.nonzero(m)
bgpix = img[ys[is_ground], xs[is_ground]]
col_lin[is_ground] = gcol * (1 - fade) + bgpix * fade
img[ys, xs] = col_lin

# ACES (Narkowicz) + sRGB
x = img * a.exposure
x = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)
x = np.clip(x, 0, 1)
x = np.where(x <= 0.0031308, x * 12.92, 1.055 * x ** (1 / 2.4) - 0.055)
out = (x * 255 + 0.5).astype(np.uint8)
im = Image.fromarray(out).resize((a.size[0], a.size[1]), Image.LANCZOS)
im.save(a.png)
print("wrote", a.png)
