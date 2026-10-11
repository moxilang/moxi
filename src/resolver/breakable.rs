//! Breakable mates — hierarchical detachment, Issue E (DOC-20261008-detachment).
//!
//! `breakable Head { hits=5 }` says the mate placing `Head` can break; when it
//! does, `Head` and everything mated under it leave as one lump. The lump tree
//! is the mate tree, so nothing here builds a structure: this resolves names
//! to flattened mate subjects, checks the trigger, and hands the scene the
//! data a consumer needs (§5 of the doc is what a consumer does with it).
//!
//! Inheritance follows joints: a thing's breakables come with every instance
//! of it, prefixed, under the instance's arguments, and their push words stay
//! in the instanced thing's space (`space`). A `symmetric_across` image breaks
//! like its source, with its own counter; a line on the image is an error.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::value::{self, Value};

use super::joint::JointScope;
use super::{subst_expr, ParamEnv};

/// When a lump breaks.
#[derive(Debug, Clone, PartialEq)]
pub enum Trigger {
    /// After this many hits on it or anything inside it.
    Hits(u32),
    /// When the game fires this event.
    Event(String),
}

/// Which way a broken lump is pushed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Push {
    /// Away from its parent, along the mate.
    Out,
    /// A direction word, in the space of the thing that declared it.
    Dir(AimDir),
}

#[derive(Debug, Clone)]
pub struct ResolvedBreak {
    /// Flattened subject part of the mate that breaks: the lump's root.
    pub part:      String,
    pub trigger:   Trigger,
    pub push:      Push,
    /// The instance the declaration came from (its root frame carries the
    /// push words); `None` for this thing's own.
    pub space:     Option<String>,
    /// Set on a `symmetric_across` image: the source whose breakable it
    /// follows (its push is the source's, reflected).
    pub mirror_of: Option<String>,
    /// `after { P material = M }`: (flattened part, material index), in order.
    pub swaps:     Vec<(String, usize)>,
    /// `after { reveal P }`: flattened parts hidden until this lump breaks.
    pub reveal:    Vec<String>,
    pub span:      Span,
}

/// Before folding `hits`: kept on the template for instances that override
/// parameters.
#[derive(Debug, Clone)]
pub(super) struct RawBreak {
    pub part:   String,
    pub hits:   Option<Expr>,
    pub event:  Option<String>,
    pub push:   Push,
    pub space:  Option<String>,
    pub swaps:  Vec<(String, usize)>,
    pub reveal: Vec<String>,
    pub span:   Span,
}

impl RawBreak {
    pub(super) fn subst(&self, env: &ParamEnv) -> RawBreak {
        RawBreak { hits: self.hits.as_ref().map(|e| subst_expr(e, env)), ..self.clone() }
    }
    pub(super) fn prefixed(&self, prefix: &str) -> RawBreak {
        RawBreak {
            part:  format!("{prefix}.{}", self.part),
            swaps: self.swaps.iter().map(|(p, m)| (format!("{prefix}.{p}"), *m)).collect(),
            reveal: self.reveal.iter().map(|p| format!("{prefix}.{p}")).collect(),
            space: Some(match &self.space {
                Some(s) => format!("{prefix}.{s}"),
                None    => prefix.to_string(),
            }),
            ..self.clone()
        }
    }
}

/// This thing's own `breakable` declarations → raw breakables on flattened
/// subjects. Every problem is a `BreakError` naming what could be named.
pub(super) fn resolve_break_decls(
    decls: &[BreakDecl],
    sc:    &JointScope,
    after: &AfterScope,
    errs:  &mut Vec<MoxiError>,
) -> Vec<RawBreak> {
    let align_of: HashMap<&str, &str> = sc.own_align.iter().map(|(w, s)| (w.as_str(), s.as_str())).collect();
    let breakable = if sc.own_align.is_empty() {
        format!("'{}' has no mates, so nothing in it can break", sc.thing)
    } else {
        format!("parts that can break in '{}': {}", sc.thing,
            sc.own_align.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>().join(", "))
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in decls {
        let name = d.part.name.clone();
        let berr = |message: String| MoxiError::BreakError { part: name.clone(), message, span: d.span };
        let Some(subject) = align_of.get(name.as_str()) else {
            let msg = if let Some(src) = sc.own_mirror.get(&name) {
                format!("is the mirror image of '{src}'; give '{src}' the breakable — the image follows, \
                         with its own count")
            } else if sc.part_names.contains_key(&name) || sc.instance_of.contains_key(&name) {
                match name.split_once('.').filter(|(i, _)| sc.instance_of.contains_key(*i)) {
                    Some((inst, _)) => format!(
                        "is placed inside the instance '{inst}' (a '{}'); declare the breakable in '{}' itself",
                        sc.instance_of[inst], sc.instance_of[inst]),
                    None => format!(
                        "is not placed by a mate (it is the root of '{}'), so there is nothing to break it \
                         from — {breakable}", sc.thing),
                }
            } else {
                format!("is not a part of '{}' — {breakable}", sc.thing)
            };
            errs.push(berr(msg));
            continue;
        };
        if !seen.insert(subject.to_string()) {
            errs.push(berr("has two `breakable` declarations; merge them into one".into()));
            continue;
        }
        match (&d.hits, &d.event) {
            (None, None) => {
                errs.push(berr("needs a trigger: `hits=N` (breaks after N hits) or `event=Name` \
                                (breaks when the game fires Name)".into()));
                continue;
            }
            (Some(_), Some(_)) => {
                errs.push(berr("has both `hits` and `event`; a lump breaks one way — keep one".into()));
                continue;
            }
            _ => {}
        }
        let push = match &d.push {
            None => Push::Out,
            Some(w) if w.name == "out" => Push::Out,
            Some(w) => match AimDir::parse(&w.name) {
                Some(dir) => Push::Dir(dir),
                None => {
                    errs.push(berr(format!(
                        "`push={}` is not a direction — use out (away from the parent, the default) or {}",
                        w.name, AimDir::WORDS.join(", "))));
                    continue;
                }
            },
        };
        let (swaps, reveal) = resolve_after(&name, &d.after, sc, after, errs);
        out.push(RawBreak {
            part:  subject.to_string(),
            hits:  d.hits.clone(),
            event: d.event.as_ref().map(|e| e.name.clone()),
            push,
            space: None,
            swaps,
            reveal,
            span:  d.span,
        });
    }
    out
}

/// Fold `hits` to a whole number ≥ 1.
pub(super) fn fold_break(raw: &RawBreak, env: &ParamEnv, errs: &mut Vec<MoxiError>) -> Option<ResolvedBreak> {
    let berr = |message: String| MoxiError::BreakError { part: raw.part.clone(), message, span: raw.span };
    let trigger = match (&raw.hits, &raw.event) {
        (Some(e), _) => match value::eval(e, env) {
            Ok(Value::Num(v)) if v >= 1.0 && v.fract() == 0.0 && v <= u32::MAX as f64 => Trigger::Hits(v as u32),
            Ok(Value::Num(v)) => {
                errs.push(berr(format!("`hits` must be a whole number of at least 1, got {v}")));
                return None;
            }
            Ok(other) => {
                errs.push(berr(format!("`hits` must be a number, got a {}", other.kind())));
                return None;
            }
            Err(e) => {
                errs.push(berr(format!("`hits`: {}", e.message)));
                return None;
            }
        },
        (None, Some(ev)) => Trigger::Event(ev.clone()),
        (None, None) => return None, // reported when declared
    };
    Some(ResolvedBreak {
        part: raw.part.clone(), trigger, push: raw.push, space: raw.space.clone(), mirror_of: None,
        swaps: raw.swaps.clone(), reveal: raw.reveal.clone(), span: raw.span,
    })
}

/// What an `after` block may name.
pub(super) struct AfterScope<'a> {
    /// Every flattened part with a shape, in declared order.
    pub part_order:     &'a [String],
    pub material_index: &'a HashMap<String, usize>,
}

/// An `after` block's lines → (swaps, reveals) on flattened parts. A name is
/// a part of this thing, a part inside an instance (`Left.Neck`), or a whole
/// instance (every part of it), as in a `state`.
fn resolve_after(
    lump:  &str,
    lines: &[AfterLine],
    sc:    &JointScope,
    after: &AfterScope,
    errs:  &mut Vec<MoxiError>,
) -> (Vec<(String, usize)>, Vec<String>) {
    let mut materials: Vec<(&String, &usize)> = after.material_index.iter().collect();
    materials.sort_by_key(|(_, i)| **i);
    let material_list = materials.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
    let berr = |message: String, span: Span| MoxiError::BreakError { part: lump.to_string(), message, span };
    let targets = |part: &Ident, errs: &mut Vec<MoxiError>| -> Vec<String> {
        if after.part_order.contains(&part.name) {
            return vec![part.name.clone()];
        }
        if sc.instance_of.contains_key(&part.name) {
            let pre = format!("{}.", part.name);
            return after.part_order.iter().filter(|p| p.starts_with(&pre)).cloned().collect();
        }
        let tops: Vec<&str> = after.part_order.iter().filter(|p| !p.contains('.')).map(|p| p.as_str())
            .chain(sc.instance_of.keys().map(|s| s.as_str())).collect();
        errs.push(berr(format!("after: no part '{}' in '{}' — parts: {}", part.name, sc.thing, tops.join(", ")), part.span));
        Vec::new()
    };
    let mut swaps: Vec<(String, usize)> = Vec::new();
    let mut reveal: Vec<String> = Vec::new();
    for l in lines {
        match l {
            AfterLine::Swap { part, material } => {
                let Some(&m) = after.material_index.get(&material.name) else {
                    errs.push(berr(format!("after: no material '{}' — materials: {material_list}", material.name), material.span));
                    continue;
                };
                for t in targets(part, errs) {
                    if swaps.iter().any(|(p, _)| *p == t) {
                        errs.push(berr(format!("after: sets '{t}' material twice"), part.span));
                        continue;
                    }
                    swaps.push((t, m));
                }
            }
            AfterLine::Reveal { part } => {
                for t in targets(part, errs) {
                    if reveal.contains(&t) {
                        errs.push(berr(format!("after: reveals '{t}' twice"), part.span));
                        continue;
                    }
                    reveal.push(t);
                }
            }
        }
    }
    (swaps, reveal)
}

/// A `symmetric_across` image breaks like its source, with its own count.
/// Applied to the thing's flattened relations, so images inside instances
/// (expanded to one `Mirror` per part) are covered too.
pub(super) fn add_mirror_images(breaks: &mut Vec<ResolvedBreak>, relations: &[Placement]) {
    let have: HashSet<String> = breaks.iter().map(|b| b.part.clone()).collect();
    let image_of: HashMap<&str, &str> = relations.iter().filter_map(|p| match p {
        Placement::Mirror { subject, source, .. } => Some((source.as_str(), subject.as_str())),
        _ => None,
    }).collect();
    let mut add = Vec::new();
    for p in relations {
        let Placement::Mirror { subject, source, .. } = p else { continue };
        if have.contains(subject) { continue; }
        if let Some(src) = breaks.iter().find(|b| b.part == *source) {
            // the after block names the images of the parts it names, where
            // they have one (a part with none, like a shared torso, is the
            // same part on both sides)
            let image = |p: &String| image_of.get(p.as_str()).map(|s| s.to_string()).unwrap_or_else(|| p.clone());
            add.push(ResolvedBreak {
                part: subject.clone(), mirror_of: Some(source.clone()),
                swaps: src.swaps.iter().map(|(p, m)| (image(p), *m)).collect(),
                reveal: src.reveal.iter().map(image).collect(),
                ..src.clone()
            });
        }
    }
    breaks.extend(add);
}

/// Checks on every `after` block (DOC-20261008-detachment §6), once the
/// thing's breakables, its instances' and its mirror images' are known:
/// - an after part is AT THE CUT: inside the lump, the part the lump hangs
///   from, or another part hanging from that one (a stump, a collar);
/// - a revealed part is revealed by one lump only, and is neither the root
///   nor the lump's own root.
pub(super) fn check_after(breaks: &[ResolvedBreak], relations: &[Placement], errs: &mut Vec<MoxiError>) {
    let parent = parents(relations);
    let children = |p: &str| -> Vec<&str> { parent.iter().filter(|(_, q)| q.as_str() == p).map(|(c, _)| c.as_str()).collect() };
    let mut revealed_by: HashMap<&str, &str> = HashMap::new();
    for b in breaks {
        if b.swaps.is_empty() && b.reveal.is_empty() { continue; }
        let berr = |message: String| MoxiError::BreakError { part: b.part.clone(), message, span: b.span };
        // the lump: its root and everything under it
        let mut lump: Vec<&str> = vec![b.part.as_str()];
        let mut i = 0;
        while i < lump.len() {
            let kids = children(lump[i]);
            lump.extend(kids);
            i += 1;
        }
        let up = parent.get(b.part.as_str()).map(|s| s.as_str());
        let mut at_cut: Vec<&str> = lump.clone();
        if let Some(u) = up {
            at_cut.push(u);
            at_cut.extend(children(u).into_iter().filter(|c| !lump.contains(c)));
        }
        let names: Vec<&String> = b.swaps.iter().map(|(p, _)| p).chain(&b.reveal).collect();
        for p in names {
            if !at_cut.contains(&p.as_str()) {
                errs.push(berr(format!(
                    "after: '{p}' is not at the cut — an after block changes the broken lump, the part it hangs \
                     from{}, or a part hanging from that one: {}",
                    up.map(|u| format!(" ('{u}')")).unwrap_or_default(), at_cut.join(", "))));
            }
        }
        for p in &b.reveal {
            if *p == b.part {
                errs.push(berr(format!("after: reveals its own lump '{p}' — a lump cannot be hidden until it breaks off; \
                    reveal a part at the cut instead (a stump)")));
            } else if up == Some(p.as_str()) {
                errs.push(berr(format!("after: reveals '{p}', the part the lump hangs from — it is there before the \
                    break; reveal a part placed at the cut instead (a stump hanging from '{p}')")));
            }
            match revealed_by.get(p.as_str()) {
                Some(other) => errs.push(berr(format!(
                    "after: '{p}' is already revealed by '{other}' — a part is revealed by one lump only{}",
                    if b.mirror_of.is_some() { "; mirror the revealed part too, so each side has its own" } else { "" }))),
                None => { revealed_by.insert(p.as_str(), b.part.as_str()); }
            }
        }
    }
}

/// The mate tree as part → parent (joints.rs's rule): a mate's object is its
/// subject's parent; a mirror image hangs from the image of its source's
/// parent when that was mirrored too, else from the same parent.
fn parents(relations: &[Placement]) -> HashMap<String, String> {
    let mut parent: HashMap<String, String> = HashMap::new();
    let mut image_of: HashMap<&str, &str> = HashMap::new();
    for p in relations {
        match p {
            Placement::Align { subject, object, .. } => { parent.insert(subject.part.clone(), object.part.clone()); }
            Placement::Mirror { subject, source, .. } => { image_of.insert(source.as_str(), subject.as_str()); }
        }
    }
    for p in relations {
        if let Placement::Mirror { subject, source, .. } = p {
            if let Some(sp) = parent.get(source).cloned() {
                let up = image_of.get(sp.as_str()).map(|s| s.to_string()).unwrap_or(sp);
                parent.insert(subject.clone(), up);
            }
        }
    }
    parent
}

#[cfg(test)]
mod tests {
    use crate::pipeline::compile_to_scene;

    fn errors(src: &str) -> String {
        match compile_to_scene(src) {
            Ok(_) => panic!("expected an error"),
            Err(es) => es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        }
    }

    const DEMON: &str = "material Bone { color = \"#ddd8c4\" }
    thing Demon(toughness=5) {
        part Body { shape = box(width=40, height=60, depth=30), material = Bone }
        part Head { shape = sphere(radius=14), material = Bone }
        part EyeR { shape = sphere(radius=3), material = Bone }
        part EyeL { shape = sphere(radius=3), material = Bone }
        part ArmR { shape = capsule(height=30, radius=4), material = Bone }
        part ArmL { shape = capsule(height=30, radius=4), material = Bone }
        relation {
            Head.bottom on Body.top
            EyeR.center on Head.north shift=(3, 5)
            EyeL.center on Head.north shift=(3, -5)
            ArmR.top on Body.east
            ArmL symmetric_across Body from=ArmR
        }
        BREAKS
    }";

    fn demon(breaks: &str) -> String { DEMON.replace("BREAKS", breaks) + "\nprint Demon" }

    #[test]
    fn a_breakable_reaches_the_scene() {
        let scene = compile_to_scene(&demon("breakable Head { hits=toughness }  breakable EyeR { hits=1 }")).unwrap();
        let part = |n: &str| scene.layers[0].parts.iter().find(|p| p.name == n).unwrap();
        let head = part("Head").breakable.as_ref().expect("Head breaks");
        assert_eq!(head.hits, Some(5));
        assert_eq!(head.event, None);
        assert!(part("EyeR").breakable.is_some());
        assert!(part("Body").breakable.is_none() && part("EyeL").breakable.is_none());
    }

    fn scene_parts(src: &str) -> std::collections::HashMap<String, crate::scene::Part> {
        let scene = compile_to_scene(src).unwrap_or_else(|e| panic!("compiles: {e:?}"));
        scene.layers.into_iter().flat_map(|l| l.parts).map(|p| (p.name.clone(), p)).collect()
    }
    fn near(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    /// `push`: `out` leaves the socket (up off the body's top; along the
    /// head's north socket for the centre-mated eye); a word is the thing's
    /// own direction; an event trigger exports the event, not hits.
    #[test]
    fn push_and_triggers_are_exported_in_the_things_space() {
        let p = scene_parts(&demon("breakable Head { hits=3 }  breakable EyeR { event=Pop }  \
                                    breakable ArmR { hits=2 push=down }"));
        let head = p["Head"].breakable.clone().unwrap();
        assert!(near(head.push, [0.0, 1.0, 0.0]), "{:?}", head.push);
        let eye = p["EyeR"].breakable.clone().unwrap();
        assert_eq!((eye.hits, eye.event.as_deref()), (None, Some("Pop")));
        assert!(near(eye.push, [0.0, 0.0, 1.0]), "centre mate: the socket's normal, {:?}", eye.push);
        assert!(near(p["ArmR"].breakable.clone().unwrap().push, [0.0, -1.0, 0.0]));
    }

    /// A mirror image breaks like its source, with its own entry, and its
    /// push is the source's reflected: east on the right arm is west on the
    /// left.
    #[test]
    fn a_mirror_image_follows_its_source_reflected() {
        let p = scene_parts(&demon("breakable ArmR { hits=2 push=east }"));
        let (r, l) = (p["ArmR"].breakable.clone().unwrap(), p["ArmL"].breakable.clone().unwrap());
        assert_eq!(l.hits, Some(2));
        assert!(near(r.push, [1.0, 0.0, 0.0]) && near(l.push, [-1.0, 0.0, 0.0]), "{:?} / {:?}", r.push, l.push);
        let p = scene_parts(&demon("breakable ArmR { hits=2 }"));
        let (r, l) = (p["ArmR"].breakable.clone().unwrap(), p["ArmL"].breakable.clone().unwrap());
        assert!(near(l.push, [-r.push[0], r.push[1], r.push[2]]), "out mirrors too: {:?} / {:?}", r.push, l.push);
    }

    /// Instances inherit a thing's breakables, prefixed, under their own
    /// arguments; push words turn with the instance; an instance-to-instance
    /// mirror gives the image's parts their source's breakables.
    #[test]
    fn instances_inherit_breakables_under_their_arguments() {
        // (no mirrored arm inside: mirroring a whole demon would mirror an image)
        let src = DEMON.replace("BREAKS", "breakable Head { hits=toughness push=north }")
            .replace("part ArmL { shape = capsule(height=30, radius=4), material = Bone }", "")
            .replace("ArmL symmetric_across Body from=ArmR", "") + "
        thing Pit {
            part Floor { shape = box(width=300, height=10, depth=300), material = Bone }
            part Wall  { shape = box(width=300, height=200, depth=10), material = Bone }
            part A     { thing = Demon(toughness=9) }
            part B     { thing = Demon }
            part C     { thing = Demon }
            relation {
                Wall.bottom on Floor.top shift=(0, -140)
                A.bottom on Floor.top shift=(-60, 0)
                B.bottom on Wall.north shift=(40, 60)
                C symmetric_across Floor from=A
            }
            breakable B { event=Fall }
        }
        print Pit";
        let p = scene_parts(&src);
        assert_eq!(p["A.Head"].breakable.as_ref().unwrap().hits, Some(9), "under A's arguments");
        assert_eq!(p["B.Head"].breakable.as_ref().unwrap().hits, Some(5), "under the defaults");
        assert_eq!(p["C.Head"].breakable.as_ref().unwrap().hits, Some(9), "the image follows A");
        assert!(near(p["A.Head"].breakable.as_ref().unwrap().push, [0.0, 0.0, 1.0]));
        // B stands on the wall's north face: its own north is turned.
        let b_north = p["B.Head"].breakable.as_ref().unwrap().push;
        let root = &p["B.Body"].frame.rot;
        let want = [root[0][2], root[1][2], root[2][2]];
        assert!(near(b_north, want) && b_north[2].abs() < 0.5, "turned with B: {b_north:?} vs {want:?}");
        // The outer thing may break the whole instance by its own mate.
        assert_eq!(p["B.Body"].breakable.as_ref().unwrap().event.as_deref(), Some("Fall"));
        // …but not a part inside it.
        let m = errors(&src.replace("breakable B { event=Fall }", "breakable B.Head { hits=1 }"));
        assert!(m.contains("is placed inside the instance 'B' (a 'Demon'); declare the breakable in 'Demon' itself"), "{m}");
    }

    /// One breakable per iteration in a `for`, and at the top level by index.
    #[test]
    fn breakables_unroll_in_loops() {
        let p = scene_parts("thing Crown(n=4) {
            part Band { shape = cylinder(height=10, radius=30) }
            for k in 0..n {
                part Spike[k] { shape = cone(height=20, radius=4) }
                relation { Spike[k].bottom on Band.top shift=(20, 0) twist=90 * k }
                breakable Spike[k] { hits=k + 1 }
            }
        }
        print Crown");
        for k in 0..4u32 {
            assert_eq!(p[&format!("Spike[{k}]")].breakable.as_ref().unwrap().hits, Some(k + 1));
        }
    }

    /// glTF: the node carries `moxi.breakable`; nothing else changes.
    #[test]
    fn gltf_nodes_carry_breakable_extras() {
        let scene = compile_to_scene(&demon("breakable Head { hits=5 }  breakable EyeR { event=Pop }")).unwrap();
        let g = crate::gltf::scene_to_gltf(&scene, None);
        let node = |n: &str| g.json["nodes"].as_array().unwrap().iter().find(|x| x["name"] == n).unwrap().clone();
        assert_eq!(node("Head")["extras"]["moxi"]["breakable"]["hits"], 5);
        assert_eq!(node("EyeR")["extras"]["moxi"]["breakable"]["event"], "Pop");
        assert!(node("EyeR")["extras"]["moxi"]["breakable"].get("hits").is_none());
        assert!(node("Body")["extras"]["moxi"].get("breakable").is_none());
        let plain = compile_to_scene(&demon("")).unwrap();
        let a = serde_json::to_string(&crate::gltf::scene_to_gltf(&plain, None).json).unwrap();
        assert!(!a.contains("breakable"), "absent unless declared");
    }

    #[test]
    fn every_problem_names_the_fix() {
        let m = errors(&demon("breakable Body { hits=1 }"));
        assert!(m.contains("breakable 'Body': is not placed by a mate (it is the root of 'Demon'), so there is \
                            nothing to break it from — parts that can break in 'Demon': Head, EyeR, EyeL, ArmR"), "{m}");
        let m = errors(&demon("breakable Haed { hits=1 }"));
        assert!(m.contains("breakable 'Haed': is not a part of 'Demon'"), "{m}");
        let m = errors(&demon("breakable ArmL { hits=1 }"));
        assert!(m.contains("is the mirror image of 'ArmR'; give 'ArmR' the breakable"), "{m}");
        let m = errors(&demon("breakable Head { push=up }"));
        assert!(m.contains("needs a trigger: `hits=N`"), "{m}");
        let m = errors(&demon("breakable Head { hits=2 event=Pop }"));
        assert!(m.contains("has both `hits` and `event`"), "{m}");
        let m = errors(&demon("breakable Head { hits=0 }"));
        assert!(m.contains("`hits` must be a whole number of at least 1, got 0"), "{m}");
        let m = errors(&demon("breakable Head { hits=2.5 }"));
        assert!(m.contains("got 2.5"), "{m}");
        let m = errors(&demon("breakable Head { hits=1 push=sideways }"));
        assert!(m.contains("`push=sideways` is not a direction — use out (away from the parent, the default) \
                            or north, south, east, west, up, down"), "{m}");
        let m = errors(&demon("breakable Head { hits=1 }  breakable Head { hits=2 }"));
        assert!(m.contains("has two `breakable` declarations"), "{m}");
        let m = errors(&demon("breakable Head { on=Pop }"));
        assert!(m.contains("the trigger key is `event` (`on` is the mate keyword)"), "{m}");
    }

    // ── after { … } and reveal (Issue G) ──

    const STUMPS: &str = "material Bone  { color = \"#ddd8c4\" }
    material Flesh { color = \"#7a1414\" }
    thing Demon {
        part Body  { shape = box(width=40, height=60, depth=30), material = Bone }
        part Neck  { shape = cylinder(height=8, radius=7), material = Bone }
        part Head  { shape = sphere(radius=14), material = Bone }
        part Stump { shape = cylinder(height=3, radius=6), material = Flesh }
        part EyeR  { shape = sphere(radius=3), material = Bone }
        part ArmR  { shape = capsule(height=30, radius=4), material = Bone }
        part ArmL  { shape = capsule(height=30, radius=4), material = Bone }
        part NubR  { shape = sphere(radius=4), material = Flesh }
        part NubL  { shape = sphere(radius=4), material = Flesh }
        relation {
            Neck.bottom  on Body.top
            Head.bottom  on Neck.top gap=-2
            Stump.bottom on Neck.top
            EyeR.center  on Head.north shift=(3, 5)
            ArmR.top     on Body.east shift=(20, 0)
            ArmL symmetric_across Body from=ArmR
            NubR.center  on Body.east shift=(20, 0)
            NubL symmetric_across Body from=NubR
        }
        BREAKS
    }";

    const PAIR: &str = "
    thing Pair {
        part Floor { shape = box(width=200, height=2, depth=60), material = Bone }
        part A { thing = Demon }
        relation { A.Body.bottom on Floor.top }
    }";

    fn stumps(breaks: &str, print: &str) -> String {
        let pair = if print == "Pair" { PAIR } else { "" };
        STUMPS.replace("BREAKS", breaks) + pair + "\nprint " + print
    }

    const AFTER: &str = "breakable Head { hits=5  after { Neck material = Flesh  Head material = Flesh  reveal Stump } }  \
                         breakable ArmR { hits=3  after { reveal NubR } }";

    #[test]
    fn after_swaps_and_reveals_reach_the_scene_and_mirrors_name_their_own_parts() {
        let p = scene_parts(&stumps(AFTER, "Demon"));
        let head = p["Head"].breakable.clone().unwrap();
        let swapped: Vec<(&str, Option<&str>)> = head.after.iter().map(|(k, l)| (k.as_str(), l.material.as_deref())).collect();
        assert_eq!(swapped, vec![("Head", Some("Flesh")), ("Neck", Some("Flesh"))]);
        assert_eq!(head.reveal, vec!["Stump".to_string()]);
        assert!(p["Stump"].hidden && p["NubR"].hidden && p["NubL"].hidden);
        assert!(!p["Neck"].hidden && !p["Head"].hidden);
        assert_eq!(p["ArmL"].breakable.clone().unwrap().reveal, vec!["NubL".to_string()], "the image reveals the image");
        // an instance brings them, prefixed
        let p = scene_parts(&stumps(AFTER, "Pair"));
        assert_eq!(p["A.Head"].breakable.clone().unwrap().reveal, vec!["A.Stump".to_string()]);
        assert!(p["A.Head"].breakable.clone().unwrap().after.contains_key("A.Neck"));
        assert!(p["A.Stump"].hidden);
        // nothing is hidden without a reveal
        let p = scene_parts(&stumps("breakable Head { hits=5 }", "Demon"));
        assert!(p.values().all(|x| !x.hidden));
    }

    #[test]
    fn hidden_parts_are_left_out_of_every_drawing_of_rest_and_kept_in_gltf() {
        let src = stumps(AFTER, "Demon");
        let plain = stumps("breakable Head { hits=5 }", "Demon");
        let parts = |s: &str| crate::pipeline::compile_source(s).unwrap().layers[0].parts;
        assert_eq!(parts(&plain) - parts(&src), 3, "Stump, NubR, NubL not rasterized");
        let scene = compile_to_scene(&src).unwrap();
        let glsl = crate::shader::glsl_map(&scene);
        assert!(glsl.contains("// Neck") && !glsl.contains("// Stump") && !glsl.contains("// NubL"));
        let html = crate::shader::html_page(&scene, "t");
        assert!(!html.contains("// Stump"));
        let dir = std::env::temp_dir().join(format!("moxi-hidden-{}", std::process::id()));
        crate::mesh::export_scene_obj(&scene, Some(2.0), dir.join("d").to_str().unwrap()).unwrap();
        let obj = std::fs::read_to_string(dir.join("d.obj")).unwrap();
        assert!(obj.contains("o Neck") && !obj.contains("o Stump") && !obj.contains("o NubR"));
        let _ = std::fs::remove_dir_all(dir);
        let g = crate::gltf::scene_to_gltf(&scene, None).json;
        let node = |n: &str| g["nodes"].as_array().unwrap().iter().find(|x| x["name"] == n).unwrap().clone();
        assert_eq!(node("Stump")["extras"]["moxi"]["hidden"], true);
        assert!(node("Neck")["extras"]["moxi"].get("hidden").is_none());
        let after = &node("Head")["extras"]["moxi"]["breakable"]["after"];
        let mats = g["materials"].as_array().unwrap();
        assert_eq!(mats[after["Neck"].as_u64().unwrap() as usize]["name"], "Flesh");
        assert_eq!(node("Head")["extras"]["moxi"]["breakable"]["reveal"], serde_json::json!(["Stump"]));
    }

    #[test]
    fn report_break_shows_what_the_break_reveals_and_swaps() {
        let src = stumps(AFTER, "Demon");
        let rest = crate::report::from_source(&src, None).unwrap();
        let t = &rest.things[0];
        assert!(!t.parts.iter().any(|p| p.name == "Stump"), "not there at rest");
        assert_eq!(t.hidden.iter().map(|h| (h.part.as_str(), h.until.as_str())).collect::<Vec<_>>(),
            vec![("Stump", "Head"), ("NubR", "ArmR"), ("NubL", "ArmL")]);
        let broke = crate::report::from_source_broken(&src, None, &["Head".to_string()]).unwrap();
        let t = &broke.things[0];
        assert!(t.parts.iter().any(|p| p.name == "Stump"), "revealed by the break");
        assert_eq!(t.torn.as_deref(), Some(&[][..]), "a stump where the head stood is no tear");
        assert!(t.looks.iter().any(|l| l.part == "Neck" && l.state == "after Head" && l.look.material.as_deref() == Some("Flesh")));
        assert!(broke.to_text(false).contains("hidden: NubR (until ArmR breaks), NubL (until ArmL breaks)"));
    }

    #[test]
    fn after_problems_name_the_fix() {
        let m = errors(&stumps("breakable EyeR { hits=1  after { ArmR material = Flesh } }", "Demon"));
        assert!(m.contains("breakable 'EyeR': after: 'ArmR' is not at the cut — an after block changes the broken lump, \
                            the part it hangs from ('Head'), or a part hanging from that one: EyeR, Head"), "{m}");
        let m = errors(&stumps("breakable Head { hits=1  after { reveal Head } }", "Demon"));
        assert!(m.contains("after: reveals its own lump 'Head'"), "{m}");
        let m = errors(&stumps("breakable Head { hits=1  after { reveal Neck } }", "Demon"));
        assert!(m.contains("after: reveals 'Neck', the part the lump hangs from"), "{m}");
        let m = errors(&stumps("breakable Head { hits=1  after { reveal Stump } }  \
                                breakable Stump { hits=1  after { reveal Stump } }", "Demon"));
        assert!(m.contains("reveals its own lump 'Stump'"), "{m}");
        let m = errors(&stumps("breakable ArmR { hits=1  after { reveal Stump } }", "Demon"));
        assert!(m.contains("'Stump' is not at the cut"), "{m}");
        // a revealed part with no image is revealed by the source and its image
        let shared = STUMPS.replace("NubR.center  on Body.east shift=(20, 0)", "NubR.center  on Body.east shift=(-20, 0)")
            .replace("NubL symmetric_across Body from=NubR", "NubL.center on Body.top shift=(0, 30)")
            .replace("BREAKS", "breakable ArmR { hits=1  after { reveal Body } }") + "\nprint Demon";
        let m = errors(&shared);
        assert!(m.contains("reveals 'Body', the part the lump hangs from"), "{m}");
        let m = errors(&stumps("breakable Head { hits=1  after { Nek material = Flesh } }", "Demon"));
        assert!(m.contains("after: no part 'Nek' in 'Demon' — parts: Body, Neck, Head"), "{m}");
        let m = errors(&stumps("breakable Head { hits=1  after { Neck material = Flsh } }", "Demon"));
        assert!(m.contains("after: no material 'Flsh' — materials: Bone, Flesh"), "{m}");
    }

    #[test]
    fn a_revealed_part_is_revealed_once_and_an_image_needs_its_own() {
        // two lumps hanging from Body both reveal NubR (it hangs from Body too)
        let m = errors(&stumps("breakable ArmR { hits=1  after { reveal NubR } }  \
                                breakable Neck { hits=1  after { reveal NubR } }", "Demon"));
        assert!(m.contains("'NubR' is already revealed by 'ArmR' — a part is revealed by one lump only"), "{m}");
        // the mirror image of ArmR would reveal the same unmirrored part
        let src = STUMPS.replace("NubL symmetric_across Body from=NubR", "NubL.center on Body.top shift=(0, 30)")
            .replace("BREAKS", "breakable ArmR { hits=1  after { reveal Neck } }") + "\nprint Demon";
        let m = errors(&src);
        assert!(m.contains("'Neck' is already revealed by 'ArmR' — a part is revealed by one lump only; mirror the \
                            revealed part too"), "{m}");
    }

    #[test]
    fn after_syntax_errors_point_at_the_form() {
        let m = errors(&stumps("breakable Head { hits=1  reveal Stump }", "Demon"));
        assert!(m.contains("`reveal` inside `after { … }`"), "{m}");
        let m = errors(&stumps("breakable Head { hits=1  after { Flesh.color = red } }", "Demon"));
        assert!(m.contains("an after block changes which material a part wears; to change a material everywhere, use a `state`"), "{m}");
        let m = errors(&stumps("breakable Head { hits=1  after { reveal Stump }  after { Neck material = Flesh } }", "Demon"));
        assert!(m.contains("one `after { … }` per breakable"), "{m}");
    }
}