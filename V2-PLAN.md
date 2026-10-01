# Moxi v2 plan (from the pre-registered eval, 2026-09-28)

The eval (moxilang/moxi-eval, tag prereg-v1) found Moxi 0.4.0 trailing
CadQuery and OpenSCAD by ~7–10 points on task success with claude-sonnet-5,
with 98.7% first-try compiles. Three systematic semantic mismatches account
for most of the gap (see moxi-eval/ANALYSIS.md). Each fix below is a general
rule; none may be validated on the eval's 30 tasks — v2 gets new tasks and a
new pre-registration.

## Language / compiler changes

1. **Unplaced parts are errors.** Every part except one root must appear as
   the subject of a placement; otherwise: `part 'RibL[0]' has no placement`.
   (Ribs failure. Generalises the existing "each part placed at most once".)
2. **Orientation-free mates keep the subject's world orientation**, or the
   docs state host-relative inheritance exactly. Decide by principle:
   "a part keeps its own orientation" is what SKILL.md already promises, so
   the compiler should honour it. Check every script in scripts/ and bench/
   for reliance on the old behaviour (parity will show it).
3. **Axis anchors for cylinders/cones/capsules**: e.g. `axis(t)` (a point on
   the axis, normal along it is wrong — define normal radial? or
   direction-free like center) and cap-centre anchors `base`/`end` for
   capsules (the teddy needed `point(...)` for this). Design before coding;
   the hinge needed "attach at the axis", the teddy "attach at the cap
   centre". Two unrelated uses → meets the bar.

Also carried from feasibility (not from eval failures, lower priority):
repetition inside one solid (18 holes written by hand), frustum, wedge.

## Documentation (SKILL.md)

- State that compass anchors lie ON the surface; show the gap to the axis.
- State frame semantics of `center` mates after the decision in (2).
- Keep examples off the eval tasks; token budget unchanged unless re-pinned.

## Eval v2

- Harder tasks so incumbents don't saturate; categories chosen on principle
  (deep attachment chains, many angled parts on curved bodies, hollow
  assemblies with through-bores, repeated features). Written before any run.
- Possibly a second model. Same harness; mutation tests + pose metamorphic
  test are now in the suite.
- Pre-register with analysis code frozen, as before.
