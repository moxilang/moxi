diff --git a/NOTES.md b/NOTES.md
index e14699b..c73f1a7 100644
--- a/NOTES.md
+++ b/NOTES.md
@@ -128,6 +128,25 @@ WALLRUN.md, RELIC.md, STATES.md. Reworded errors (reviewable): the
 material property list, the state attribute lists, '{' after a state
 name.
 
+Issue G, `after { … }` and `reveal` (2026-10-11): a breakable's after block
+holds `Part material = M` swaps and `reveal Part`; `breakable.rs`
+resolves them (names as in a state: a part, an instance part, a whole
+instance), mirrors map them through `symmetric_across` images, and
+`check_after` keeps every line at the cut (the lump, its parent, the
+parent's other children) and each revealed part to one lump. IR:
+`BreakOut.after` {part: Look}, `BreakOut.reveal`, `Part.hidden`. Hidden
+parts are left out of the voxel loop (`compile_source_posed`) and of
+every drawing of rest through `Scene::at_rest()` (mesh, glsl/web, the
+bevy viewer — one call per entry point instead of a filter per loop);
+glTF keeps them (`moxi.hidden`, `moxi.breakable.after` as material
+indices). The report leaves still-hidden parts out (a `hidden:` line);
+`--break` shows what it reveals, lists its swaps as `look:` lines and does
+not count a revealed part against its own lump as torn. Demo STUMP.md.
+Every existing output byte-identical (none declares `after`).
+Found while testing: `print Demon` prints nothing of Demon when another
+thing in the file instances it — the instancing thing is printed instead
+(belongs with L3's `print A, B`).
+
 Remaining to a shippable v1: list values (D.2) → `for` blocks (E) →
 lathe/sweep → M4 (moxi eval, the model-facing half of the bench).
 
diff --git a/SKILL.md b/SKILL.md
index ce16841..80dbc0f 100644
--- a/SKILL.md
+++ b/SKILL.md
@@ -274,6 +274,15 @@ cannot break. To make several sibling parts one piece, make them a `thing`
 and break the instance. A mirror image breaks like its source. Instances
 inherit their thing's breakables.
 
+`after { … }` says what is left once the piece has broken:
+`breakable Head { hits=5  after { Neck material = Flesh  reveal Stump } }`.
+`Part material = M` swaps a part's look; `reveal Part` names a part that is
+not there until the break (a stump, an exposed ring). An after line touches
+only the cut: the piece itself, the part it hung from, or another part
+hanging from that one. A revealed part is placed like any part, revealed by
+one piece only; a mirror image's after names the images of those parts, so
+mirror a stump along with its arm.
+
 ## Checks
 
 `constraint A above B` (also `below`, `inside`, `surrounds`) fails the
@@ -295,6 +304,8 @@ but overlaps the head). Some overlap is intended; read the line and decide.
 Names are the printed ones (`D.Head` inside an instance). A part that is not
 declared `breakable` is an error that lists the ones that are. Nothing moves:
 the lumps stay where they were solved, `--json` adds `broken` and `torn`.
+Parts still waiting for their break are left out and listed on a `hidden:`
+line; `--break` shows the parts it reveals and its after swaps as `look:` lines.
 
 ## Poses
 
diff --git a/docs/skill_preamble.md b/docs/skill_preamble.md
index 8fd8ff3..6fd337e 100644
--- a/docs/skill_preamble.md
+++ b/docs/skill_preamble.md
@@ -272,6 +272,15 @@ cannot break. To make several sibling parts one piece, make them a `thing`
 and break the instance. A mirror image breaks like its source. Instances
 inherit their thing's breakables.
 
+`after { … }` says what is left once the piece has broken:
+`breakable Head { hits=5  after { Neck material = Flesh  reveal Stump } }`.
+`Part material = M` swaps a part's look; `reveal Part` names a part that is
+not there until the break (a stump, an exposed ring). An after line touches
+only the cut: the piece itself, the part it hung from, or another part
+hanging from that one. A revealed part is placed like any part, revealed by
+one piece only; a mirror image's after names the images of those parts, so
+mirror a stump along with its arm.
+
 ## Checks
 
 `constraint A above B` (also `below`, `inside`, `surrounds`) fails the
@@ -293,6 +302,8 @@ but overlaps the head). Some overlap is intended; read the line and decide.
 Names are the printed ones (`D.Head` inside an instance). A part that is not
 declared `breakable` is an error that lists the ones that are. Nothing moves:
 the lumps stay where they were solved, `--json` adds `broken` and `torn`.
+Parts still waiting for their break are left out and listed on a `hidden:`
+line; `--break` shows the parts it reveals and its after swaps as `look:` lines.
 
 ## Poses
 
diff --git a/src/ast/mod.rs b/src/ast/mod.rs
index bcd2abd..51bbc3d 100644
--- a/src/ast/mod.rs
+++ b/src/ast/mod.rs
@@ -160,9 +160,21 @@ pub struct BreakDecl {
     pub event: Option<Ident>,
     /// `out` (default) or a direction word: the kick a consumer gives the lump.
     pub push:  Option<Ident>,
+    /// `after { Neck material = Flesh  reveal NeckStump }`: the thing once
+    /// this lump has broken (Issue G).
+    pub after: Vec<AfterLine>,
     pub span:  Span,
 }
 
+/// One line of a breakable's `after { … }` block.
+#[derive(Debug, Clone)]
+pub enum AfterLine {
+    /// `Neck material = Flesh` — the part wears another material once broken.
+    Swap { part: Ident, material: Ident },
+    /// `reveal NeckStump` — a part hidden until this lump breaks.
+    Reveal { part: Ident },
+}
+
 /// One number of a mate: `shift` and `lean` are two each.
 #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
 pub enum Qual { Twist, Pitch, Gap, ShiftA, ShiftB, LeanA, LeanB }
diff --git a/src/bevy_viewer.rs b/src/bevy_viewer.rs
index 4ef8b58..b8ce740 100644
--- a/src/bevy_viewer.rs
+++ b/src/bevy_viewer.rs
@@ -88,6 +88,7 @@ mod inner {
     // ── Entry point ────────────────────────────────────────────────────
 
     pub fn view_scene_bevy(scene: Scene) {
+        let scene = scene.at_rest().into_owned();   // revealed parts are not there at rest
         App::new()
             .insert_resource(SceneRes(scene))
             .add_plugins(DefaultPlugins.set(WindowPlugin {
diff --git a/src/gltf.rs b/src/gltf.rs
index e569ae3..65364c5 100644
--- a/src/gltf.rs
+++ b/src/gltf.rs
@@ -22,6 +22,8 @@
 //! viewer) and, for a consumer that combines them, as node extras
 //! `moxi.states` {state: material} plus the scene's `moxi.stateOrder` (later
 //! wins per part) and `moxi.stateFamilies` {family: [states]}.
+//! A breakable's `after` exports as `moxi.breakable.after` {part: material}
+//! and `reveal` [parts]; revealed parts carry `moxi.hidden: true`.
 //! Node extras carry `{"moxi":{"part","parent","thing"}}`, plus `joint` (ranges) and
 //! `breakable` (`hits` or `event`, and `push` in the thing's space) when declared.
 
@@ -122,6 +124,10 @@ pub fn scene_to_gltf(scene: &Scene, cell: Option<f64>) -> Gltf {
             if let Some(b) = &part.breakable {
                 node["extras"]["moxi"]["breakable"] = json!(b);
             }
+            // Revealed by a breakable's `after`: a consumer hides it at load.
+            if part.hidden {
+                node["extras"]["moxi"]["hidden"] = json!(true);
+            }
 
             // Geometry in node space.
             let expr = part.shape.to_expr();
@@ -192,6 +198,17 @@ pub fn scene_to_gltf(scene: &Scene, cell: Option<f64>) -> Gltf {
                 };
                 let m = material(part.material.as_ref(), &part.color, part.emissive, part.opacity, part.blend.is_some(),
                                  (part.roughness, part.metalness));
+                // the lump's `after` swaps, as the material each named node
+                // switches to when it breaks
+                if let Some(b) = part.breakable.as_ref().filter(|b| !b.after.is_empty()) {
+                    let mut after = serde_json::Map::new();
+                    for (p, look) in &b.after {
+                        let am = material(look.material.as_ref(), &look.color, look.emissive, look.opacity,
+                                          look.blend.is_some(), (look.roughness, look.metalness));
+                        after.insert(p.clone(), json!(am));
+                    }
+                    node["extras"]["moxi"]["breakable"]["after"] = Value::Object(after);
+                }
                 let mut prim = json!({ "attributes": { "POSITION": pa, "NORMAL": na }, "indices": ia, "material": m });
                 // one mapping per distinct look this part takes in some state
                 let mut mappings: Vec<(usize, Vec<usize>)> = Vec::new();
diff --git a/src/mesh.rs b/src/mesh.rs
index 2601741..49ed994 100644
--- a/src/mesh.rs
+++ b/src/mesh.rs
@@ -176,6 +176,7 @@ pub fn surface_nets(shape: &ShapeExpr, cell: f64, vs: f64) -> TriMesh {
 /// Write every part of a scene as a smooth mesh: `<path>.obj` with an
 /// `.mtl` sidecar, one `o` group per part, materials per color.
 pub fn export_scene_obj(scene: &Scene, cell: Option<f64>, path: &str) -> anyhow::Result<()> {
+    let scene = &*scene.at_rest();   // revealed parts are not there at rest
     if let Some(parent) = Path::new(path).parent() {
         std::fs::create_dir_all(parent)?;
     }
diff --git a/src/parser/mod.rs b/src/parser/mod.rs
index 9aeaa8d..d79731c 100644
--- a/src/parser/mod.rs
+++ b/src/parser/mod.rs
@@ -35,6 +35,11 @@ impl Parser {
         &self.peek().kind
     }
 
+    /// The kind `k` tokens ahead of the cursor, if there is one.
+    fn peek_at(&self, k: usize) -> Option<&TokenKind> {
+        self.tokens.get(self.cursor + k).map(|t| &t.kind)
+    }
+
     fn span(&self) -> Span {
         self.peek().span
     }
@@ -360,7 +365,7 @@ impl Parser {
         Ok(SpinDecl { name, lines, span })
     }
 
-    /// `breakable PART { hits=N | event=Name  [push=word] }` — contextual.
+    /// `breakable PART { hits=N | event=Name  [push=word]  [after { … }] }` — contextual.
     /// Which keys are present is checked here; what they mean (exactly one
     /// trigger, a whole number of hits, a push word) in the resolver, which
     /// can name the part.
@@ -375,11 +380,34 @@ impl Parser {
         }
         self.expect_kind(&TokenKind::LBrace,
             "'{' opening the breakable, e.g. `breakable Head { hits=5 }`")?;
-        let mut d = BreakDecl { part, hits: None, event: None, push: None, span };
-        let keys = "hits=N (breaks after N hits), event=Name (breaks when the game fires Name) \
-                    or push=out|north|south|east|west|up|down";
+        let mut d = BreakDecl { part, hits: None, event: None, push: None, after: Vec::new(), span };
+        let keys = "hits=N (breaks after N hits), event=Name (breaks when the game fires Name), \
+                    push=out|north|south|east|west|up|down or after { … } (the thing once it has broken)";
+        let mut had_after = false;
         while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
             let kspan = self.span();
+            if let TokenKind::Ident(k) = self.peek_kind().clone() {
+                if k == "after" && !self.next_is_eq() {
+                    if had_after {
+                        return Err(MoxiError::UnexpectedToken {
+                            got:      "a second 'after'".to_string(),
+                            expected: "one `after { … }` per breakable; put every line in it".to_string(),
+                            span:     kspan,
+                        });
+                    }
+                    had_after = true;
+                    self.advance();
+                    d.after = self.parse_after_block()?;
+                    continue;
+                }
+                if k == "reveal" && !self.next_is_eq() {
+                    return Err(MoxiError::UnexpectedToken {
+                        got:      "'reveal'".to_string(),
+                        expected: "`reveal` inside `after { … }`, e.g. `breakable Head { hits=5  after { reveal NeckStump } }`".to_string(),
+                        span:     kspan,
+                    });
+                }
+            }
             let key = match self.peek_kind().clone() {
                 TokenKind::Ident(k) if self.next_is_eq() && matches!(k.as_str(), "hits" | "event" | "push") => k,
                 TokenKind::On => return Err(MoxiError::UnexpectedToken {
@@ -416,6 +444,57 @@ impl Parser {
         Ok(d)
     }
 
+    /// `after { (Part material = Mat | reveal Part)* }` inside a breakable.
+    fn parse_after_block(&mut self) -> Result<Vec<AfterLine>, MoxiError> {
+        self.expect_kind(&TokenKind::LBrace,
+            "'{' opening the after block, e.g. `after { Neck material = Flesh  reveal NeckStump }`")?;
+        let line_hint = "an after line — `Part material = Material` (the part's look once broken) or \
+                         `reveal Part` (a part hidden until this lump breaks)";
+        let mut lines = Vec::new();
+        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
+            let lspan = self.span();
+            let TokenKind::Ident(first) = self.peek_kind().clone() else {
+                return Err(MoxiError::UnexpectedToken {
+                    got: format!("{:?}", self.peek_kind()), expected: line_hint.to_string(), span: lspan,
+                });
+            };
+            let path = |p: &mut Self| -> Result<Ident, MoxiError> {
+                let mut part = p.parse_indexed_ident()?;
+                while matches!(p.peek_kind(), TokenKind::Dot) {
+                    p.advance();
+                    let next = p.parse_indexed_ident()?;
+                    part.name = format!("{}.{}", part.name, next.name);
+                }
+                Ok(part)
+            };
+            if first == "reveal" && matches!(self.peek_at(1), Some(TokenKind::Ident(_))) {
+                self.advance();
+                lines.push(AfterLine::Reveal { part: path(self)? });
+                continue;
+            }
+            let part = path(self)?;
+            match self.peek_kind() {
+                TokenKind::Material => {
+                    self.advance();
+                    self.expect_kind(&TokenKind::Eq, "'=' after `material`, e.g. `Neck material = Flesh`")?;
+                    let material = self.expect_ident()?;
+                    lines.push(AfterLine::Swap { part, material });
+                }
+                TokenKind::Eq => return Err(MoxiError::UnexpectedToken {
+                    got:      format!("'{} ='", part.name),
+                    expected: "a part's material swap (`Neck material = Flesh`) — an after block changes \
+                               which material a part wears; to change a material everywhere, use a `state`".to_string(),
+                    span:     part.span,
+                }),
+                _ => return Err(MoxiError::UnexpectedToken {
+                    got: part.name.clone(), expected: line_hint.to_string(), span: part.span,
+                }),
+            }
+        }
+        self.expect_kind(&TokenKind::RBrace, "'}' closing the after block")?;
+        Ok(lines)
+    }
+
     /// `joint PART { twist=a..b  pitch=a..b  gap=a..b  shift=(a..b, c..d)  lean=(a..b, c..d) }`
     fn parse_joint_decl(&mut self) -> Result<JointDecl, MoxiError> {
         let span = self.span();
diff --git a/src/pipeline.rs b/src/pipeline.rs
index f5a0639..ef68d3c 100644
--- a/src/pipeline.rs
+++ b/src/pipeline.rs
@@ -539,11 +539,18 @@ pub fn compile_source_posed(source: &str, pose: Option<&str>) -> Result<WorldOut
     let mut layers: Vec<LayerInfo> = Vec::new();
 
     for layer in &placed {
-        let grid = rasterize_entity(&layer.parts, layer.voxel_size);
+        // revealed parts are not there at rest (Issue G)
+        let hidden = resolved.entities.iter().find(|e| e.name == layer.thing).map(hidden_parts).unwrap_or_default();
+        let shown: Vec<(String, ShapeExpr, u16, Frame)>;
+        let parts = if hidden.is_empty() { &layer.parts } else {
+            shown = layer.parts.iter().filter(|p| !hidden.contains(p.0.as_str())).cloned().collect();
+            &shown
+        };
+        let grid = rasterize_entity(parts, layer.voxel_size);
         let (gw, gh, gd) = grid.dims();
         layers.push(LayerInfo {
             name: layer.thing.clone(), dims: [gw, gh, gd], voxels: grid.filled_count(),
-            parts: layer.parts.len(),
+            parts: parts.len(),
         });
 
         // The grid's (0,0,0) is the world minimum of the layer's AABB, so
@@ -552,7 +559,7 @@ pub fn compile_source_posed(source: &str, pose: Option<&str>) -> Result<WorldOut
         // convention: a scene is a thing whose parts are placed by
         // relation, and those relations are the only thing that decides
         // where anything sits.
-        let origin = grid_origin(&layer.parts, layer.voxel_size);
+        let origin = grid_origin(parts, layer.voxel_size);
 
         all_voxels.extend(grid_to_scene(&grid, &resolved.atoms, origin).voxels);
     }
@@ -638,6 +645,11 @@ pub fn solve_notes(source: &str, pose: Option<&str>) -> Result<Vec<(String, Stri
     Ok(out)
 }
 
+/// Parts a breakable's `after` block reveals: hidden until that lump breaks.
+fn hidden_parts(e: &crate::resolver::ResolvedEntity) -> HashSet<&str> {
+    e.breakables.iter().flat_map(|b| b.reveal.iter().map(|p| p.as_str())).collect()
+}
+
 pub fn compile_to_scene(source: &str) -> Result<crate::scene::Scene, Vec<CompileError>> {
     compile_to_scene_posed(source, None)
 }
@@ -665,6 +677,20 @@ pub fn compile_to_scene_posed(source: &str, pose: Option<&str>) -> Result<crate:
         let mats = layer.materials;
         let ent = resolved.entities.iter().find(|e| e.name == layer.thing);
         let pushes = ent.map(|e| push_vectors(e, &layer.parts, &joints)).unwrap_or_default();
+        let hidden = ent.map(hidden_parts).unwrap_or_default();
+        // a material's look, for a breakable's `after` swaps
+        let look_of = |m: usize| {
+            let x = resolved.materials.get(m);
+            Look {
+                material:  x.map(|x| x.name.clone()),
+                color:     color_of_material(m).unwrap_or_else(|| "#ff00ff".to_string()),
+                emissive:  x.and_then(|x| x.emissive),
+                opacity:   x.and_then(|x| x.opacity),
+                blend:     x.filter(|x| x.additive).map(|_| "additive".to_string()),
+                roughness: x.and_then(|x| x.roughness),
+                metalness: x.and_then(|x| x.metalness),
+            }
+        };
         let parts: Vec<Part> = layer.parts.into_iter().map(|(name, shape, atom_id, frame)| {
             let atom = resolved.atoms.get(atom_id.saturating_sub(1) as usize);
             let mat = mats.get(&name).and_then(|m| resolved.materials.get(*m));
@@ -698,7 +724,10 @@ pub fn compile_to_scene_posed(source: &str, pose: Option<&str>) -> Result<crate:
                     hits:  match &b.trigger { Trigger::Hits(n) => Some(*n), Trigger::Event(_) => None },
                     event: match &b.trigger { Trigger::Event(e) => Some(e.clone()), Trigger::Hits(_) => None },
                     push:  pushes.get(&name).copied().unwrap_or([0.0, 1.0, 0.0]),
+                    after: b.swaps.iter().map(|(p, m)| (p.clone(), look_of(*m))).collect(),
+                    reveal: b.reveal.clone(),
                 }),
+                hidden: hidden.contains(name.as_str()),
                 name,
             }
         }).collect();
diff --git a/src/report.rs b/src/report.rs
index 71e7a0f..88e8a6e 100644
--- a/src/report.rs
+++ b/src/report.rs
@@ -106,6 +106,17 @@ pub struct ThingReport {
     /// `--state`: parts two or more of the on states change, and which wins.
     #[serde(default, skip_serializing_if = "Vec::is_empty")]
     pub overlaps:   Vec<String>,
+    /// Parts a breakable reveals that are not there yet (left out of the
+    /// rows, contacts and pieces); `--break` on their lump shows them.
+    #[serde(default, skip_serializing_if = "Vec::is_empty")]
+    pub hidden:     Vec<HiddenRow>,
+}
+
+#[derive(Debug, Clone, Serialize)]
+pub struct HiddenRow {
+    pub part:  String,
+    /// The lump whose break reveals it.
+    pub until: String,
 }
 
 #[derive(Debug, Clone, Serialize)]
@@ -367,6 +378,36 @@ struct Body {
 }
 
 fn thing_report(layer: &Layer, broken: &[String]) -> ThingReport {
+    // Revealed parts (Issue G) are not there until their lump breaks: the
+    // report reads the thing without them, and with them once `--break`
+    // names the lump that reveals them.
+    let mut revealer: HashMap<&str, &str> = HashMap::new();
+    for p in &layer.parts {
+        for r in p.breakable.iter().flat_map(|b| &b.reveal) { revealer.insert(r.as_str(), p.name.as_str()); }
+    }
+    let still_hidden: Vec<String> = layer.parts.iter()
+        .filter(|p| p.hidden && revealer.get(p.name.as_str()).is_none_or(|l| !broken.iter().any(|b| b == l)))
+        .map(|p| p.name.clone()).collect();
+    let mut t = if still_hidden.is_empty() {
+        thing_report_shown(layer, broken)
+    } else {
+        let mut shown = layer.clone();
+        shown.parts.retain(|p| !still_hidden.contains(&p.name));
+        thing_report_shown(&shown, broken)
+    };
+    t.hidden = still_hidden.iter().map(|h| HiddenRow {
+        part: h.clone(), until: revealer.get(h.as_str()).map(|s| s.to_string()).unwrap_or_default(),
+    }).collect();
+    // a broken lump's `after` swaps
+    for p in layer.parts.iter().filter(|p| broken.contains(&p.name)) {
+        for (part, look) in p.breakable.iter().flat_map(|b| &b.after) {
+            t.looks.push(LookRow { part: part.clone(), state: format!("after {}", p.name), look: look.clone() });
+        }
+    }
+    t
+}
+
+fn thing_report_shown(layer: &Layer, broken: &[String]) -> ThingReport {
     let vs  = layer.voxel_size;
     let tol = vs;
 
@@ -500,9 +541,20 @@ fn thing_report(layer: &Layer, broken: &[String]) -> ThingReport {
         let across = |a: usize, b: usize| {
             lump_of[a] == Some(a) && rows[a].parent.as_deref() == Some(rows[b].name.as_str())
         };
+        // Nor is a part the break reveals against the lump that revealed it:
+        // a stump stands where the lump left.
+        let index: HashMap<&str, usize> = rows.iter().enumerate().map(|(i, r)| (r.name.as_str(), i)).collect();
+        let mut revealed_by: HashMap<usize, usize> = HashMap::new();
+        for &l in &lumps {
+            for r in layer.parts[l].breakable.iter().flat_map(|b| &b.reveal) {
+                if let Some(&i) = index.get(r.as_str()) { revealed_by.insert(i, l); }
+            }
+        }
+        let shown = |a: usize, b: usize| revealed_by.get(&a).is_some_and(|&l| lump_of[b] == Some(l));
         torn = Some(
             edges.iter()
-                .filter(|&&(i, j)| lump_of[i] != lump_of[j] && !across(i, j) && !across(j, i))
+                .filter(|&&(i, j)| lump_of[i] != lump_of[j] && !across(i, j) && !across(j, i)
+                    && !shown(i, j) && !shown(j, i))
                 .map(|&(i, j)| [rows[i].name.clone(), rows[j].name.clone()])
                 .collect(),
         );
@@ -520,6 +572,7 @@ fn thing_report(layer: &Layer, broken: &[String]) -> ThingReport {
         notes:      Vec::new(),
         looks:      Vec::new(),
         overlaps:   Vec::new(),
+        hidden:     Vec::new(),
     }
 }
 
@@ -1085,6 +1138,10 @@ fn thing_text(t: &ThingReport, full: bool) -> String {
     for o in &t.overlaps {
         lines.push(format!("overlap: {o}"));
     }
+    if !t.hidden.is_empty() {
+        lines.push(format!("hidden: {}", t.hidden.iter()
+            .map(|h| format!("{} (until {} breaks)", h.part, h.until)).collect::<Vec<_>>().join(", ")));
+    }
 
     lines.join("\n")
 }
diff --git a/src/resolver/breakable.rs b/src/resolver/breakable.rs
index bc2cc83..f24bf8d 100644
--- a/src/resolver/breakable.rs
+++ b/src/resolver/breakable.rs
@@ -50,6 +50,10 @@ pub struct ResolvedBreak {
     /// Set on a `symmetric_across` image: the source whose breakable it
     /// follows (its push is the source's, reflected).
     pub mirror_of: Option<String>,
+    /// `after { P material = M }`: (flattened part, material index), in order.
+    pub swaps:     Vec<(String, usize)>,
+    /// `after { reveal P }`: flattened parts hidden until this lump breaks.
+    pub reveal:    Vec<String>,
     pub span:      Span,
 }
 
@@ -57,12 +61,14 @@ pub struct ResolvedBreak {
 /// parameters.
 #[derive(Debug, Clone)]
 pub(super) struct RawBreak {
-    pub part:  String,
-    pub hits:  Option<Expr>,
-    pub event: Option<String>,
-    pub push:  Push,
-    pub space: Option<String>,
-    pub span:  Span,
+    pub part:   String,
+    pub hits:   Option<Expr>,
+    pub event:  Option<String>,
+    pub push:   Push,
+    pub space:  Option<String>,
+    pub swaps:  Vec<(String, usize)>,
+    pub reveal: Vec<String>,
+    pub span:   Span,
 }
 
 impl RawBreak {
@@ -72,6 +78,8 @@ impl RawBreak {
     pub(super) fn prefixed(&self, prefix: &str) -> RawBreak {
         RawBreak {
             part:  format!("{prefix}.{}", self.part),
+            swaps: self.swaps.iter().map(|(p, m)| (format!("{prefix}.{p}"), *m)).collect(),
+            reveal: self.reveal.iter().map(|p| format!("{prefix}.{p}")).collect(),
             space: Some(match &self.space {
                 Some(s) => format!("{prefix}.{s}"),
                 None    => prefix.to_string(),
@@ -83,7 +91,12 @@ impl RawBreak {
 
 /// This thing's own `breakable` declarations → raw breakables on flattened
 /// subjects. Every problem is a `BreakError` naming what could be named.
-pub(super) fn resolve_break_decls(decls: &[BreakDecl], sc: &JointScope, errs: &mut Vec<MoxiError>) -> Vec<RawBreak> {
+pub(super) fn resolve_break_decls(
+    decls: &[BreakDecl],
+    sc:    &JointScope,
+    after: &AfterScope,
+    errs:  &mut Vec<MoxiError>,
+) -> Vec<RawBreak> {
     let align_of: HashMap<&str, &str> = sc.own_align.iter().map(|(w, s)| (w.as_str(), s.as_str())).collect();
     let breakable = if sc.own_align.is_empty() {
         format!("'{}' has no mates, so nothing in it can break", sc.thing)
@@ -144,12 +157,15 @@ pub(super) fn resolve_break_decls(decls: &[BreakDecl], sc: &JointScope, errs: &m
                 }
             },
         };
+        let (swaps, reveal) = resolve_after(&name, &d.after, sc, after, errs);
         out.push(RawBreak {
             part:  subject.to_string(),
             hits:  d.hits.clone(),
             event: d.event.as_ref().map(|e| e.name.clone()),
             push,
             space: None,
+            swaps,
+            reveal,
             span:  d.span,
         });
     }
@@ -179,26 +195,182 @@ pub(super) fn fold_break(raw: &RawBreak, env: &ParamEnv, errs: &mut Vec<MoxiErro
         (None, None) => return None, // reported when declared
     };
     Some(ResolvedBreak {
-        part: raw.part.clone(), trigger, push: raw.push, space: raw.space.clone(), mirror_of: None, span: raw.span,
+        part: raw.part.clone(), trigger, push: raw.push, space: raw.space.clone(), mirror_of: None,
+        swaps: raw.swaps.clone(), reveal: raw.reveal.clone(), span: raw.span,
     })
 }
 
+/// What an `after` block may name.
+pub(super) struct AfterScope<'a> {
+    /// Every flattened part with a shape, in declared order.
+    pub part_order:     &'a [String],
+    pub material_index: &'a HashMap<String, usize>,
+}
+
+/// An `after` block's lines → (swaps, reveals) on flattened parts. A name is
+/// a part of this thing, a part inside an instance (`Left.Neck`), or a whole
+/// instance (every part of it), as in a `state`.
+fn resolve_after(
+    lump:  &str,
+    lines: &[AfterLine],
+    sc:    &JointScope,
+    after: &AfterScope,
+    errs:  &mut Vec<MoxiError>,
+) -> (Vec<(String, usize)>, Vec<String>) {
+    let mut materials: Vec<(&String, &usize)> = after.material_index.iter().collect();
+    materials.sort_by_key(|(_, i)| **i);
+    let material_list = materials.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
+    let berr = |message: String, span: Span| MoxiError::BreakError { part: lump.to_string(), message, span };
+    let targets = |part: &Ident, errs: &mut Vec<MoxiError>| -> Vec<String> {
+        if after.part_order.contains(&part.name) {
+            return vec![part.name.clone()];
+        }
+        if sc.instance_of.contains_key(&part.name) {
+            let pre = format!("{}.", part.name);
+            return after.part_order.iter().filter(|p| p.starts_with(&pre)).cloned().collect();
+        }
+        let tops: Vec<&str> = after.part_order.iter().filter(|p| !p.contains('.')).map(|p| p.as_str())
+            .chain(sc.instance_of.keys().map(|s| s.as_str())).collect();
+        errs.push(berr(format!("after: no part '{}' in '{}' — parts: {}", part.name, sc.thing, tops.join(", ")), part.span));
+        Vec::new()
+    };
+    let mut swaps: Vec<(String, usize)> = Vec::new();
+    let mut reveal: Vec<String> = Vec::new();
+    for l in lines {
+        match l {
+            AfterLine::Swap { part, material } => {
+                let Some(&m) = after.material_index.get(&material.name) else {
+                    errs.push(berr(format!("after: no material '{}' — materials: {material_list}", material.name), material.span));
+                    continue;
+                };
+                for t in targets(part, errs) {
+                    if swaps.iter().any(|(p, _)| *p == t) {
+                        errs.push(berr(format!("after: sets '{t}' material twice"), part.span));
+                        continue;
+                    }
+                    swaps.push((t, m));
+                }
+            }
+            AfterLine::Reveal { part } => {
+                for t in targets(part, errs) {
+                    if reveal.contains(&t) {
+                        errs.push(berr(format!("after: reveals '{t}' twice"), part.span));
+                        continue;
+                    }
+                    reveal.push(t);
+                }
+            }
+        }
+    }
+    (swaps, reveal)
+}
+
 /// A `symmetric_across` image breaks like its source, with its own count.
 /// Applied to the thing's flattened relations, so images inside instances
 /// (expanded to one `Mirror` per part) are covered too.
 pub(super) fn add_mirror_images(breaks: &mut Vec<ResolvedBreak>, relations: &[Placement]) {
     let have: HashSet<String> = breaks.iter().map(|b| b.part.clone()).collect();
+    let image_of: HashMap<&str, &str> = relations.iter().filter_map(|p| match p {
+        Placement::Mirror { subject, source, .. } => Some((source.as_str(), subject.as_str())),
+        _ => None,
+    }).collect();
     let mut add = Vec::new();
     for p in relations {
         let Placement::Mirror { subject, source, .. } = p else { continue };
         if have.contains(subject) { continue; }
         if let Some(src) = breaks.iter().find(|b| b.part == *source) {
-            add.push(ResolvedBreak { part: subject.clone(), mirror_of: Some(source.clone()), ..src.clone() });
+            // the after block names the images of the parts it names, where
+            // they have one (a part with none, like a shared torso, is the
+            // same part on both sides)
+            let image = |p: &String| image_of.get(p.as_str()).map(|s| s.to_string()).unwrap_or_else(|| p.clone());
+            add.push(ResolvedBreak {
+                part: subject.clone(), mirror_of: Some(source.clone()),
+                swaps: src.swaps.iter().map(|(p, m)| (image(p), *m)).collect(),
+                reveal: src.reveal.iter().map(image).collect(),
+                ..src.clone()
+            });
         }
     }
     breaks.extend(add);
 }
 
+/// Checks on every `after` block (DOC-20261008-detachment §6), once the
+/// thing's breakables, its instances' and its mirror images' are known:
+/// - an after part is AT THE CUT: inside the lump, the part the lump hangs
+///   from, or another part hanging from that one (a stump, a collar);
+/// - a revealed part is revealed by one lump only, and is neither the root
+///   nor the lump's own root.
+pub(super) fn check_after(breaks: &[ResolvedBreak], relations: &[Placement], errs: &mut Vec<MoxiError>) {
+    let parent = parents(relations);
+    let children = |p: &str| -> Vec<&str> { parent.iter().filter(|(_, q)| q.as_str() == p).map(|(c, _)| c.as_str()).collect() };
+    let mut revealed_by: HashMap<&str, &str> = HashMap::new();
+    for b in breaks {
+        if b.swaps.is_empty() && b.reveal.is_empty() { continue; }
+        let berr = |message: String| MoxiError::BreakError { part: b.part.clone(), message, span: b.span };
+        // the lump: its root and everything under it
+        let mut lump: Vec<&str> = vec![b.part.as_str()];
+        let mut i = 0;
+        while i < lump.len() {
+            let kids = children(lump[i]);
+            lump.extend(kids);
+            i += 1;
+        }
+        let up = parent.get(b.part.as_str()).map(|s| s.as_str());
+        let mut at_cut: Vec<&str> = lump.clone();
+        if let Some(u) = up {
+            at_cut.push(u);
+            at_cut.extend(children(u).into_iter().filter(|c| !lump.contains(c)));
+        }
+        let names: Vec<&String> = b.swaps.iter().map(|(p, _)| p).chain(&b.reveal).collect();
+        for p in names {
+            if !at_cut.contains(&p.as_str()) {
+                errs.push(berr(format!(
+                    "after: '{p}' is not at the cut — an after block changes the broken lump, the part it hangs \
+                     from{}, or a part hanging from that one: {}",
+                    up.map(|u| format!(" ('{u}')")).unwrap_or_default(), at_cut.join(", "))));
+            }
+        }
+        for p in &b.reveal {
+            if *p == b.part {
+                errs.push(berr(format!("after: reveals its own lump '{p}' — a lump cannot be hidden until it breaks off; \
+                    reveal a part at the cut instead (a stump)")));
+            } else if up == Some(p.as_str()) {
+                errs.push(berr(format!("after: reveals '{p}', the part the lump hangs from — it is there before the \
+                    break; reveal a part placed at the cut instead (a stump hanging from '{p}')")));
+            }
+            match revealed_by.get(p.as_str()) {
+                Some(other) => errs.push(berr(format!(
+                    "after: '{p}' is already revealed by '{other}' — a part is revealed by one lump only{}",
+                    if b.mirror_of.is_some() { "; mirror the revealed part too, so each side has its own" } else { "" }))),
+                None => { revealed_by.insert(p.as_str(), b.part.as_str()); }
+            }
+        }
+    }
+}
+
+/// The mate tree as part → parent (joints.rs's rule): a mate's object is its
+/// subject's parent; a mirror image hangs from the image of its source's
+/// parent when that was mirrored too, else from the same parent.
+fn parents(relations: &[Placement]) -> HashMap<String, String> {
+    let mut parent: HashMap<String, String> = HashMap::new();
+    let mut image_of: HashMap<&str, &str> = HashMap::new();
+    for p in relations {
+        match p {
+            Placement::Align { subject, object, .. } => { parent.insert(subject.part.clone(), object.part.clone()); }
+            Placement::Mirror { subject, source, .. } => { image_of.insert(source.as_str(), subject.as_str()); }
+        }
+    }
+    for p in relations {
+        if let Placement::Mirror { subject, source, .. } = p {
+            if let Some(sp) = parent.get(source).cloned() {
+                let up = image_of.get(sp.as_str()).map(|s| s.to_string()).unwrap_or(sp);
+                parent.insert(subject.clone(), up);
+            }
+        }
+    }
+    parent
+}
+
 #[cfg(test)]
 mod tests {
     use crate::pipeline::compile_to_scene;
@@ -376,4 +548,158 @@ mod tests {
         let m = errors(&demon("breakable Head { on=Pop }"));
         assert!(m.contains("the trigger key is `event` (`on` is the mate keyword)"), "{m}");
     }
-}
+
+    // ── after { … } and reveal (Issue G) ──
+
+    const STUMPS: &str = "material Bone  { color = \"#ddd8c4\" }
+    material Flesh { color = \"#7a1414\" }
+    thing Demon {
+        part Body  { shape = box(width=40, height=60, depth=30), material = Bone }
+        part Neck  { shape = cylinder(height=8, radius=7), material = Bone }
+        part Head  { shape = sphere(radius=14), material = Bone }
+        part Stump { shape = cylinder(height=3, radius=6), material = Flesh }
+        part EyeR  { shape = sphere(radius=3), material = Bone }
+        part ArmR  { shape = capsule(height=30, radius=4), material = Bone }
+        part ArmL  { shape = capsule(height=30, radius=4), material = Bone }
+        part NubR  { shape = sphere(radius=4), material = Flesh }
+        part NubL  { shape = sphere(radius=4), material = Flesh }
+        relation {
+            Neck.bottom  on Body.top
+            Head.bottom  on Neck.top gap=-2
+            Stump.bottom on Neck.top
+            EyeR.center  on Head.north shift=(3, 5)
+            ArmR.top     on Body.east shift=(20, 0)
+            ArmL symmetric_across Body from=ArmR
+            NubR.center  on Body.east shift=(20, 0)
+            NubL symmetric_across Body from=NubR
+        }
+        BREAKS
+    }";
+
+    const PAIR: &str = "
+    thing Pair {
+        part Floor { shape = box(width=200, height=2, depth=60), material = Bone }
+        part A { thing = Demon }
+        relation { A.Body.bottom on Floor.top }
+    }";
+
+    fn stumps(breaks: &str, print: &str) -> String {
+        let pair = if print == "Pair" { PAIR } else { "" };
+        STUMPS.replace("BREAKS", breaks) + pair + "\nprint " + print
+    }
+
+    const AFTER: &str = "breakable Head { hits=5  after { Neck material = Flesh  Head material = Flesh  reveal Stump } }  \
+                         breakable ArmR { hits=3  after { reveal NubR } }";
+
+    #[test]
+    fn after_swaps_and_reveals_reach_the_scene_and_mirrors_name_their_own_parts() {
+        let p = scene_parts(&stumps(AFTER, "Demon"));
+        let head = p["Head"].breakable.clone().unwrap();
+        let swapped: Vec<(&str, Option<&str>)> = head.after.iter().map(|(k, l)| (k.as_str(), l.material.as_deref())).collect();
+        assert_eq!(swapped, vec![("Head", Some("Flesh")), ("Neck", Some("Flesh"))]);
+        assert_eq!(head.reveal, vec!["Stump".to_string()]);
+        assert!(p["Stump"].hidden && p["NubR"].hidden && p["NubL"].hidden);
+        assert!(!p["Neck"].hidden && !p["Head"].hidden);
+        assert_eq!(p["ArmL"].breakable.clone().unwrap().reveal, vec!["NubL".to_string()], "the image reveals the image");
+        // an instance brings them, prefixed
+        let p = scene_parts(&stumps(AFTER, "Pair"));
+        assert_eq!(p["A.Head"].breakable.clone().unwrap().reveal, vec!["A.Stump".to_string()]);
+        assert!(p["A.Head"].breakable.clone().unwrap().after.contains_key("A.Neck"));
+        assert!(p["A.Stump"].hidden);
+        // nothing is hidden without a reveal
+        let p = scene_parts(&stumps("breakable Head { hits=5 }", "Demon"));
+        assert!(p.values().all(|x| !x.hidden));
+    }
+
+    #[test]
+    fn hidden_parts_are_left_out_of_every_drawing_of_rest_and_kept_in_gltf() {
+        let src = stumps(AFTER, "Demon");
+        let plain = stumps("breakable Head { hits=5 }", "Demon");
+        let parts = |s: &str| crate::pipeline::compile_source(s).unwrap().layers[0].parts;
+        assert_eq!(parts(&plain) - parts(&src), 3, "Stump, NubR, NubL not rasterized");
+        let scene = compile_to_scene(&src).unwrap();
+        let glsl = crate::shader::glsl_map(&scene);
+        assert!(glsl.contains("// Neck") && !glsl.contains("// Stump") && !glsl.contains("// NubL"));
+        let html = crate::shader::html_page(&scene, "t");
+        assert!(!html.contains("// Stump"));
+        let dir = std::env::temp_dir().join(format!("moxi-hidden-{}", std::process::id()));
+        crate::mesh::export_scene_obj(&scene, Some(2.0), dir.join("d").to_str().unwrap()).unwrap();
+        let obj = std::fs::read_to_string(dir.join("d.obj")).unwrap();
+        assert!(obj.contains("o Neck") && !obj.contains("o Stump") && !obj.contains("o NubR"));
+        let _ = std::fs::remove_dir_all(dir);
+        let g = crate::gltf::scene_to_gltf(&scene, None).json;
+        let node = |n: &str| g["nodes"].as_array().unwrap().iter().find(|x| x["name"] == n).unwrap().clone();
+        assert_eq!(node("Stump")["extras"]["moxi"]["hidden"], true);
+        assert!(node("Neck")["extras"]["moxi"].get("hidden").is_none());
+        let after = &node("Head")["extras"]["moxi"]["breakable"]["after"];
+        let mats = g["materials"].as_array().unwrap();
+        assert_eq!(mats[after["Neck"].as_u64().unwrap() as usize]["name"], "Flesh");
+        assert_eq!(node("Head")["extras"]["moxi"]["breakable"]["reveal"], serde_json::json!(["Stump"]));
+    }
+
+    #[test]
+    fn report_break_shows_what_the_break_reveals_and_swaps() {
+        let src = stumps(AFTER, "Demon");
+        let rest = crate::report::from_source(&src, None).unwrap();
+        let t = &rest.things[0];
+        assert!(!t.parts.iter().any(|p| p.name == "Stump"), "not there at rest");
+        assert_eq!(t.hidden.iter().map(|h| (h.part.as_str(), h.until.as_str())).collect::<Vec<_>>(),
+            vec![("Stump", "Head"), ("NubR", "ArmR"), ("NubL", "ArmL")]);
+        let broke = crate::report::from_source_broken(&src, None, &["Head".to_string()]).unwrap();
+        let t = &broke.things[0];
+        assert!(t.parts.iter().any(|p| p.name == "Stump"), "revealed by the break");
+        assert_eq!(t.torn.as_deref(), Some(&[][..]), "a stump where the head stood is no tear");
+        assert!(t.looks.iter().any(|l| l.part == "Neck" && l.state == "after Head" && l.look.material.as_deref() == Some("Flesh")));
+        assert!(broke.to_text(false).contains("hidden: NubR (until ArmR breaks), NubL (until ArmL breaks)"));
+    }
+
+    #[test]
+    fn after_problems_name_the_fix() {
+        let m = errors(&stumps("breakable EyeR { hits=1  after { ArmR material = Flesh } }", "Demon"));
+        assert!(m.contains("breakable 'EyeR': after: 'ArmR' is not at the cut — an after block changes the broken lump, \
+                            the part it hangs from ('Head'), or a part hanging from that one: EyeR, Head"), "{m}");
+        let m = errors(&stumps("breakable Head { hits=1  after { reveal Head } }", "Demon"));
+        assert!(m.contains("after: reveals its own lump 'Head'"), "{m}");
+        let m = errors(&stumps("breakable Head { hits=1  after { reveal Neck } }", "Demon"));
+        assert!(m.contains("after: reveals 'Neck', the part the lump hangs from"), "{m}");
+        let m = errors(&stumps("breakable Head { hits=1  after { reveal Stump } }  \
+                                breakable Stump { hits=1  after { reveal Stump } }", "Demon"));
+        assert!(m.contains("reveals its own lump 'Stump'"), "{m}");
+        let m = errors(&stumps("breakable ArmR { hits=1  after { reveal Stump } }", "Demon"));
+        assert!(m.contains("'Stump' is not at the cut"), "{m}");
+        // a revealed part with no image is revealed by the source and its image
+        let shared = STUMPS.replace("NubR.center  on Body.east shift=(20, 0)", "NubR.center  on Body.east shift=(-20, 0)")
+            .replace("NubL symmetric_across Body from=NubR", "NubL.center on Body.top shift=(0, 30)")
+            .replace("BREAKS", "breakable ArmR { hits=1  after { reveal Body } }") + "\nprint Demon";
+        let m = errors(&shared);
+        assert!(m.contains("reveals 'Body', the part the lump hangs from"), "{m}");
+        let m = errors(&stumps("breakable Head { hits=1  after { Nek material = Flesh } }", "Demon"));
+        assert!(m.contains("after: no part 'Nek' in 'Demon' — parts: Body, Neck, Head"), "{m}");
+        let m = errors(&stumps("breakable Head { hits=1  after { Neck material = Flsh } }", "Demon"));
+        assert!(m.contains("after: no material 'Flsh' — materials: Bone, Flesh"), "{m}");
+    }
+
+    #[test]
+    fn a_revealed_part_is_revealed_once_and_an_image_needs_its_own() {
+        // two lumps hanging from Body both reveal NubR (it hangs from Body too)
+        let m = errors(&stumps("breakable ArmR { hits=1  after { reveal NubR } }  \
+                                breakable Neck { hits=1  after { reveal NubR } }", "Demon"));
+        assert!(m.contains("'NubR' is already revealed by 'ArmR' — a part is revealed by one lump only"), "{m}");
+        // the mirror image of ArmR would reveal the same unmirrored part
+        let src = STUMPS.replace("NubL symmetric_across Body from=NubR", "NubL.center on Body.top shift=(0, 30)")
+            .replace("BREAKS", "breakable ArmR { hits=1  after { reveal Neck } }") + "\nprint Demon";
+        let m = errors(&src);
+        assert!(m.contains("'Neck' is already revealed by 'ArmR' — a part is revealed by one lump only; mirror the \
+                            revealed part too"), "{m}");
+    }
+
+    #[test]
+    fn after_syntax_errors_point_at_the_form() {
+        let m = errors(&stumps("breakable Head { hits=1  reveal Stump }", "Demon"));
+        assert!(m.contains("`reveal` inside `after { … }`"), "{m}");
+        let m = errors(&stumps("breakable Head { hits=1  after { Flesh.color = red } }", "Demon"));
+        assert!(m.contains("an after block changes which material a part wears; to change a material everywhere, use a `state`"), "{m}");
+        let m = errors(&stumps("breakable Head { hits=1  after { reveal Stump }  after { Neck material = Flesh } }", "Demon"));
+        assert!(m.contains("one `after { … }` per breakable"), "{m}");
+    }
+}
\ No newline at end of file
diff --git a/src/resolver/mod.rs b/src/resolver/mod.rs
index f0a7fdf..31fe1a5 100644
--- a/src/resolver/mod.rs
+++ b/src/resolver/mod.rs
@@ -439,6 +439,11 @@ fn walk_break(b: &BreakDecl, f: &mut dyn FnMut(&Expr) -> Expr, n: &mut dyn FnMut
         hits:  b.hits.as_ref().map(&mut *f),
         event: b.event.clone(),
         push:  b.push.clone(),
+        after: b.after.iter().map(|l| match l {
+            AfterLine::Swap { part, material } => AfterLine::Swap {
+                part: Ident { name: n(&part.name), span: part.span }, material: material.clone() },
+            AfterLine::Reveal { part } => AfterLine::Reveal { part: Ident { name: n(&part.name), span: part.span } },
+        }).collect(),
         span:  b.span,
     }
 }
@@ -1781,7 +1786,10 @@ impl Resolver {
                 thing: &e.name.name, own_align: &own_align, own_mirror: &own_mirror,
                 part_names: &part_names, instance_of: &instance_of,
             };
-            let own = breakable::resolve_break_decls(&e.breakables, &scope, &mut errs);
+            let part_order: Vec<String> = parts.iter().map(|p| p.name.clone()).collect();
+            let material_index = self.material_index.clone();
+            let after_scope = breakable::AfterScope { part_order: &part_order, material_index: &material_index };
+            let own = breakable::resolve_break_decls(&e.breakables, &scope, &after_scope, &mut errs);
             // a mirrored instance's parts follow their sources (below)
             inherited_breaks.retain(|b| {
                 let inst = b.part.split('.').next().unwrap_or("");
@@ -1791,6 +1799,7 @@ impl Resolver {
             let mut folded: Vec<ResolvedBreak> = raw.iter()
                 .filter_map(|b| breakable::fold_break(b, &env_default, &mut errs)).collect();
             breakable::add_mirror_images(&mut folded, &relations);
+            breakable::check_after(&folded, &relations, &mut errs);
             self.errors.extend(errs);
             (folded, raw)
         };
diff --git a/src/scene.rs b/src/scene.rs
index 24a9ab4..5cbcb5d 100644
--- a/src/scene.rs
+++ b/src/scene.rs
@@ -160,6 +160,10 @@ pub struct Part {
     /// everything under it in the joint tree leave as one lump.
     #[serde(default, skip_serializing_if = "Option::is_none")]
     pub breakable: Option<BreakOut>,
+    /// Revealed by a breakable's `after` block: not there until that lump
+    /// breaks. Backends that draw rest skip it (`Scene::at_rest`).
+    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
+    pub hidden: bool,
 }
 
 /// A breakable mate (DOC-20261008-detachment §5–§7). Exactly one of `hits`
@@ -174,6 +178,13 @@ pub struct BreakOut {
     pub event: Option<String>,
     /// Unit vector in the thing's space: which way to kick the broken lump.
     pub push:  [f64; 3],
+    /// `after { P material = M }`: each named part's look once this lump has
+    /// broken (applied per node by a consumer; breaks combine).
+    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
+    pub after: std::collections::BTreeMap<String, Look>,
+    /// `after { reveal P }`: parts hidden until this lump breaks.
+    #[serde(default, skip_serializing_if = "Vec::is_empty")]
+    pub reveal: Vec<String>,
 }
 
 #[derive(Debug, Clone, Serialize, Deserialize)]
@@ -401,6 +412,21 @@ impl Shape {
 }
 
 impl Scene {
+    /// The scene as it stands at rest: without the parts a breakable's
+    /// `after` block reveals. Every backend that DRAWS rest (mesh, the
+    /// raymarcher, the viewer) reads this; glTF and the report keep hidden
+    /// parts. Borrowed unchanged when nothing is hidden.
+    pub fn at_rest(&self) -> std::borrow::Cow<'_, Scene> {
+        if !self.layers.iter().any(|l| l.parts.iter().any(|p| p.hidden)) {
+            return std::borrow::Cow::Borrowed(self);
+        }
+        let mut s = self.clone();
+        for l in &mut s.layers {
+            l.parts.retain(|p| !p.hidden);
+        }
+        std::borrow::Cow::Owned(s)
+    }
+
     pub fn to_json_pretty(&self) -> String {
         serde_json::to_string_pretty(self).expect("scene is always serializable")
     }
@@ -504,6 +530,7 @@ mod tests {
                     metalness: None,
                     joint: None,
                     breakable: None,
+                    hidden: false,
                 }],
             }],
         };
diff --git a/src/shader.rs b/src/shader.rs
index 65cf292..6ec259e 100644
--- a/src/shader.rs
+++ b/src/shader.rs
@@ -27,6 +27,7 @@ use crate::scene::{Scene, Shape};
 /// The `map()` function and its helpers only — for a host that already
 /// has a renderer and wants to drop the scene into it.
 pub fn glsl_map(scene: &Scene) -> String {
+    let scene = &*scene.at_rest();   // revealed parts are not there at rest
     let mut out = String::new();
     out.push_str(GLSL_PRIMITIVES);
     emit_materials(scene, &mut out);
@@ -36,6 +37,7 @@ pub fn glsl_map(scene: &Scene) -> String {
 
 /// A complete `#version 300 es` fragment shader.
 pub fn glsl_fragment(scene: &Scene) -> String {
+    let scene = &*scene.at_rest();
     let mut out = String::new();
     out.push_str(GLSL_HEADER);
     out.push_str(&glsl_map(scene));
@@ -59,6 +61,7 @@ fn has_surface(scene: &Scene) -> bool {
 
 /// A self-contained HTML page: WebGL2, the shader, an orbit camera.
 pub fn html_page(scene: &Scene, title: &str) -> String {
+    let scene = &*scene.at_rest();
     let (center, radius) = scene_bounds(scene);
     let heavy = scene.layers.iter().map(|l| l.parts.len()).sum::<usize>() > 60;
     HTML_TEMPLATE
