//! Scene IR → GLSL raymarcher.
//!
//! Every part becomes an inlined signed-distance evaluation in one `map()`
//! function, with the part's frame inverse baked in as a constant matrix
//! and a bounding-sphere early-out in front of it. The rest of the shader
//! is a conventional raymarcher: sphere-trace, tetrahedron normals, soft
//! shadow, ambient occlusion, a sky.
//!
//! This is the infinite-resolution renderer: no voxels, no mesh, the
//! surface is evaluated per pixel on the user's GPU. Formulas mirror
//! `geometry::distance` — keep the two in step.
//!
//! Known mismatch: blob and heightfield noise use a GLSL hash rather than
//! the Rust 64-bit hash, so their bumps differ in detail from the voxel
//! output while the overall shape agrees. The march steps at 0.7× distance
//! because those two fields are bounds rather than true SDFs.

use std::fmt::Write as _;

use crate::anchors::analytic_extents;
use crate::frame::{Frame, Mat3, Vec3};
use crate::pipeline::{compile_to_scene, CompileError};
use crate::scene::{Scene, Shape};

// ── Public surface ─────────────────────────────────────────────────────

/// The `map()` function and its helpers only — for a host that already
/// has a renderer and wants to drop the scene into it.
pub fn glsl_map(scene: &Scene) -> String {
    let mut out = String::new();
    out.push_str(GLSL_PRIMITIVES);
    emit_materials(scene, &mut out);
    emit_map(scene, &mut out);
    out
}

/// A complete `#version 300 es` fragment shader.
pub fn glsl_fragment(scene: &Scene) -> String {
    let mut out = String::new();
    out.push_str(GLSL_HEADER);
    out.push_str(&glsl_map(scene));
    out.push_str(GLSL_RENDER);
    out
}

/// A self-contained HTML page: WebGL2, the shader, an orbit camera.
pub fn html_page(scene: &Scene, title: &str) -> String {
    let (center, radius) = scene_bounds(scene);
    let heavy = scene.layers.iter().map(|l| l.parts.len()).sum::<usize>() > 60;
    HTML_TEMPLATE
        .replace("{{TITLE}}", title)
        .replace("{{FRAG}}", &glsl_fragment(scene).replace("</script>", "<\\/script>"))
        .replace("{{CX}}", &g(center.x))
        .replace("{{CY}}", &g(center.y))
        .replace("{{CZ}}", &g(center.z))
        .replace("{{RADIUS}}", &g(radius.max(1.0)))
        .replace("{{SCALE}}", if heavy { "0.5" } else { "1.0" })
}

pub fn glsl_from_source(source: &str) -> Result<String, Vec<CompileError>> {
    Ok(glsl_map(&compile_to_scene(source)?))
}

pub fn html_from_source(source: &str, title: &str) -> Result<String, Vec<CompileError>> {
    Ok(html_page(&compile_to_scene(source)?, title))
}

// ── Number formatting ──────────────────────────────────────────────────

/// A float literal GLSL accepts: always has a decimal point or exponent.
fn g(v: f64) -> String {
    if !v.is_finite() { return "0.0".to_string(); }
    let s = format!("{v:?}");
    if s.contains('.') || s.contains('e') { s } else { format!("{s}.0") }
}

fn v3(v: Vec3) -> String {
    format!("vec3({}, {}, {})", g(v.x), g(v.y), g(v.z))
}

/// `M` as a GLSL mat3 whose action is `M⁻¹ = Mᵀ`: GLSL mat3 takes
/// COLUMNS, and the columns of Mᵀ are the rows of M.
fn mat3_transpose(m: &Mat3) -> String {
    let r = m.0;
    format!(
        "mat3(vec3({}, {}, {}), vec3({}, {}, {}), vec3({}, {}, {}))",
        g(r[0][0]), g(r[0][1]), g(r[0][2]),
        g(r[1][0]), g(r[1][1]), g(r[1][2]),
        g(r[2][0]), g(r[2][1]), g(r[2][2]),
    )
}

// ── Materials ──────────────────────────────────────────────────────────

fn hex_to_rgb(hex: &str) -> Vec3 {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 { return Vec3::new(1.0, 0.0, 1.0); }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(255) as f64 / 255.0;
    Vec3::new(c(0), c(2), c(4))
}

fn material_table(scene: &Scene) -> Vec<String> {
    let mut colors: Vec<String> = Vec::new();
    for l in &scene.layers {
        for p in &l.parts {
            if !colors.contains(&p.color) {
                colors.push(p.color.clone());
            }
        }
    }
    colors
}

fn emit_materials(scene: &Scene, out: &mut String) {
    let colors = material_table(scene);
    let n = colors.len().max(1);
    let _ = writeln!(out, "const int NUM_MATS = {n};");
    let _ = writeln!(out, "vec3 matColor(int i) {{");
    for (i, c) in colors.iter().enumerate() {
        let rgb = hex_to_rgb(c);
        let _ = writeln!(out, "    if (i == {i}) return {};", v3(rgb));
    }
    let _ = writeln!(out, "    return vec3(1.0, 0.0, 1.0);");
    let _ = writeln!(out, "}}\n");
}

// ── map() ──────────────────────────────────────────────────────────────

fn emit_map(scene: &Scene, out: &mut String) {
    let colors = material_table(scene);

    let _ = writeln!(out, "float map(vec3 p, out int mat) {{");
    let _ = writeln!(out, "    float best = 1e9;");
    let _ = writeln!(out, "    mat = -1;");

    let mut counter = 0usize;
    for layer in &scene.layers {
        for part in &layer.parts {
            let frame = part.frame.to_frame();
            let mat   = colors.iter().position(|c| *c == part.color).unwrap_or(0);
            let (bc, br) = bounding_sphere(&part.shape, &frame);

            let _ = writeln!(out, "    // {}", part.name);
            let _ = writeln!(out, "    if (length(p - {}) - {} < best) {{", v3(bc), g(br));
            counter += 1;
            let q = format!("q{counter}");
            let _ = writeln!(out, "        vec3 {q} = {} * (p - {});",
                             mat3_transpose(&frame.rot), v3(frame.pos));
            let d = emit_shape(&part.shape, &q, out, &mut counter, layer.voxel_size);
            let _ = writeln!(out, "        if ({d} < best) {{ best = {d}; mat = {mat}; }}");
            let _ = writeln!(out, "    }}");
        }
    }

    let _ = writeln!(out, "    return best;");
    let _ = writeln!(out, "}}\n");
    let _ = writeln!(out, "float mapD(vec3 p) {{ int m; return map(p, m); }}\n");
}

/// A fresh variable name. A free function rather than a closure over `n`:
/// a closure would hold its mutable borrow for the whole scope, and every
/// recursive `emit_shape` call needs `n` too.
fn fresh(prefix: &str, n: &mut usize) -> String {
    *n += 1;
    format!("{prefix}{n}")
}

fn emit_shape(shape: &Shape, p: &str, out: &mut String, n: &mut usize, vs: f64) -> String {
    match shape {
        Shape::Sphere { radius } => {
            let d = fresh("d", n);
            let _ = writeln!(out, "        float {d} = length({p}) - {};", g(*radius));
            d
        }
        Shape::Ellipsoid { rx, ry, rz } => {
            let d = fresh("d", n);
            let _ = writeln!(out, "        float {d} = sdEllipsoid({p}, vec3({}, {}, {}));", g(*rx), g(*ry), g(*rz));
            d
        }
        Shape::Box { width, height, depth } => {
            let d = fresh("d", n);
            let _ = writeln!(out, "        float {d} = sdBox({p}, vec3({}, {}, {}));",
                             g(width / 2.0), g(height / 2.0), g(depth / 2.0));
            d
        }
        Shape::Cylinder { height, radius } => {
            let d = fresh("d", n);
            let _ = writeln!(out, "        float {d} = sdCylinder({p}, {}, {});", g(*height), g(*radius));
            d
        }
        Shape::Cone { height, radius } => {
            let d = fresh("d", n);
            let _ = writeln!(out, "        float {d} = sdCone({p}, {}, {});", g(*height), g(*radius));
            d
        }
        Shape::Blob { radius, roughness } => {
            let d = fresh("d", n);
            let _ = writeln!(out, "        float {d} = sdBlob({p}, {}, {}, {});", g(*radius), g(*roughness), g(vs));
            d
        }
        Shape::Heightfield { radius, max_height, noise, seed } => {
            let d = fresh("d", n);
            let _ = writeln!(out, "        float {d} = sdHeightfield({p}, {}, {}, {}, {}, {});",
                             g(*radius), g(*max_height), g(*noise), g(*seed as f64), g(vs));
            d
        }
        Shape::Shell { inner, inner_offset } => {
            let di = emit_shape(inner, p, out, n, vs);
            let d = fresh("d", n);
            let _ = writeln!(out, "        float {d} = max({di}, -{di} - {});", g(*inner_offset));
            d
        }
        Shape::Extrude { profile, height } => {
            let q = fresh("q", n);
            let _ = writeln!(out, "        vec3 {q} = vec3({p}.x, {p}.y - clamp({p}.y, 0.0, {}), {p}.z);", g(*height));
            emit_shape(profile, &q, out, n, vs)
        }
        Shape::Union { shapes, blend } => {
            let mut ds: Vec<String> = Vec::with_capacity(shapes.len());
            for s in shapes {
                ds.push(emit_shape(s, p, out, n, vs));
            }
            let d = fresh("d", n);
            let expr = if *blend > 0.0 {
                ds.iter().skip(1).fold(ds[0].clone(), |acc, x| format!("smin({acc}, {x}, {})", g(*blend)))
            } else {
                ds.iter().skip(1).fold(ds[0].clone(), |acc, x| format!("min({acc}, {x})"))
            };
            let _ = writeln!(out, "        float {d} = {expr};");
            d
        }
        Shape::Intersect { shapes } => {
            let mut ds: Vec<String> = Vec::with_capacity(shapes.len());
            for s in shapes {
                ds.push(emit_shape(s, p, out, n, vs));
            }
            let d = fresh("d", n);
            let expr = ds.iter().skip(1).fold(ds[0].clone(), |acc, x| format!("max({acc}, {x})"));
            let _ = writeln!(out, "        float {d} = {expr};");
            d
        }
        Shape::Difference { base, cuts } => {
            let db = emit_shape(base, p, out, n, vs);
            let mut dc: Vec<String> = Vec::with_capacity(cuts.len());
            for s in cuts {
                dc.push(emit_shape(s, p, out, n, vs));
            }
            let d = fresh("d", n);
            let expr = dc.iter().fold(db, |acc, x| format!("max({acc}, -{x})"));
            let _ = writeln!(out, "        float {d} = {expr};");
            d
        }
        Shape::At { inner, x, y, z } => {
            let q = fresh("q", n);
            let _ = writeln!(out, "        vec3 {q} = {p} - vec3({}, {}, {});", g(*x), g(*y), g(*z));
            emit_shape(inner, &q, out, n, vs)
        }
        Shape::Spin { inner, axis, degrees } => {
            let rad = degrees.to_radians();
            let r = match axis.as_str() {
                "x" | "X" => Mat3::rot_x(rad),
                "z" | "Z" => Mat3::rot_z(rad),
                _         => Mat3::rot_y(rad),
            };
            let q = fresh("q", n);
            let _ = writeln!(out, "        vec3 {q} = {} * {p};", mat3_transpose(&r));
            emit_shape(inner, &q, out, n, vs)
        }
    }
}

// ── Bounds ─────────────────────────────────────────────────────────────

fn bounding_sphere(shape: &Shape, frame: &Frame) -> (Vec3, f64) {
    let e = analytic_extents(&shape.to_expr());
    let c = frame.apply_point(e.center());
    let half = e.max.sub(e.min).scale(0.5);
    // A little slack for blends and the heightfield's rounded elevation.
    (c, half.length() * 1.05 + 0.5)
}

fn scene_bounds(scene: &Scene) -> (Vec3, f64) {
    let mut min = Vec3::new(f64::MAX, f64::MAX, f64::MAX);
    let mut max = Vec3::new(f64::MIN, f64::MIN, f64::MIN);
    for l in &scene.layers {
        for p in &l.parts {
            let f = p.frame.to_frame();
            let e = analytic_extents(&p.shape.to_expr());
            for &cx in &[e.min.x, e.max.x] {
                for &cy in &[e.min.y, e.max.y] {
                    for &cz in &[e.min.z, e.max.z] {
                        let w = f.apply_point(Vec3::new(cx, cy, cz));
                        min = Vec3::new(min.x.min(w.x), min.y.min(w.y), min.z.min(w.z));
                        max = Vec3::new(max.x.max(w.x), max.y.max(w.y), max.z.max(w.z));
                    }
                }
            }
        }
    }
    if min.x == f64::MAX { return (Vec3::ZERO, 10.0); }
    let center = min.add(max).scale(0.5);
    (center, max.sub(min).scale(0.5).length())
}

// ── GLSL text ──────────────────────────────────────────────────────────

const GLSL_HEADER: &str = r#"#version 300 es
precision highp float;
out vec4 fragColor;
uniform vec2  uRes;
uniform float uTime;
uniform vec3  uCamPos;
uniform vec3  uTarget;
uniform float uFar;

"#;

const GLSL_PRIMITIVES: &str = r#"// ── primitives (mirror geometry::distance) ─────────────────────────
float smin(float a, float b, float k) {
    float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}
float sdBox(vec3 p, vec3 b) {
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}
// base at origin, axis +Y
float sdCylinder(vec3 p, float h, float r) {
    vec2 d = vec2(length(p.xz) - r, abs(p.y - h * 0.5) - h * 0.5);
    return min(max(d.x, d.y), 0.0) + length(max(d, 0.0));
}
// base at origin radius r, apex at +h
float sdCone(vec3 p, float h, float r1) {
    float hh = h * 0.5;
    vec2 q  = vec2(length(p.xz), p.y - hh);
    vec2 k1 = vec2(0.0, hh);
    vec2 k2 = vec2(-r1, 2.0 * hh);
    vec2 ca = vec2(q.x - min(q.x, (q.y < 0.0) ? r1 : 0.0), abs(q.y) - hh);
    vec2 cb = q - k1 + k2 * clamp(dot(k1 - q, k2) / dot(k2, k2), 0.0, 1.0);
    float s = (cb.x < 0.0 && ca.y < 0.0) ? -1.0 : 1.0;
    return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}
float sdEllipsoid(vec3 p, vec3 r) {
    float k0 = length(p / r);
    float k1 = length(p / (r * r));
    return k1 < 1e-6 ? -min(r.x, min(r.y, r.z)) : k0 * (k0 - 1.0) / k1;
}
float hash13(vec3 p) {
    p = fract(p * 0.1031);
    p += dot(p, p.zyx + 31.32);
    return fract((p.x + p.y) * p.z);
}
float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}
// Rust keys noise on rounded local voxel indices; mirror that so the
// lumps are voxel-scale, even though the hash itself differs.
float sdBlob(vec3 p, float r, float rough, float vs) {
    vec3 k = floor(p / vs + 0.5);
    float n = hash13(k) * 2.0 - 1.0;
    return length(p) - r * (1.0 + rough * n);
}
float terrainNoise(vec2 k, float seed, float scale) {
    float value = 0.0, amp = 1.0, freq = scale, maxv = 0.0;
    for (int o = 0; o < 4; o++) {
        vec2 s = floor(k * freq);
        float h = hash12(s + seed * 0.001 + float(o) * 7.31) * 2.0 - 1.0;
        value += (h * 0.5 + 0.5) * amp;
        maxv  += amp;
        amp   *= 0.5;
        freq  *= 2.0;
    }
    return value / maxv;
}
float sdHeightfield(vec3 p, float radius, float maxh, float noiseAmt, float seed, float vs) {
    vec2 k = floor(p.xz / vs + 0.5);
    float rxz = length(p.xz);
    float fade = max(1.0 - (rxz / radius) * (rxz / radius), 0.0);
    float mhv  = ceil(maxh / vs);
    float elev = floor(terrainNoise(k, seed, noiseAmt) * fade * mhv + 0.5) * vs;
    return max(max(p.y - elev, rxz - radius), -p.y);
}

"#;

const GLSL_RENDER: &str = r#"// ── renderer ───────────────────────────────────────────────────────
vec3 calcNormal(vec3 p) {
    const vec2 e = vec2(0.01, -0.01);
    return normalize(
        e.xyy * mapD(p + e.xyy) + e.yyx * mapD(p + e.yyx) +
        e.yxy * mapD(p + e.yxy) + e.xxx * mapD(p + e.xxx));
}
float softShadow(vec3 ro, vec3 rd, float mint, float maxt, float k) {
    float res = 1.0;
    float t = mint;
    for (int i = 0; i < 24; i++) {
        float h = mapD(ro + rd * t);
        if (h < 0.001) return 0.0;
        res = min(res, k * h / t);
        t += clamp(h, 0.05, 1.0);
        if (t > maxt) break;
    }
    return clamp(res, 0.0, 1.0);
}
float calcAO(vec3 p, vec3 n) {
    float occ = 0.0, sca = 1.0;
    for (int i = 0; i < 5; i++) {
        float h = 0.02 + 0.3 * float(i) / 4.0;
        float d = mapD(p + h * n);
        occ += (h - d) * sca;
        sca *= 0.7;
    }
    return clamp(1.0 - 2.0 * occ, 0.0, 1.0);
}
vec3 sky(vec3 rd) {
    return mix(vec3(0.16, 0.18, 0.22), vec3(0.45, 0.60, 0.85), smoothstep(-0.2, 0.6, rd.y));
}
void main() {
    vec2 uv = (gl_FragCoord.xy * 2.0 - uRes) / uRes.y;

    vec3 ro = uCamPos;
    vec3 fw = normalize(uTarget - ro);
    vec3 rt = normalize(cross(fw, vec3(0.0, 1.0, 0.0)));
    vec3 up = cross(rt, fw);
    vec3 rd = normalize(uv.x * rt + uv.y * up + 1.7 * fw);

    // Sphere trace. Step at 0.7× because blob and heightfield are bounds,
    // not true SDFs.
    float t = 0.0;
    int mat = -1;
    bool hit = false;
    for (int i = 0; i < 160; i++) {
        vec3 p = ro + rd * t;
        int m;
        float d = map(p, m);
        if (d < 0.002 * t + 0.0005) { hit = true; mat = m; break; }
        t += d * 0.7;
        if (t > uFar) break;
    }

    vec3 col = sky(rd);
    if (hit) {
        vec3 p = ro + rd * t;
        vec3 n = calcNormal(p);
        vec3 base = matColor(mat);

        vec3 lightDir = normalize(vec3(-0.5, 0.9, 0.4));
        float dif = clamp(dot(n, lightDir), 0.0, 1.0);
        float sha = softShadow(p + n * 0.02, lightDir, 0.05, uFar * 0.5, 12.0);
        float ao  = calcAO(p, n);
        float amb = 0.5 + 0.5 * n.y;
        vec3 hal  = normalize(lightDir - rd);
        float spe = pow(clamp(dot(n, hal), 0.0, 1.0), 24.0) * dif * sha * 0.3;

        col  = base * (0.25 * amb * ao * vec3(0.6, 0.7, 0.9) + 1.1 * dif * sha * vec3(1.0, 0.95, 0.85));
        col += spe;
        col  = mix(col, sky(rd), 1.0 - exp(-0.0004 * t * t));
    }

    col = pow(col, vec3(0.4545));
    fragColor = vec4(col, 1.0);
}
"#;

const HTML_TEMPLATE: &str = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>{{TITLE}} — Moxi</title>
<style>
  html, body { margin: 0; height: 100%; background: #101214; overflow: hidden; }
  canvas { width: 100%; height: 100%; display: block; }
  #hud { position: fixed; left: 12px; bottom: 10px; color: #aab; font: 12px/1.4 system-ui, sans-serif; opacity: .8; }
</style>
</head>
<body>
<canvas id="c"></canvas>
<div id="hud">{{TITLE}} · drag to orbit · wheel to zoom · rendered on your GPU from the Moxi scene</div>
<script id="frag" type="x-shader/x-fragment">{{FRAG}}</script>
<script>
const canvas = document.getElementById('c');
const gl = canvas.getContext('webgl2');
if (!gl) { document.getElementById('hud').textContent = 'WebGL2 is required.'; throw new Error('no webgl2'); }

const VERT = `#version 300 es
void main() {
  vec2 v = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  gl_Position = vec4(v * 2.0 - 1.0, 0.0, 1.0);
}`;
function compile(type, src) {
  const s = gl.createShader(type);
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
    console.error(gl.getShaderInfoLog(s));
    document.getElementById('hud').textContent = 'Shader failed to compile — see console.';
    throw new Error('shader');
  }
  return s;
}
const prog = gl.createProgram();
gl.attachShader(prog, compile(gl.VERTEX_SHADER, VERT));
gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, document.getElementById('frag').textContent));
gl.linkProgram(prog);
gl.useProgram(prog);

const uRes = gl.getUniformLocation(prog, 'uRes');
const uTime = gl.getUniformLocation(prog, 'uTime');
const uCamPos = gl.getUniformLocation(prog, 'uCamPos');
const uTarget = gl.getUniformLocation(prog, 'uTarget');
const uFar = gl.getUniformLocation(prog, 'uFar');

const target = [{{CX}}, {{CY}}, {{CZ}}];
const sceneRadius = {{RADIUS}};
const scale = {{SCALE}};
let yaw = 0.6, pitch = 0.35, dist = sceneRadius * 2.6;
let dragging = false, lx = 0, ly = 0;

canvas.addEventListener('mousedown', e => { dragging = true; lx = e.clientX; ly = e.clientY; });
window.addEventListener('mouseup', () => dragging = false);
window.addEventListener('mousemove', e => {
  if (!dragging) return;
  yaw   += (e.clientX - lx) * 0.005;
  pitch += (e.clientY - ly) * 0.005;
  pitch  = Math.max(-1.5, Math.min(1.5, pitch));
  lx = e.clientX; ly = e.clientY;
});
canvas.addEventListener('wheel', e => {
  dist *= Math.exp(e.deltaY * 0.001);
  dist  = Math.max(sceneRadius * 0.3, Math.min(sceneRadius * 12, dist));
  e.preventDefault();
}, { passive: false });

function resize() {
  const dpr = Math.min(window.devicePixelRatio || 1, 2) * scale;
  canvas.width  = Math.floor(canvas.clientWidth  * dpr);
  canvas.height = Math.floor(canvas.clientHeight * dpr);
  gl.viewport(0, 0, canvas.width, canvas.height);
}
window.addEventListener('resize', resize);
resize();

const t0 = performance.now();
function frame() {
  const cx = target[0] + dist * Math.cos(pitch) * Math.sin(yaw);
  const cy = target[1] + dist * Math.sin(pitch);
  const cz = target[2] + dist * Math.cos(pitch) * Math.cos(yaw);
  gl.uniform2f(uRes, canvas.width, canvas.height);
  gl.uniform1f(uTime, (performance.now() - t0) / 1000);
  gl.uniform3f(uCamPos, cx, cy, cz);
  gl.uniform3f(uTarget, target[0], target[1], target[2]);
  gl.uniform1f(uFar, sceneRadius * 14.0);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  requestAnimationFrame(frame);
}
frame();
</script>
</body>
</html>
"#;

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // `r##"…"##`, not `r#"…"#`: the source contains `"#8fd18f"`, and the
    // `"#` in it would close a single-hash raw string.
    const SRC: &str = r##"
material Skin { color = "#8fd18f" }
material Eye  { color = black }
thing Sprout {
    part Body { shape = union(sphere(radius=5), at(sphere(radius=3.2), y=6), blend=2.5), material = Skin }
    part EyeL { shape = sphere(radius=0.7), material = Eye }
    relation { EyeL.south on Body.north shift=(6.2, -1.3) gap=-2.4 }
    resolve voxel_size = 1.0
}
print Sprout detail=low
"##;

    fn balanced(s: &str) -> bool {
        let mut depth = 0i32;
        for c in s.chars() {
            match c { '{' => depth += 1, '}' => depth -= 1, _ => {} }
            if depth < 0 { return false; }
        }
        depth == 0
    }

    #[test]
    fn map_contains_every_part_and_is_balanced() {
        let glsl = glsl_from_source(SRC).expect("compiles");
        assert!(glsl.contains("float map(vec3 p, out int mat)"));
        assert!(glsl.contains("// Body"));
        assert!(glsl.contains("// EyeL"));
        assert!(glsl.contains("smin("), "the blended union must lower to smin");
        assert!(balanced(&glsl), "unbalanced braces in generated GLSL");
    }

    #[test]
    fn materials_are_deduplicated_and_indexed() {
        let scene = compile_to_scene(SRC).unwrap();
        let glsl  = glsl_map(&scene);
        assert!(glsl.contains("const int NUM_MATS = 2;"));
        assert!(glsl.contains("mat = 0;") && glsl.contains("mat = 1;"));
    }

    #[test]
    fn float_literals_always_have_a_decimal_point() {
        assert_eq!(g(5.0), "5.0");
        assert_eq!(g(2.5), "2.5");
        assert!(g(1e-9).contains('e') || g(1e-9).contains('.'));
    }

    #[test]
    fn transposed_matrix_emits_rows_as_columns() {
        let m = Mat3([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        assert_eq!(
            mat3_transpose(&m),
            "mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0))"
        );
    }

    #[test]
    fn html_embeds_the_shader_and_camera() {
        let html = html_from_source(SRC, "Sprout").expect("compiles");
        assert!(html.contains("float map(vec3 p, out int mat)"));
        assert!(html.contains("const sceneRadius = "));
        assert!(!html.contains("{{"), "unfilled template placeholder");
    }

    #[test]
    fn codegen_is_deterministic() {
        assert_eq!(glsl_from_source(SRC).unwrap(), glsl_from_source(SRC).unwrap());
    }
}