//! States — living models, step 4 (DOC-20261004-living-models-design §3.3).
//!
//! `state Angry { Eyes.color = "#ff2414"  EyeR material = Hole }` overrides
//! MATERIALS only: a material's `color` / `emissive` / `opacity` / `roughness` /
//! `metalness`, or which material a part
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
    /// `state X in Family`: at most one state of a family is on at a time.
    pub family:         Option<String>,
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
    /// Opacity values as written, likewise.
    pub opacity_exprs:  Vec<(usize, Expr)>,
    /// Roughness and metalness values as written, likewise ("roughness" or
    /// "metalness", material index, expression).
    pub surface_exprs:  Vec<(&'static str, usize, Expr)>,
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
        let mut opacity_exprs = Vec::new();
        for (m, e) in &self.opacity_exprs {
            let e = subst_expr(e, env);
            match value::eval(&e, env) {
                Ok(Value::Num(v)) if (0.0..=1.0).contains(&v) => {
                    for (am, a) in attrs.iter_mut() {
                        if am == m { if let StateAttr::Opacity(x) = a { *x = v; } }
                    }
                }
                _ => errs.push(MoxiError::StateError { state: name.clone(), message:
                    "an opacity value is not a number between 0 and 1 under this instance's arguments".into(), span: self.span }),
            }
            opacity_exprs.push((*m, e));
        }
        let mut surface_exprs = Vec::new();
        for (key, m, e) in &self.surface_exprs {
            let e = subst_expr(e, env);
            match value::eval(&e, env) {
                Ok(Value::Num(v)) if (0.0..=1.0).contains(&v) => {
                    for (am, a) in attrs.iter_mut() {
                        if am != m { continue; }
                        match a {
                            StateAttr::Roughness(x) if *key == "roughness" => *x = v,
                            StateAttr::Metalness(x) if *key == "metalness" => *x = v,
                            _ => {}
                        }
                    }
                }
                _ => errs.push(MoxiError::StateError { state: name.clone(), message:
                    format!("a {key} value is not a number between 0 and 1 under this instance's arguments"), span: self.span }),
            }
            surface_exprs.push((*key, *m, e));
        }
        ResolvedState {
            name,
            family: self.family.as_ref().map(|f| format!("{prefix}.{f}")),
            part_materials: self.part_materials.iter().map(|(p, m)| (format!("{prefix}.{p}"), *m)).collect(),
            attrs,
            scope: Some(match &self.scope { Some(s) => format!("{prefix}.{s}"), None => prefix.to_string() }),
            emissive_exprs,
            opacity_exprs,
            surface_exprs,
            span: self.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateAttr {
    /// Unresolved colour (a palette name or hex), like a material's.
    Color(String),
    Emissive(f64),
    /// 0 (invisible) ..= 1 (solid).
    Opacity(f64),
    /// 0 (mirror-smooth) ..= 1 (chalk).
    Roughness(f64),
    /// 0 (stone, bone) ..= 1 (bare metal).
    Metalness(f64),
}

/// What the two ends of `roughness` / `metalness` mean, for range errors.
pub(crate) fn surface_ends(key: &str) -> &'static str {
    if key == "roughness" { "0 mirror-smooth, 1 chalk-dry" } else { "0 stone or bone, 1 bare metal" }
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
        if let Some(f) = &d.family {
            if f.name == "rest" || decls.iter().any(|o| o.name.name == f.name) {
                errs.push(serr(format!("family '{}' has the name of a state — a family names the states that \
                    exclude each other (`state Calm in Eyes`, `state Angry in Eyes`); give it its own name", f.name), f.span));
            }
        }
        let mut st = ResolvedState { name: name.clone(), family: d.family.as_ref().map(|f| f.name.clone()), part_materials: Vec::new(), attrs: Vec::new(),
            scope: None, emissive_exprs: Vec::new(), opacity_exprs: Vec::new(), surface_exprs: Vec::new(), span: d.span };
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
                        "opacity" => match value::eval(value, env) {
                            Ok(Value::Num(v)) if (0.0..=1.0).contains(&v) => StateAttr::Opacity(v),
                            Ok(Value::Num(v)) => {
                                errs.push(serr(format!("'{}' opacity must be between 0 (invisible) and 1 (solid), got {v}", mid.name), attr.span));
                                continue;
                            }
                            Ok(other) => {
                                errs.push(serr(format!("'{}' opacity must be a number, got a {}", mid.name, other.kind()), attr.span));
                                continue;
                            }
                            Err(e) => { errs.push(serr(format!("'{}' opacity: {}", mid.name, e.message), attr.span)); continue; }
                        },
                        key @ ("roughness" | "metalness") => match value::eval(value, env) {
                            Ok(Value::Num(v)) if (0.0..=1.0).contains(&v) =>
                                if key == "roughness" { StateAttr::Roughness(v) } else { StateAttr::Metalness(v) },
                            Ok(Value::Num(v)) => {
                                errs.push(serr(format!("'{}' {key} must be between 0 and 1 ({}), got {v}", mid.name, surface_ends(key)), attr.span));
                                continue;
                            }
                            Ok(other) => {
                                errs.push(serr(format!("'{}' {key} must be a number, got a {}", mid.name, other.kind()), attr.span));
                                continue;
                            }
                            Err(e) => { errs.push(serr(format!("'{}' {key}: {}", mid.name, e.message), attr.span)); continue; }
                        },
                        "blend" => {
                            errs.push(serr(format!(
                                "a state cannot change '{}' blend — blend is what the material is, not how it looks \
                                 right now; give the part another material instead (`Part material = Other`)", mid.name), attr.span));
                            continue;
                        }
                        other => {
                            errs.push(serr(format!(
                                "a state sets a material's color, emissive, opacity, roughness or metalness, not '{other}' (a state never \
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
                    if matches!(a, StateAttr::Opacity(_)) { st.opacity_exprs.push((m, value.clone())); }
                    match a {
                        StateAttr::Roughness(_) => st.surface_exprs.push(("roughness", m, value.clone())),
                        StateAttr::Metalness(_) => st.surface_exprs.push(("metalness", m, value.clone())),
                        _ => {}
                    }
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
    fn families_resolve_and_an_inherited_family_is_the_instances() {
        let s = scene("state Calm in Look { Eyes.color = blue }  state Mad in Look { Eyes.color = red }  state Out { EyeR material = Hole }");
        let fam: Vec<(&str, Option<&str>)> = s.layers[0].states.iter().map(|st| (st.name.as_str(), st.family.as_deref())).collect();
        assert_eq!(fam, vec![("Calm", Some("Look")), ("Mad", Some("Look")), ("Out", None)]);
        let m = errors("state Calm in Out { Eyes.color = blue }  state Out { EyeR material = Hole }");
        assert!(m.contains("state 'Calm': family 'Out' has the name of a state"), "{m}");
        // a state inherited from an instance brings its family, prefixed
        let src = EYES.replace("STATES", "").replace("anchor socket = Ball.center",
            "anchor socket = Ball.center\n    state Wide in Lid { Skin.color = white }");
        let s = compile_to_scene(&src).unwrap();
        let wide = s.layers.iter().flat_map(|l| &l.states).find(|st| st.name == "Third.Wide").expect("inherited");
        assert_eq!(wide.family.as_deref(), Some("Third.Lid"));
    }

    #[test]
    fn inherited_states_combine_before_the_things_own() {
        let src = EYES.replace("STATES", "state Own { Skin.color = red }").replace("anchor socket = Ball.center",
            "anchor socket = Ball.center\n    state Wide { Skin.color = white }");
        let s = compile_to_scene(&src).unwrap();
        let face = s.layers.iter().find(|l| l.thing == "Face").unwrap();
        let declared: Vec<&str> = face.states.iter().map(|st| st.name.as_str()).collect();
        assert_eq!(declared, vec!["Own", "Third.Wide"], "the variants keep their order");
        let order: Vec<&str> = crate::scene::state_order(&face.states).iter().map(|st| st.name.as_str()).collect();
        assert_eq!(order, vec!["Third.Wide", "Own"], "the thing's own word is the last");
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
        let m = errors("state A { Eyes.shine = 1 }");
        assert!(m.contains("a state sets a material's color, emissive, opacity, roughness or metalness, not 'shine'"), "{m}");
        let m = errors("state A { Eyes.roughness = 1.5 }");
        assert!(m.contains("'Eyes' roughness must be between 0 and 1 (0 mirror-smooth, 1 chalk-dry), got 1.5"), "{m}");
        let m = errors("state A { Eyes.metalness = -0.1 }");
        assert!(m.contains("'Eyes' metalness must be between 0 and 1 (0 stone or bone, 1 bare metal), got -0.1"), "{m}");
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

    // ── See-through materials (ISSUE-20261004-see-through) ──

    const FIRE: &str = r##"
material Wick  { color = "#222222" }
material Flame { color = "#ff7a22", emissive = 1.6, opacity = 0.55, blend = additive }
material Smoke { color = "#555555", opacity = 0.3 }
material Glass { color = "#ccddee", opacity = 0.4, blend = normal }
thing Candle {
    part Stick { shape = cylinder(height=10, radius=2), material = Wick }
    part Fire  { shape = cone(height=6, radius=1.5), material = Flame }
    part Puff  { shape = sphere(radius=2), material = Smoke }
    part Jar   { shape = sphere(radius=1), material = Glass }
    relation { Fire.bottom on Stick.top  Puff.bottom on Fire.top  Jar.top on Stick.bottom }
    STATES
    resolve voxel_size = 1
}
print Candle
"##;

    fn fire(states: &str) -> Result<crate::scene::Scene, String> {
        compile_to_scene(&FIRE.replace("STATES", states))
            .map_err(|es| es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"))
    }

    #[test]
    fn opacity_and_blend_reach_the_scene() {
        let s = fire("").unwrap();
        let part = |n: &str| s.layers[0].parts.iter().find(|p| p.name == n).unwrap().clone();
        let f = part("Fire");
        assert_eq!((f.opacity, f.blend.as_deref(), f.emissive), (Some(0.55), Some("additive"), Some(1.6)));
        let p = part("Puff");
        assert_eq!((p.opacity, p.blend.as_deref()), (Some(0.3), None), "see-through, normal blend");
        assert_eq!((part("Jar").blend.as_deref(), part("Stick").opacity), (None, None),
            "`blend = normal` and an opaque material store nothing");
    }

    #[test]
    fn a_state_fades_a_material() {
        let s = fire("state Out { Flame.opacity = 0.05  Smoke.opacity = 1 }").unwrap();
        let st = &s.layers[0].states[0];
        assert_eq!(st.looks["Fire"].opacity, Some(0.05));
        assert_eq!(st.looks["Fire"].blend.as_deref(), Some("additive"), "the look keeps the material's blend");
        assert_eq!(st.looks["Puff"].opacity, None, "opacity 1 = solid again");
    }

    #[test]
    fn see_through_errors_list_the_vocabulary() {
        let bad = |from: &str, to: &str| {
            compile_to_scene(&FIRE.replace("STATES", "").replace(from, to)).unwrap_err()
                .iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")
        };
        let m = bad("opacity = 0.3", "opacity = 1.5");
        assert!(m.contains("material 'Smoke': opacity must be between 0 (invisible) and 1 (solid), got 1.5"), "{m}");
        let m = bad("blend = additive", "blend = glow");
        assert!(m.contains("material 'Flame': no blend 'glow' — blend is one of: normal, additive"), "{m}");
        let m = fire("state A { Flame.opacity = -0.2 }").unwrap_err();
        assert!(m.contains("'Flame' opacity must be between 0 (invisible) and 1 (solid), got -0.2"), "{m}");
        let m = fire("state A { Flame.blend = normal }").unwrap_err();
        assert!(m.contains("a state cannot change 'Flame' blend"), "{m}");
    }

    #[test]
    fn material_properties_are_checked() {
        let bad = EYES.replace("STATES", "").replace("emissive = 1.3", "emisive = 1.3");
        let m = compile_to_scene(&bad).unwrap_err().iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
        assert!(m.contains("material 'Eyes': no property 'emisive' — a material has: color, voxel_atom, emissive, opacity, blend, roughness, metalness"), "{m}");
        let bad = EYES.replace("STATES", "").replace("emissive = 1.3", "emissive = -2");
        let m = compile_to_scene(&bad).unwrap_err().iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
        assert!(m.contains("emissive must be ≥ 0"), "{m}");
    }
}
