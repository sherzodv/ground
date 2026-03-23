/// Resolve pass: `ParseRes` → `IrRes`.
///
/// Passes:
///   1. Mirror scope tree from parse arena.
///   2. Register type/link names in their scopes.
///   3. Resolve type bodies and link types.
///   4. Register instance names (enables forward references).
///   5. Resolve instance fields (validated against link type patterns).
///   6. Resolve deploys.
use std::collections::{HashMap, HashSet};

use crate::ast::{self, *};
use crate::ir::*;
use crate::ir::ScopeKind as IrScopeKind;

// ---------------------------------------------------------------------------
// Resolver context
// ---------------------------------------------------------------------------

struct Ctx {
    types:  Vec<IrTypeDef>,
    links:  Vec<IrLinkDef>,
    insts:  Vec<IrInstDef>,
    scopes: Vec<IrScope>,
    errors: Vec<IrError>,
}

impl Ctx {
    fn new() -> Self {
        let root = IrScope {
            kind:      IrScopeKind::Pack,
            name:      None,
            parent:    None,
            types:     HashMap::new(),
            links:     HashMap::new(),
            insts:     HashMap::new(),
            packs:     HashMap::new(),
            ambiguous: HashSet::new(),
        };
        Ctx { types: vec![], links: vec![], insts: vec![], scopes: vec![root], errors: vec![] }
    }

    fn alloc_type(&mut self, name: Option<String>, scope: ScopeId, loc: IrLoc, body: IrTypeBody) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        if let Some(n) = &name {
            if self.scopes[scope.0 as usize].types.contains_key(n) {
                self.push_error(format!("duplicate type name '{}' in scope", n));
            } else {
                self.scopes[scope.0 as usize].types.insert(n.clone(), id);
            }
        }
        self.types.push(IrTypeDef { name, scope, loc, body });
        id
    }

    fn alloc_link(&mut self, name: Option<String>, scope: ScopeId, loc: IrLoc, link_type: IrLinkType) -> LinkId {
        let id = LinkId(self.links.len() as u32);
        if let Some(n) = &name {
            if self.scopes[scope.0 as usize].links.contains_key(n) {
                self.push_error(format!("duplicate link name '{}' in scope", n));
            } else {
                self.scopes[scope.0 as usize].links.insert(n.clone(), id);
            }
        }
        self.links.push(IrLinkDef { name, scope, loc, link_type });
        id
    }

    fn alloc_inst(&mut self, name: String, scope: ScopeId, loc: IrLoc, type_id: TypeId) -> InstId {
        let id = InstId(self.insts.len() as u32);
        if self.scopes[scope.0 as usize].insts.contains_key(&name) {
            self.push_error(format!("duplicate instance name '{}' in scope", name));
        } else {
            self.scopes[scope.0 as usize].insts.insert(name.clone(), id);
        }
        self.insts.push(IrInstDef { type_id, name, type_hint: None, scope, loc, fields: vec![] });
        id
    }

    fn push_error(&mut self, message: String) {
        self.errors.push(IrError { message, loc: IrLoc { unit: 0, start: 0, end: 0 } });
    }

    /// Mark `name` as ambiguous in `scope`: removes it from all namespace maps and
    /// records it so lookups stop the parent-chain walk with None.
    fn mark_ambiguous(&mut self, scope: ScopeId, name: &str) {
        let s = &mut self.scopes[scope.0 as usize];
        s.types.remove(name);
        s.links.remove(name);
        s.insts.remove(name);
        s.packs.remove(name);
        s.ambiguous.insert(name.to_string());
    }

    fn lookup_type(&self, scope: ScopeId, name: &str) -> Option<TypeId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) { return None; }
        if let Some(&id) = s.types.get(name) { return Some(id); }
        s.parent.and_then(|p| self.lookup_type(p, name))
    }

    fn lookup_link(&self, scope: ScopeId, name: &str) -> Option<LinkId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) { return None; }
        if let Some(&id) = s.links.get(name) { return Some(id); }
        s.parent.and_then(|p| self.lookup_link(p, name))
    }

    fn lookup_inst(&self, scope: ScopeId, name: &str) -> Option<InstId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) { return None; }
        if let Some(&id) = s.insts.get(name) { return Some(id); }
        s.parent.and_then(|p| self.lookup_inst(p, name))
    }

    fn lookup_pack(&self, scope: ScopeId, name: &str) -> Option<ScopeId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) { return None; }
        if let Some(&id) = s.packs.get(name) { return Some(id); }
        s.parent.and_then(|p| self.lookup_pack(p, name))
    }
}

fn ir_loc(loc: &AstNodeLoc) -> IrLoc {
    IrLoc { unit: loc.unit, start: loc.start, end: loc.end }
}

// ---------------------------------------------------------------------------
// Group ref helpers
// ---------------------------------------------------------------------------

/// Build the source repr of a Group segment: `{inner:repr}trailing`.
fn group_repr(inner: &AstRef, trailing: Option<&str>) -> String {
    let inner_repr = inner.segments.iter()
        .filter_map(|s| s.inner.as_plain())
        .collect::<Vec<_>>().join(":");
    format!("{{{}}}{}", inner_repr, trailing.unwrap_or(""))
}

/// True if any segment in `r` is a Group that has not been reduced to plain.
fn has_group(r: &AstRef) -> bool {
    r.segments.iter().any(|s| matches!(&s.inner.value, AstRefSegVal::Group(..)))
}

/// Convert an `AstRef` to its string repr, rendering remaining Group segments
/// as `{inner}trailing` and joining plain segments with `:`.
fn ref_to_repr(r: &AstRef) -> String {
    r.segments.iter().map(|s| match &s.inner.value {
        AstRefSegVal::Plain(v)        => v.clone(),
        AstRefSegVal::Group(g, trail) => group_repr(g, trail.as_deref()),
    }).collect::<Vec<_>>().join(":")
}

/// Attempt to reduce `{this:field_name}` to the plain value already resolved
/// for `field_name` on the current instance.  Returns `None` if the inner ref
/// does not match the `this:xxx` pattern or the field is not yet in the map.
fn reduce_this_group(
    inner:       &AstRef,
    trailing:    Option<&str>,
    this_fields: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let segs = &inner.segments;
    if segs.len() == 2
        && segs[0].inner.as_plain() == Some("this")
    {
        if let Some(field_name) = segs[1].inner.as_plain() {
            if let Some(value) = this_fields.get(field_name) {
                return Some(match trailing {
                    Some(t) => format!("{}{}", value, t),
                    None    => value.clone(),
                });
            }
        }
    }
    None
}

/// Reduce all `{this:xxx}` Group segments in `r` using already-resolved
/// instance field values.  Non-`this` groups are left as-is.
fn reduce_ast_ref(r: &AstRef, this_fields: &std::collections::HashMap<String, String>) -> AstRef {
    let segments = r.segments.iter().map(|seg| {
        let new_val = match &seg.inner.value {
            AstRefSegVal::Group(inner, trailing) => {
                match reduce_this_group(inner, trailing.as_deref(), this_fields) {
                    Some(plain) => AstRefSegVal::Plain(plain),
                    None        => seg.inner.value.clone(),
                }
            }
            AstRefSegVal::Plain(_) => seg.inner.value.clone(),
        };
        AstNode { loc: seg.loc.clone(), inner: AstRefSeg { value: new_val, is_opt: seg.inner.is_opt } }
    }).collect();
    AstRef { segments }
}

/// Extract the variant name from a plain single-segment IrRef (`Plain("foo")`).
/// Returns None for typed or multi-segment variants.
fn plain_variant_name(r: &IrRef) -> Option<&str> {
    if let [seg] = r.segments.as_slice() {
        if let IrRefSegValue::Plain(s) = &seg.value { return Some(s); }
    }
    None
}

/// Convert a resolved `IrValue` to a plain string for use as a `{this:xxx}`
/// substitution target.  Only scalar values can be plainified.
fn ir_value_to_plain_str(v: &IrValue, ctx: &Ctx) -> Option<String> {
    match v {
        IrValue::Str(s)           => Some(s.clone()),
        IrValue::Ref(s)           => Some(s.clone()),
        IrValue::Int(n)           => Some(n.to_string()),
        IrValue::Variant(tid, idx, _) => {
            if let IrTypeBody::Enum(variants) = &ctx.types[tid.0 as usize].body {
                variants.get(*idx as usize).and_then(plain_variant_name).map(|s| s.to_string())
            } else {
                None
            }
        }
        _                         => None,
    }
}

// ---------------------------------------------------------------------------
// Ref resolution (def-side)
//
// Each segment is resolved independently in the lexical scope.
// Keywords `pack` / `type` / `link` are kind-filters consumed without storing.
// Unresolvable segments become Plain(String).
// ---------------------------------------------------------------------------

fn resolve_ref(segments: &[AstNode<AstRefSeg>], ctx: &Ctx, scope: ScopeId) -> IrRef {
    let mut result    = Vec::new();
    let mut kind_hint: Option<&str> = None;

    for seg in segments {
        // Group segments ({inner:ref}) are not statically resolvable —
        // pass through as Plain preserving the source repr.
        let val = match seg.inner.as_plain() {
            Some(v) => v,
            None => {
                if let AstRefSegVal::Group(inner, trailing) = &seg.inner.value {
                    result.push(IrRefSeg {
                        value: IrRefSegValue::Plain(group_repr(inner, trailing.as_deref())),
                        is_opt: seg.inner.is_opt,
                    });
                }
                kind_hint = None;
                continue;
            }
        };

        match val {
            "pack" | "type" | "link" => {
                kind_hint = Some(val);
                continue;
            }
            _ => {}
        }

        let resolved = match kind_hint {
            Some("type") => ctx.lookup_type(scope, val)
                .map(IrRefSegValue::Type)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.to_string())),

            Some("link") => ctx.lookup_link(scope, val)
                .map(IrRefSegValue::Link)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.to_string())),

            Some("pack") => ctx.lookup_pack(scope, val)
                .map(IrRefSegValue::Pack)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.to_string())),

            _ => {
                // No hint — try type, then inst, then link, then pack; else plain.
                if let Some(id) = ctx.lookup_type(scope, val) {
                    IrRefSegValue::Type(id)
                } else if let Some(id) = ctx.lookup_inst(scope, val) {
                    IrRefSegValue::Inst(id)
                } else if let Some(id) = ctx.lookup_link(scope, val) {
                    IrRefSegValue::Link(id)
                } else if let Some(id) = ctx.lookup_pack(scope, val) {
                    IrRefSegValue::Pack(id)
                } else {
                    IrRefSegValue::Plain(val.to_string())
                }
            }
        };

        result.push(IrRefSeg { value: resolved, is_opt: seg.inner.is_opt });
        kind_hint = None;
    }

    IrRef { segments: result }
}

// ---------------------------------------------------------------------------
// Pass 1 — mirror scope tree
// ---------------------------------------------------------------------------

fn pass1_mirror_scopes(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    // scopes[0] is root — already created in Ctx::new().
    // IR scope IDs map 1-to-1 with AST scope IDs.
    for ast_scope in parse_scopes.iter().skip(1) {
        let parent = ast_scope.parent.map(|id| ScopeId(id.0)).unwrap_or(ScopeId(0));
        let kind   = match ast_scope.kind {
            ast::ScopeKind::Pack => IrScopeKind::Pack,
            ast::ScopeKind::Type => IrScopeKind::Type,
        };
        let name = ast_scope.name.as_ref().map(|n| n.inner.clone());

        let new_id = ScopeId(ctx.scopes.len() as u32);
        ctx.scopes.push(IrScope {
            kind,
            name:      name.clone(),
            parent:    Some(parent),
            types:     HashMap::new(),
            links:     HashMap::new(),
            insts:     HashMap::new(),
            packs:     HashMap::new(),
            ambiguous: HashSet::new(),
        });

        // Only register Pack scopes by name — Type scopes are registered
        // as type symbols by pass2.
        if kind == IrScopeKind::Pack {
            if let Some(n) = name {
                ctx.scopes[parent.0 as usize].packs.insert(n, new_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Import pass — process `use` statements
// ---------------------------------------------------------------------------
//
// Run twice:
//   ImportKind::TypesLinksAndPacks  — after pass2 (local types/links registered),
//                                     before pass4 (so imported types are visible
//                                     when instances look up their type name).
//   ImportKind::Insts               — after pass4 (local instances registered),
//                                     before pass5 (so imported instances are
//                                     visible in field value resolution).

#[derive(Clone, Copy, PartialEq)]
enum ImportKind { TypesLinksAndPacks, Insts }

fn pass_imports(parse_scopes: &[AstScope], ctx: &mut Ctx, kind: ImportKind) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstDef::Use(u) = def {
                let loc = ir_loc(&u.loc);
                resolve_use(&u.inner.path, scope, ctx, &loc, kind);
            }
        }
    }
}

fn resolve_use(path: &AstRef, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc, kind: ImportKind) {
    let segs = &path.segments;
    let mut i = 0;

    // Consume optional `pack` keyword.
    if segs.get(i).map(|s| s.inner.as_plain().unwrap_or("")) == Some("pack") {
        i += 1;
    }

    let pack_name = match segs.get(i) {
        Some(s) => s.inner.as_plain().unwrap_or("").to_string(),
        None => {
            if kind == ImportKind::TypesLinksAndPacks {
                ctx.errors.push(IrError { message: "use: expected pack name".into(), loc: loc.clone() });
            }
            return;
        }
    };
    i += 1;

    let src = match ctx.lookup_pack(into, &pack_name) {
        Some(s) => s,
        None => {
            if kind == ImportKind::TypesLinksAndPacks {
                ctx.errors.push(IrError {
                    message: format!("use: pack '{}' not found", pack_name),
                    loc: loc.clone(),
                });
            }
            return;
        }
    };

    // `use pack:std` — register the pack name itself, then done.
    if i >= segs.len() {
        if kind == ImportKind::TypesLinksAndPacks {
            try_import_pack(&pack_name, src, into, ctx, loc);
        }
        return;
    }

    // Parse optional kind hint.
    let kind_hint = match segs.get(i).map(|s| s.inner.as_plain().unwrap_or("")) {
        Some("type") => { i += 1; Some("type") }
        Some("link") => { i += 1; Some("link") }
        Some("inst") => { i += 1; Some("inst") }
        _ => None,
    };

    let name = match segs.get(i) {
        Some(s) => s.inner.as_plain().unwrap_or("").to_string(),
        None => {
            if kind == ImportKind::TypesLinksAndPacks {
                ctx.errors.push(IrError {
                    message: "use: expected name or '*' after kind specifier".into(),
                    loc: loc.clone(),
                });
            }
            return;
        }
    };

    if name == "*" {
        do_wildcard_import(src, into, kind_hint, ctx, loc, kind);
    } else {
        do_specific_import(&name, src, into, kind_hint, ctx, loc, kind);
    }
}

fn do_wildcard_import(
    src:       ScopeId,
    into:      ScopeId,
    kind_hint: Option<&str>,
    ctx:       &mut Ctx,
    loc:       &IrLoc,
    kind:      ImportKind,
) {
    let src_scope = ctx.scopes[src.0 as usize].clone();

    if kind == ImportKind::TypesLinksAndPacks {
        if kind_hint.is_none() || kind_hint == Some("type") {
            for (name, &tid) in &src_scope.types {
                try_import_type(name, tid, into, ctx, loc);
            }
        }
        if kind_hint.is_none() || kind_hint == Some("link") {
            for (name, &lid) in &src_scope.links {
                try_import_link(name, lid, into, ctx, loc);
            }
        }
        if kind_hint.is_none() {
            for (name, &sid) in &src_scope.packs {
                try_import_pack(name, sid, into, ctx, loc);
            }
        }
    }
    if kind == ImportKind::Insts {
        if kind_hint.is_none() || kind_hint == Some("inst") {
            for (name, &iid) in &src_scope.insts {
                try_import_inst(name, iid, into, ctx, loc);
            }
        }
    }
}

fn do_specific_import(
    name:      &str,
    src:       ScopeId,
    into:      ScopeId,
    kind_hint: Option<&str>,
    ctx:       &mut Ctx,
    loc:       &IrLoc,
    kind:      ImportKind,
) {
    let src_scope = ctx.scopes[src.0 as usize].clone();
    let mut found = false;

    if kind == ImportKind::TypesLinksAndPacks {
        if kind_hint.is_none() || kind_hint == Some("type") {
            if let Some(&tid) = src_scope.types.get(name) {
                try_import_type(name, tid, into, ctx, loc);
                found = true;
            }
        }
        if kind_hint.is_none() || kind_hint == Some("link") {
            if let Some(&lid) = src_scope.links.get(name) {
                try_import_link(name, lid, into, ctx, loc);
                found = true;
            }
        }
        if kind_hint.is_none() {
            if let Some(&sid) = src_scope.packs.get(name) {
                try_import_pack(name, sid, into, ctx, loc);
                found = true;
            }
        }
        // Only emit "not found" when this is not an inst-only kind hint,
        // AND we're not expecting to find it in the inst pass.
        if !found && kind_hint.is_some() && kind_hint != Some("inst") {
            ctx.errors.push(IrError {
                message: format!("use: '{}' not found in pack", name),
                loc: loc.clone(),
            });
        }
    }
    if kind == ImportKind::Insts {
        if kind_hint.is_none() || kind_hint == Some("inst") {
            if let Some(&iid) = src_scope.insts.get(name) {
                try_import_inst(name, iid, into, ctx, loc);
                found = true;
            }
        }
        // If kind_hint is inst-specific and nothing found, report error.
        if !found && kind_hint == Some("inst") {
            ctx.errors.push(IrError {
                message: format!("use: '{}' not found in pack", name),
                loc: loc.clone(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Import helpers — insert a single name into the destination scope,
// detecting and marking ambiguity if a conflict exists.
// ---------------------------------------------------------------------------

fn try_import_type(name: &str, tid: TypeId, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc) {
    if ctx.scopes[into.0 as usize].ambiguous.contains(name) { return; }
    // Types and links are resolved through separate lookup functions, so they
    // can share a name without ambiguity — only a type/type or type/pack clash counts.
    let conflict = ctx.scopes[into.0 as usize].types.contains_key(name)
        || ctx.scopes[into.0 as usize].packs.contains_key(name);
    if conflict {
        ctx.errors.push(IrError {
            message: format!("ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate", name),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
    } else {
        ctx.scopes[into.0 as usize].types.insert(name.to_string(), tid);
    }
}

fn try_import_link(name: &str, lid: LinkId, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc) {
    if ctx.scopes[into.0 as usize].ambiguous.contains(name) { return; }
    // Types and links are resolved through separate lookup functions, so they
    // can share a name without ambiguity — only a link/link or link/pack clash counts.
    let conflict = ctx.scopes[into.0 as usize].links.contains_key(name)
        || ctx.scopes[into.0 as usize].packs.contains_key(name);
    if conflict {
        ctx.errors.push(IrError {
            message: format!("ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate", name),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
    } else {
        ctx.scopes[into.0 as usize].links.insert(name.to_string(), lid);
    }
}

fn try_import_pack(name: &str, sid: ScopeId, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc) {
    if ctx.scopes[into.0 as usize].ambiguous.contains(name) { return; }
    let conflict = ctx.scopes[into.0 as usize].types.contains_key(name)
        || ctx.scopes[into.0 as usize].links.contains_key(name)
        || ctx.scopes[into.0 as usize].packs.contains_key(name);
    if conflict {
        ctx.errors.push(IrError {
            message: format!("ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate", name),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
    } else {
        ctx.scopes[into.0 as usize].packs.insert(name.to_string(), sid);
    }
}

fn try_import_inst(name: &str, iid: InstId, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc) {
    if ctx.scopes[into.0 as usize].ambiguous.contains(name) { return; }
    if ctx.scopes[into.0 as usize].insts.contains_key(name) {
        ctx.errors.push(IrError {
            message: format!("ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate", name),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
    } else {
        ctx.scopes[into.0 as usize].insts.insert(name.to_string(), iid);
    }
}

// ---------------------------------------------------------------------------
// Pass 2 — register type and link names (placeholder bodies)
// ---------------------------------------------------------------------------

fn pass2_register_names(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            match def {
                AstDef::Type(td) => {
                    if let Some(name_node) = &td.inner.name {
                        let id = TypeId(ctx.types.len() as u32);
                        ctx.scopes[scope.0 as usize].types.insert(name_node.inner.clone(), id);
                        ctx.types.push(IrTypeDef {
                            name:  Some(name_node.inner.clone()),
                            scope,
                            loc:   ir_loc(&td.loc),
                            body:  IrTypeBody::Enum(vec![]),  // placeholder
                        });
                    }
                }
                AstDef::Link(ld) => {
                    if let Some(name_node) = &ld.inner.name {
                        let id = LinkId(ctx.links.len() as u32);
                        ctx.scopes[scope.0 as usize].links.insert(name_node.inner.clone(), id);
                        ctx.links.push(IrLinkDef {
                            name:      Some(name_node.inner.clone()),
                            scope,
                            loc:       ir_loc(&ld.loc),
                            link_type: IrLinkType::Primitive(IrPrimitive::Reference),  // placeholder
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 3 — resolve type bodies and link types
// ---------------------------------------------------------------------------

fn pass3_resolve_types_and_links(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    // Collect all work upfront to avoid mid-iteration borrow issues.
    let mut type_work: Vec<(TypeId, AstNode<AstTypeDef>, ScopeId)> = vec![];
    let mut link_work: Vec<(LinkId, AstNode<AstLinkDef>, ScopeId)> = vec![];

    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            match def {
                AstDef::Type(td) => {
                    if let Some(name_node) = &td.inner.name {
                        if let Some(&tid) = ctx.scopes[scope.0 as usize].types.get(&name_node.inner) {
                            type_work.push((tid, td.clone(), scope));
                        }
                    }
                }
                AstDef::Link(ld) => {
                    if let Some(name_node) = &ld.inner.name {
                        if let Some(&lid) = ctx.scopes[scope.0 as usize].links.get(&name_node.inner) {
                            link_work.push((lid, ld.clone(), scope));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    for (tid, td, scope) in type_work {
        // Use the type's own scope for struct body resolution so that hoisted
        // sub-types and struct links land in the right place.
        let type_scope = td.inner.scope.map(|s| ScopeId(s.0)).unwrap_or(scope);
        let body       = resolve_type_body(&td.inner.body, ctx, type_scope);
        ctx.types[tid.0 as usize].body = body;
    }

    for (lid, ld, scope) in link_work {
        let lt = resolve_link_type(&ld.inner.ty.inner, ctx, scope, &ir_loc(&ld.loc));
        ctx.links[lid.0 as usize].link_type = lt;
    }
}

// ---------------------------------------------------------------------------
// Pass 3b — flatten single-variant enum type aliases
// ---------------------------------------------------------------------------
//
// `type loc = type:zone` parses as a 1-element enum whose sole variant is a
// ref to `zone`.  This pass rewrites such types to carry the same body as the
// referenced type, making the definition behave as a true type alias.
//
// Only non-struct targets are flattened: an alias pointing at a struct is
// left as-is so that typed-enum field matching continues to work correctly.
// Cycles (e.g. `type a = type:b`, `type b = type:a`) are detected via an
// in-stack flag and left unchanged.

fn flatten_alias(idx: usize, types: &mut Vec<IrTypeDef>, in_stack: &mut Vec<bool>) {
    if in_stack[idx] { return; }

    let body = types[idx].body.clone();
    if let IrTypeBody::Enum(ref variants) = body {
        if variants.len() == 1 {
            let segs = &variants[0].segments;
            if segs.len() == 1 && !segs[0].is_opt {
                if let IrRefSegValue::Type(tid) = segs[0].value {
                    let target_idx = tid.0 as usize;
                    if target_idx != idx {
                        in_stack[idx] = true;
                        flatten_alias(target_idx, types, in_stack);
                        in_stack[idx] = false;

                        let target_body = types[target_idx].body.clone();
                        if !matches!(target_body, IrTypeBody::Struct(_)) {
                            types[idx].body = target_body;
                        }
                    }
                }
            }
        }
    }
}

fn pass3b_flatten_alias_types(ctx: &mut Ctx) {
    let n = ctx.types.len();
    let mut in_stack = vec![false; n];
    for i in 0..n {
        flatten_alias(i, &mut ctx.types, &mut in_stack);
    }
}

fn resolve_type_body(body: &AstNode<AstTypeDefBody>, ctx: &mut Ctx, scope: ScopeId) -> IrTypeBody {
    match &body.inner {
        AstTypeDefBody::Primitive(p) => IrTypeBody::Primitive(convert_primitive(p)),

        AstTypeDefBody::Enum(refs) => {
            let variants = refs.iter()
                .map(|r| resolve_ref(&r.inner.segments, ctx, scope))
                .collect();
            IrTypeBody::Enum(variants)
        }

        AstTypeDefBody::Struct(items) => {
            IrTypeBody::Struct(resolve_struct_links(items, ctx, scope))
        }

        AstTypeDefBody::Ref(_) | AstTypeDefBody::List(_) => {
            ctx.errors.push(IrError {
                message: "ref/list body not valid for a named type definition".into(),
                loc: ir_loc(&body.loc),
            });
            IrTypeBody::Enum(vec![])
        }
    }
}

fn resolve_struct_links(items: &[AstNode<AstStructItem>], ctx: &mut Ctx, scope: ScopeId) -> Vec<LinkId> {
    let mut link_ids = Vec::new();

    for item in items {
        match &item.inner {
            AstStructItem::LinkDef(ld) => {
                let name = ld.inner.name.as_ref().map(|n| n.inner.clone());
                let loc  = ir_loc(&ld.loc);
                let lt   = resolve_link_type(&ld.inner.ty.inner, ctx, scope, &loc);
                let lid  = ctx.alloc_link(name, scope, loc, lt);
                link_ids.push(lid);
            }
            AstStructItem::TypeDef(_) => {
                // Inline type-defs are hoisted to the type scope by parse;
                // they are processed via pass2/pass3 on that scope's defs.
            }
        }
    }

    link_ids
}

fn resolve_link_type(td: &AstTypeDef, ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrLinkType {
    match &td.body.inner {
        AstTypeDefBody::Primitive(p) => IrLinkType::Primitive(convert_primitive(p)),

        AstTypeDefBody::List(inner) => {
            let elem_refs: Vec<AstRef> = match &inner.inner.body.inner {
                AstTypeDefBody::Ref(r)     => vec![r.clone()],
                AstTypeDefBody::Enum(refs) => refs.iter().map(|r| r.inner.clone()).collect(),
                _ => {
                    ctx.errors.push(IrError {
                        message: "list element type must be a ref or enum of refs".into(),
                        loc: loc.clone(),
                    });
                    return IrLinkType::Primitive(IrPrimitive::Reference);
                }
            };
            let ir_refs: Vec<IrRef> = elem_refs.iter().map(|r| {
                let ir_ref = resolve_ref(&r.segments, ctx, scope);
                for seg in &ir_ref.segments {
                    if !seg.is_opt {
                        if let IrRefSegValue::Plain(s) = &seg.value {
                            ctx.errors.push(IrError {
                                message: format!("unresolved type ref '{}'", s),
                                loc: loc.clone(),
                            });
                        }
                    }
                }
                ir_ref
            }).collect();
            IrLinkType::List(ir_refs)
        }

        AstTypeDefBody::Struct(items) => {
            // Inline named struct — resolve it and produce a single-segment Ref.
            let name = td.name.as_ref().map(|n| n.inner.clone());
            let type_scope = td.scope.map(|s| ScopeId(s.0)).unwrap_or(scope);
            let body = IrTypeBody::Struct(resolve_struct_links(items, ctx, type_scope));
            let tid  = ctx.alloc_type(name, type_scope, loc.clone(), body);
            IrLinkType::Ref(IrRef { segments: vec![IrRefSeg { value: IrRefSegValue::Type(tid), is_opt: false }] })
        }

        AstTypeDefBody::Ref(r) => {
            let ir_ref = resolve_ref(&r.segments, ctx, scope);
            for seg in &ir_ref.segments {
                if let IrRefSegValue::Plain(s) = &seg.value {
                    ctx.errors.push(IrError {
                        message: format!("unresolved type ref '{}'", s),
                        loc: loc.clone(),
                    });
                }
            }
            IrLinkType::Ref(ir_ref)
        }

        AstTypeDefBody::Enum(refs) => {
            // Anonymous inline enum — allocate an anonymous type.
            let variants = refs.iter()
                .map(|r| resolve_ref(&r.inner.segments, ctx, scope))
                .collect();
            let tid = ctx.alloc_type(None, scope, loc.clone(), IrTypeBody::Enum(variants));
            IrLinkType::Ref(IrRef { segments: vec![IrRefSeg { value: IrRefSegValue::Type(tid), is_opt: false }] })
        }
    }
}

fn convert_primitive(p: &AstPrimitive) -> IrPrimitive {
    match p {
        AstPrimitive::String    => IrPrimitive::String,
        AstPrimitive::Integer   => IrPrimitive::Integer,
        AstPrimitive::Reference => IrPrimitive::Reference,
    }
}

// ---------------------------------------------------------------------------
// Pass 4 — register instance names
// ---------------------------------------------------------------------------

fn lookup_type_by_ref(ref_: &AstRef, ctx: &Ctx, scope: ScopeId) -> Option<TypeId> {
    let segs = &ref_.segments;
    match segs.len() {
        0 => None,
        1 => ctx.lookup_type(scope, segs[0].inner.as_plain()?),
        _ => {
            let mut cur = scope;
            for seg in &segs[..segs.len() - 1] {
                cur = ctx.lookup_pack(cur, seg.inner.as_plain()?)?;
            }
            ctx.lookup_type(cur, segs.last().unwrap().inner.as_plain()?)
        }
    }
}

fn pass4_register_insts(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstDef::Inst(inst) = def {
                let type_ref = &inst.inner.type_name.inner;
                let type_id  = match lookup_type_by_ref(type_ref, ctx, scope) {
                    Some(tid) => tid,
                    None => {
                        let ref_name = type_ref.segments.iter()
                            .map(|s| s.inner.as_plain().unwrap_or("{...}"))
                            .collect::<Vec<_>>().join(":");
                        ctx.errors.push(IrError {
                            message: format!("unknown type '{}'", ref_name),
                            loc: ir_loc(&inst.inner.type_name.loc),
                        });
                        continue;
                    }
                };
                ctx.alloc_inst(inst.inner.inst_name.inner.clone(), scope, ir_loc(&inst.loc), type_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 5 — resolve instance fields
// ---------------------------------------------------------------------------

fn pass5_resolve_inst_fields(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    let mut work: Vec<(InstId, Vec<AstNode<AstField>>, ScopeId)> = vec![];

    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstDef::Inst(inst) = def {
                let name = &inst.inner.inst_name.inner;
                if let Some(iid) = ctx.lookup_inst(scope, name) {
                    work.push((iid, inst.inner.fields.clone(), scope));
                }
            }
        }
    }

    for (iid, fields, scope) in work {
        let type_id  = ctx.insts[iid.0 as usize].type_id;
        let link_ids = match ctx.types.get(type_id.0 as usize).map(|t| &t.body) {
            Some(IrTypeBody::Struct(ids)) => ids.clone(),
            _                             => vec![],
        };

        // Collect anonymous field values for the unnamed link (if any).
        let anon_link = link_ids.iter().find(|&&lid| ctx.links[lid.0 as usize].name.is_none()).copied();
        let anon_vals: Vec<&AstNode<AstValue>> = fields.iter().filter_map(|af| {
            if let AstField::Anon(v) = &af.inner { Some(v) } else { None }
        }).collect();

        let mut resolved: Vec<IrField> = resolve_named_fields(&fields, &link_ids, ctx, scope);

        // Resolve anonymous values against the unnamed link.
        if let (Some(lid), false) = (anon_link, anon_vals.is_empty()) {
            let link_type = ctx.links[lid.0 as usize].link_type.clone();
            let loc       = ir_loc(&anon_vals[0].loc);
            match &link_type {
                IrLinkType::List(patterns) => {
                    let items = anon_vals.iter()
                        .map(|v| resolve_list_item(&v.inner, patterns, ctx, scope, &ir_loc(&v.loc)))
                        .collect();
                    resolved.push(IrField { link_id: lid, name: "_".into(), loc, value: IrValue::List(items) });
                }
                _ => {
                    if anon_vals.len() > 1 {
                        ctx.errors.push(IrError {
                            message: "multiple values defined for a non-List field '_'".into(),
                            loc: loc.clone(),
                        });
                    } else {
                        let ir_val = resolve_value(&anon_vals[0].inner, &link_type, ctx, scope, &loc);
                        resolved.push(IrField { link_id: lid, name: "_".into(), loc, value: ir_val });
                    }
                }
            }
        }

        ctx.insts[iid.0 as usize].fields = resolved;
    }
}

// ---------------------------------------------------------------------------
// Named field resolution (shared by pass5 and inline struct values)
// ---------------------------------------------------------------------------

fn resolve_named_fields(
    ast_fields: &[AstNode<AstField>],
    link_ids:   &[LinkId],
    ctx:        &mut Ctx,
    scope:      ScopeId,
) -> Vec<IrField> {
    // Group named fields by link, preserving first-occurrence order.
    let mut groups: Vec<(String, IrLoc, LinkId, Vec<&AstNode<AstValue>>)> = Vec::new();

    for af in ast_fields {
        let AstField::Named { name, value } = &af.inner else { continue; };
        let field_name = &name.inner;
        let loc        = ir_loc(&name.loc);

        let lid = match link_ids.iter().find(|&&lid| {
            ctx.links[lid.0 as usize].name.as_deref() == Some(field_name)
        }) {
            Some(&lid) => lid,
            None => {
                ctx.errors.push(IrError {
                    message: format!("unknown field '{}'", field_name),
                    loc: loc.clone(),
                });
                continue;
            }
        };

        if let Some(group) = groups.iter_mut().find(|g| g.2 == lid) {
            group.3.push(value);
        } else {
            groups.push((field_name.clone(), loc, lid, vec![value]));
        }
    }

    // Pass 1: resolve plain (non-group) fields to build the full this_fields map.
    // Field order in instances does not affect {this:xxx} reduction.
    let mut this_fields: HashMap<String, String> = HashMap::new();
    let mut pre_resolved: Vec<Option<IrField>> = (0..groups.len()).map(|_| None).collect();

    for (i, (field_name, loc, lid, values)) in groups.iter().enumerate() {
        if values.iter().any(|v| matches!(&v.inner, AstValue::Ref(r) if has_group(r))) { continue; }

        let link_type = ctx.links[lid.0 as usize].link_type.clone();
        let opt_val: Option<IrValue> = if values.len() > 1 {
            match &link_type {
                IrLinkType::List(patterns) => {
                    let items = values.iter()
                        .map(|v| resolve_list_item(&v.inner, patterns, ctx, scope, &ir_loc(&v.loc)))
                        .collect();
                    Some(IrValue::List(items))
                }
                _ => {
                    ctx.errors.push(IrError {
                        message: format!("multiple values defined for a non-List field '{}'", field_name),
                        loc: loc.clone(),
                    });
                    None
                }
            }
        } else {
            Some(resolve_value(&values[0].inner, &link_type, ctx, scope, &ir_loc(&values[0].loc)))
        };

        if let Some(ir_val) = opt_val {
            if let Some(plain) = ir_value_to_plain_str(&ir_val, ctx) {
                this_fields.insert(field_name.clone(), plain);
            }
            pre_resolved[i] = Some(IrField { link_id: *lid, name: field_name.clone(), loc: loc.clone(), value: ir_val });
        }
    }

    // Pass 2: reduce group fields using the full this_fields, resolve in source order.
    groups.into_iter().enumerate().filter_map(|(i, (field_name, loc, lid, values))| {
        if let Some(cached) = pre_resolved[i].take() {
            return Some(cached);
        }

        let link_type = ctx.links[lid.0 as usize].link_type.clone();
        let reduced: Vec<AstNode<AstValue>> = values.iter().map(|v| {
            if let AstValue::Ref(r) = &v.inner {
                if has_group(r) {
                    return AstNode { loc: v.loc.clone(), inner: AstValue::Ref(reduce_ast_ref(r, &this_fields)) };
                }
            }
            (*v).clone()
        }).collect();

        let ir_val = if reduced.len() > 1 {
            match &link_type {
                IrLinkType::List(patterns) => {
                    let items = reduced.iter()
                        .map(|v| resolve_list_item(&v.inner, patterns, ctx, scope, &ir_loc(&v.loc)))
                        .collect();
                    IrValue::List(items)
                }
                _ => {
                    ctx.errors.push(IrError {
                        message: format!("multiple values defined for a non-List field '{}'", field_name),
                        loc: loc.clone(),
                    });
                    return None;
                }
            }
        } else {
            resolve_value(&reduced[0].inner, &link_type, ctx, scope, &ir_loc(&reduced[0].loc))
        };

        Some(IrField { link_id: lid, name: field_name, loc, value: ir_val })
    }).collect()
}

// ---------------------------------------------------------------------------
// Value resolution (guided by IrLinkType pattern)
// ---------------------------------------------------------------------------

fn resolve_value(v: &AstValue, link_type: &IrLinkType, ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrValue {
    // Unresolved Group segments cannot be statically resolved — pass through as Ref.
    if let AstValue::Ref(r) = v {
        if has_group(r) { return IrValue::Ref(ref_to_repr(r)); }
    }

    match link_type {
        IrLinkType::Primitive(IrPrimitive::Integer) => {
            let s = match v {
                AstValue::Ref(r) => r.segments[0].inner.as_plain().unwrap_or("").to_string(),
                AstValue::Str(s) => s.clone(),
                _ => { ctx.errors.push(IrError { message: "expected integer".into(), loc: loc.clone() }); return IrValue::Int(0); }
            };
            match s.parse::<i64>() {
                Ok(n)  => IrValue::Int(n),
                Err(_) => { ctx.errors.push(IrError { message: format!("expected integer, got '{}'", s), loc: loc.clone() }); IrValue::Int(0) }
            }
        }

        IrLinkType::Primitive(IrPrimitive::String) => {
            match v {
                AstValue::Str(s) => IrValue::Str(s.clone()),
                AstValue::Ref(r) => IrValue::Str(r.segments[0].inner.as_plain().unwrap_or("").to_string()),
                _ => { ctx.errors.push(IrError { message: "expected string".into(), loc: loc.clone() }); IrValue::Str(String::new()) }
            }
        }

        IrLinkType::Primitive(IrPrimitive::Reference) => {
            match v {
                AstValue::Str(s) => IrValue::Ref(s.clone()),
                AstValue::Ref(r) => IrValue::Ref(
                    r.segments.iter().map(|s| s.inner.as_plain().unwrap_or("")).collect::<Vec<_>>().join(":")
                ),
                _ => { ctx.errors.push(IrError { message: "expected reference".into(), loc: loc.clone() }); IrValue::Ref(String::new()) }
            }
        }

        IrLinkType::Ref(pattern) => {
            // Inline struct literal `{ field: value ... }` — allocate an anonymous instance.
            if let AstValue::Struct { type_hint, fields: ast_fields } = v {
                if pattern.segments.len() == 1 {
                    if let IrRefSegValue::Type(tid) = &pattern.segments[0].value {
                        let type_id  = *tid;
                        match ctx.types.get(type_id.0 as usize).map(|t| t.body.clone()) {
                            Some(IrTypeBody::Struct(link_ids)) => {
                                // Validate and extract type hint if present.
                                let resolved_hint = if let Some(hint) = type_hint {
                                    let hint_name = hint.inner.segments.last()
                                        .and_then(|s| s.inner.as_plain())
                                        .unwrap_or("");
                                    if let Some(hint_tid) = ctx.lookup_type(scope, hint_name) {
                                        if hint_tid != type_id {
                                            let expected = ctx.types[type_id.0 as usize].name.clone()
                                                .unwrap_or_else(|| "?".into());
                                            ctx.push_error(format!(
                                                "type hint '{}' does not match expected type '{}'",
                                                hint_name, expected
                                            ));
                                        }
                                    } else {
                                        ctx.push_error(format!("unknown type '{}' in type hint", hint_name));
                                    }
                                    Some(hint_name.to_string())
                                } else {
                                    None
                                };
                                let iid = InstId(ctx.insts.len() as u32);
                                ctx.insts.push(IrInstDef {
                                    type_id,
                                    name: "_".into(),
                                    type_hint: resolved_hint,
                                    scope,
                                    loc: loc.clone(),
                                    fields: vec![],
                                });
                                let fields = resolve_named_fields(ast_fields, &link_ids, ctx, scope);
                                ctx.insts[iid.0 as usize].fields = fields;
                                return IrValue::Inst(iid);
                            }
                            Some(IrTypeBody::Enum(variants)) => {
                                // Struct literal against an enum type — hint identifies the variant.
                                let hint = match type_hint {
                                    None => {
                                        let enum_name = ctx.types[type_id.0 as usize].name.as_deref()
                                            .unwrap_or("?");
                                        ctx.push_error(format!(
                                            "type hint required for enum '{}'", enum_name
                                        ));
                                        return IrValue::Ref(String::new());
                                    }
                                    Some(h) => h,
                                };
                                let hint_name = hint.inner.segments.last()
                                    .and_then(|s| s.inner.as_plain())
                                    .unwrap_or("");
                                let hint_tid = match ctx.lookup_type(scope, hint_name) {
                                    None => {
                                        ctx.push_error(format!("unknown type '{}' in type hint", hint_name));
                                        return IrValue::Ref(String::new());
                                    }
                                    Some(t) => t,
                                };
                                let variant_idx = variants.iter().enumerate().find_map(|(i, r)| {
                                    if let [seg] = r.segments.as_slice() {
                                        if let IrRefSegValue::Type(vt) = &seg.value {
                                            if *vt == hint_tid { return Some(i); }
                                        }
                                    }
                                    None
                                });
                                let idx = match variant_idx {
                                    None => {
                                        let enum_name = ctx.types[type_id.0 as usize].name.as_deref()
                                            .unwrap_or("?");
                                        ctx.push_error(format!(
                                            "'{}' is not a variant of '{}'", hint_name, enum_name
                                        ));
                                        return IrValue::Ref(String::new());
                                    }
                                    Some(i) => i,
                                };
                                let inner_link_ids = match ctx.types.get(hint_tid.0 as usize)
                                    .map(|t| t.body.clone())
                                {
                                    Some(IrTypeBody::Struct(ids)) => ids,
                                    _ => {
                                        ctx.push_error(format!(
                                            "variant '{}' is not a struct type", hint_name
                                        ));
                                        return IrValue::Ref(String::new());
                                    }
                                };
                                let iid = InstId(ctx.insts.len() as u32);
                                ctx.insts.push(IrInstDef {
                                    type_id: hint_tid,
                                    name: "_".into(),
                                    type_hint: Some(hint_name.to_string()),
                                    scope,
                                    loc: loc.clone(),
                                    fields: vec![],
                                });
                                let fields = resolve_named_fields(ast_fields, &inner_link_ids, ctx, scope);
                                ctx.insts[iid.0 as usize].fields = fields;
                                return IrValue::Variant(type_id, idx as u32, Some(Box::new(IrValue::Inst(iid))));
                            }
                            _ => {
                                ctx.push_error("inline struct value requires a struct-typed link".into());
                                return IrValue::Ref(String::new());
                            }
                        };
                    }
                }
                ctx.push_error("inline struct value only valid for single struct-typed link".into());
                return IrValue::Ref(String::new());
            }
            resolve_value_against_ref(v, pattern, ctx, scope, loc)
        }

        IrLinkType::List(elem_patterns) => {
            match v {
                AstValue::List(items) => {
                    let vals = items.iter()
                        .map(|item| resolve_list_item(&item.inner, elem_patterns, ctx, scope, &ir_loc(&item.loc)))
                        .collect();
                    IrValue::List(vals)
                }
                _ => { ctx.errors.push(IrError { message: "expected list".into(), loc: loc.clone() }); IrValue::List(vec![]) }
            }
        }
    }
}

fn resolve_value_against_ref(v: &AstValue, pattern: &IrRef, ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrValue {
    let segs = match v {
        AstValue::Ref(r) => {
            if has_group(r) { return IrValue::Ref(ref_to_repr(r)); }
            &r.segments
        }
        AstValue::Str(s) => return IrValue::Ref(s.clone()),
        _ => { ctx.errors.push(IrError { message: "expected ref value".into(), loc: loc.clone() }); return IrValue::Ref(String::new()); }
    };

    if pattern.segments.len() == 1 {
        return resolve_single_seg_value(segs[0].inner.as_plain().unwrap_or(""), &pattern.segments[0], ctx, scope, loc);
    }

    // Multi-segment typed path — segment counts must match.
    if segs.len() != pattern.segments.len() {
        ctx.errors.push(IrError {
            message: format!(
                "typed path has {} segment(s), expected {}",
                segs.len(), pattern.segments.len()
            ),
            loc: loc.clone(),
        });
    }

    let vals: Vec<IrValue> = pattern.segments.iter().zip(segs.iter()).map(|(pat_seg, val_seg)| {
        resolve_single_seg_value(val_seg.inner.as_plain().unwrap_or(""), pat_seg, ctx, scope, loc)
    }).collect();
    IrValue::Path(vals)
}

fn resolve_single_seg_value(raw: &str, pat_seg: &IrRefSeg, ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrValue {
    match &pat_seg.value {
        IrRefSegValue::Type(tid) => {
            match ctx.types.get(tid.0 as usize).map(|t| t.body.clone()) {
                Some(IrTypeBody::Enum(variants)) => {
                    // Try each variant in order: plain string match, then typed (instance lookup).
                    for (idx, variant_ref) in variants.iter().enumerate() {
                        if let [seg] = variant_ref.segments.as_slice() {
                            match &seg.value {
                                IrRefSegValue::Plain(name) if name == raw => {
                                    return IrValue::Variant(*tid, idx as u32, None);
                                }
                                IrRefSegValue::Type(inner_tid) => {
                                    if let Some(iid) = ctx.lookup_inst(scope, raw) {
                                        if ctx.insts[iid.0 as usize].type_id == *inner_tid {
                                            return IrValue::Variant(*tid, idx as u32, Some(Box::new(IrValue::Inst(iid))));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    let type_name = ctx.types[tid.0 as usize].name.as_deref().unwrap_or("?");
                    ctx.errors.push(IrError {
                        message: format!("'{}' is not a variant of '{}'", raw, type_name),
                        loc: loc.clone(),
                    });
                    IrValue::Variant(*tid, 0, None)
                }
                Some(IrTypeBody::Struct(_)) => {
                    match ctx.lookup_inst(scope, raw) {
                        Some(iid) => IrValue::Inst(iid),
                        None => {
                            ctx.errors.push(IrError {
                                message: format!("'{}' is not a known instance of Type#{}", raw, tid.0),
                                loc: loc.clone(),
                            });
                            IrValue::Ref(raw.to_string())
                        }
                    }
                }
                Some(IrTypeBody::Primitive(IrPrimitive::Integer)) => {
                    match raw.parse::<i64>() {
                        Ok(n)  => IrValue::Int(n),
                        Err(_) => { ctx.errors.push(IrError { message: format!("expected integer, got '{}'", raw), loc: loc.clone() }); IrValue::Int(0) }
                    }
                }
                Some(IrTypeBody::Primitive(_)) => IrValue::Str(raw.to_string()),
                None => IrValue::Ref(raw.to_string()),
            }
        }
        IrRefSegValue::Inst(iid) => IrValue::Inst(*iid),
        _                        => IrValue::Ref(raw.to_string()),
    }
}

fn resolve_list_item(v: &AstValue, patterns: &[IrRef], ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrValue {
    let AstValue::Ref(r) = v else {
        ctx.errors.push(IrError { message: "list item must be a reference".into(), loc: loc.clone() });
        return IrValue::Ref(String::new());
    };
    if has_group(r) { return IrValue::Ref(ref_to_repr(r)); }

    // For typed-path values like `service:api`, the first segment is a type qualifier
    // and the last segment is the actual instance name.
    let is_typed_path = r.segments.len() > 1
        && r.segments[0].inner.as_plain().map_or(false, |v| ctx.lookup_type(scope, v).is_some());
    let inst_name = if is_typed_path {
        r.segments.last().unwrap().inner.as_plain().unwrap_or("")
    } else {
        r.segments[0].inner.as_plain().unwrap_or("")
    };

    // Find which element pattern matches this instance's type.
    let matched_pattern = patterns.iter().find(|pattern| {
        if let Some(base_seg) = pattern.segments.first() {
            if let IrRefSegValue::Type(tid) = &base_seg.value {
                if let Some(iid) = ctx.lookup_inst(scope, inst_name) {
                    return ctx.insts[iid.0 as usize].type_id == *tid;
                }
            }
        }
        false
    }).or_else(|| patterns.first());

    // For typed-path values against a single-segment pattern, resolve using just the instance name.
    if is_typed_path {
        if let Some(pattern) = matched_pattern {
            if pattern.segments.len() == 1 {
                return resolve_single_seg_value(inst_name, &pattern.segments[0], ctx, scope, loc);
            }
        }
    }

    match matched_pattern {
        Some(pattern) => resolve_value_against_ref(v, pattern, ctx, scope, loc),
        None          => IrValue::Ref(String::new()),
    }
}

// ---------------------------------------------------------------------------
// Pass 6 — resolve deploys
// ---------------------------------------------------------------------------

fn pass6_resolve_deploys(parse_scopes: &[AstScope], ctx: &mut Ctx) -> Vec<IrDeployDef> {
    let mut deploys = Vec::new();

    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstDef::Deploy(dep) = def {
                let what   = resolve_ref(&dep.inner.what.inner.segments,   ctx, scope);
                let target = resolve_ref(&dep.inner.target.inner.segments, ctx, scope);
                let name   = resolve_ref(&dep.inner.name.inner.segments,   ctx, scope);
                let loc    = ir_loc(&dep.loc);

                let fields = dep.inner.fields.iter().filter_map(|af| {
                    let AstField::Named { name: fname, value } = &af.inner else { return None; };
                    let field_name = &fname.inner;
                    let floc       = ir_loc(&fname.loc);

                    let (lid, link_type) = match ctx.lookup_link(scope, field_name) {
                        Some(lid) => {
                            let lt = ctx.links[lid.0 as usize].link_type.clone();
                            (lid, lt)
                        }
                        None => {
                            ctx.errors.push(IrError {
                                message: format!("unknown link '{}'", field_name),
                                loc: floc.clone(),
                            });
                            return None;
                        }
                    };

                    let ir_val = resolve_value(&value.inner, &link_type, ctx, scope, &ir_loc(&value.loc));
                    Some(IrField { link_id: lid, name: field_name.clone(), loc: floc, value: ir_val })
                }).collect();

                deploys.push(IrDeployDef { what, target, name, loc, fields });
            }
        }
    }

    deploys
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn resolve(res: ParseRes) -> IrRes {
    let mut ctx = Ctx::new();

    pass1_mirror_scopes(&res.scopes, &mut ctx);
    pass2_register_names(&res.scopes, &mut ctx);
    pass_imports(&res.scopes, &mut ctx, ImportKind::TypesLinksAndPacks);
    pass4_register_insts(&res.scopes, &mut ctx);
    pass3_resolve_types_and_links(&res.scopes, &mut ctx);
    pass3b_flatten_alias_types(&mut ctx);
    pass_imports(&res.scopes, &mut ctx, ImportKind::Insts);
    pass5_resolve_inst_fields(&res.scopes, &mut ctx);

    let deploys = pass6_resolve_deploys(&res.scopes, &mut ctx);

    let mut errors = ctx.errors;
    errors.extend(res.errors.iter().map(|e| IrError {
        message: e.message.clone(),
        loc:     IrLoc { unit: e.loc.unit, start: e.loc.start, end: e.loc.end },
    }));

    IrRes { types: ctx.types, links: ctx.links, insts: ctx.insts, deploys, scopes: ctx.scopes, errors }
}
