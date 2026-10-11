//! Imports — one file's things in another (Issue J, DOC-20261011-imports).
//!
//! `from "lib/flame.md" import Flame, material Blood as Gore` is resolved on
//! the PARSED documents, before the one resolver: the loader parses the main
//! file and every file it imports (each once, cycles refused), works out
//! which declarations the imports need — the named ones and everything they
//! use from their own file, transitively — and copies those into the main
//! document under their final names. The resolver never learns that files
//! exist.
//!
//! Names: a declaration of the main file keeps its name; an imported one
//! the main file names takes that name (the `as` alias, or its own); every
//! other declaration a closure drags along is PRIVATE and gets a key no
//! script can write (`Ember@lib_flame_md`), so it never clashes and cannot
//! be named. Exports show a material's name before the `@`
//! (`shown_name`), so `Blood` stays `Blood` in a glb.
//!
//! Prints in imported files are ignored (a library stays runnable on its
//! own). Generators and legacy declarations are not importable. Imported
//! `fn`s keep their names; two different files defining the same `fn` name
//! is an error for now.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::lexer::Lexer;
use crate::parser::Parser;

// ── Where files come from ──────────────────────────────────────────────────

/// Reads an imported file by its normalized path (relative to the main
/// file's folder, `/`-separated, possibly starting with `../`).
pub trait Sources {
    fn read(&self, path: &str) -> Result<String, String>;
}

/// No files: a single-source compile. Any `from` line is an error.
pub struct NoSources;

impl Sources for NoSources {
    fn read(&self, _path: &str) -> Result<String, String> {
        Err("this compile has only one file".into())
    }
}

/// Files from a map (tests; the browser's `compile_moxi_files`).
pub struct MapSources(pub HashMap<String, String>);

impl Sources for MapSources {
    fn read(&self, path: &str) -> Result<String, String> {
        self.0.get(path).cloned().ok_or_else(|| "no such file".to_string())
    }
}

/// Files on disk, under the main file's folder (the CLI).
pub struct FsSources {
    pub root: std::path::PathBuf,
}

impl Sources for FsSources {
    fn read(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(self.root.join(path)).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "no such file".to_string(),
            _ => e.to_string(),
        })
    }
}

/// What a compile reads: one source text (no imports), or a main file and
/// where to find the files it imports. Every public compile entry point
/// takes `impl Into<Program>`, so a `&str` works as it always has.
#[derive(Clone, Copy)]
pub enum Program<'a> {
    Text(&'a str),
    Files { main: &'a str, sources: &'a dyn Sources },
}

impl<'a> From<&'a str> for Program<'a> {
    fn from(s: &'a str) -> Self { Program::Text(s) }
}

impl<'a> From<&'a String> for Program<'a> {
    fn from(s: &'a String) -> Self { Program::Text(s) }
}

/// The name a declaration shows outside the compiler: a private key's part
/// before `@`.
pub fn shown_name(key: &str) -> &str {
    key.split('@').next().unwrap_or(key)
}

// ── The result ─────────────────────────────────────────────────────────────

/// An error found while loading, with the stage and the file it is in
/// (`None` = the main file).
pub struct LoadError {
    pub stage: &'static str,
    pub file:  Option<String>,
    pub error: MoxiError,
}

pub struct Loaded {
    /// The main document with every needed imported declaration merged in.
    pub doc:   Document,
    /// File table: index = `Span::file`; 0 is the main file.
    pub files: Vec<String>,
    /// Imported things by final name → (file, name in that file).
    pub from:  HashMap<String, (String, String)>,
    /// Lex and parse errors: reported with the resolver's, as one list.
    pub errors: Vec<LoadError>,
}

// ── Loading ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind { Thing, Material }

struct Unit {
    path:      String,
    items:     Vec<TopLevel>,
    imports:   Vec<ImportDecl>,
    /// per import line: the unit it reads (None if it could not be loaded)
    targets:   Vec<Option<usize>>,
}

impl Unit {
    fn thing(&self, n: &str) -> Option<&EntityDecl> {
        self.items.iter().find_map(|i| match i { TopLevel::EntityDecl(e) if e.name.name == n => Some(&**e), _ => None })
    }
    fn material(&self, n: &str) -> Option<&MaterialDecl> {
        self.items.iter().find_map(|i| match i { TopLevel::MaterialDecl(m) if m.name.name == n => Some(m), _ => None })
    }
    fn has(&self, k: Kind, n: &str) -> bool {
        match k { Kind::Thing => self.thing(n).is_some(), Kind::Material => self.material(n).is_some() }
    }
    fn names(&self, k: Kind) -> Vec<&str> {
        self.items.iter().filter_map(|i| match (k, i) {
            (Kind::Thing, TopLevel::EntityDecl(e)) => Some(e.name.name.as_str()),
            (Kind::Material, TopLevel::MaterialDecl(m)) => Some(m.name.name.as_str()),
            _ => None,
        }).collect()
    }
    fn generator(&self, n: &str) -> bool {
        self.items.iter().any(|i| matches!(i, TopLevel::GeneratorDecl(g) if g.name.name == n))
    }
}

/// Parse one file as file number `id`: fence masking, lexing, parsing; every
/// token's span carries `id`.
fn parse_file(text: &str, id: u16, label: Option<&str>, errs: &mut Vec<LoadError>) -> Document {
    let (masked, fence_errors) = crate::lexer::fence::preprocess(text);
    let (mut tokens, lex_errors) = Lexer::new(&masked).tokenize();
    for t in &mut tokens { t.span.file = id as u32; }
    let file = label.map(str::to_string);
    for e in lex_errors.into_iter().chain(fence_errors) {
        errs.push(LoadError { stage: "lex", file: file.clone(), error: e });
    }
    let (doc, parse_errors) = Parser::new(tokens).parse();
    for e in parse_errors {
        errs.push(LoadError { stage: "parse", file: file.clone(), error: e });
    }
    doc
}

/// `dir/of/importer` + `rel` → a normalized path, or why it is refused.
fn join(importer: &str, rel: &str) -> Result<String, String> {
    if rel.starts_with('/') || rel.contains("://") || rel.contains('\\') || rel.get(1..2) == Some(":") {
        return Err(format!("\"{rel}\" is not a relative path — write it relative to the importing file, \
                            with `/` (\"lib/flame.md\", \"../shared/limbs.md\")"));
    }
    let mut parts: Vec<&str> = importer.split('/').collect();
    parts.pop(); // the importing file's own name
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|p| *p != "..") { parts.pop(); } else { parts.push(".."); }
            }
            s => parts.push(s),
        }
    }
    Ok(parts.join("/"))
}

fn sanitize(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// Load `program` into one document. Errors come back with their stage and
/// file; on any error the document is not usable.
pub fn load(program: Program) -> Result<Loaded, Vec<LoadError>> {
    let (main_path, main_text, sources): (&str, String, &dyn Sources) = match program {
        Program::Text(t) => ("", t.to_string(), &NoSources),
        Program::Files { main, sources } => {
            let text = sources.read(main).map_err(|e| vec![LoadError {
                stage: "import", file: None,
                error: MoxiError::ImportError { message: format!("cannot read \"{main}\": {e}"), span: Span::new(1, 1) },
            }])?;
            (main, text, sources)
        }
    };
    let mut errs: Vec<LoadError> = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    let main_doc = parse_file(&main_text, 0, None, &mut errs);
    let split = |doc: Document| {
        let (imports, items): (Vec<TopLevel>, Vec<TopLevel>) =
            doc.items.into_iter().partition(|i| matches!(i, TopLevel::ImportDecl(_)));
        let imports: Vec<ImportDecl> = imports.into_iter().map(|i| match i { TopLevel::ImportDecl(d) => d, _ => unreachable!() }).collect();
        (imports, items)
    };
    let (imports, items) = split(main_doc);
    if imports.is_empty() {
        // no imports: the document as parsed
        return Ok(Loaded { doc: Document { items }, files: vec![main_path.to_string()], from: HashMap::new(), errors: errs });
    }
    let name_of = |p: &str| if p.is_empty() { "this script".to_string() } else { p.to_string() };
    units.push(Unit { path: main_path.to_string(), items, targets: vec![None; imports.len()], imports });

    // Load every file reachable by imports, depth first, each once.
    let mut by_path: HashMap<String, usize> = HashMap::new();
    by_path.insert(main_path.to_string(), 0);
    #[allow(clippy::too_many_arguments)]
    fn visit(
        u: usize, stack: &mut Vec<usize>, units: &mut Vec<Unit>, by_path: &mut HashMap<String, usize>,
        sources: &dyn Sources, errs: &mut Vec<LoadError>, name_of: &dyn Fn(&str) -> String, done: &mut Vec<usize>,
    ) {
        stack.push(u);
        for k in 0..units[u].imports.len() {
            let (rel, span) = (units[u].imports[k].path.clone(), units[u].imports[k].span);
            let file = (u > 0).then(|| units[u].path.clone());
            let ierr = |message: String| LoadError { stage: "import", file: file.clone(), error: MoxiError::ImportError { message, span } };
            let path = match join(&units[u].path, &rel) {
                Ok(p) => p,
                Err(m) => { errs.push(ierr(m)); continue; }
            };
            if let Some(&t) = by_path.get(&path) {
                if let Some(pos) = stack.iter().position(|s| *s == t) {
                    let chain: Vec<String> = stack[pos..].iter().map(|s| name_of(&units[*s].path))
                        .chain(std::iter::once(name_of(&path))).collect();
                    errs.push(ierr(format!("imports go round: {}", chain.join(" → "))));
                    continue;
                }
                units[u].targets[k] = Some(t);
                continue;
            }
            let text = match sources.read(&path) {
                Ok(t) => t,
                Err(e) => {
                    let hint = if units[0].path.is_empty() {
                        " — a single-source compile has no files to import from; compile the script from its \
                         file (the CLI), or pass the imported files (`compile_moxi_files`)".to_string()
                    } else { String::new() };
                    errs.push(ierr(format!("cannot read \"{rel}\" (from {}): {e}{hint}", name_of(&units[u].path))));
                    continue;
                }
            };
            let id = units.len() as u16;
            let doc = parse_file(&text, id, Some(&path), errs);
            let (imports, items) = {
                let (imports, items): (Vec<TopLevel>, Vec<TopLevel>) =
                    doc.items.into_iter().partition(|i| matches!(i, TopLevel::ImportDecl(_)));
                (imports.into_iter().map(|i| match i { TopLevel::ImportDecl(d) => d, _ => unreachable!() }).collect::<Vec<_>>(), items)
            };
            units.push(Unit { path: path.clone(), items, targets: vec![None; imports.len()], imports });
            by_path.insert(path, id as usize);
            units[u].targets[k] = Some(id as usize);
            visit(id as usize, stack, units, by_path, sources, errs, name_of, done);
        }
        stack.pop();
        done.push(u);
    }
    let mut stack = Vec::new();
    // `done`: files in the order they finish — a file after everything it imports
    let mut done = Vec::new();
    visit(0, &mut stack, &mut units, &mut by_path, sources, &mut errs, &name_of, &mut done);

    // Every file's import lines → its scope: (kind, local name) → (unit, name there).
    // The main file's imports also fix the final names of what they name.
    let mut scope: Vec<HashMap<(Kind, String), (usize, String)>> = vec![HashMap::new(); units.len()];
    let mut public: HashMap<(usize, Kind, String), String> = HashMap::new();
    for u in 0..units.len() {
        for n in units[u].names(Kind::Thing) { scope[u].insert((Kind::Thing, n.to_string()), (u, n.to_string())); }
        for n in units[u].names(Kind::Material) { scope[u].insert((Kind::Material, n.to_string()), (u, n.to_string())); }
        let file = (u > 0).then(|| units[u].path.clone());
        for (k, imp) in units[u].imports.iter().enumerate() {
            let Some(t) = units[u].targets[k] else { continue };
            for item in &imp.items {
                let kind = if item.material { Kind::Material } else { Kind::Thing };
                let local = item.alias.as_ref().unwrap_or(&item.name);
                let ierr = |message: String, span: Span| LoadError {
                    stage: "import", file: file.clone(), error: MoxiError::ImportError { message, span } };
                let target = &units[t];
                if !target.has(kind, &item.name.name) {
                    let n = &item.name.name;
                    let msg = if kind == Kind::Thing && target.has(Kind::Material, n) {
                        format!("'{n}' in \"{}\" is a material — import it with `material {n}`", imp.path)
                    } else if kind == Kind::Material && target.has(Kind::Thing, n) {
                        format!("'{n}' in \"{}\" is a thing — import it without `material`", imp.path)
                    } else if target.generator(n) {
                        format!("'{n}' in \"{}\" is a generator — only things and materials can be imported", imp.path)
                    } else {
                        format!("\"{}\" has no {} '{n}' — things: {}; materials: {} (import a material with `material {}`)",
                            imp.path, if kind == Kind::Thing { "thing" } else { "material" },
                            list(&target.names(Kind::Thing)), list(&target.names(Kind::Material)),
                            target.names(Kind::Material).first().copied().unwrap_or("Name"))
                    };
                    errs.push(ierr(msg, item.name.span));
                    continue;
                }
                let key = (kind, local.name.clone());
                if let Some((owner, _)) = scope[u].get(&key) {
                    let what = if kind == Kind::Thing { "thing" } else { "material" };
                    let msg = if *owner == u {
                        format!("'{}' is already a {what} in {} — import it under another name: `{}{} as {}2`",
                            local.name, name_of(&units[u].path), if item.material { "material " } else { "" },
                            item.name.name, item.name.name)
                    } else {
                        format!("'{}' is imported twice — each name once per file", local.name)
                    };
                    errs.push(ierr(msg, local.span));
                    continue;
                }
                scope[u].insert(key, (t, item.name.name.clone()));
                if u == 0 {
                    let pk = (t, kind, item.name.name.clone());
                    if let Some(prev) = public.get(&pk) {
                        errs.push(ierr(format!("'{}' from \"{}\" is already imported as '{prev}' — import it once",
                            item.name.name, imp.path), local.span));
                        continue;
                    }
                    public.insert(pk, local.name.clone());
                }
            }
        }
    }
    if errs.iter().any(|e| e.stage == "import") { return Err(errs); }

    // Final names.
    let final_name = |u: usize, kind: Kind, n: &str| -> String {
        if u == 0 { return n.to_string(); }
        public.get(&(u, kind, n.to_string())).cloned().unwrap_or_else(|| format!("{n}@{}", sanitize(&units[u].path)))
    };
    let resolve = |u: usize, kind: Kind, n: &str| -> Option<(usize, String)> { scope[u].get(&(kind, n.to_string())).cloned() };

    // The closure: what the main file's imports need, transitively.
    let mut needed: Vec<(usize, Kind, String)> = Vec::new();
    let mut seen: HashSet<(usize, Kind, String)> = HashSet::new();
    for item in units[0].imports.iter().enumerate().flat_map(|(k, imp)| imp.items.iter().map(move |i| (k, i))) {
        let (k, it) = item;
        let Some(t) = units[0].targets[k] else { continue };
        let kind = if it.material { Kind::Material } else { Kind::Thing };
        if seen.insert((t, kind, it.name.name.clone())) { needed.push((t, kind, it.name.name.clone())); }
    }
    let mut i = 0;
    while i < needed.len() {
        let (u, kind, n) = needed[i].clone();
        i += 1;
        if kind == Kind::Material { continue; }
        let Some(e) = units[u].thing(&n) else { continue };
        let mut refs: Vec<(Kind, String)> = Vec::new();
        entity_refs(e, &mut refs);
        for (rk, rn) in refs {
            if let Some((t, tn)) = resolve(u, rk, &rn) {
                if t != 0 && seen.insert((t, rk, tn.clone())) { needed.push((t, rk, tn)); }
            }
        }
    }

    // Merge: the main file's items, then each needed declaration renamed
    // with its references rewritten, then the fns of every file that gave
    // something.
    // A thing must be declared before it is instanced: imported declarations
    // go first, a file after the files it imports, each in its file's order.
    let rank = |u: usize| done.iter().position(|d| *d == u).unwrap_or(usize::MAX);
    let pos = |u: usize, k: Kind, n: &str| units[u].items.iter().position(|i| match (k, i) {
        (Kind::Thing, TopLevel::EntityDecl(e)) => e.name.name == n,
        (Kind::Material, TopLevel::MaterialDecl(m)) => m.name.name == n,
        _ => false,
    }).unwrap_or(usize::MAX);
    needed.sort_by_key(|(u, k, n)| (rank(*u), pos(*u, *k, n)));
    let mut items: Vec<TopLevel> = Vec::new();
    let mut from: HashMap<String, (String, String)> = HashMap::new();
    let mut giving: Vec<usize> = Vec::new();
    for (u, kind, n) in &needed {
        if !giving.contains(u) { giving.push(*u); }
        let key = final_name(*u, *kind, n);
        let rename = |k: Kind, local: &str| -> Option<String> { resolve(*u, k, local).map(|(t, tn)| final_name(t, k, &tn)) };
        match kind {
            Kind::Thing => {
                let mut e = units[*u].thing(n).expect("needed").clone();
                e.name.name = key.clone();
                rename_entity(&mut e, &rename);
                from.insert(key, (units[*u].path.clone(), n.clone()));
                items.push(TopLevel::EntityDecl(Box::new(e)));
            }
            Kind::Material => {
                let mut m = units[*u].material(n).expect("needed").clone();
                m.name.name = key;
                items.push(TopLevel::MaterialDecl(m));
            }
        }
    }
    let mut fn_owner: HashMap<String, usize> = units[0].items.iter()
        .filter_map(|i| match i { TopLevel::FnDecl(f) => Some((f.name.name.clone(), 0usize)), _ => None }).collect();
    let mut fns: Vec<TopLevel> = Vec::new();
    for u in giving {
        for it in &units[u].items {
            let TopLevel::FnDecl(f) = it else { continue };
            if let Some(&o) = fn_owner.get(&f.name.name) {
                errs.push(LoadError { stage: "import", file: Some(units[u].path.clone()), error: MoxiError::ImportError {
                    message: format!("fn '{}' is defined in both {} and {} — rename one (imported fns keep their names)",
                        f.name.name, name_of(&units[o].path), units[u].path), span: f.span } });
                continue;
            }
            fn_owner.insert(f.name.name.clone(), u);
            fns.push(it.clone());
        }
    }
    let items: Vec<TopLevel> = fns.into_iter().chain(items).chain(units[0].items.iter().cloned()).collect();
    if errs.iter().any(|e| e.stage == "import") { return Err(errs); }
    Ok(Loaded { doc: Document { items }, files: units.iter().map(|u| u.path.clone()).collect(), from, errors: errs })
}

fn list(v: &[&str]) -> String {
    if v.is_empty() { "none".into() } else { v.join(", ") }
}

// ── Walking a thing's references ───────────────────────────────────────────

/// Every thing and material name a thing refers to: instanced things, part
/// materials, state lines, after swaps — in loops too.
fn entity_refs(e: &EntityDecl, out: &mut Vec<(Kind, String)>) {
    fn parts(ps: &[PartDecl], out: &mut Vec<(Kind, String)>) {
        for p in ps {
            if let Some(t) = &p.entity { out.push((Kind::Thing, t.name.clone())); }
            if let Some(m) = &p.material { out.push((Kind::Material, m.name.clone())); }
        }
    }
    fn breaks(bs: &[BreakDecl], out: &mut Vec<(Kind, String)>) {
        for b in bs {
            for l in &b.after {
                if let AfterLine::Swap { material, .. } = l { out.push((Kind::Material, material.name.clone())); }
            }
        }
    }
    fn loops(ls: &[ForBlock], out: &mut Vec<(Kind, String)>) {
        for l in ls { parts(&l.parts, out); breaks(&l.breakables, out); loops(&l.loops, out); }
    }
    parts(&e.parts, out);
    breaks(&e.breakables, out);
    loops(&e.loops, out);
    for st in &e.states {
        for l in &st.lines {
            match l {
                StateLine::Attr { material, .. } => out.push((Kind::Material, material.name.clone())),
                StateLine::PartMaterial { material, .. } => out.push((Kind::Material, material.name.clone())),
            }
        }
    }
}

/// Rewrite a thing's references to their final names (`rename` returns
/// `None` for a name not in scope — left for the resolver to report).
fn rename_entity(e: &mut EntityDecl, rename: &dyn Fn(Kind, &str) -> Option<String>) {
    let fix = |id: &mut Ident, k: Kind| { if let Some(n) = rename(k, &id.name) { id.name = n; } };
    fn parts(ps: &mut [PartDecl], fix: &dyn Fn(&mut Ident, Kind)) {
        for p in ps {
            if let Some(t) = &mut p.entity { fix(t, Kind::Thing); }
            if let Some(m) = &mut p.material { fix(m, Kind::Material); }
        }
    }
    fn breaks(bs: &mut [BreakDecl], fix: &dyn Fn(&mut Ident, Kind)) {
        for b in bs {
            for l in &mut b.after {
                if let AfterLine::Swap { material, .. } = l { fix(material, Kind::Material); }
            }
        }
    }
    fn loops(ls: &mut [ForBlock], fix: &dyn Fn(&mut Ident, Kind)) {
        for l in ls { parts(&mut l.parts, fix); breaks(&mut l.breakables, fix); loops(&mut l.loops, fix); }
    }
    parts(&mut e.parts, &fix);
    breaks(&mut e.breakables, &fix);
    loops(&mut e.loops, &fix);
    for st in &mut e.states {
        for l in &mut st.lines {
            match l {
                StateLine::Attr { material, .. } => fix(material, Kind::Material),
                StateLine::PartMaterial { material, .. } => fix(material, Kind::Material),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{compile_to_scene, CompileError};

    fn files(list: &[(&str, &str)]) -> MapSources {
        MapSources(list.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect())
    }
    fn scene(src: &MapSources) -> crate::scene::Scene {
        compile_to_scene(Program::Files { main: "main.md", sources: src }).unwrap_or_else(|e| panic!("{e:?}"))
    }
    fn errors(src: &MapSources) -> Vec<CompileError> {
        compile_to_scene(Program::Files { main: "main.md", sources: src }).expect_err("an error")
    }
    fn text(es: &[CompileError]) -> String { es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n") }

    const LIB: &str = r##"
material Bone  { color = "#ddd8c4" }
material Flesh { color = "#7a1414" }
material Gold  { color = "#d4a017", metalness = 1 }
thing Nub(r=2) { part Ball { shape = sphere(radius=r), material = Flesh } }
thing Arm(len=20) {
    part Upper { shape = capsule(height=len, radius=3), material = Bone }
    part Hand  { shape = sphere(radius=3), material = Flesh }
    part Tip   { thing = Nub(r=1) }
    part Ring  { shape = torus(major_radius=3, minor_radius=0.6), material = Gold }
    relation {
        Hand.bottom on Upper.top
        Tip.Ball.bottom on Hand.top
        Ring.center on Upper.top
    }
    pose Up { Hand lean=(30, 0) }
    state Raw { Bone.color = "#aa3333" }
    joint Hand { lean=(-45..45, 0..0) }
    breakable Hand { hits=2 }
}
print Arm
"##;

    #[test]
    fn an_imported_thing_brings_its_poses_states_joints_and_breakables_prefixed() {
        let src = files(&[("lib/arm.md", LIB), ("main.md", r##"
from "lib/arm.md" import Arm
material Bone { color = "#000000" }
thing Body {
    part Torso { shape = box(width=30, height=40, depth=20), material = Bone }
    part L { thing = Arm(len=30) }
    part R { thing = Arm }
    relation { L.Upper.bottom on Torso.east  R.Upper.bottom on Torso.west }
}
print Body
"##)]);
        let s = scene(&src);
        let l = &s.layers[0];
        assert_eq!(l.thing, "Body");
        let part = |n: &str| l.parts.iter().find(|p| p.name == n).unwrap_or_else(|| panic!("{n}"));
        assert_eq!(part("Torso").color, "#000000", "this file's Bone");
        assert_eq!(part("L.Upper").color, "#ddd8c4", "the library's Bone, privately");
        assert_eq!(part("L.Upper").material.as_deref(), Some("Bone"), "shown as written");
        assert!(part("L.Hand").breakable.is_some() && part("R.Hand").breakable.is_some());
        let states: Vec<&str> = l.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(states, vec!["L.Raw", "R.Raw"]);
        assert!(crate::pipeline::compile_to_scene_posed(Program::Files { main: "main.md", sources: &src }, Some("L.Up")).is_ok());
        assert_eq!(part("L.Upper").from, Some(crate::scene::FromOut { file: "lib/arm.md".into(), thing: "Arm".into() }));
        assert_eq!(part("L.Tip.Ball").from.as_ref().map(|f| f.thing.as_str()), Some("Nub"));
        // the library's own print is ignored; the arm is only what Body instances
        assert_eq!(s.layers.len(), 1);
    }

    #[test]
    fn as_renames_and_an_imported_material_is_yours() {
        let src = files(&[("lib/arm.md", LIB), ("main.md", r##"
from "lib/arm.md" import Arm as Limb, material Gold, material Flesh as Meat
thing Body {
    part Torso { shape = box(width=30, height=40, depth=20), material = Gold }
    part Pad   { shape = sphere(radius=4), material = Meat }
    part L { thing = Limb }
    relation { L.Upper.bottom on Torso.east  Pad.bottom on Torso.top }
    state Dull { Gold.metalness = 0 }
}
print Body
"##)]);
        let s = scene(&src);
        let part = |n: &str| s.layers[0].parts.iter().find(|p| p.name == n).unwrap();
        assert_eq!(part("Torso").metalness, Some(1.0));
        assert_eq!(part("Pad").material.as_deref(), Some("Meat"));
        // Gold is ONE material: the state dulls the arm's ring too
        let dull = &s.layers[0].states.iter().find(|s| s.name == "Dull").unwrap().looks;
        assert!(dull.contains_key("Torso") && dull.contains_key("L.Ring"));
    }

    #[test]
    fn name_errors_list_what_the_file_has() {
        let main = |line: &str| files(&[("lib/arm.md", LIB), ("main.md", &format!("{line}\nthing T {{ part A {{ shape = sphere(radius=1), material = M }} }}\nmaterial M {{ color = red }}\nprint T\n"))]);
        let m = text(&errors(&main(r#"from "lib/arm.md" import Amr"#)));
        assert!(m.contains("import: \"lib/arm.md\" has no thing 'Amr' — things: Nub, Arm; materials: Bone, Flesh, Gold"), "{m}");
        let m = text(&errors(&main(r#"from "lib/arm.md" import Bone"#)));
        assert!(m.contains("'Bone' in \"lib/arm.md\" is a material — import it with `material Bone`"), "{m}");
        let m = text(&errors(&main(r#"from "lib/arm.md" import material Arm"#)));
        assert!(m.contains("'Arm' in \"lib/arm.md\" is a thing — import it without `material`"), "{m}");
        let m = text(&errors(&main(r#"from "lib/arm.md" import material M"#)));
        assert!(m.contains("has no material 'M'"), "{m}");
        let m = text(&errors(&main(r#"from "lib/arm.md" import Arm, Arm"#)));
        assert!(m.contains("'Arm' is imported twice"), "{m}");
        let m = text(&errors(&main(r#"from "lib/arm.md" import Arm, Arm as Arm2"#)));
        assert!(m.contains("'Arm' from \"lib/arm.md\" is already imported as 'Arm' — import it once"), "{m}");
        let m = text(&errors(&main(r#"from "lib/arn.md" import Arm"#)));
        assert!(m.contains("cannot read \"lib/arn.md\" (from main.md): no such file"), "{m}");
        let m = text(&errors(&main(r#"from "/abs/arm.md" import Arm"#)));
        assert!(m.contains("\"/abs/arm.md\" is not a relative path"), "{m}");
    }

    #[test]
    fn a_local_clash_suggests_as() {
        let src = files(&[("lib/arm.md", LIB), ("main.md", r##"
from "lib/arm.md" import Arm, material Bone
material Bone { color = red }
thing Arm { part A { shape = sphere(radius=1), material = Bone } }
print Arm
"##)]);
        let m = text(&errors(&src));
        assert!(m.contains("'Arm' is already a thing in main.md — import it under another name: `Arm as Arm2`"), "{m}");
        assert!(m.contains("'Bone' is already a material in main.md — import it under another name: `material Bone as Bone2`"), "{m}");
    }

    #[test]
    fn a_cycle_is_named_and_a_shared_file_is_read_once() {
        let a = "from \"b.md\" import B\nmaterial M { color = red }\nthing A { part X { thing = B } }\n";
        let b = "from \"a.md\" import A\nmaterial M { color = red }\nthing B { part Y { shape = sphere(radius=1), material = M } }\n";
        let src = files(&[("a.md", a), ("b.md", b), ("main.md", "from \"a.md\" import A\nprint A\n")]);
        let m = text(&errors(&src));
        assert!(m.contains("imports go round: a.md → b.md → a.md"), "{m}");
        // main imports Nub directly and through arm: one Nub, used both ways
        let src = files(&[("lib/arm.md", LIB), ("lib/hand.md", "from \"arm.md\" import Arm, Nub\nthing Hand2 { part A { thing = Arm }  part N { thing = Nub }  relation { N.Ball.bottom on A.Upper.bottom } }\n"),
            ("main.md", "from \"lib/hand.md\" import Hand2\nfrom \"lib/arm.md\" import Nub\nthing T { part H { thing = Hand2 }  part K { thing = Nub }  relation { K.Ball.bottom on H.A.Upper.bottom } }\nprint T\n")]);
        let s = scene(&src);
        assert!(s.layers[0].parts.iter().any(|p| p.name == "K.Ball") && s.layers[0].parts.iter().any(|p| p.name == "H.N.Ball"));
    }

    #[test]
    fn two_levels_deep_and_paths_climb_with_dotdot() {
        let src = files(&[("lib/arm.md", LIB),
            ("parts/hand.md", "from \"../lib/arm.md\" import Arm\nthing Hand2 { part A { thing = Arm(len=8) } }\n"),
            ("main.md", "from \"parts/hand.md\" import Hand2\nthing T { part H { thing = Hand2 } }\nprint T\n")]);
        let s = scene(&src);
        assert!(s.layers[0].parts.iter().any(|p| p.name == "H.A.Tip.Ball"));
        assert_eq!(join("a/b/c.md", "../d.md").unwrap(), "a/d.md");
        assert_eq!(join("c.md", "../x/d.md").unwrap(), "../x/d.md");
        assert_eq!(join("a/c.md", "./lib/./d.md").unwrap(), "a/lib/d.md");
    }

    #[test]
    fn an_error_inside_an_imported_file_names_that_file() {
        let bad = "material M { color = red }\nthing Arm {\n  part A { shape = sphere(radius=1), material = M }\n  part B { shape = sphere(radius=1), material = Nope }\n  relation { B.bottom on A.top }\n}\n";
        let src = files(&[("lib/bad.md", bad), ("main.md", "from \"lib/bad.md\" import Arm\nthing T { part X { thing = Arm } }\nprint T\n")]);
        let es = errors(&src);
        let e = es.iter().find(|e| e.message.contains("Nope")).unwrap_or_else(|| panic!("{es:?}"));
        assert_eq!((e.file.as_deref(), e.line), (Some("lib/bad.md"), Some(4)));
        let parse = files(&[("lib/bad.md", "thing Arm { part A { shape = = } }"), ("main.md", "from \"lib/bad.md\" import Arm\n")]);
        let es = errors(&parse);
        assert!(es.iter().any(|e| e.stage == "parse" && e.file.as_deref() == Some("lib/bad.md")), "{es:?}");
        // errors in the main file carry no file: the wire JSON is as before
        let main = compile_to_scene("thing T { part A { shape = sphere(radius=1), material = Nope } }\nprint T").unwrap_err();
        assert!(main.iter().all(|e| e.file.is_none()));
        assert!(!serde_json::to_string(&main).unwrap().contains("\"file\""));
    }

    #[test]
    fn single_source_compiles_say_how_to_import_and_fns_must_not_clash() {
        let m = text(&compile_to_scene("from \"lib/arm.md\" import Arm\n").unwrap_err());
        assert!(m.contains("a single-source compile has no files to import from"), "{m}");
        let lib = "fn w(x) = x * 2\nmaterial M { color = red }\nthing A { part P { shape = sphere(radius=w(1)), material = M } }\n";
        let src = files(&[("lib.md", lib), ("main.md", "from \"lib.md\" import A\nfn w(x) = x * 3\nthing T { part X { thing = A } }\nprint T\n")]);
        let m = text(&errors(&src));
        assert!(m.contains("fn 'w' is defined in both main.md and lib.md — rename one"), "{m}");
        let src = files(&[("lib.md", lib), ("main.md", "from \"lib.md\" import A\nthing T { part X { thing = A } }\nprint T\n")]);
        assert!(scene(&src).layers[0].parts.iter().any(|p| p.name == "X.P"), "the library's fn comes along");
        let gen = "generator G { scatter A count=2 }\nmaterial M { color = red }\nthing A { part P { shape = sphere(radius=1), material = M } }\n";
        let src = files(&[("lib.md", gen), ("main.md", "from \"lib.md\" import G\n")]);
        let m = text(&errors(&src));
        assert!(m.contains("'G' in \"lib.md\" is a generator — only things and materials can be imported") || m.contains("has no thing 'G'"), "{m}");
    }

    #[test]
    fn import_syntax_errors_show_the_form() {
        let m = text(&compile_to_scene("import Arm\n").unwrap_err());
        assert!(m.contains("the file first: `from \"lib/flame.md\" import Flame`"), "{m}");
        let m = text(&compile_to_scene("from \"x.md\" Arm\n").unwrap_err());
        assert!(m.contains("`import` after the file"), "{m}");
    }
}
