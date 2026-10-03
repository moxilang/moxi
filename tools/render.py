# /// script
# requires-python = ">=3.9"
# dependencies = ["playwright>=1.45", "pillow>=10", "numpy>=1.24"]
# ///
"""Render a `moxi web` page in headless Chromium and save a PNG.

    uv run tools/render.py scripts/LAMP.md            # moxi web → output/LAMP.html → output/LAMP.png
    uv run tools/render.py scripts/LAMP.md --expect-color "#ffff00"
    uv run tools/render.py output/LAMP.html           # an already-built page
    uv run tools/render.py --install                  # one-time: fetch Chromium

Driven by `cargo make render` / `cargo make render-check` (Makefile.toml); you
rarely call it directly. The point is that a session can SEE what a script
built: the PNG is the artifact, the checks below only say whether it is worth
looking at.

Checks (exit 1 on any failure, with the reason):
  1. The page raised no JS error and logged no console error (a GLSL compile
     failure in the generated shader lands here).
  2. Something other than sky is on screen. The sky is a pure function of the
     view ray (`sky()` in src/shader.rs) and the camera starts at a fixed pose
     (HTML_TEMPLATE), so this script recomputes the sky for every pixel and
     counts the pixels that differ. If shader.rs changes its sky or default
     camera, update SKY_* / CAMERA_* below — the coverage check will fail
     loudly (≈100% "object") until you do.
  3. Optional --expect-color: at least --min-color-px object pixels carry the
     hue of that material colour (shading changes brightness, not hue).

A .md script is compiled with `cargo run --release -- web <script> --out
output` from the repo root (set MOXI_BIN to use a built `moxi` instead); a
compile error stops here with the compiler's own message and exit 1.

Chromium: Playwright's own build by default; set MOXI_CHROMIUM to use another
Chromium/Chrome binary. WebGL runs on SwiftShader (CPU) so the result does not
depend on the machine's GPU.
"""
from __future__ import annotations

import argparse
import colorsys
import os
import re
import subprocess
import sys
from pathlib import Path

# ── Mirrors of src/shader.rs (keep in sync) ────────────────────────────────
SKY_LOW = (0.16, 0.18, 0.22)    # sky(): mix(low, high, smoothstep(-0.2, 0.6, rd.y))
SKY_HIGH = (0.45, 0.60, 0.85)
SKY_EDGES = (-0.2, 0.6)
GAMMA = 0.4545                  # col = pow(col, vec3(0.4545))
FOCAL = 1.7                     # rd = normalize(uv.x*rt + uv.y*up + 1.7*fw)
CAMERA_YAW, CAMERA_PITCH, CAMERA_DIST = 0.5, 0.35, 2.6   # dist = sceneRadius * 2.6

BG_TOLERANCE = 14               # max per-channel |png - sky| (0..255) still counted as sky
CHROMIUM_ARGS = ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"]


def fail(msg: str) -> None:
    print(f"render: FAIL — {msg}", file=sys.stderr)
    sys.exit(1)


def install() -> None:
    sys.exit(subprocess.call([sys.executable, "-m", "playwright", "install", "chromium"]))


ROOT = Path(__file__).resolve().parent.parent


def build_page(script: Path) -> Path:
    """`moxi web` the script into output/ and return the page path."""
    moxi = [os.environ["MOXI_BIN"]] if os.environ.get("MOXI_BIN") else ["cargo", "run", "--release", "--quiet", "--"]
    out = ROOT / "output"
    rc = subprocess.call(moxi + ["web", str(script), "--out", str(out)], cwd=ROOT)
    if rc != 0:
        fail(f"`moxi web {script}` failed (exit {rc}) — see the compiler errors above")
    return out / f"{script.stem}.html"


def scene_camera(html: str) -> tuple[list[float], float]:
    t = re.search(r"const target = \[([^\]]+)\];", html)
    r = re.search(r"const sceneRadius = ([0-9.eE+-]+);", html)
    if not (t and r):
        fail("page has no `const target` / `const sceneRadius` — HTML_TEMPLATE in shader.rs changed; update tools/render.py")
    return [float(v) for v in t.group(1).split(",")], float(r.group(1))


def sky_image(w: int, h: int, target: list[float], radius: float):
    import numpy as np

    tx, ty, tz = target
    d = radius * CAMERA_DIST
    ro = np.array([
        tx + d * np.cos(CAMERA_PITCH) * np.sin(CAMERA_YAW),
        ty + d * np.sin(CAMERA_PITCH),
        tz + d * np.cos(CAMERA_PITCH) * np.cos(CAMERA_YAW),
    ])
    fw = np.array(target) - ro
    fw /= np.linalg.norm(fw)
    rt = np.cross(fw, [0.0, 1.0, 0.0])
    rt /= np.linalg.norm(rt)
    up = np.cross(rt, fw)

    # gl_FragCoord is bottom-up at pixel centres; PNG rows are top-down.
    fx = np.arange(w) + 0.5
    fy = (h - 1 - np.arange(h)) + 0.5
    ux = (fx * 2.0 - w) / h
    uy = (fy * 2.0 - h) / h
    UX, UY = np.meshgrid(ux, uy)
    rd = UX[..., None] * rt + UY[..., None] * up + FOCAL * fw
    rd /= np.linalg.norm(rd, axis=-1, keepdims=True)

    e0, e1 = SKY_EDGES
    s = np.clip((rd[..., 1] - e0) / (e1 - e0), 0.0, 1.0)
    s = s * s * (3.0 - 2.0 * s)
    col = np.array(SKY_LOW) + (np.array(SKY_HIGH) - np.array(SKY_LOW)) * s[..., None]
    return np.power(col, GAMMA) * 255.0


def parse_hex(c: str) -> tuple[float, float, float]:
    m = re.fullmatch(r"#?([0-9a-fA-F]{6})", c.strip())
    if not m:
        fail(f"--expect-color wants a hex colour like #ffff00, got {c!r}")
    v = m.group(1)
    return tuple(int(v[i:i + 2], 16) / 255.0 for i in (0, 2, 4))


def hue_matches(rgb_px, target_rgb, mask, hue_tol_deg: float = 20.0):
    import numpy as np

    th, ts, _ = colorsys.rgb_to_hsv(*target_rgb)
    if ts < 0.2:
        fail("--expect-color must be a saturated colour (a grey has no hue to look for)")
    px = rgb_px.astype(float) / 255.0
    mx, mn = px.max(-1), px.min(-1)
    sat = np.where(mx > 0, (mx - mn) / np.maximum(mx, 1e-9), 0.0)
    r, g, b = px[..., 0], px[..., 1], px[..., 2]
    delta = np.maximum(mx - mn, 1e-9)
    hue = np.where(mx == r, ((g - b) / delta) % 6, np.where(mx == g, (b - r) / delta + 2, (r - g) / delta + 4)) / 6.0
    dh = np.abs(hue - th)
    dh = np.minimum(dh, 1.0 - dh) * 360.0
    return mask & (sat > 0.35) & (mx > 0.25) & (dh < hue_tol_deg)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("target", nargs="?", help="a .md script (compiled with `moxi web`) or a page it wrote")
    ap.add_argument("--png", help="output PNG (default: next to the HTML)")
    ap.add_argument("--size", default="800x600", help="viewport WxH (default 800x600)")
    ap.add_argument("--min-coverage", type=float, default=0.005, help="min fraction of non-sky pixels (default 0.005)")
    ap.add_argument("--max-coverage", type=float, default=0.95, help="max fraction of non-sky pixels (default 0.95)")
    ap.add_argument("--expect-color", action="append", default=[], help="hex material colour that must be visible; repeatable")
    ap.add_argument("--min-color-px", type=int, default=150, help="pixels each --expect-color needs (default 150)")
    ap.add_argument("--timeout", type=float, default=60.0, help="seconds to wait for the first frames (default 60)")
    ap.add_argument("--install", action="store_true", help="download Playwright's Chromium and exit")
    a = ap.parse_args()

    if a.install:
        install()
    if not a.target:
        ap.error("a .md script or .html page is required (or --install)")

    import numpy as np
    from PIL import Image
    from playwright.sync_api import Error as PwError, sync_playwright

    src = Path(a.target).resolve()
    if not src.is_file():
        fail(f"no such file: {src}")
    html_path = build_page(src) if src.suffix.lower() == ".md" else src
    png_path = Path(a.png) if a.png else html_path.with_suffix(".png")
    w, h = (int(v) for v in a.size.lower().split("x"))
    target, radius = scene_camera(html_path.read_text(encoding="utf-8"))

    errors: list[str] = []
    with sync_playwright() as p:
        exe = os.environ.get("MOXI_CHROMIUM") or None
        try:
            browser = p.chromium.launch(headless=True, args=CHROMIUM_ARGS, executable_path=exe)
        except PwError as e:
            fail(f"could not start Chromium ({str(e).splitlines()[0]}). Run `cargo make render-setup` once, or set MOXI_CHROMIUM.")
        page = browser.new_page(viewport={"width": w, "height": h}, device_scale_factor=1)
        page.on("pageerror", lambda e: errors.append(f"page error: {e}"))
        page.on("console", lambda m: errors.append(f"console.{m.type}: {m.text}") if m.type == "error" else None)
        page.goto(html_path.as_uri())
        # Three animation frames: the first draw is issued synchronously, the
        # rAFs make sure it has been composited before the screenshot.
        page.evaluate(
            "() => new Promise(r => { let n = 0; const f = () => (++n >= 3 ? r() : requestAnimationFrame(f)); requestAnimationFrame(f); })"
        )
        hud = page.evaluate("() => (document.getElementById('hud') || {}).textContent || ''")
        page.add_style_tag(content="#hud { display: none !important; }")
        page.evaluate("() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))")
        png_path.parent.mkdir(parents=True, exist_ok=True)
        page.locator("#c").screenshot(path=str(png_path), timeout=a.timeout * 1000)
        browser.close()

    print(f"render: wrote {png_path}")
    if "required" in hud or "failed" in hud.lower():
        errors.append(f"page says: {hud.strip()}")
    if errors:
        fail("the page reported errors:\n  " + "\n  ".join(errors))

    img = np.asarray(Image.open(png_path).convert("RGB"))
    ih, iw, _ = img.shape
    sky = sky_image(iw, ih, target, radius)
    obj = np.abs(img.astype(float) - sky).max(-1) > BG_TOLERANCE
    cov = float(obj.mean())
    ys, xs = np.nonzero(obj)
    bbox = f"x {xs.min()}–{xs.max()}, y {ys.min()}–{ys.max()}" if len(xs) else "none"
    print(f"render: {iw}x{ih}, object covers {cov:.1%} of the frame ({bbox})")

    if cov < a.min_coverage:
        fail(f"only {cov:.2%} of the frame is not sky (< {a.min_coverage:.2%}) — nothing visible rendered")
    if cov > a.max_coverage:
        fail(f"{cov:.1%} of the frame differs from the sky model (> {a.max_coverage:.0%}) — either the camera is inside "
             "the object or src/shader.rs changed sky()/the camera and tools/render.py needs the same constants")

    for c in a.expect_color:
        n = int(hue_matches(img, parse_hex(c), obj).sum())
        print(f"render: {n} object pixels with the hue of {c}")
        if n < a.min_color_px:
            fail(f"expected colour {c} on {a.min_color_px}+ pixels, found {n} — that material's part is missing or off-screen")
    print("render: ok")


if __name__ == "__main__":
    main()
