//! States — living models, step 4 (DOC-20261004-living-models-design §3.3).
//!
//! `state Angry { Eyes.color = "#ff2414"  EyeR material = Hole }` overrides
//! MATERIALS only: a material's `color` / `emissive`, or which material a part
//! wears. Resolved here into material indices and part names; the pipeline
//! turns each state into per-part looks (only the parts that differ from
//! rest) and `moxi gltf` exports them as KHR_materials_variants.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::value::{self, Value};

use super::{subst_expr, ParamEnv};

#[derive(Debug, Clone)]
pub struct ResolvedState {
    pub name:           String,
    /// (flattened part, material index) — which material a part wears.
    pub part_materials: Vec<(String, usize)>,
    /// (material index, attribute) — applied to every part wearing that
    /// material in this state.
    pub attrs:          Vec<(usize, StateAttr)>,
    /// `Some(inst)` for a state inherited from an instance: its material
    /// attributes change only parts named `inst.…`.
    pub scope:          Option<String>,
    /// Emissive values as written, for instances that override parameters.
    pub emissive_exprs: Vec<(usize, Expr)>,
    pub span:           Span,
}

impl ResolvedState {
    /// The same state under an instance: name and parts prefixed, scoped to
    /// the instance, emissive values re-evaluated under its arguments.
    pub(super) fn inherited(&self, prefix: &str, env: &ParamEnv, errs: &mut Vec<MoxiError>) -> ResolvedState {
        let name = format!("{prefix}.{}", self.name);
        let mut attrs = self.attrs.clone();
        let mut emissive_exprs = Vec::new();
        for (m, e) in &self.emissive_exprs {
            let e = subst_expr(e, env);
            match value::eval(&e, env) {
                Ok(Value::Num(v)) if v >= 0.0 => {
                    for (am, a) in attrs.iter_mut() {
                        if am == m { if let StateAttr::Emissive(x) = a { *x = v; } }
                    }
                }
                _ => errs.push(MoxiError::StateError { state: name.clone(), message:
                    "an emissive value is not a number ≥ 0 under this instance's arguments".into(), span: self.span }),
            }
            emissive_exprs.push((*m, e));
        }
        ResolvedState {
            name,
            part_materials: self.part_materials.iter().map(|(p, m)| (format!("{prefix}.{p}"), *m)).collect(),
            attrs,
            scope: Some(match &self.scope { Some(s) => format!("{prefix}.{s}"), None => prefix.to_string() }),
            emissive_exprs,
            span: self.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateAttr {
    /// Unresolved colour (a palette name or hex), like a material's.
    Color(String),
    Emissive(f64),
}

pub(super) fn walk_state_exprs(st: &mut StateDecl, f: &mut dyn FnMut(&Expr) -> Expr) {
    for l in &mut st.lines {
        if let StateLine::Attr { value, .. } = l {
            *value = f(value);
        }
    }
}

pub(super) struct StateScope<'a> {
    pub thing:          &'a str,
    pub part_names:     &'a HashMap<String, Span>,
    /// declared order of shaped parts, for messages
    pub part_order:     &'a [String],
    pub instance_of:    &'a HashMap<String, String>,
    pub material_index: &'a HashMap<String, usize>,
}

pub(super) fn resolve_states(
    decls:     &[StateDecl],
    sc:        &StateScope,
    env:       &ParamEnv,
    color_str: &dyn Fn(&Expr) -> String,
    errs:      &mut Vec<MoxiError>,
) -> Vec<ResolvedState> {
    let mut materials: Vec<(&String, &usize)> = sc.material_index.iter().collect();
    materials.sort_by_key(|(_, i)| **i);
    let material_list = materials.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in decls {
        let name = d.name.name.clone();
        let serr = |message: String, span: Span| MoxiError::StateError { state: name.clone(), message, span };
        if name == "rest" {
            errs.push(serr("'rest' is the script's own look (every material as written) and cannot be \
                            redefined".into(), d.name.span));
            continue;
        }
        if !seen.insert(name.clone()) {
            errs.push(MoxiError::DuplicateName { name: name.clone(), span: d.name.span });
            continue;
        }
        let mut st = ResolvedState { name: name.clone(), part_materials: Vec::new(), attrs: Vec::new(),
            scope: None, emissive_exprs: Vec::new(), span: d.span };
        let material = |id: &Ident, errs: &mut Vec<MoxiError>| -> Option<usize> {
            let m = sc.material_index.get(&id.name).copied();
            if m.is_none() {
                errs.push(serr(format!("no material '{}' — materials: {material_list}", id.name), id.span));
            }
            m
        };
        for line in &d.lines {
            match line {
                StateLine::Attr { material: mid, attr, value } => {
                    let Some(m) = material(mid, errs) else { continue };
                    let a = match attr.name.as_str() {
                        "color" => StateAttr::Color(color_str(value)),
                        "emissive" => match value::eval(value, env) {
                            Ok(Value::Num(v)) if v >= 0.0 => StateAttr::Emissive(v),
                            Ok(Value::Num(v)) => {
                                errs.push(serr(format!("'{}' emissive must be ≥ 0 (0 = no glow), got {v}", mid.name), attr.span));
                                continue;
                            }
                            Ok(other) => {
                                errs.push(serr(format!("'{}' emissive must be a number, got a {}", mid.name, other.kind()), attr.span));
                                continue;
                            }
                            Err(e) => { errs.push(serr(format!("'{}' emissive: {}", mid.name, e.message), attr.span)); continue; }
                        },
                        other => {
                            errs.push(serr(format!(
                                "a state sets a material's color or emissive, not '{other}' (a state never \
                                 changes shapes or placement — that is a pose)"), attr.span));
                            continue;
                        }
                    };
                    let same_kind = |b: &StateAttr| std::mem::discriminant(b) == std::mem::discriminant(&a);
                    if st.attrs.iter().any(|(m2, b)| *m2 == m && same_kind(b)) {
                        errs.push(serr(format!("sets '{}' {} twice", mid.name, attr.name), attr.span));
                        continue;
                    }
                    if matches!(a, StateAttr::Emissive(_)) { st.emissive_exprs.push((m, value.clone())); }
                    st.attrs.push((m, a));
                }
                StateLine::PartMaterial { part, material: mid } => {
                    let Some(m) = material(mid, errs) else { continue };
                    let targets: Vec<String> = if sc.part_names.contains_key(&part.name) {
                        vec![part.name.clone()]
                    } else if sc.instance_of.contains_key(&part.name) {
                        let pre = format!("{}.", part.name);
                        sc.part_order.iter().filter(|p| p.starts_with(&pre)).cloned().collect()
                    } else {
                        let tops: Vec<&str> = sc.part_order.iter()
                            .filter(|p| !p.contains('.'))
                            .map(|p| p.as_str())
                            .chain(sc.instance_of.keys().map(|s| s.as_str()))
                            .collect();
                        errs.push(serr(format!("no part '{}' in '{}' — parts: {}", part.name, sc.thing,
                            tops.join(", ")), part.span));
                        continue;
                    };
                    for t in targets {
                        if st.part_materials.iter().any(|(p, _)| *p == t) {
                            errs.push(serr(format!("sets '{t}' material twice"), part.span));
                            continue;
                        }
                        st.part_materials.push((t, m));
                    }
                }
            }
        }
        out.push(st);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::pipeline::compile_to_scene;

    const EYES: &str = r##"
material Skin { color = peach }
material Eyes { color = "#39ff6a", emissive = 1.3 }
material Hole { color = black }
thing Eye {
    part Ball { shape = sphere(radius=2), material = Eyes }
    part Lid  { shape = sphere(radius=1), material = Skin }
    relation { Lid.bottom on Ball.top }
    anchor socket = Ball.center
    resolve voxel_size = 1
}
thing Face {
    part Head { shape = sphere(radius=10), material = Skin }
    part EyeR { shape = sphere(radius=2), material = Eyes }
    part EyeL { shape = sphere(radius=2), material = Eyes }
    part Third { thing = Eye }
    relation {
        EyeR.center on Head.surface(yaw=20, pitch=10)
        EyeL.center on Head.surface(yaw=-20, pitch=10)
        Third.socket on Head.top
    }
    STATES
    resolve voxel_size = 1
}
print Face
"##;

    fn scene(states: &str) -> crate::scene::Scene {
        compile_to_scene(&EYES.replace("STATES", states)).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn errors(states: &str) -> String {
        match compile_to_scene(&EYES.replace("STATES", states)) {
            Ok(_) => panic!("expected an error for {states}"),
            Err(es) => es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        }
    }

    #[test]
    fn rest_carries_emissive() {
        let s = scene("");
        let eye = s.layers[0].parts.iter().find(|p| p.name == "EyeR").unwrap();
        assert_eq!(eye.emissive, Some(1.3));
        assert_eq!(eye.material.as_deref(), Some("Eyes"));
        assert!(s.layers[0].states.is_empty());
    }

    /// A material attribute applies to every part wearing that material —
    /// instance parts included — and only differing parts are listed.
    #[test]
    fn a_state_recolours_every_part_wearing_the_material() {
        let s = scene(r##"state Angry { Eyes.color = "#ff2414"  Eyes.emissive = 2 }"##);
        let st = &s.layers[0].states[0];
        assert_eq!(st.name, "Angry");
        assert_eq!(st.looks.keys().cloned().collect::<Vec<_>>(), vec!["EyeL", "EyeR", "Third.Ball"]);
        let l = &st.looks["EyeR"];
        assert_eq!((l.color.as_str(), l.emissive), ("#ff2414", Some(2.0)));
    }

    #[test]
    fn a_part_can_change_material() {
        let s = scene("state Dead { EyeR material = Hole  Third material = Hole }");
        let st = &s.layers[0].states[0];
        assert_eq!(st.looks.keys().cloned().collect::<Vec<_>>(), vec!["EyeR", "Third.Ball", "Third.Lid"],
            "an instance name means all its parts");
        let l = &st.looks["EyeR"];
        assert_eq!((l.material.as_deref(), l.color.as_str(), l.emissive), (Some("Hole"), "#000000", None));
    }

    #[test]
    fn state_errors_list_the_vocabulary() {
        let m = errors("state A { Eyez.color = red }");
        assert!(m.contains("state 'A': no material 'Eyez' — materials: Skin, Eyes, Hole"), "{m}");
        let m = errors("state A { Eyes.roughness = 1 }");
        assert!(m.contains("a state sets a material's color or emissive, not 'roughness'"), "{m}");
        let m = errors("state A { EyeX material = Hole }");
        assert!(m.contains("no part 'EyeX' in 'Face' — parts: Head, EyeR, EyeL, Third"), "{m}");
        let m = errors("state A { Eyes.emissive = -1 }");
        assert!(m.contains("'Eyes' emissive must be ≥ 0"), "{m}");
        let m = errors("state A { EyeR material = Hole  EyeR material = Skin }");
        assert!(m.contains("sets 'EyeR' material twice"), "{m}");
        let m = errors("state A { Eyes.color = red  Eyes.color = blue }");
        assert!(m.contains("sets 'Eyes' color twice"), "{m}");
        assert!(errors("state rest { Eyes.color = red }").contains("'rest' is the script's own look"));
        assert!(errors("state A { Eyes.color = red }  state A { Eyes.color = blue }").contains("'A' is already defined"));
    }

    #[test]
    fn material_properties_are_checked() {
        let bad = EYES.replace("STATES", "").replace("emissive = 1.3", "emisive = 1.3");
        let m = compile_to_scene(&bad).unwrap_err().iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
        assert!(m.contains("material 'Eyes': no property 'emisive' — a material has: color, voxel_atom, emissive"), "{m}");
        let bad = EYES.replace("STATES", "").replace("emissive = 1.3", "emissive = -2");
        let m = compile_to_scene(&bad).unwrap_err().iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
        assert!(m.contains("emissive must be ≥ 0"), "{m}");
    }
}
