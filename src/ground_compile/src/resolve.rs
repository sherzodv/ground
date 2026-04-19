/// Resolve pass: `ParseRes` → `IrRes`.
///
/// Passes:
///   1. Mirror scope tree from parse arena.
///   2. Register type/link names in their scopes.
///   3. Resolve type bodies and link types.
///   4. Register instance names (enables forward references).
///   5. Resolve instance fields (validated against link type patterns).
///   6. Resolve deploys.
///   7. Resolve type function definitions.
use std::collections::{HashMap, HashSet};

use crate::ast::{self, *};
use crate::ir::*;
use crate::ir::ScopeKind as IrScopeKind;

// ---------------------------------------------------------------------------
// Resolver context
// ---------------------------------------------------------------------------

struct Ctx {
    types:    Vec<IrTypeDef>,
    links:    Vec<IrLinkDef>,
    funs:     Vec<IrFunDef>,
    type_fns: Vec<IrTypeFnDef>,
    scopes:   Vec<IrScope>,
    errors:   Vec<IrError>,
}

impl Ctx {
    fn new() -> Self {
        let root = IrScope {
            kind:           IrScopeKind::Pack,
            name:           None,
            parent:         None,
            types:          HashMap::new(),
            links:          HashMap::new(),
            funs:           HashMap::new(),
            packs:          HashMap::new(),
            type_fns:       HashMap::new(),
            anon_type_fns:  HashMap::new(),
            anon_pair_fns:  HashMap::new(),
            ambiguous:      HashSet::new(),
            ts_fns:         HashSet::new(),
        };
        Ctx { types: vec![], links: vec![], funs: vec![], type_fns: vec![], scopes: vec![root], errors: vec![] }
    }

    fn scope_has_ts_fn(&self, scope: ScopeId, name: &str) -> bool {
        let s = &self.scopes[scope.0 as usize];
        if s.ts_fns.contains(name) { return true; }
        s.parent.map_or(false, |p| self.scope_has_ts_fn(p, name))
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

    /// Allocate a struct-body field link without registering it in the scope's
    /// link name map.  Struct field resolution walks the type's `link_ids` list
    /// directly (not the scope map), so registration is not needed — and
    /// skipping it prevents false "duplicate link name" errors when an imported
    /// link shares the same name as a struct field.
    fn alloc_struct_link(&mut self, name: Option<String>, scope: ScopeId, loc: IrLoc, link_type: IrLinkType) -> LinkId {
        let id = LinkId(self.links.len() as u32);
        self.links.push(IrLinkDef { name, scope, loc, link_type });
        id
    }

    fn alloc_fun(&mut self, name: String, scope: ScopeId, loc: IrLoc, type_id: TypeId, parent: Option<TypeId>) -> FunId {
        let id = FunId(self.funs.len() as u32);
        self.scopes[scope.0 as usize].funs
            .entry(name.clone())
            .or_default()
            .push(id);
        self.funs.push(IrFunDef {
            type_id, parent, name, type_hint: None, scope, loc,
            fields: vec![], mapper_fn: None, inputs: vec![], outputs: vec![],
        });
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
        s.funs.remove(name);
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

    fn lookup_fun(&self, scope: ScopeId, name: &str) -> Option<FunId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) { return None; }
        if let Some(ids) = s.funs.get(name) {
            if ids.len() == 1 { return Some(ids[0]); }
            if !ids.is_empty() { return None; } // ambiguous type-wise — caller must use lookup_fun_typed
        }
        s.parent.and_then(|p| self.lookup_fun(p, name))
    }

    fn lookup_fun_typed(&self, scope: ScopeId, name: &str, type_id: TypeId) -> Option<FunId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) { return None; }
        if let Some(ids) = s.funs.get(name) {
            if let Some(&fid) = ids.iter().find(|&&fid| self.funs[fid.0 as usize].type_id == type_id) {
                return Some(fid);
            }
        }
        s.parent.and_then(|p| self.lookup_fun_typed(p, name, type_id))
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

fn builtin_primitive_from_ref(r: &AstRef) -> Option<IrPrimitive> {
    if r.segments.len() != 1 {
        return None;
    }
    match r.segments[0].inner.as_plain()? {
        "string" => Some(IrPrimitive::String),
        "integer" => Some(IrPrimitive::Integer),
        "reference" => Some(IrPrimitive::Reference),
        _ => None,
    }
}

fn has_explicit_mapper(td: &AstDef) -> bool {
    td.mapper.is_some()
}

fn effective_mapper_fn(td: &AstDef) -> Option<String> {
    if let Some(mapper) = &td.mapper {
        return Some(ref_to_repr(&mapper.inner));
    }
    if td.input.is_empty() {
        return None;
    }
    match &td.output.inner {
        AstDefO::Unit => None,
        _ => Some(td.name.inner.clone()),
    }
}

fn output_has_schema_items(output: &AstDefO) -> bool {
    match output {
        AstDefO::Struct(items) => items.iter().any(|item| match &item.inner {
            AstStructItem::Field(fd) => fd.inner.kind == AstStructFieldKind::Def,
            AstStructItem::Def(_) => true,
            AstStructItem::Anon(_) => false,
        }),
        _ => false,
    }
}

fn is_apply_def(td: &AstDef) -> bool {
    td.input.is_empty() && has_explicit_mapper(td) && !output_has_schema_items(&td.output.inner)
}

fn collect_apply_fields(items: &[AstNode<AstStructItem>]) -> Vec<AstNode<AstField>> {
    let mut out = Vec::new();
    for item in items {
        match &item.inner {
            AstStructItem::Field(fd) => {
                if fd.inner.kind != AstStructFieldKind::Set {
                    continue;
                }
                let AstStructFieldBody::Value(value) = &fd.inner.body else { continue; };
                if let Some(name) = &fd.inner.name {
                    out.push(AstNode {
                        loc: fd.loc.clone(),
                        inner: AstField::Named {
                            name: name.clone(),
                            value: value.clone(),
                            via: false,
                        },
                    });
                }
            }
            AstStructItem::Anon(v) => {
                out.push(AstNode { loc: v.loc.clone(), inner: AstField::Anon(v.clone()) });
            }
            AstStructItem::Def(_) => {}
        }
    }
    out
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
            "pack" | "type" | "link" | "def" => {
                kind_hint = Some(val);
                continue;
            }
            _ => {}
        }

        let resolved = match kind_hint {
            Some("type") | Some("def") => ctx.lookup_type(scope, val)
                .map(IrRefSegValue::Type)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.to_string())),

            Some("link") => ctx.lookup_link(scope, val)
                .map(IrRefSegValue::Link)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.to_string())),

            Some("pack") => ctx.lookup_pack(scope, val)
                .map(IrRefSegValue::Pack)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.to_string())),

            _ => {
                // No hint — try type, then fun, then link, then pack; else plain.
                if let Some(id) = ctx.lookup_type(scope, val) {
                    IrRefSegValue::Type(id)
                } else if let Some(id) = ctx.lookup_fun(scope, val) {
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
            ast::ScopeKind::Struct => IrScopeKind::Struct,
        };
        let name = ast_scope.name.as_ref().map(|n| n.inner.clone());

        let new_id = ScopeId(ctx.scopes.len() as u32);
        ctx.scopes.push(IrScope {
            kind,
            name:           name.clone(),
            parent:         Some(parent),
            types:          HashMap::new(),
            links:          HashMap::new(),
            funs:           HashMap::new(),
            packs:          HashMap::new(),
            type_fns:       HashMap::new(),
            anon_type_fns:  HashMap::new(),
            anon_pair_fns:  HashMap::new(),
            ambiguous:      HashSet::new(),
            ts_fns:         HashSet::new(),
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
// TS function registration — associate ts_src function names with pack scopes
// ---------------------------------------------------------------------------

/// Extract top-level TypeScript function names from source text.
/// Matches: `[export] [async] function name(` patterns.
fn extract_ts_fn_names(ts_src: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut tokens = ts_src.split_ascii_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        // Strip `export` / `async` prefixes, then check for `function`.
        let tok = tok.trim_start_matches("export").trim();
        let tok = tok.trim_start_matches("async").trim();
        if tok == "function" {
            if let Some(next) = tokens.peek() {
                // Function name ends at `(` — trim it off.
                let name = next.trim_end_matches('(')
                    .split('(').next().unwrap_or("").trim();
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

/// Register TS function names from each unit's `ts_src` into the
/// corresponding pack scope.
fn pass_register_ts_fns(parse_res: &ParseRes, ctx: &mut Ctx) {
    for (i, scope_id) in parse_res.unit_scope_ids.iter().enumerate() {
        if let Some(Some(ts_src)) = parse_res.unit_ts_srcs.get(i) {
            let ir_scope_id = ScopeId(scope_id.0);
            for name in extract_ts_fn_names(ts_src) {
                ctx.scopes[ir_scope_id.0 as usize].ts_fns.insert(name);
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
            if let AstItem::Use(u) = def {
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
        Some("fn")   => { i += 1; Some("fn")   }
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
        if kind_hint.is_none() || kind_hint == Some("fn") {
            for name in &src_scope.ts_fns {
                ctx.scopes[into.0 as usize].ts_fns.insert(name.clone());
            }
        }
    }
    if kind == ImportKind::Insts {
        if kind_hint.is_none() || kind_hint == Some("inst") {
            for (name, fids) in &src_scope.funs {
                for &fid in fids {
                    try_import_fun(name, fid, into, ctx, loc);
                }
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
        if kind_hint.is_none() || kind_hint == Some("fn") {
            if src_scope.ts_fns.contains(name) {
                ctx.scopes[into.0 as usize].ts_fns.insert(name.to_string());
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
            if let Some(fids) = src_scope.funs.get(name) {
                for &fid in fids {
                    try_import_fun(name, fid, into, ctx, loc);
                }
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
    // Idempotent: same type already imported from the same source — skip silently.
    if let Some(&existing) = ctx.scopes[into.0 as usize].types.get(name) {
        if existing == tid { return; }
        // Local definition in this scope — it wins, skip the import silently.
        if ctx.types[existing.0 as usize].scope == into { return; }
        // Two different imports conflict.
        ctx.errors.push(IrError {
            message: format!("ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate", name),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
        return;
    }
    // Types and links are resolved through separate lookup functions, so they
    // can share a name without ambiguity — only a type/type or type/pack clash counts.
    let conflict = ctx.scopes[into.0 as usize].packs.contains_key(name);
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
    // Idempotent: same link already imported from the same source — skip silently.
    if let Some(&existing) = ctx.scopes[into.0 as usize].links.get(name) {
        if existing == lid { return; }
        // Local definition in this scope — it wins, skip the import silently.
        if ctx.links[existing.0 as usize].scope == into { return; }
        // Two different imports conflict.
        ctx.errors.push(IrError {
            message: format!("ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate", name),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
        return;
    }
    // Types and links are resolved through separate lookup functions, so they
    // can share a name without ambiguity — only a link/link or link/pack clash counts.
    let conflict = ctx.scopes[into.0 as usize].packs.contains_key(name);
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
    // Idempotent: same pack already imported from the same scope — skip silently.
    if let Some(&existing) = ctx.scopes[into.0 as usize].packs.get(name) {
        if existing == sid { return; }
        ctx.errors.push(IrError {
            message: format!("ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate", name),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
        return;
    }
    let conflict = ctx.scopes[into.0 as usize].types.contains_key(name)
        || ctx.scopes[into.0 as usize].links.contains_key(name);
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

fn try_import_fun(name: &str, fid: FunId, into: ScopeId, ctx: &mut Ctx, _loc: &IrLoc) {
    if ctx.scopes[into.0 as usize].ambiguous.contains(name) { return; }
    if let Some(existing) = ctx.scopes[into.0 as usize].funs.get(name) {
        // Idempotent: same fun already present — skip.
        if existing.contains(&fid) { return; }
        // Local definition in this scope wins — skip the import silently.
        if existing.iter().any(|&eid| ctx.funs[eid.0 as usize].scope == into) { return; }
    }
    ctx.scopes[into.0 as usize].funs
        .entry(name.to_string())
        .or_default()
        .push(fid);
}

// ---------------------------------------------------------------------------
// Pass 2 — register type and link names (placeholder bodies)
// ---------------------------------------------------------------------------

fn pass2_register_names(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstItem::Def(td) = def {
                // Mapping-application output defs (`name = type_ref { values }`) are instances,
                // not type declarations — they don't register a new type, only a mapping entry.
                if is_apply_def(&td.inner) { continue; }
                // Both plain defs (no input) and mapper defs (with input) register a
                // named type so they can be referenced.
                let id = TypeId(ctx.types.len() as u32);
                ctx.scopes[scope.0 as usize].types.insert(td.inner.name.inner.clone(), id);
                ctx.types.push(IrTypeDef {
                    name:  Some(td.inner.name.inner.clone()),
                    scope,
                    loc:   ir_loc(&td.loc),
                    body:  IrTypeBody::Enum(vec![]),  // placeholder
                });
            }
        }
    }
}

fn find_child_struct_scope(parse_scopes: &[AstScope], parent: ScopeId, name: &str) -> Option<ScopeId> {
    parse_scopes.iter().enumerate().find_map(|(idx, scope)| {
        let scope_parent = scope.parent.map(|id| ScopeId(id.0));
        let scope_name = scope.name.as_ref().map(|n| n.inner.as_str());
        if scope.kind == ast::ScopeKind::Struct && scope_parent == Some(parent) && scope_name == Some(name) {
            Some(ScopeId(idx as u32))
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// Pass 3 — resolve type bodies and link types
// ---------------------------------------------------------------------------

fn pass3_resolve_types_and_links(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    // Collect all work upfront to avoid mid-iteration borrow issues.
    let mut def_work:  Vec<(TypeId, AstNode<AstDef>, ScopeId)> = vec![];
    let mut mapper_work: Vec<(TypeId, AstNode<AstDef>, ScopeId)> = vec![];

    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstItem::Def(td) = def {
                // Mapper-application output defs don't define a type body — skip.
                if is_apply_def(&td.inner) { continue; }
                if let Some(&tid) = ctx.scopes[scope.0 as usize].types.get(&td.inner.name.inner) {
                    // Route to mapper_work if the def has inputs or names an explicit mapper.
                    let is_mapper = !is_apply_def(&td.inner)
                        && (!td.inner.input.is_empty() || has_explicit_mapper(&td.inner));
                    if is_mapper {
                        mapper_work.push((tid, td.clone(), scope));
                    } else {
                        def_work.push((tid, td.clone(), scope));
                    }
                }
            }
        }
    }

    for (tid, td, scope) in def_work {
        let body_scope = find_child_struct_scope(parse_scopes, scope, &td.inner.name.inner).unwrap_or(scope);
        let body = resolve_top_def_body(&td.inner, ctx, body_scope);
        ctx.types[tid.0 as usize].body = body;
    }

    for (type_id, td, scope) in mapper_work {
        let name = td.inner.name.inner.clone();
        let _loc = ir_loc(&td.loc);

        // Resolve input field links (the fields before "=").
        let inputs: Vec<LinkId> = td.inner.input.iter().map(|field| {
            let fname = field.inner.name.as_ref().map(|n| n.inner.clone());
            let floc  = ir_loc(&field.loc);
            let lt    = resolve_link_type(&field.inner.ty.inner, ctx, scope, &floc);
            ctx.alloc_struct_link(fname, scope, floc, lt)
        }).collect();

        // Resolve output struct links (the fields after "=").
        let body_scope = find_child_struct_scope(parse_scopes, scope, &td.inner.name.inner).unwrap_or(scope);
        let outputs = match &td.inner.output.inner {
            AstDefO::Struct(items) => resolve_struct_links(items, ctx, body_scope),
            _                              => vec![],
        };

        // Set the type body to all fields (inputs + outputs) so that instances can provide
        // input values and the resolver accepts them during field validation.
        let all_links: Vec<LinkId> = inputs.iter().chain(outputs.iter()).copied().collect();
        ctx.types[type_id.0 as usize].body = IrTypeBody::Struct(all_links);

        let mapper_fn = effective_mapper_fn(&td.inner);

        if let Some(mapper_name) = &mapper_fn {
            // Validate that the mapper function is visible in scope.
            if !ctx.scope_has_ts_fn(scope, mapper_name) {
                ctx.errors.push(IrError {
                    message: format!(
                        "mapper function '{}' not in scope; define it in a co-located \
                         .ts file or import it with `use pack:fn_name`",
                        mapper_name
                    ),
                    loc: _loc.clone(),
                });
            }
        }

        // Find the mapping registered for this def in pass4 and attach the mapper fields.
        let fids = ctx.scopes[scope.0 as usize].funs.get(&name).cloned().unwrap_or_default();
        let fid = fids.iter().copied().find(|&fid| ctx.funs[fid.0 as usize].type_id == type_id);
        if let Some(fid) = fid {
            ctx.funs[fid.0 as usize].mapper_fn = mapper_fn;
            ctx.funs[fid.0 as usize].inputs   = inputs;
            ctx.funs[fid.0 as usize].outputs  = outputs;
        }
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

/// Resolve the body of an `AstDef` into an `IrTypeBody`.
/// Called from pass3 (top-level defs) and `resolve_struct_links` (nested defs).
fn resolve_top_def_body(td: &AstDef, ctx: &mut Ctx, scope: ScopeId) -> IrTypeBody {
    match &td.output.inner {
        AstDefO::Unit => IrTypeBody::Enum(vec![]),
        AstDefO::TypeExpr(type_node) => {
            match &type_node.inner {
                AstTypeExpr::Ref(r) => {
                    if let Some(p) = builtin_primitive_from_ref(r) {
                        return IrTypeBody::Primitive(p);
                    }
                    let ir_ref = resolve_ref(&r.segments, ctx, scope);
                    IrTypeBody::Enum(vec![ir_ref])
                }
                _ => resolve_type_body(type_node, ctx, scope),
            }
        }
        AstDefO::Struct(items) => {
            IrTypeBody::Struct(resolve_struct_links(items, ctx, scope))
        }
    }
}

fn resolve_type_body(body: &AstNode<AstTypeExpr>, ctx: &mut Ctx, scope: ScopeId) -> IrTypeBody {
    match &body.inner {
        AstTypeExpr::Primitive(p) => IrTypeBody::Primitive(convert_primitive(p)),

        AstTypeExpr::Ref(r) => {
            if let Some(p) = builtin_primitive_from_ref(r) {
                return IrTypeBody::Primitive(p);
            }
            ctx.errors.push(IrError {
                message: "ref/list body not valid for a named type definition".into(),
                loc: ir_loc(&body.loc),
            });
            IrTypeBody::Enum(vec![])
        }

        AstTypeExpr::Enum(refs) => {
            let variants = refs.iter()
                .map(|r| resolve_ref(&r.inner.segments, ctx, scope))
                .collect();
            IrTypeBody::Enum(variants)
        }

        AstTypeExpr::Struct(items) => {
            IrTypeBody::Struct(resolve_struct_links(items, ctx, scope))
        }

        AstTypeExpr::Unit => IrTypeBody::Enum(vec![]),

        AstTypeExpr::List(_) => {
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
            AstStructItem::Field(fd) => {
                let loc = ir_loc(&fd.loc);
                if fd.inner.kind != AstStructFieldKind::Def {
                    continue;
                }
                let AstStructFieldBody::Type(ty) = &fd.inner.body else {
                    continue;
                };
                let name = fd.inner.name.as_ref().map(|n| n.inner.clone());
                let lt   = resolve_link_type(&ty.inner, ctx, scope, &loc);
                let lid  = ctx.alloc_struct_link(name, scope, loc, lt);
                link_ids.push(lid);
            }
            AstStructItem::Anon(_) => {}
            AstStructItem::Def(_) => {}
        }
    }

    link_ids
}

fn resolve_link_type(td: &AstTypeExpr, ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrLinkType {
    match td {
        AstTypeExpr::Primitive(p) => IrLinkType::Primitive(convert_primitive(p)),

        AstTypeExpr::Ref(r) => {
            if let Some(p) = builtin_primitive_from_ref(r) {
                return IrLinkType::Primitive(p);
            }
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

        AstTypeExpr::List(inner) => {
            // Primitive list shorthand: `[ string ]`, `[ integer ]`, `[ boolean ]`.
            if let AstTypeExpr::Primitive(p) = &inner.inner {
                let prim = convert_primitive(p);
                let tid = ctx.alloc_type(None, scope, loc.clone(), IrTypeBody::Primitive(prim));
                return IrLinkType::List(vec![IrRef {
                    segments: vec![IrRefSeg { value: IrRefSegValue::Type(tid), is_opt: false }],
                }]);
            }
            if let AstTypeExpr::Ref(r) = &inner.inner {
                if let Some(prim) = builtin_primitive_from_ref(r) {
                    let tid = ctx.alloc_type(None, scope, loc.clone(), IrTypeBody::Primitive(prim));
                    return IrLinkType::List(vec![IrRef {
                        segments: vec![IrRefSeg { value: IrRefSegValue::Type(tid), is_opt: false }],
                    }]);
                }
            }
            let is_single_ref = matches!(&inner.inner, AstTypeExpr::Ref(_));
            let elem_refs: Vec<AstRef> = match &inner.inner {
                AstTypeExpr::Ref(r)     => vec![r.clone()],
                AstTypeExpr::Enum(refs) => refs.iter().map(|r| r.inner.clone()).collect(),
                _ => {
                    ctx.errors.push(IrError {
                        message: "list element type must be a ref or enum of refs".into(),
                        loc: loc.clone(),
                    });
                    return IrLinkType::Primitive(IrPrimitive::Reference);
                }
            };
            let ir_refs: Vec<IrRef> = elem_refs.iter()
                .map(|r| resolve_ref(&r.segments, ctx, scope))
                .collect();
            let has_typed_variant = ir_refs.iter().any(|ir_ref| {
                ir_ref.segments.iter().any(|seg| matches!(seg.value, IrRefSegValue::Type(_)))
            });
            for (r, ir_ref) in elem_refs.iter().zip(ir_refs.iter()) {
                // Validate unresolved refs when:
                //   • single-type list: `[ aws_subnet ]` — always validate
                //   • multi-segment enum variant: `[ type:service | type:databasee ]` — validate
                //   • mixed typed enum: `[ service | databasee ]` — validate all variants
                //   • plain single-segment enum variant: `[ FARGATE | EC2 ]` — skip (string constants)
                let should_validate = is_single_ref || r.segments.len() > 1 || has_typed_variant;
                if should_validate {
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
                }
            }
            IrLinkType::List(ir_refs)
        }

        AstTypeExpr::Struct(items) => {
            let body = IrTypeBody::Struct(resolve_struct_links(items, ctx, scope));
            let tid  = ctx.alloc_type(None, scope, loc.clone(), body);
            IrLinkType::Ref(IrRef { segments: vec![IrRefSeg { value: IrRefSegValue::Type(tid), is_opt: false }] })
        }

        AstTypeExpr::Enum(refs) => {
            // Anonymous inline enum — allocate an anonymous type.
            let variants = refs.iter()
                .map(|r| resolve_ref(&r.inner.segments, ctx, scope))
                .collect();
            let tid = ctx.alloc_type(None, scope, loc.clone(), IrTypeBody::Enum(variants));
            IrLinkType::Ref(IrRef { segments: vec![IrRefSeg { value: IrRefSegValue::Type(tid), is_opt: false }] })
        }

        AstTypeExpr::Unit => {
            IrLinkType::Primitive(IrPrimitive::Reference)
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

fn pass4_register_funs(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            match def {
                // Every named def is a named entity. Plain defs become root mappings.
                // Defs with an explicit mapper ref and no inputs are treated as applications.
                AstItem::Def(td) => {
                    let name = &td.inner.name.inner;
                    if is_apply_def(&td.inner) {
                        let mapper = td.inner.mapper.as_ref().unwrap();
                        let type_id = match lookup_type_by_ref(&mapper.inner, ctx, scope) {
                            Some(tid) => tid,
                            None => {
                                let ref_name = ref_to_repr(&mapper.inner);
                                ctx.errors.push(IrError {
                                    message: format!("unknown type '{}'", ref_name),
                                    loc: ir_loc(&mapper.loc),
                                });
                                continue;
                            }
                        };
                        ctx.alloc_fun(name.clone(), scope, ir_loc(&td.loc), type_id, Some(type_id));
                    } else {
                        // Root mapping (parent=None) for type defs and bare defs.
                        if let Some(&type_id) = ctx.scopes[scope.0 as usize].types.get(name) {
                            ctx.alloc_fun(name.clone(), scope, ir_loc(&td.loc), type_id, None);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 5 — resolve instance fields
// ---------------------------------------------------------------------------

fn pass5_resolve_fun_fields(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    let mut work: Vec<(FunId, Vec<AstNode<AstField>>, ScopeId)> = vec![];

    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstItem::Def(td) = def {
                if is_apply_def(&td.inner) {
                    let name = &td.inner.name.inner;
                    // Try typed lookup first, fall back to unambiguous single lookup.
                    let mapper = td.inner.mapper.as_ref().unwrap();
                    let fid = if let Some(type_id) = lookup_type_by_ref(&mapper.inner, ctx, scope) {
                        ctx.lookup_fun_typed(scope, name, type_id)
                            .or_else(|| ctx.lookup_fun(scope, name))
                    } else {
                        ctx.lookup_fun(scope, name)
                    };
                    if let Some(fid) = fid {
                        let fields = match &td.inner.output.inner {
                            AstDefO::Unit => vec![],
                            AstDefO::Struct(items) => collect_apply_fields(items),
                            AstDefO::TypeExpr(_) => vec![],
                        };
                        work.push((fid, fields, scope));
                    }
                }
            }
        }
    }

    for (fid, fields, scope) in work {
        let type_id  = ctx.funs[fid.0 as usize].type_id;
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
                    resolved.push(IrField { link_id: lid, name: "_".into(), loc, via: false, value: IrValue::List(items) });
                }
                _ => {
                    if anon_vals.len() > 1 {
                        ctx.errors.push(IrError {
                            message: "multiple values defined for a non-List field '_'".into(),
                            loc: loc.clone(),
                        });
                    } else {
                        let ir_val = resolve_value(&anon_vals[0].inner, &link_type, ctx, scope, &loc);
                        resolved.push(IrField { link_id: lid, name: "_".into(), loc, via: false, value: ir_val });
                    }
                }
            }
        }

        ctx.funs[fid.0 as usize].fields = resolved;
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
    // The `via` bool is taken from the first occurrence of a given link field.
    let mut groups: Vec<(String, IrLoc, LinkId, Vec<&AstNode<AstValue>>, bool)> = Vec::new();

    for af in ast_fields {
        let AstField::Named { name, value, via } = &af.inner else { continue; };
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
            groups.push((field_name.clone(), loc, lid, vec![value], *via));
        }
    }

    // Pass 1: resolve plain (non-group) fields to build the full this_fields map.
    // Field order in instances does not affect {this:xxx} reduction.
    let mut this_fields: HashMap<String, String> = HashMap::new();
    let mut pre_resolved: Vec<Option<IrField>> = (0..groups.len()).map(|_| None).collect();

    for (i, (field_name, loc, lid, values, via)) in groups.iter().enumerate() {
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
            pre_resolved[i] = Some(IrField { link_id: *lid, name: field_name.clone(), loc: loc.clone(), via: *via, value: ir_val });
        }
    }

    // Pass 2: reduce group fields using the full this_fields, resolve in source order.
    groups.into_iter().enumerate().filter_map(|(i, (field_name, loc, lid, values, via))| {
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

        Some(IrField { link_id: lid, name: field_name, loc, via, value: ir_val })
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
                                let fid = FunId(ctx.funs.len() as u32);
                                ctx.funs.push(IrFunDef {
                                    type_id, parent: Some(type_id),
                                    name: "_".into(), type_hint: resolved_hint,
                                    scope, loc: loc.clone(), fields: vec![],
                                    mapper_fn: None, inputs: vec![], outputs: vec![],
                                });
                                let fields = resolve_named_fields(ast_fields, &link_ids, ctx, scope);
                                ctx.funs[fid.0 as usize].fields = fields;
                                return IrValue::Inst(fid);
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
                                let fid = FunId(ctx.funs.len() as u32);
                                ctx.funs.push(IrFunDef {
                                    type_id: hint_tid, parent: Some(hint_tid),
                                    name: "_".into(), type_hint: Some(hint_name.to_string()),
                                    scope, loc: loc.clone(), fields: vec![],
                                    mapper_fn: None, inputs: vec![], outputs: vec![],
                                });
                                let fields = resolve_named_fields(ast_fields, &inner_link_ids, ctx, scope);
                                ctx.funs[fid.0 as usize].fields = fields;
                                return IrValue::Variant(type_id, idx as u32, Some(Box::new(IrValue::Inst(fid))));
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
                                    // Use typed lookup to find instances of the inner type by name.
                                    let iid = ctx.lookup_fun_typed(scope, raw, *inner_tid)
                                        .or_else(|| ctx.lookup_fun(scope, raw)
                                            .filter(|&i| ctx.funs[i.0 as usize].type_id == *inner_tid));
                                    if let Some(iid) = iid {
                                        return IrValue::Variant(*tid, idx as u32, Some(Box::new(IrValue::Inst(iid))));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // For unit/marker types (e.g. `def secret`), `raw` may be a fun name.
                    if let Some(fid) = ctx.lookup_fun_typed(scope, raw, *tid) {
                        return IrValue::Inst(fid);
                    }
                    let type_name = ctx.types[tid.0 as usize].name.as_deref().unwrap_or("?");
                    ctx.errors.push(IrError {
                        message: format!("'{}' is not a variant of '{}'", raw, type_name),
                        loc: loc.clone(),
                    });
                    IrValue::Variant(*tid, 0, None)
                }
                Some(IrTypeBody::Struct(_)) => {
                    // Use typed lookup first — handles multi-type same-name funs.
                    let fid = ctx.lookup_fun_typed(scope, raw, *tid)
                        .or_else(|| ctx.lookup_fun(scope, raw));
                    match fid {
                        Some(fid) => IrValue::Inst(fid),
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
    // Handle inline struct literals `{ field: val ... }` — find the first matching struct pattern.
    if let AstValue::Struct { .. } = v {
        for pattern in patterns {
            if pattern.segments.len() == 1 {
                if let IrRefSegValue::Type(tid) = &pattern.segments[0].value {
                    if matches!(ctx.types.get(tid.0 as usize).map(|t| &t.body), Some(IrTypeBody::Struct(_))) {
                        return resolve_value(v, &IrLinkType::Ref(pattern.clone()), ctx, scope, loc);
                    }
                }
            }
        }
        ctx.errors.push(IrError { message: "list item must be a reference".into(), loc: loc.clone() });
        return IrValue::Ref(String::new());
    }

    let AstValue::Ref(r) = v else {
        ctx.errors.push(IrError { message: "list item must be a reference".into(), loc: loc.clone() });
        return IrValue::Ref(String::new());
    };
    if has_group(r) { return IrValue::Ref(ref_to_repr(r)); }

    // `def:TYPE:NAME` — explicit type-qualified instance reference.
    // Resolve directly by type+name, bypassing pattern matching.
    if r.segments.len() >= 2 && r.segments[0].inner.as_plain() == Some("def") {
        let type_name = r.segments[1].inner.as_plain().unwrap_or("");
        let inst_name_seg = r.segments.get(2);
        if let Some(type_id) = ctx.lookup_type(scope, type_name) {
            let name = inst_name_seg.and_then(|s| s.inner.as_plain()).unwrap_or(type_name);
            if let Some(fid) = ctx.lookup_fun_typed(scope, name, type_id) {
                return IrValue::Inst(fid);
            }
            ctx.errors.push(IrError {
                message: format!("no instance '{}' of type '{}' in scope", name, type_name),
                loc: loc.clone(),
            });
        } else {
            ctx.errors.push(IrError {
                message: format!("unknown type '{}' in def: qualifier", type_name),
                loc: loc.clone(),
            });
        }
        return IrValue::Ref(String::new());
    }

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
                return ctx.lookup_fun_typed(scope, inst_name, *tid).is_some();
            }
        }
        false
    }).or_else(|| patterns.first());

    // For primitive-typed patterns (e.g. `[ string ]`), join all ref segments as a string value.
    if let Some(pattern) = matched_pattern {
        if pattern.segments.len() == 1 {
            if let IrRefSegValue::Type(tid) = &pattern.segments[0].value {
                if let Some(IrTypeBody::Primitive(_)) = ctx.types.get(tid.0 as usize).map(|t| &t.body) {
                    let s = r.segments.iter()
                        .filter_map(|s| s.inner.as_plain())
                        .collect::<Vec<_>>()
                        .join(":");
                    return IrValue::Str(s);
                }
            }
        }
    }

    // For typed-path values against a single-segment pattern, resolve using just the instance name.
    if is_typed_path {
        if let Some(pattern) = matched_pattern {
            if pattern.segments.len() == 1 {
                return resolve_single_seg_value(inst_name, &pattern.segments[0], ctx, scope, loc);
            }
        }
    }

    let v_ref = AstValue::Ref(r.clone());
    match matched_pattern {
        Some(pattern) => resolve_value_against_ref(&v_ref, pattern, ctx, scope, loc),
        None          => IrValue::Ref(String::new()),
    }
}

// ---------------------------------------------------------------------------
// Pass 7 — resolve type function definitions
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Pass 6 — resolve plans
// ---------------------------------------------------------------------------

fn pass6_resolve_plans(parse_scopes: &[AstScope], ctx: &mut Ctx) -> Vec<IrPlanDef> {
    // Collect first to avoid mid-iteration borrow issues.
    let work: Vec<(String, IrLoc, Vec<AstNode<AstField>>, ScopeId)> =
        parse_scopes.iter().enumerate().flat_map(|(scope_idx, ast_scope)| {
            let scope = ScopeId(scope_idx as u32);
            ast_scope.defs.iter().filter_map(move |def| {
                let AstItem::Plan(p) = def else { return None; };
                Some((
                    p.inner.name.inner.clone(),
                    IrLoc { unit: p.loc.unit, start: p.loc.start, end: p.loc.end },
                    p.inner.fields.clone(),
                    scope,
                ))
            })
        }).collect();

    let mut plans = Vec::new();
    for (name, loc, ast_fields, scope) in work {
        // Resolve plan-level fields against the named deploy instance's type.
        let fields = if ast_fields.is_empty() {
            vec![]
        } else {
            let link_ids = ctx.lookup_fun(scope, &name)
                .and_then(|fid| {
                    let type_id = ctx.funs[fid.0 as usize].type_id;
                    match ctx.types.get(type_id.0 as usize).map(|t| t.body.clone()) {
                        Some(IrTypeBody::Struct(ids)) => Some(ids),
                        _ => None,
                    }
                })
                .unwrap_or_default();
            resolve_named_fields(&ast_fields, &link_ids, ctx, scope)
        };
        plans.push(IrPlanDef { name, loc, fields });
    }
    plans
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn resolve(res: ParseRes) -> IrRes {
    let mut ctx = Ctx::new();

    pass1_mirror_scopes(&res.scopes, &mut ctx);
    pass_register_ts_fns(&res, &mut ctx);
    pass2_register_names(&res.scopes, &mut ctx);
    pass_imports(&res.scopes, &mut ctx, ImportKind::TypesLinksAndPacks);
    pass4_register_funs(&res.scopes, &mut ctx);
    pass3_resolve_types_and_links(&res.scopes, &mut ctx);
    pass3b_flatten_alias_types(&mut ctx);
    pass_imports(&res.scopes, &mut ctx, ImportKind::Insts);
    pass5_resolve_fun_fields(&res.scopes, &mut ctx);
    let plans = pass6_resolve_plans(&res.scopes, &mut ctx);

    let mut errors = ctx.errors;
    errors.extend(res.errors.iter().map(|e| IrError {
        message: e.message.clone(),
        loc:     IrLoc { unit: e.loc.unit, start: e.loc.start, end: e.loc.end },
    }));

    IrRes {
        types:    ctx.types,
        links:    ctx.links,
        funs:     ctx.funs,
        plans,
        type_fns: ctx.type_fns,
        scopes:   ctx.scopes,
        errors,
    }
}
