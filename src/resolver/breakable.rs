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
    pub span:      Span,
}

/// Before folding `hits`: kept on the template for instances that override
/// parameters.
#[derive(Debug, Clone)]
pub(super) struct RawBreak {
    pub part:  String,
    pub hits:  Option<Expr>,
    pub event: Option<String>,
    pub push:  Push,
    pub space: Option<String>,
    pub span:  Span,
}

impl RawBreak {
    pub(super) fn subst(&self, env: &ParamEnv) -> RawBreak {
        RawBreak { hits: self.hits.as_ref().map(|e| subst_expr(e, env)), ..self.clone() }
    }
    pub(super) fn prefixed(&self, prefix: &str) -> RawBreak {
        RawBreak {
            part:  format!("{prefix}.{}", self.part),
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
pub(super) fn resolve_break_decls(decls: &[BreakDecl], sc: &JointScope, errs: &mut Vec<MoxiError>) -> Vec<RawBreak> {
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
        out.push(RawBreak {
            part:  subject.to_string(),
            hits:  d.hits.clone(),
            event: d.event.as_ref().map(|e| e.name.clone()),
            push,
            space: None,
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
        part: raw.part.clone(), trigger, push: raw.push, space: raw.space.clone(), mirror_of: None, span: raw.span,
    })
}

/// A `symmetric_across` image breaks like its source, with its own count.
/// Applied to the thing's flattened relations, so images inside instances
/// (expanded to one `Mirror` per part) are covered too.
pub(super) fn add_mirror_images(breaks: &mut Vec<ResolvedBreak>, relations: &[Placement]) {
    let have: HashSet<String> = breaks.iter().map(|b| b.part.clone()).collect();
    let mut add = Vec::new();
    for p in relations {
        let Placement::Mirror { subject, source, .. } = p else { continue };
        if have.contains(subject) { continue; }
        if let Some(src) = breaks.iter().find(|b| b.part == *source) {
            add.push(ResolvedBreak { part: subject.clone(), mirror_of: Some(source.clone()), ..src.clone() });
        }
    }
    breaks.extend(add);
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
}
