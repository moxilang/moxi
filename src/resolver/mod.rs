use std::collections::HashMap;

use crate::ast::*;
use crate::error::{MoxiError, Span};

#[derive(Debug, Clone)]
pub struct ResolvedAtom {
    pub name:  String,
    pub color: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedMaterial {
    pub name:        String,
    pub color:       String,
    pub atom_index:  usize,
    pub extra_props: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPart {
    pub name:           String,
    pub shape:          Option<ShapeExpr>,
    pub material_index: Option<usize>,
    pub anchor:         Option<String>,
    pub attach_to:      Option<AttachSpec>,
}

#[derive(Debug, Clone)]
pub struct ResolvedEntity {
    pub name:        String,
    pub parts:       Vec<ResolvedPart>,
    pub relations:   Vec<RelationStmt>,
    pub constraints: Vec<ConstraintStmt>,
    pub resolve:     Option<ResolveOpts>,
}

#[derive(Debug, Clone)]
pub struct ResolvedScene {
    pub atoms:     Vec<ResolvedAtom>,
    pub materials: Vec<ResolvedMaterial>,
    pub entities:  Vec<ResolvedEntity>,
    pub prints:    Vec<PrintStmt>,
    pub refines:   Vec<RefineStmt>,
}

pub struct Resolver {
    errors:          Vec<MoxiError>,
    atom_index:      HashMap<String, usize>,
    material_index:  HashMap<String, usize>,
    entity_index:    HashMap<String, usize>,
    generator_index: HashMap<String, usize>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            errors:          Vec::new(),
            atom_index:      HashMap::new(),
            material_index:  HashMap::new(),
            entity_index:    HashMap::new(),
            generator_index: HashMap::new(),
        }
    }

    pub fn resolve(mut self, doc: Document) -> (ResolvedScene, Vec<MoxiError>) {
        // Pass 1 — register all names so forward references work
        for item in &doc.items {
            match item {
                TopLevel::AtomDecl(a)      => self.register_atom(a),
                TopLevel::MaterialDecl(m)  => self.register_material_name(m),
                TopLevel::EntityDecl(e)    => self.register_entity_name(e),
                TopLevel::GeneratorDecl(g) => self.register_generator_name(g),
                _ => {}
            }
        }

        // Pass 2 — resolve bodies
        let mut atoms     = Vec::new();
        let mut materials = Vec::new();
        let mut entities  = Vec::new();
        let mut prints    = Vec::new();
        let mut refines   = Vec::new();

        for item in doc.items {
            match item {
                TopLevel::AtomDecl(a) => {
                    atoms.push(self.resolve_atom(a));
                }
                TopLevel::MaterialDecl(m) => {
                    if let Some(mat) = self.resolve_material(m) {
                        materials.push(mat);
                    }
                }
                TopLevel::EntityDecl(e) => {
                    if let Some(ent) = self.resolve_entity(e) {
                        entities.push(ent);
                    }
                }
                TopLevel::PrintStmt(p) => {
                    self.check_entity_ref(&p.target);
                    prints.push(p);
                }
                TopLevel::RefineStmt(r) => {
                    if let Some(root) = r.path.first() {
                        self.check_entity_ref(root);
                    }
                    refines.push(r);
                }
                _ => {}
            }
        }

        (ResolvedScene { atoms, materials, entities, prints, refines }, self.errors)
    }

    // ── Pass 1: registration ───────────────────────────────────────────────

    fn register_atom(&mut self, a: &AtomDecl) {
        let idx = self.atom_index.len();
        if self.atom_index.insert(a.name.name.clone(), idx).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: a.name.name.clone(), span: a.name.span,
            });
        }
    }

    fn register_material_name(&mut self, m: &MaterialDecl) {
        let idx = self.material_index.len();
        if self.material_index.insert(m.name.name.clone(), idx).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: m.name.name.clone(), span: m.name.span,
            });
        }
    }

    fn register_entity_name(&mut self, e: &EntityDecl) {
        let idx = self.entity_index.len();
        if self.entity_index.insert(e.name.name.clone(), idx).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: e.name.name.clone(), span: e.name.span,
            });
        }
    }

    fn register_generator_name(&mut self, g: &GeneratorDecl) {
        let idx = self.generator_index.len();
        if self.generator_index.insert(g.name.name.clone(), idx).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: g.name.name.clone(), span: g.name.span,
            });
        }
    }

    // ── Pass 2: body resolution ────────────────────────────────────────────

    fn resolve_atom(&self, a: AtomDecl) -> ResolvedAtom {
        let color = self.extract_str_prop(&a.props, "color")
            .unwrap_or_else(|| "white".to_string());
        ResolvedAtom { name: a.name.name, color }
    }

    fn resolve_material(&mut self, m: MaterialDecl) -> Option<ResolvedMaterial> {
        let color = self.extract_str_prop(&m.props, "color")
            .unwrap_or_else(|| "white".to_string());

        let atom_name = self.extract_str_prop(&m.props, "voxel_atom");
        let atom_index = match atom_name {
            Some(ref name) => match self.atom_index.get(name).copied() {
                Some(idx) => idx,
                None => {
                    self.errors.push(MoxiError::UndefinedAtom {
                        name: name.clone(), span: m.name.span,
                    });
                    return None;
                }
            },
            None => 0,
        };

        let mut extra_props = HashMap::new();
        for prop in &m.props {
            if prop.key != "color" && prop.key != "voxel_atom" {
                extra_props.insert(prop.key.clone(), self.expr_to_str(&prop.value));
            }
        }

        Some(ResolvedMaterial { name: m.name.name, color, atom_index, extra_props })
    }

    fn resolve_entity(&mut self, e: EntityDecl) -> Option<ResolvedEntity> {
        let mut parts = Vec::new();
        let mut part_names: HashMap<String, Span> = HashMap::new();

        for part in e.parts {
            if part_names.contains_key(&part.name.name) {
                self.errors.push(MoxiError::DuplicateName {
                    name: part.name.name.clone(), span: part.name.span,
                });
                continue;
            }
            part_names.insert(part.name.name.clone(), part.name.span);

            let material_index = match &part.material {
                Some(mat) => match self.material_index.get(&mat.name).copied() {
                    Some(idx) => Some(idx),
                    None => {
                        self.errors.push(MoxiError::UndefinedMaterial {
                            name: mat.name.clone(), span: mat.span,
                        });
                        None
                    }
                },
                None => None,
            };

            parts.push(ResolvedPart {
                name:           part.name.name,
                shape:          part.shape,
                material_index,
                anchor:         part.anchor.map(|a| a.name),
                attach_to:      part.attach_to,
            });
        }

        // Validate relation names reference known parts
        for rel in &e.relations {
            self.check_part_ref(&rel.subject, &part_names);
            self.check_part_ref(&rel.object,  &part_names);
        }

        // Validate constraint names reference known parts
        for con in &e.constraints {
            match &con.expr {
                ConstraintExpr::Relation(r) => {
                    self.check_part_ref(&r.subject, &part_names);
                    self.check_part_ref(&r.object,  &part_names);
                }
                ConstraintExpr::Bound { name, .. } => {
                    self.check_part_ref(name, &part_names);
                }
            }
        }

        Some(ResolvedEntity {
            name:        e.name.name,
            parts,
            relations:   e.relations,
            constraints: e.constraints,
            resolve:     e.resolve,
        })
    }

    fn check_entity_ref(&mut self, ident: &Ident) {
        if !self.entity_index.contains_key(&ident.name) {
            self.errors.push(MoxiError::UndefinedName {
                name: ident.name.clone(), span: ident.span,
            });
        }
    }

    fn check_part_ref(&mut self, ident: &Ident, known: &HashMap<String, Span>) {
        if !known.contains_key(&ident.name) {
            self.errors.push(MoxiError::UndefinedName {
                name: ident.name.clone(), span: ident.span,
            });
        }
    }

    // ── Helpers ────────────────────────────────────────────────────────────

    fn extract_str_prop(&self, props: &[Prop], key: &str) -> Option<String> {
        props.iter().find(|p| p.key == key).map(|p| self.expr_to_str(&p.value))
    }

    fn expr_to_str(&self, expr: &Expr) -> String {
        match expr {
            Expr::Ident(i) => i.name.clone(),
            Expr::Str(s)   => s.clone(),
            Expr::Int(n)   => n.to_string(),
            Expr::Float(f) => f.to_string(),
            _              => "<complex>".to_string(),
        }
    }
}