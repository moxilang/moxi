# Moxi actions: semantics

Status: settled for v0.5.0 (2026-10-11; §9 accepted by Andrew). Written from the compiler
(`src/resolver/action.rs`, `scene.rs`, `gltf.rs`) and the reference
interpreter that ships with the interactive-assets study's harness. Where
the two disagree with this text, this text is wrong and gets fixed before
the tag.
The interactive-assets study measures exactly what this document states.

## 1. What an action is

An action belongs to one `thing`. It has a name, at most one trigger, and an
ordered list of effect lines:

```
action Name { [trigger P hits=N]  effect  effect … }
```

Moxi declares; the consumer runs. The compiler checks every name and number
and exports the action; a small interpreter in the consumer runs it. What is
not the asset's own structure (how pieces fall, sound, damage, AI, the
default push speed) belongs to the consumer.

## 2. Effects

Every effect line may end in `delay=t` (seconds, t ≥ 0, default 0).

| Line | Meaning at run time |
|---|---|
| `detach P [push=out\|word] [strength=s]` | P and everything mated under it leave the thing as one piece, from where they are, kicked along the push direction at speed s (the thing's units per second; absent: the consumer's default). The piece keeps its own actions and counts. |
| `reveal P` / `hide P` | P becomes visible / invisible. Any part some action reveals is hidden when the asset loads. |
| `P material = M` | P now wears material M (exported as index and name). |
| `state S on` / `state S off` | Turns state S on or off. Turning on a state in a family turns the family's other states off. Where two on states change the same part, the one declared later in the source wins that part. |
| `play C` | Starts clip C. An `animate` without `loop` plays once and holds its last keyframe; with `loop` it repeats; a `revolve` turns until something stops it. |
| `run A` | Runs action A now (at this line's time), as described in §4. |
| `for k in a..b { … }` | Repetition, unrolled at compile time. |

`push`: `out` (the default) is away from what P hangs from (the normal of the
mate that places it); a compass or vertical word (`up`, `north`, …) is a
direction in the thing's own space. A mirrored copy's pushes are reflected.

## 3. Triggers and hits

1. A hit lands on one part.
2. It counts for every **armed** action whose trigger part is that part or
   contains it (an ancestor in the mate tree), walking outward from the part
   hit, and only through parts still attached to the thing: a part inside a
   piece that has come off no longer counts toward triggers outside it.
3. An action is **armed** while at least one part it detaches is still on.
   An action that detaches nothing is always armed, so it can run again.
4. When an action's count reaches N, it runs and its count goes back to 0.
   If one hit completes several actions, the innermost runs first.
5. When a part comes off, the counts of the actions triggered by that part
   go back to 0.
6. An action with no trigger runs only when the consumer fires it by name.
   Firing an unarmed action does nothing; firing does not touch counts. If
   several actions share the fired name (instances), the deepest runs first.

## 4. Time

1. An action's effects are started in line order. A line with no delay
   happens at once; a line with `delay=t` happens t seconds after the action
   started.
2. `run A` at time τ starts A at τ; A's own delays count from τ. Delays
   along a chain of `run`s therefore add up.
3. `run` starts A whether or not A is armed, and does not change A's count.
4. Effects due at the same moment happen in the order they were scheduled.
   The interpreter is deterministic: the same hits and fires at the same
   times always give the same result.

## 5. Checked at compile time

A script with any of these does not compile; every message lists the valid
names.

- An unknown part, material, state, clip or action.
- `detach` of the thing's root, or of a part no mate places.
- Two actions with the same name.
- `hits` not a whole number ≥ 1; `delay` < 0; `strength` ≤ 0.
- A cycle of `run`s (`A → B → A`), named in the error. The language stays
  total: every action finishes in a bounded number of steps.

`moxi report --run A,B` prints the scene after those actions (and the actions
they run): the pieces, what was revealed, hidden and re-dressed, and any
contact a cut tears through. Nothing moves: it reports structure, not time.

## 6. Composition

- An instance brings its thing's actions, prefixed with the instance name
  (`D.Explode`, triggers and targets `D.Head`), with `hits`, `delay` and
  `strength` evaluated under the instance's arguments.
- A thing's own actions may name parts inside its instances (`detach D.Head`,
  `trigger D.Body hits=1`).
- A mirrored instance brings its source's actions, renamed to its own parts,
  pushes reflected. A mirrored part inside a thing does not: write its action.
- Naming a whole instance: in a trigger it means the instance's root (hits
  anywhere inside it count); in `detach` the instance leaves as one piece; in
  `reveal`, `hide` or a material swap it means every part of it.
- Inside a `for`, an action's name is indexed (`action BreakLeg[k] { … }`).
- Imported things (`from "f.md" import T`) behave like local things.

## 7. Export

glTF `scenes[0].extras.moxi.actions`: a list of
`{ name, trigger: {part, hits} | null, effects: [...] }`, each effect
`{ do: detach|reveal|hide|look|state|play|run, …, delay }`; parts are node
names, pushes unit vectors in the printed thing's space, looks a material
index plus name. States travel as material variants plus `stateOrder` and
`stateFamilies`. Nodes nest by the mate tree, so a piece is a node and its
subtree.

## 8. Not in v0.5.0

Triggers other than hits and firing by name (contact, a state turning on,
timers); `every` / `children of` targets; a KH## 9. Settled (Andrew, 2026-10-11)

1. **Effects aimed at a part that has already come off.** `detach` of a part
   already off does nothing. `reveal`, `hide`, a material swap or a nested
   `detach` aimed at a part inside a fallen piece are defined only while the
   part is attached; what a consumer does otherwise is its own business (a consumer
   may freeze pieces as copies). The interactive-assets study's tasks
   never rely on it.
2. **`run` and `fire` differ on purpose.** `run` starts an action even when
   it is unarmed (§4.3): it is the asset's own sequence. `fire` of an
   unarmed action does nothing (§3.6): it is the consumer asking.
3. **Effects due at the same moment** happen in the order they were
   scheduled (§4.4). This is part of the semantics, not an accident of the
   interpreter.
