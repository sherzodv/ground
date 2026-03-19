/// Resolve pass: `ParseRes` → `IrRes`.
///
/// Passes:
///   1. Mirror scope tree from parse arena.
///   2. Register type/link names in their scopes.
///   3. Resolve type bodies and link types.
///   4. Register instance names (enables forward references).
///   5. Resolve instance fields (validated against link type patterns).
///   6. Resolve deploys.
use std::collections::HashMap;

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
            kind:   IrScopeKind::Pack,
            name:   None,
            parent: None,
            types:  HashMap::new(),
            links:  HashMap::new(),
            insts:  HashMap::new(),
            packs:  HashMap::new(),
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
        self.insts.push(IrInstDef { type_id, name, scope, loc, fields: vec![] });
        id
    }

    fn push_error(&mut self, message: String) {
        self.errors.push(IrError { message, loc: IrLoc { unit: 0, start: 0, end: 0 } });
    }

    fn lookup_type(&self, scope: ScopeId, name: &str) -> Option<TypeId> {
        let s = &self.scopes[scope.0 as usize];
        if let Some(&id) = s.types.get(name) { return Some(id); }
        s.parent.and_then(|p| self.lookup_type(p, name))
    }

    fn lookup_link(&self, scope: ScopeId, name: &str) -> Option<LinkId> {
        let s = &self.scopes[scope.0 as usize];
        if let Some(&id) = s.links.get(name) { return Some(id); }
        s.parent.and_then(|p| self.lookup_link(p, name))
    }

    fn lookup_inst(&self, scope: ScopeId, name: &str) -> Option<InstId> {
        let s = &self.scopes[scope.0 as usize];
        if let Some(&id) = s.insts.get(name) { return Some(id); }
        s.parent.and_then(|p| self.lookup_inst(p, name))
    }

    fn lookup_pack(&self, scope: ScopeId, name: &str) -> Option<ScopeId> {
        let s = &self.scopes[scope.0 as usize];
        if let Some(&id) = s.packs.get(name) { return Some(id); }
        s.parent.and_then(|p| self.lookup_pack(p, name))
    }
}

fn ir_loc(loc: &AstNodeLoc) -> IrLoc {
    IrLoc { unit: loc.unit, start: loc.start, end: loc.end }
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
        let val = &seg.inner.value;

        match val.as_str() {
            "pack" | "type" | "link" => {
                kind_hint = Some(val.as_str());
                continue;
            }
            _ => {}
        }

        let resolved = match kind_hint {
            Some("type") => ctx.lookup_type(scope, val)
                .map(IrRefSegValue::Type)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.clone())),

            Some("link") => ctx.lookup_link(scope, val)
                .map(IrRefSegValue::Link)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.clone())),

            Some("pack") => ctx.lookup_pack(scope, val)
                .map(IrRefSegValue::Pack)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.clone())),

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
                    IrRefSegValue::Plain(val.clone())
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
            name: name.clone(),
            parent: Some(parent),
            types:  HashMap::new(),
            links:  HashMap::new(),
            insts:  HashMap::new(),
            packs:  HashMap::new(),
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

fn resolve_type_body(body: &AstNode<AstTypeDefBody>, ctx: &mut Ctx, scope: ScopeId) -> IrTypeBody {
    match &body.inner {
        AstTypeDefBody::Primitive(p) => IrTypeBody::Primitive(convert_primitive(p)),

        AstTypeDefBody::Enum(refs) => {
            let variants = refs.iter()
                .map(|r| r.inner.segments[0].inner.value.clone())
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
                .map(|r| r.inner.segments[0].inner.value.clone())
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

fn pass4_register_insts(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstDef::Inst(inst) = def {
                let type_name = &inst.inner.type_name.inner;
                let type_id   = match ctx.lookup_type(scope, type_name) {
                    Some(tid) => tid,
                    None => {
                        ctx.errors.push(IrError {
                            message: format!("unknown type '{}'", type_name),
                            loc: ir_loc(&inst.inner.type_name.loc),
                        });
                        continue;  // skip this instance rather than using a bad TypeId
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
            let ir_val    = match &link_type {
                IrLinkType::List(patterns) => {
                    let items = anon_vals.iter()
                        .map(|v| resolve_list_item(&v.inner, patterns, ctx, scope, &ir_loc(&v.loc)))
                        .collect();
                    IrValue::List(items)
                }
                _ => resolve_value(&anon_vals[0].inner, &link_type, ctx, scope, &loc),
            };
            resolved.push(IrField { link_id: lid, name: "_".into(), loc, value: ir_val });
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
    ast_fields.iter().filter_map(|af| {
        let AstField::Named { name, value } = &af.inner else { return None; };
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
                return None;
            }
        };

        let link_type = ctx.links[lid.0 as usize].link_type.clone();
        let ir_val    = resolve_value(&value.inner, &link_type, ctx, scope, &ir_loc(&value.loc));
        Some(IrField { link_id: lid, name: field_name.clone(), loc, value: ir_val })
    }).collect()
}

// ---------------------------------------------------------------------------
// Value resolution (guided by IrLinkType pattern)
// ---------------------------------------------------------------------------

fn resolve_value(v: &AstValue, link_type: &IrLinkType, ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrValue {
    match link_type {
        IrLinkType::Primitive(IrPrimitive::Integer) => {
            let s = match v {
                AstValue::Ref(r) => r.segments[0].inner.value.clone(),
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
                AstValue::Ref(r) => IrValue::Str(r.segments[0].inner.value.clone()),
                _ => { ctx.errors.push(IrError { message: "expected string".into(), loc: loc.clone() }); IrValue::Str(String::new()) }
            }
        }

        IrLinkType::Primitive(IrPrimitive::Reference) => {
            match v {
                AstValue::Str(s) => IrValue::Ref(s.clone()),
                AstValue::Ref(r) => IrValue::Ref(
                    r.segments.iter().map(|s| s.inner.value.as_str()).collect::<Vec<_>>().join(":")
                ),
                _ => { ctx.errors.push(IrError { message: "expected reference".into(), loc: loc.clone() }); IrValue::Ref(String::new()) }
            }
        }

        IrLinkType::Ref(pattern) => {
            // Inline struct literal `{ field: value ... }` — allocate an anonymous instance.
            if let AstValue::Struct(ast_fields) = v {
                if pattern.segments.len() == 1 {
                    if let IrRefSegValue::Type(tid) = &pattern.segments[0].value {
                        let type_id  = *tid;
                        let link_ids = match ctx.types.get(type_id.0 as usize).map(|t| t.body.clone()) {
                            Some(IrTypeBody::Struct(ids)) => ids,
                            _ => {
                                ctx.push_error("inline struct value requires a struct-typed link".into());
                                return IrValue::Ref(String::new());
                            }
                        };
                        let iid = InstId(ctx.insts.len() as u32);
                        ctx.insts.push(IrInstDef { type_id, name: "_".into(), scope, loc: loc.clone(), fields: vec![] });
                        let fields = resolve_named_fields(ast_fields, &link_ids, ctx, scope);
                        ctx.insts[iid.0 as usize].fields = fields;
                        return IrValue::Inst(iid);
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
        AstValue::Ref(r) => &r.segments,
        AstValue::Str(s) => return IrValue::Ref(s.clone()),
        _ => { ctx.errors.push(IrError { message: "expected ref value".into(), loc: loc.clone() }); return IrValue::Ref(String::new()); }
    };

    if pattern.segments.len() == 1 {
        return resolve_single_seg_value(&segs[0].inner.value, &pattern.segments[0], ctx, scope, loc);
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
        resolve_single_seg_value(&val_seg.inner.value, pat_seg, ctx, scope, loc)
    }).collect();
    IrValue::Path(vals)
}

fn resolve_single_seg_value(raw: &str, pat_seg: &IrRefSeg, ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrValue {
    match &pat_seg.value {
        IrRefSegValue::Type(tid) => {
            match ctx.types.get(tid.0 as usize).map(|t| t.body.clone()) {
                Some(IrTypeBody::Enum(variants)) => {
                    match variants.iter().position(|v| v == raw) {
                        Some(idx) => IrValue::Variant(*tid, idx as u32),
                        None => {
                            ctx.errors.push(IrError {
                                message: format!("'{}' is not a variant of Type#{}", raw, tid.0),
                                loc: loc.clone(),
                            });
                            IrValue::Variant(*tid, 0)
                        }
                    }
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

    // For typed-path values like `service:api`, the first segment is a type qualifier
    // and the last segment is the actual instance name.
    let is_typed_path = r.segments.len() > 1
        && ctx.lookup_type(scope, &r.segments[0].inner.value).is_some();
    let inst_name = if is_typed_path {
        r.segments.last().unwrap().inner.value.as_str()
    } else {
        r.segments[0].inner.value.as_str()
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
    pass3_resolve_types_and_links(&res.scopes, &mut ctx);
    pass4_register_insts(&res.scopes, &mut ctx);
    pass5_resolve_inst_fields(&res.scopes, &mut ctx);

    let deploys = pass6_resolve_deploys(&res.scopes, &mut ctx);

    let mut errors = ctx.errors;
    errors.extend(res.errors.iter().map(|e| IrError {
        message: e.message.clone(),
        loc:     IrLoc { unit: e.loc.unit, start: e.loc.start, end: e.loc.end },
    }));

    IrRes { types: ctx.types, links: ctx.links, insts: ctx.insts, deploys, scopes: ctx.scopes, errors }
}
