/// Resolve pass: `ParseRes` → `IrRes`.
///
/// Passes:
///   1. Mirror scope tree from parse arena.
///   2. Register shape/field names in their scopes.
///   3. Resolve shape bodies and field shapes.
///   4. Register def names (enables forward references).
///   5. Resolve def fields (validated against field type patterns).
use std::collections::{HashMap, HashSet};

use crate::ast::{self, *};
use crate::ir::ScopeKind as IrScopeKind;
use crate::ir::*;

// ---------------------------------------------------------------------------
// Resolver context
// ---------------------------------------------------------------------------

struct Ctx {
    shapes: Vec<IrShapeDef>,
    defs: Vec<IrDef>,
    scopes: Vec<IrScope>,
    errors: Vec<IrError>,
}

impl Ctx {
    fn new() -> Self {
        let root = IrScope {
            kind: IrScopeKind::Pack,
            name: None,
            parent: None,
            shapes: HashMap::new(),
            defs: HashMap::new(),
            packs: HashMap::new(),
            ambiguous: HashSet::new(),
            ts_fns: HashSet::new(),
        };
        Ctx {
            shapes: vec![],
            defs: vec![],
            scopes: vec![root],
            errors: vec![],
        }
    }

    fn scope_has_ts_fn(&self, scope: ScopeId, name: &str) -> bool {
        let s = &self.scopes[scope.0 as usize];
        if s.ts_fns.contains(name) {
            return true;
        }
        s.parent.map_or(false, |p| self.scope_has_ts_fn(p, name))
    }

    fn alloc_shape(
        &mut self,
        name: Option<String>,
        scope: ScopeId,
        loc: IrLoc,
        body: IrShapeBody,
    ) -> ShapeId {
        let id = ShapeId(self.shapes.len() as u32);
        if let Some(n) = &name {
            if self.scopes[scope.0 as usize].shapes.contains_key(n) {
                self.push_error(format!("duplicate type name '{}' in scope", n), loc.clone());
            } else {
                self.scopes[scope.0 as usize].shapes.insert(n.clone(), id);
            }
        }
        self.shapes.push(IrShapeDef {
            name,
            scope,
            loc,
            body,
        });
        id
    }

    fn alloc_def(
        &mut self,
        planned: bool,
        name: String,
        scope: ScopeId,
        loc: IrLoc,
        shape_id: ShapeId,
        base_def: Option<DefId>,
    ) -> DefId {
        let id = DefId(self.defs.len() as u32);
        self.scopes[scope.0 as usize]
            .defs
            .entry(name.clone())
            .or_default()
            .push(id);
        self.defs.push(IrDef {
            planned,
            shape_id,
            base_def,
            name,
            type_hint: None,
            scope,
            loc,
            fields: vec![],
            mapper_fn: None,
            inputs: vec![],
            outputs: vec![],
        });
        id
    }

    fn push_error(&mut self, message: String, loc: IrLoc) {
        self.errors.push(IrError { message, loc });
    }

    /// Mark `name` as ambiguous in `scope`: removes it from all namespace maps and
    /// records it so lookups stop the parent-chain walk with None.
    fn mark_ambiguous(&mut self, scope: ScopeId, name: &str) {
        let s = &mut self.scopes[scope.0 as usize];
        s.shapes.remove(name);
        s.defs.remove(name);
        s.packs.remove(name);
        s.ambiguous.insert(name.to_string());
    }

    fn lookup_shape(&self, scope: ScopeId, name: &str) -> Option<ShapeId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) {
            return None;
        }
        if let Some(&id) = s.shapes.get(name) {
            if !self.is_planned_name(scope, name) {
                return Some(id);
            }
        }
        s.parent.and_then(|p| self.lookup_shape(p, name))
    }

    fn lookup_def(&self, scope: ScopeId, name: &str) -> Option<DefId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) {
            return None;
        }
        if let Some(ids) = s.defs.get(name) {
            let visible: Vec<DefId> = ids
                .iter()
                .copied()
                .filter(|fid| !self.defs[fid.0 as usize].planned)
                .collect();
            if visible.len() == 1 {
                return Some(visible[0]);
            }
            if !visible.is_empty() {
                return None;
            } // ambiguous type-wise — caller must use lookup_def_typed
        }
        s.parent.and_then(|p| self.lookup_def(p, name))
    }

    fn lookup_def_typed(&self, scope: ScopeId, name: &str, shape_id: ShapeId) -> Option<DefId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) {
            return None;
        }
        if let Some(ids) = s.defs.get(name) {
            if let Some(&fid) = ids.iter().find(|&&fid| {
                let def = &self.defs[fid.0 as usize];
                !def.planned && self.def_satisfies_shape(fid, shape_id)
            }) {
                return Some(fid);
            }
        }
        s.parent
            .and_then(|p| self.lookup_def_typed(p, name, shape_id))
    }

    fn lookup_def_any(&self, scope: ScopeId, name: &str) -> Option<DefId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) {
            return None;
        }
        if let Some(ids) = s.defs.get(name) {
            if ids.len() == 1 {
                return Some(ids[0]);
            }
            if !ids.is_empty() {
                return None;
            }
        }
        s.parent.and_then(|p| self.lookup_def_any(p, name))
    }

    fn lookup_def_typed_any(&self, scope: ScopeId, name: &str, shape_id: ShapeId) -> Option<DefId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) {
            return None;
        }
        if let Some(ids) = s.defs.get(name) {
            if let Some(&fid) = ids
                .iter()
                .find(|&&fid| self.def_satisfies_shape(fid, shape_id))
            {
                return Some(fid);
            }
        }
        s.parent
            .and_then(|p| self.lookup_def_typed_any(p, name, shape_id))
    }

    fn def_satisfies_shape(&self, def_id: DefId, shape_id: ShapeId) -> bool {
        let mut cur = Some(def_id);
        while let Some(fid) = cur {
            let def = &self.defs[fid.0 as usize];
            if def.shape_id == shape_id {
                return true;
            }
            cur = def.base_def;
        }
        false
    }

    fn lookup_pack(&self, scope: ScopeId, name: &str) -> Option<ScopeId> {
        let s = &self.scopes[scope.0 as usize];
        if s.ambiguous.contains(name) {
            return None;
        }
        if let Some(&id) = s.packs.get(name) {
            return Some(id);
        }
        s.parent.and_then(|p| self.lookup_pack(p, name))
    }

    fn is_planned_name(&self, scope: ScopeId, name: &str) -> bool {
        let s = &self.scopes[scope.0 as usize];
        if let Some(ids) = s.defs.get(name) {
            if ids.iter().any(|fid| self.defs[fid.0 as usize].planned) {
                return true;
            }
        }
        s.parent.map_or(false, |p| self.is_planned_name(p, name))
    }
}

fn ir_loc(loc: &AstNodeLoc) -> IrLoc {
    IrLoc {
        unit: loc.unit,
        start: loc.start,
        end: loc.end,
    }
}

fn builtin_primitive_from_ref(r: &AstRef) -> Option<IrPrimitive> {
    if r.segments.len() != 1 {
        return None;
    }
    match r.segments[0].inner.as_plain()? {
        "string" => Some(IrPrimitive::String),
        "integer" => Some(IrPrimitive::Integer),
        "boolean" => Some(IrPrimitive::Boolean),
        "reference" => Some(IrPrimitive::Reference),
        "ipv4" => Some(IrPrimitive::Ipv4),
        "ipv4net" => Some(IrPrimitive::Ipv4Net),
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

fn lookup_base_shape_and_def(
    td: &AstDef,
    ctx: &Ctx,
    scope: ScopeId,
) -> (Option<ShapeId>, Option<DefId>) {
    let Some(mapper) = &td.mapper else {
        return (None, None);
    };
    let Some(shape_id) = lookup_type_by_ref(&mapper.inner, ctx, scope) else {
        return (None, None);
    };
    let base_def = lookup_def_by_ref_and_shape(&mapper.inner, ctx, scope, shape_id);
    (Some(shape_id), base_def)
}

fn lookup_def_by_ref_and_shape(
    ref_: &AstRef,
    ctx: &Ctx,
    scope: ScopeId,
    shape_id: ShapeId,
) -> Option<DefId> {
    let parts = qualified_plain_segments(&ref_.segments);
    let (last_qual, name) = *parts.last()?;
    if last_qual == Some("pack") {
        return None;
    }

    let cur = if parts.len() == 1 {
        scope
    } else {
        lookup_pack_path_qualified(ctx, scope, &parts[..parts.len() - 1])?
    };

    ctx.lookup_def_typed(cur, name, shape_id)
        .or_else(|| ctx.lookup_def(cur, name))
}

fn lookup_ts_fn_by_ref(ref_: &AstRef, ctx: &Ctx, scope: ScopeId) -> Option<String> {
    let parts = qualified_plain_segments(&ref_.segments);
    let (last_qual, name) = *parts.last()?;
    if last_qual == Some("pack") || last_qual == Some("def") {
        return None;
    }

    let cur = if parts.len() == 1 {
        scope
    } else {
        lookup_pack_path_qualified(ctx, scope, &parts[..parts.len() - 1])?
    };

    ctx.scopes[cur.0 as usize]
        .ts_fns
        .contains(name)
        .then(|| name.to_string())
}

fn output_has_schema_items(output: &AstDefO) -> bool {
    match output {
        AstDefO::Struct(items) => items.iter().any(|item| match &item.inner {
            AstStructItem::Field(fd) => fd.inner.kind == AstStructFieldKind::Def,
            AstStructItem::Def(_) => true,
            AstStructItem::Anon(_) => false,
            AstStructItem::Comment(_) => false,
        }),
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DefKind {
    Plain,
    Mapper,
    Apply,
    ComposedShape,
}

fn classify_def(td: &AstDef) -> DefKind {
    if !td.input.is_empty() {
        return DefKind::Mapper;
    }
    if !has_explicit_mapper(td) {
        return DefKind::Plain;
    }
    if output_has_schema_items(&td.output.inner) {
        return DefKind::ComposedShape;
    }
    DefKind::Apply
}

fn collect_apply_fields(items: &[AstNode<AstStructItem>]) -> Vec<AstNode<AstField>> {
    let mut out = Vec::new();
    for item in items {
        match &item.inner {
            AstStructItem::Field(fd) => {
                if fd.inner.kind != AstStructFieldKind::Set {
                    continue;
                }
                let AstStructFieldBody::Value(value) = &fd.inner.body else {
                    continue;
                };
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
                out.push(AstNode {
                    loc: v.loc.clone(),
                    inner: AstField::Anon(v.clone()),
                });
            }
            AstStructItem::Def(_) => {}
            AstStructItem::Comment(_) => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Group ref helpers
// ---------------------------------------------------------------------------

/// Build the source repr of a Group segment: `{inner:repr}trailing`.
fn group_repr(inner: &AstRef, trailing: Option<&str>) -> String {
    let inner_repr = inner
        .segments
        .iter()
        .filter_map(|s| s.inner.as_plain())
        .collect::<Vec<_>>()
        .join(":");
    format!("{{{}}}{}", inner_repr, trailing.unwrap_or(""))
}

/// True if any segment in `r` is a Group that has not been reduced to plain.
fn has_group(r: &AstRef) -> bool {
    r.segments
        .iter()
        .any(|s| matches!(&s.inner.value, AstRefSegVal::Group(..)))
}

/// Convert an `AstRef` to its string repr, rendering remaining Group segments
/// as `{inner}trailing` and joining plain segments with `:`.
fn ref_to_repr(r: &AstRef) -> String {
    r.segments
        .iter()
        .map(|s| match &s.inner.value {
            AstRefSegVal::Plain(v) => v.clone(),
            AstRefSegVal::Group(g, trail) => group_repr(g, trail.as_deref()),
        })
        .collect::<Vec<_>>()
        .join(":")
}

/// Attempt to reduce `{this:field_name}` to the plain value already resolved
/// for `field_name` on the current instance.  Returns `None` if the inner ref
/// does not match the `this:xxx` pattern or the field is not yet in the map.
fn reduce_this_group(
    inner: &AstRef,
    trailing: Option<&str>,
    this_fields: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let segs = &inner.segments;
    if segs.len() == 2 && segs[0].inner.as_plain() == Some("this") {
        if let Some(field_name) = segs[1].inner.as_plain() {
            if let Some(value) = this_fields.get(field_name) {
                return Some(match trailing {
                    Some(t) => format!("{}{}", value, t),
                    None => value.clone(),
                });
            }
        }
    }
    None
}

/// Reduce all `{this:xxx}` Group segments in `r` using already-resolved
/// instance field values.  Non-`this` groups are left as-is.
fn reduce_ast_ref(r: &AstRef, this_fields: &std::collections::HashMap<String, String>) -> AstRef {
    let segments = r
        .segments
        .iter()
        .map(|seg| {
            let new_val = match &seg.inner.value {
                AstRefSegVal::Group(inner, trailing) => {
                    match reduce_this_group(inner, trailing.as_deref(), this_fields) {
                        Some(plain) => AstRefSegVal::Plain(plain),
                        None => seg.inner.value.clone(),
                    }
                }
                AstRefSegVal::Plain(_) => seg.inner.value.clone(),
            };
            AstNode {
                loc: seg.loc.clone(),
                inner: AstRefSeg {
                    value: new_val,
                    is_opt: seg.inner.is_opt,
                },
            }
        })
        .collect();
    AstRef { segments }
}

/// Extract the variant name from a plain single-segment IrRef (`Plain("foo")`).
/// Returns None for typed or multi-segment variants.
fn plain_variant_name(r: &IrRef) -> Option<&str> {
    if let [seg] = r.segments.as_slice() {
        if let IrRefSegValue::Plain(s) = &seg.value {
            return Some(s);
        }
    }
    None
}

/// Convert a resolved `IrValue` to a plain string for use as a `{this:xxx}`
/// substitution target.  Only scalar values can be plainified.
fn ir_value_to_plain_str(v: &IrValue, ctx: &Ctx) -> Option<String> {
    match v {
        IrValue::Str(s) => Some(s.clone()),
        IrValue::Ref(s) => Some(s.clone()),
        IrValue::Int(n) => Some(n.to_string()),
        IrValue::Variant(tid, idx, _) => {
            if let IrShapeBody::Enum(variants) = &ctx.shapes[tid.0 as usize].body {
                variants
                    .get(*idx as usize)
                    .and_then(plain_variant_name)
                    .map(|s| s.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Ref resolution (def-side)
//
// Each segment is resolved independently in the lexical scope.
// Keywords `pack` / `def` are kind-filters consumed without storing.
// Unresolvable segments become Plain(String).
// ---------------------------------------------------------------------------

fn resolve_ref(segments: &[AstNode<AstRefSeg>], ctx: &Ctx, scope: ScopeId) -> IrRef {
    let mut result = Vec::new();
    let mut kind_hint: Option<&str> = None;
    let mut cur_scope = scope;

    for (idx, seg) in segments.iter().enumerate() {
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
            "pack" | "def" => {
                kind_hint = Some(val);
                continue;
            }
            _ => {}
        }

        let resolved = match kind_hint {
            Some("def") => ctx
                .lookup_shape(cur_scope, val)
                .map(IrRefSegValue::Shape)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.to_string())),

            Some("pack") => ctx
                .lookup_pack(cur_scope, val)
                .map(IrRefSegValue::Pack)
                .unwrap_or_else(|| IrRefSegValue::Plain(val.to_string())),

            _ => {
                // No hint — try def/type, then instance, then pack; else plain.
                if let Some(id) = ctx.lookup_shape(cur_scope, val) {
                    IrRefSegValue::Shape(id)
                } else if let Some(id) = ctx.lookup_def(cur_scope, val) {
                    IrRefSegValue::Def(id)
                } else if let Some(id) = ctx.lookup_pack(cur_scope, val) {
                    IrRefSegValue::Pack(id)
                } else {
                    IrRefSegValue::Plain(val.to_string())
                }
            }
        };

        if let IrRefSegValue::Pack(next_scope) = resolved {
            cur_scope = next_scope;
            if idx + 1 < segments.len() {
                kind_hint = None;
                continue;
            }
        }
        result.push(IrRefSeg {
            value: resolved,
            is_opt: seg.inner.is_opt,
        });
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
        let parent = ast_scope
            .parent
            .map(|id| ScopeId(id.0))
            .unwrap_or(ScopeId(0));
        let kind = match ast_scope.kind {
            ast::ScopeKind::Pack => IrScopeKind::Pack,
            ast::ScopeKind::Struct => IrScopeKind::Struct,
        };
        let name = ast_scope.name.as_ref().map(|n| n.inner.clone());

        let new_id = ScopeId(ctx.scopes.len() as u32);
        ctx.scopes.push(IrScope {
            kind,
            name: name.clone(),
            parent: Some(parent),
            shapes: HashMap::new(),
            defs: HashMap::new(),
            packs: HashMap::new(),
            ambiguous: HashSet::new(),
            ts_fns: HashSet::new(),
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
                let name = next
                    .trim_end_matches('(')
                    .split('(')
                    .next()
                    .unwrap_or("")
                    .trim();
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
    for unit in &parse_res.units {
        let Some(ts_src) = unit.ts_src.as_deref() else {
            continue;
        };
        let ir_scope_id = ScopeId(unit.scope_id.0);
        for name in extract_ts_fn_names(ts_src) {
            ctx.scopes[ir_scope_id.0 as usize].ts_fns.insert(name);
        }
    }
}

// ---------------------------------------------------------------------------
// Import pass — process `use` statements
// ---------------------------------------------------------------------------
//
// Run twice:
//   ImportKind::TypesLinksAndPacks  — after pass2 (local shapes/fields/packs registered),
//                                     before pass4 (so imported shapes are visible
//                                     when instances look up their type name).
//   ImportKind::Insts               — after pass4 (local instances registered),
//                                     before pass5 (so imported instances are
//                                     visible in field value resolution).

#[derive(Clone, Copy, PartialEq)]
enum ImportKind {
    TypesLinksAndPacks,
    Insts,
}

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

#[derive(Clone, Copy)]
enum UseMode<'a> {
    Pack,
    ImportOne { name: &'a str, defs_only: bool },
    ImportAll { defs_only: bool },
}

fn resolve_use(path: &AstRef, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc, kind: ImportKind) {
    let mut names: Vec<&str> = path
        .segments
        .iter()
        .filter_map(|s| s.inner.as_plain())
        .collect();
    if names.first().copied() == Some("pack") {
        names.remove(0);
    }

    if names.is_empty() {
        if kind == ImportKind::TypesLinksAndPacks {
            ctx.errors.push(IrError {
                message: "use: expected pack name".into(),
                loc: loc.clone(),
            });
        }
        return;
    }

    let (pack_path, mode) = if names.last().copied() == Some("*") {
        if names.len() >= 2 && names[names.len() - 2] == "def" {
            (
                &names[..names.len() - 2],
                UseMode::ImportAll { defs_only: true },
            )
        } else {
            (
                &names[..names.len() - 1],
                UseMode::ImportAll { defs_only: false },
            )
        }
    } else if names.len() >= 2 && names[names.len() - 2] == "def" {
        (
            &names[..names.len() - 2],
            UseMode::ImportOne {
                name: names[names.len() - 1],
                defs_only: true,
            },
        )
    } else if lookup_pack_path(ctx, into, &names).is_some() {
        (&names[..], UseMode::Pack)
    } else if names.len() >= 2 {
        (
            &names[..names.len() - 1],
            UseMode::ImportOne {
                name: names[names.len() - 1],
                defs_only: false,
            },
        )
    } else {
        (&names[..], UseMode::Pack)
    };

    if pack_path.is_empty() {
        if kind == ImportKind::TypesLinksAndPacks {
            ctx.errors.push(IrError {
                message: "use: expected pack name".into(),
                loc: loc.clone(),
            });
        }
        return;
    }

    let src = match lookup_pack_path(ctx, into, pack_path) {
        Some(s) => s,
        None => {
            if kind == ImportKind::TypesLinksAndPacks {
                ctx.errors.push(IrError {
                    message: format!("use: pack '{}' not found", pack_path.join(":")),
                    loc: loc.clone(),
                });
            }
            return;
        }
    };
    let alias = *pack_path.last().unwrap();

    match mode {
        UseMode::Pack => {
            if kind == ImportKind::TypesLinksAndPacks {
                try_import_pack(alias, src, into, ctx, loc);
            }
        }
        UseMode::ImportOne { name, defs_only } => match kind {
            ImportKind::TypesLinksAndPacks => {
                if !defs_only {
                    if let Some(shape_id) = ctx.scopes[src.0 as usize].shapes.get(name).copied() {
                        try_import_shape(name, shape_id, into, ctx, loc);
                    }
                    if let Some(pack_id) = ctx.scopes[src.0 as usize].packs.get(name).copied() {
                        try_import_pack(name, pack_id, into, ctx, loc);
                    }
                } else if let Some(shape_id) = ctx.scopes[src.0 as usize].shapes.get(name).copied()
                {
                    try_import_shape(name, shape_id, into, ctx, loc);
                }
            }
            ImportKind::Insts => {
                if let Some(ids) = ctx.scopes[src.0 as usize].defs.get(name).cloned() {
                    let visible: Vec<DefId> = ids
                        .into_iter()
                        .filter(|id| !ctx.defs[id.0 as usize].planned)
                        .collect();
                    for id in visible {
                        try_import_def(name, id, into, ctx, loc);
                    }
                }
            }
        },
        UseMode::ImportAll { defs_only } => match kind {
            ImportKind::TypesLinksAndPacks => {
                let shape_names: Vec<(String, ShapeId)> = ctx.scopes[src.0 as usize]
                    .shapes
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                for (name, sid) in shape_names {
                    try_import_shape(&name, sid, into, ctx, loc);
                }
                if !defs_only {
                    let pack_names: Vec<(String, ScopeId)> = ctx.scopes[src.0 as usize]
                        .packs
                        .iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .collect();
                    for (name, sid) in pack_names {
                        try_import_pack(&name, sid, into, ctx, loc);
                    }
                }
            }
            ImportKind::Insts => {
                let def_names: Vec<(String, Vec<DefId>)> = ctx.scopes[src.0 as usize]
                    .defs
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (name, ids) in def_names {
                    let visible: Vec<DefId> = ids
                        .into_iter()
                        .filter(|id| !ctx.defs[id.0 as usize].planned)
                        .collect();
                    for id in visible {
                        try_import_def(&name, id, into, ctx, loc);
                    }
                }
            }
        },
    }
}

fn lookup_pack_path(ctx: &Ctx, scope: ScopeId, path: &[&str]) -> Option<ScopeId> {
    let (first, rest) = path.split_first()?;
    let mut cur = ctx.lookup_pack(scope, first)?;
    for name in rest {
        cur = *ctx.scopes[cur.0 as usize].packs.get(*name)?;
    }
    Some(cur)
}

// ---------------------------------------------------------------------------
// Import helpers — insert a single name into the destination scope,
// detecting and marking ambiguity if a conflict exists.
// ---------------------------------------------------------------------------

fn try_import_pack(name: &str, sid: ScopeId, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc) {
    if ctx.scopes[into.0 as usize].ambiguous.contains(name) {
        return;
    }
    // Idempotent: same pack already imported from the same scope — skip silently.
    if let Some(&existing) = ctx.scopes[into.0 as usize].packs.get(name) {
        if existing == sid {
            return;
        }
        ctx.errors.push(IrError {
            message: format!(
                "ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate",
                name
            ),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
        return;
    }
    let conflict = ctx.scopes[into.0 as usize].shapes.contains_key(name);
    if conflict {
        ctx.errors.push(IrError {
            message: format!(
                "ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate",
                name
            ),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
    } else {
        ctx.scopes[into.0 as usize]
            .packs
            .insert(name.to_string(), sid);
    }
}

fn try_import_shape(name: &str, sid: ShapeId, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc) {
    if ctx.scopes[into.0 as usize].ambiguous.contains(name) {
        return;
    }
    if let Some(&existing) = ctx.scopes[into.0 as usize].shapes.get(name) {
        if existing == sid {
            return;
        }
        ctx.errors.push(IrError {
            message: format!(
                "ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate",
                name
            ),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
        return;
    }
    let conflict = ctx.scopes[into.0 as usize].packs.contains_key(name);
    if conflict {
        ctx.errors.push(IrError {
            message: format!(
                "ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate",
                name
            ),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
    } else {
        ctx.scopes[into.0 as usize]
            .shapes
            .insert(name.to_string(), sid);
    }
}

fn try_import_def(name: &str, fid: DefId, into: ScopeId, ctx: &mut Ctx, loc: &IrLoc) {
    if ctx.scopes[into.0 as usize].ambiguous.contains(name) {
        return;
    }
    let defs = &mut ctx.scopes[into.0 as usize].defs;
    if let Some(existing) = defs.get(name) {
        if existing.contains(&fid) {
            return;
        }
        ctx.errors.push(IrError {
            message: format!(
                "ambiguous ref '{}': defined in multiple sources, use a prefix to disambiguate",
                name
            ),
            loc: loc.clone(),
        });
        ctx.mark_ambiguous(into, name);
        return;
    }
    defs.insert(name.to_string(), vec![fid]);
}

// ---------------------------------------------------------------------------
// Pass 2 — register shape names (placeholder bodies)
// ---------------------------------------------------------------------------

fn pass2_register_names(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstItem::Def(td) = def {
                if classify_def(&td.inner) == DefKind::Apply {
                    continue;
                }
                let id = ShapeId(ctx.shapes.len() as u32);
                ctx.scopes[scope.0 as usize]
                    .shapes
                    .insert(td.inner.name.inner.clone(), id);
                ctx.shapes.push(IrShapeDef {
                    name: Some(td.inner.name.inner.clone()),
                    scope,
                    loc: ir_loc(&td.loc),
                    body: IrShapeBody::Unit, // placeholder
                });
            }
        }
    }
}

fn find_child_struct_scope(
    parse_scopes: &[AstScope],
    parent: ScopeId,
    name: &str,
) -> Option<ScopeId> {
    parse_scopes.iter().enumerate().find_map(|(idx, scope)| {
        let scope_parent = scope.parent.map(|id| ScopeId(id.0));
        let scope_name = scope.name.as_ref().map(|n| n.inner.as_str());
        if scope.kind == ast::ScopeKind::Struct
            && scope_parent == Some(parent)
            && scope_name == Some(name)
        {
            Some(ScopeId(idx as u32))
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// Pass 3 — resolve shape bodies and field shapes
// ---------------------------------------------------------------------------

fn pass3_resolve_types_and_links(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    // Collect all work upfront to avoid mid-iteration borrow issues.
    let mut def_work: Vec<(ShapeId, AstNode<AstDef>, ScopeId)> = vec![];
    let mut mapper_work: Vec<(ShapeId, AstNode<AstDef>, ScopeId)> = vec![];

    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstItem::Def(td) = def {
                let def_kind = classify_def(&td.inner);
                if def_kind == DefKind::Apply {
                    continue;
                }
                if let Some(&tid) = ctx.scopes[scope.0 as usize]
                    .shapes
                    .get(&td.inner.name.inner)
                {
                    if matches!(def_kind, DefKind::Mapper | DefKind::ComposedShape) {
                        mapper_work.push((tid, td.clone(), scope));
                    } else {
                        def_work.push((tid, td.clone(), scope));
                    }
                }
            }
        }
    }

    for (tid, td, scope) in def_work {
        let body_scope =
            find_child_struct_scope(parse_scopes, scope, &td.inner.name.inner).unwrap_or(scope);
        let body = resolve_top_def_body(&td.inner, ctx, body_scope);
        ctx.shapes[tid.0 as usize].body = body;
    }

    for (shape_id, td, scope) in mapper_work {
        let name = td.inner.name.inner.clone();
        let _loc = ir_loc(&td.loc);

        // Resolve input fields (the fields before "=").
        let inputs: Vec<IrStructFieldDef> = td
            .inner
            .input
            .iter()
            .map(|field| {
                let fname = field.inner.name.as_ref().map(|n| n.inner.clone());
                let floc = ir_loc(&field.loc);
                let field_type = resolve_field_type(&field.inner.ty.inner, ctx, scope, &floc);
                IrStructFieldDef {
                    name: fname,
                    field_type,
                }
            })
            .collect();

        // Resolve output fields (the fields after "=").
        let body_scope =
            find_child_struct_scope(parse_scopes, scope, &td.inner.name.inner).unwrap_or(scope);
        let outputs = match &td.inner.output.inner {
            AstDefO::Struct(items) => resolve_struct_fields(items, ctx, body_scope),
            _ => vec![],
        };

        let all_fields: Vec<IrStructFieldDef> = if td.inner.input.is_empty() {
            if let (Some(base_shape_id), _) = lookup_base_shape_and_def(&td.inner, ctx, scope) {
                let inherited = match ctx.shapes.get(base_shape_id.0 as usize).map(|s| &s.body) {
                    Some(IrShapeBody::Struct(fields)) => fields.clone(),
                    _ => vec![],
                };
                merge_struct_fields(&inherited, &outputs, ctx, &_loc)
            } else {
                outputs.clone()
            }
        } else {
            // Mapper defs include their inputs in the visible shape so downstream defs
            // can provide those values during composition.
            inputs
                .iter()
                .cloned()
                .chain(outputs.iter().cloned())
                .collect()
        };
        ctx.shapes[shape_id.0 as usize].body = IrShapeBody::Struct(all_fields);

        let mut mapper_fn = effective_mapper_fn(&td.inner);
        if td.inner.input.is_empty() {
            let (_, base_def) = lookup_base_shape_and_def(&td.inner, ctx, scope);
            if base_def.is_some() {
                mapper_fn = None;
            }
        }

        if let Some(mapper_name) = mapper_fn.clone() {
            if !ctx.scope_has_ts_fn(scope, &mapper_name) {
                if let Some(explicit) = &td.inner.mapper {
                    if let Some(ts_name) = lookup_ts_fn_by_ref(&explicit.inner, ctx, scope) {
                        mapper_fn = Some(ts_name);
                    } else {
                        ctx.errors.push(IrError {
                            message: format!(
                                "mapper function '{}' not in scope; define it in a co-located \
                                 .ts file or import its pack with `use pack:<name>`",
                                mapper_name
                            ),
                            loc: _loc.clone(),
                        });
                    }
                } else {
                    ctx.errors.push(IrError {
                        message: format!(
                            "mapper function '{}' not in scope; define it in a co-located \
                             .ts file or import its pack with `use pack:<name>`",
                            mapper_name
                        ),
                        loc: _loc.clone(),
                    });
                }
            }
        }

        // Find the mapping registered for this def in pass4 and attach the mapper fields.
        let fids = ctx.scopes[scope.0 as usize]
            .defs
            .get(&name)
            .cloned()
            .unwrap_or_default();
        let fid = fids
            .iter()
            .copied()
            .find(|&fid| ctx.def_satisfies_shape(fid, shape_id));
        if let Some(fid) = fid {
            ctx.defs[fid.0 as usize].mapper_fn = mapper_fn;
            ctx.defs[fid.0 as usize].inputs = inputs;
            ctx.defs[fid.0 as usize].outputs = outputs;
        }
    }
}

// ---------------------------------------------------------------------------
// Pass 3b — flatten single-variant enum type aliases
// ---------------------------------------------------------------------------
//
// `type loc = type:zone` parses as a 1-element enum whose sole variant is a
// ref to `zone`.  This pass rewrites such shapes to carry the same body as the
// referenced type, making the definition behave as a true type alias.
//
// Only non-struct targets are flattened: an alias pointing at a struct is
// left as-is so that typed-enum field matching continues to work correctly.
// Cycles (e.g. `type a = type:b`, `type b = type:a`) are detected via an
// in-stack flag and left unchanged.

fn flatten_alias(idx: usize, shapes: &mut Vec<IrShapeDef>, in_stack: &mut Vec<bool>) {
    if in_stack[idx] {
        return;
    }

    let body = shapes[idx].body.clone();
    if let IrShapeBody::Enum(ref variants) = body {
        if variants.len() == 1 {
            let segs = &variants[0].segments;
            if segs.len() == 1 && !segs[0].is_opt {
                if let IrRefSegValue::Shape(tid) = segs[0].value {
                    let target_idx = tid.0 as usize;
                    if target_idx != idx {
                        in_stack[idx] = true;
                        flatten_alias(target_idx, shapes, in_stack);
                        in_stack[idx] = false;

                        let target_body = shapes[target_idx].body.clone();
                        if !matches!(target_body, IrShapeBody::Struct(_)) {
                            shapes[idx].body = target_body;
                        }
                    }
                }
            }
        }
    }
}

fn pass3b_flatten_alias_types(ctx: &mut Ctx) {
    let n = ctx.shapes.len();
    let mut in_stack = vec![false; n];
    for i in 0..n {
        flatten_alias(i, &mut ctx.shapes, &mut in_stack);
    }
}

/// Resolve the body of an `AstDef` into an `IrShapeBody`.
/// Called from pass3 for top-level and nested defs.
fn resolve_top_def_body(td: &AstDef, ctx: &mut Ctx, scope: ScopeId) -> IrShapeBody {
    match &td.output.inner {
        AstDefO::Unit => IrShapeBody::Unit,
        AstDefO::TypeExpr(type_node) => match &type_node.inner {
            AstTypeExpr::Ref(r) => {
                if let Some(p) = builtin_primitive_from_ref(r) {
                    return IrShapeBody::Primitive(p);
                }
                let ir_ref = resolve_ref(&r.segments, ctx, scope);
                IrShapeBody::Enum(vec![ir_ref])
            }
            _ => resolve_type_body(type_node, ctx, scope),
        },
        AstDefO::Struct(items) => IrShapeBody::Struct(resolve_struct_fields(items, ctx, scope)),
    }
}

fn resolve_type_body(body: &AstNode<AstTypeExpr>, ctx: &mut Ctx, scope: ScopeId) -> IrShapeBody {
    match &body.inner {
        AstTypeExpr::Primitive(p) => IrShapeBody::Primitive(convert_primitive(p)),

        AstTypeExpr::Ref(r) => {
            if let Some(p) = builtin_primitive_from_ref(r) {
                return IrShapeBody::Primitive(p);
            }
            ctx.errors.push(IrError {
                message: "ref/list body not valid for a named type definition".into(),
                loc: ir_loc(&body.loc),
            });
            IrShapeBody::Unit
        }

        AstTypeExpr::Enum(refs) => {
            let variants = refs
                .iter()
                .map(|r| resolve_ref(&r.inner.segments, ctx, scope))
                .collect();
            IrShapeBody::Enum(variants)
        }

        AstTypeExpr::Struct(items) => IrShapeBody::Struct(resolve_struct_fields(items, ctx, scope)),

        AstTypeExpr::Unit => IrShapeBody::Unit,

        AstTypeExpr::List(_) => {
            ctx.errors.push(IrError {
                message: "ref/list body not valid for a named type definition".into(),
                loc: ir_loc(&body.loc),
            });
            IrShapeBody::Unit
        }
    }
}

fn resolve_struct_fields(
    items: &[AstNode<AstStructItem>],
    ctx: &mut Ctx,
    scope: ScopeId,
) -> Vec<IrStructFieldDef> {
    let mut fields = Vec::new();

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
                let field_type = resolve_field_type(&ty.inner, ctx, scope, &loc);
                let _ = loc;
                fields.push(IrStructFieldDef { name, field_type });
            }
            AstStructItem::Anon(_) => {}
            AstStructItem::Def(_) => {}
            AstStructItem::Comment(_) => {}
        }
    }

    fields
}

fn merge_struct_fields(
    inherited: &[IrStructFieldDef],
    local: &[IrStructFieldDef],
    ctx: &mut Ctx,
    loc: &IrLoc,
) -> Vec<IrStructFieldDef> {
    let mut merged = inherited.to_vec();
    for field in local {
        if let Some(name) = &field.name {
            if merged.iter().any(|f| f.name.as_ref() == Some(name)) {
                ctx.errors.push(IrError {
                    message: format!("field '{}' already exists in inherited shape", name),
                    loc: loc.clone(),
                });
                continue;
            }
        }
        merged.push(field.clone());
    }
    merged
}

fn resolve_field_type(td: &AstTypeExpr, ctx: &mut Ctx, scope: ScopeId, loc: &IrLoc) -> IrFieldType {
    match td {
        AstTypeExpr::Primitive(p) => IrFieldType::Primitive(convert_primitive(p)),

        AstTypeExpr::Ref(r) => {
            if let Some(p) = builtin_primitive_from_ref(r) {
                return IrFieldType::Primitive(p);
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
            IrFieldType::Ref(ir_ref)
        }

        AstTypeExpr::List(inner) => {
            // Primitive list shorthand: `[ string ]`, `[ integer ]`, `[ boolean ]`.
            if let AstTypeExpr::Primitive(p) = &inner.inner {
                let prim = convert_primitive(p);
                let tid = ctx.alloc_shape(None, scope, loc.clone(), IrShapeBody::Primitive(prim));
                return IrFieldType::List(vec![IrRef {
                    segments: vec![IrRefSeg {
                        value: IrRefSegValue::Shape(tid),
                        is_opt: false,
                    }],
                }]);
            }
            if let AstTypeExpr::Ref(r) = &inner.inner {
                if let Some(prim) = builtin_primitive_from_ref(r) {
                    let tid =
                        ctx.alloc_shape(None, scope, loc.clone(), IrShapeBody::Primitive(prim));
                    return IrFieldType::List(vec![IrRef {
                        segments: vec![IrRefSeg {
                            value: IrRefSegValue::Shape(tid),
                            is_opt: false,
                        }],
                    }]);
                }
            }
            let is_single_ref = matches!(&inner.inner, AstTypeExpr::Ref(_));
            let elem_refs: Vec<AstRef> = match &inner.inner {
                AstTypeExpr::Ref(r) => vec![r.clone()],
                AstTypeExpr::Enum(refs) => refs.iter().map(|r| r.inner.clone()).collect(),
                _ => {
                    ctx.errors.push(IrError {
                        message: "list element type must be a ref or enum of refs".into(),
                        loc: loc.clone(),
                    });
                    return IrFieldType::Primitive(IrPrimitive::Reference);
                }
            };
            let ir_refs: Vec<IrRef> = elem_refs
                .iter()
                .map(|r| resolve_ref(&r.segments, ctx, scope))
                .collect();
            let has_typed_variant = ir_refs.iter().any(|ir_ref| {
                ir_ref
                    .segments
                    .iter()
                    .any(|seg| matches!(seg.value, IrRefSegValue::Shape(_)))
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
            IrFieldType::List(ir_refs)
        }

        AstTypeExpr::Struct(items) => {
            let body = IrShapeBody::Struct(resolve_struct_fields(items, ctx, scope));
            let tid = ctx.alloc_shape(None, scope, loc.clone(), body);
            IrFieldType::Ref(IrRef {
                segments: vec![IrRefSeg {
                    value: IrRefSegValue::Shape(tid),
                    is_opt: false,
                }],
            })
        }

        AstTypeExpr::Enum(refs) => {
            // Anonymous inline enum — allocate an anonymous type.
            let variants = refs
                .iter()
                .map(|r| resolve_ref(&r.inner.segments, ctx, scope))
                .collect();
            let tid = ctx.alloc_shape(None, scope, loc.clone(), IrShapeBody::Enum(variants));
            IrFieldType::Ref(IrRef {
                segments: vec![IrRefSeg {
                    value: IrRefSegValue::Shape(tid),
                    is_opt: false,
                }],
            })
        }

        AstTypeExpr::Unit => IrFieldType::Primitive(IrPrimitive::Reference),
    }
}

fn convert_primitive(p: &AstPrimitive) -> IrPrimitive {
    match p {
        AstPrimitive::String => IrPrimitive::String,
        AstPrimitive::Integer => IrPrimitive::Integer,
        AstPrimitive::Boolean => IrPrimitive::Boolean,
        AstPrimitive::Reference => IrPrimitive::Reference,
        AstPrimitive::Ipv4 => IrPrimitive::Ipv4,
        AstPrimitive::Ipv4Net => IrPrimitive::Ipv4Net,
    }
}

fn parse_ipv4(raw: &str) -> Option<Ipv4Addr> {
    raw.parse::<Ipv4Addr>().ok()
}

fn is_valid_ipv4net(raw: &str) -> bool {
    let Some((addr, prefix)) = raw.split_once('/') else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u8>() else {
        return false;
    };
    prefix <= 32 && parse_ipv4(addr).is_some()
}

// ---------------------------------------------------------------------------
// Pass 4 — register instance names
// ---------------------------------------------------------------------------

fn lookup_type_by_ref(ref_: &AstRef, ctx: &Ctx, scope: ScopeId) -> Option<ShapeId> {
    let parts = qualified_plain_segments(&ref_.segments);
    let (last_qual, name) = *parts.last()?;
    if last_qual == Some("pack") {
        return None;
    }

    let cur = if parts.len() == 1 {
        scope
    } else {
        lookup_pack_path_qualified(ctx, scope, &parts[..parts.len() - 1])?
    };

    ctx.lookup_shape(cur, name)
}

fn qualified_plain_segments<'a>(segs: &'a [AstNode<AstRefSeg>]) -> Vec<(Option<&'a str>, &'a str)> {
    let mut out = Vec::new();
    let mut pending: Option<&'a str> = None;

    for seg in segs {
        let Some(name) = seg.inner.as_plain() else {
            pending = None;
            continue;
        };

        if pending.is_none() && (name == "pack" || name == "def") {
            pending = Some(name);
            continue;
        }

        out.push((pending.take(), name));
    }

    out
}

fn lookup_pack_path_qualified(
    ctx: &Ctx,
    scope: ScopeId,
    parts: &[(Option<&str>, &str)],
) -> Option<ScopeId> {
    let (first_qual, first) = *parts.first()?;
    if first_qual == Some("def") {
        return None;
    }

    let mut cur = ctx.lookup_pack(scope, first)?;
    for (qual, name) in &parts[1..] {
        if *qual == Some("def") {
            return None;
        }
        cur = *ctx.scopes[cur.0 as usize].packs.get(*name)?;
    }
    Some(cur)
}

fn target_shape_from_ir_ref(pattern: &IrRef) -> Option<ShapeId> {
    let last = pattern.segments.last()?;
    let IrRefSegValue::Shape(shape_id) = last.value else {
        return None;
    };
    if pattern.segments[..pattern.segments.len().saturating_sub(1)]
        .iter()
        .all(|seg| matches!(seg.value, IrRefSegValue::Pack(_)))
    {
        Some(shape_id)
    } else {
        None
    }
}

fn split_pack_prefix<'a>(
    segs: &'a [AstNode<AstRefSeg>],
    ctx: &Ctx,
    scope: ScopeId,
) -> (ScopeId, &'a [AstNode<AstRefSeg>]) {
    let mut cur = scope;
    let mut i = 0;
    while i + 1 < segs.len() {
        let Some(name) = segs[i].inner.as_plain() else {
            break;
        };
        let Some(next) = ctx.lookup_pack(cur, name) else {
            break;
        };
        cur = next;
        i += 1;
    }
    (cur, &segs[i..])
}

fn scope_and_value_segs<'a>(
    segs: &'a [AstNode<AstRefSeg>],
    ctx: &Ctx,
    scope: ScopeId,
) -> (ScopeId, &'a [AstNode<AstRefSeg>]) {
    if segs.len() > 1 {
        if let Some(first) = segs[0].inner.as_plain() {
            if ctx.lookup_shape(scope, first).is_some() {
                return (scope, segs);
            }
        }
    }
    split_pack_prefix(segs, ctx, scope)
}

fn pass4_register_defs(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            match def {
                // Every named def is a named entity. Plain defs become root mappings.
                // Defs with an explicit mapper ref and no inputs are treated as applications.
                AstItem::Def(td) => {
                    let name = &td.inner.name.inner;
                    match classify_def(&td.inner) {
                        DefKind::Apply => {
                            let mapper = td.inner.mapper.as_ref().unwrap();
                            let shape_id = match lookup_type_by_ref(&mapper.inner, ctx, scope) {
                                Some(tid) => tid,
                                None => {
                                    let ref_name = ref_to_repr(&mapper.inner);
                                    ctx.errors.push(IrError {
                                        message: format!("unknown shape '{}'", ref_name),
                                        loc: ir_loc(&mapper.loc),
                                    });
                                    continue;
                                }
                            };
                            let (_, base_def) = lookup_base_shape_and_def(&td.inner, ctx, scope);
                            ctx.alloc_def(
                                td.inner.planned,
                                name.clone(),
                                scope,
                                ir_loc(&td.loc),
                                shape_id,
                                base_def,
                            );
                        }
                        DefKind::Plain | DefKind::Mapper | DefKind::ComposedShape => {
                            if let Some(&shape_id) = ctx.scopes[scope.0 as usize].shapes.get(name) {
                                let base_def =
                                    if matches!(classify_def(&td.inner), DefKind::ComposedShape) {
                                        let (_, base_def) =
                                            lookup_base_shape_and_def(&td.inner, ctx, scope);
                                        base_def
                                    } else {
                                        None
                                    };
                                ctx.alloc_def(
                                    td.inner.planned,
                                    name.clone(),
                                    scope,
                                    ir_loc(&td.loc),
                                    shape_id,
                                    base_def,
                                );
                            }
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

fn pass5_resolve_def_fields(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    let mut work: Vec<(DefId, Vec<AstNode<AstField>>, ScopeId)> = vec![];

    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            if let AstItem::Def(td) = def {
                if matches!(
                    classify_def(&td.inner),
                    DefKind::Apply | DefKind::ComposedShape
                ) {
                    let name = &td.inner.name.inner;
                    let own_shape_id = ctx.scopes[scope.0 as usize].shapes.get(name).copied();
                    let fid = own_shape_id
                        .and_then(|shape_id| ctx.lookup_def_typed_any(scope, name, shape_id))
                        .or_else(|| ctx.lookup_def_any(scope, name));
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
        let shape_id = ctx.defs[fid.0 as usize].shape_id;
        let field_defs = match ctx.shapes.get(shape_id.0 as usize).map(|t| &t.body) {
            Some(IrShapeBody::Struct(fields)) => fields.clone(),
            _ => vec![],
        };

        // Collect anonymous field values for the unnamed field (if any).
        let anon_field_idx = field_defs.iter().position(|f| f.name.is_none());
        let anon_vals: Vec<&AstNode<AstValue>> = fields
            .iter()
            .filter_map(|af| {
                if let AstField::Anon(v) = &af.inner {
                    Some(v)
                } else {
                    None
                }
            })
            .collect();

        let mut resolved: Vec<IrField> = resolve_named_fields(&fields, &field_defs, ctx, scope);

        // Resolve anonymous values against the unnamed field.
        if let (Some(field_idx), false) = (anon_field_idx, anon_vals.is_empty()) {
            let field_type = field_defs[field_idx].field_type.clone();
            let loc = ir_loc(&anon_vals[0].loc);
            match &field_type {
                IrFieldType::List(patterns) => {
                    let items = anon_vals
                        .iter()
                        .map(|v| resolve_list_item(&v.inner, patterns, ctx, scope, &ir_loc(&v.loc)))
                        .collect();
                    resolved.push(IrField {
                        field_idx: field_idx as u32,
                        name: "_".into(),
                        loc,
                        via: false,
                        value: IrValue::List(items),
                    });
                }
                _ => {
                    if anon_vals.len() > 1 {
                        ctx.errors.push(IrError {
                            message: "multiple values defined for a non-List field '_'".into(),
                            loc: loc.clone(),
                        });
                    } else {
                        let ir_val =
                            resolve_value(&anon_vals[0].inner, &field_type, ctx, scope, &loc);
                        resolved.push(IrField {
                            field_idx: field_idx as u32,
                            name: "_".into(),
                            loc,
                            via: false,
                            value: ir_val,
                        });
                    }
                }
            }
        }

        ctx.defs[fid.0 as usize].fields = resolved;
    }
}

fn pass_bind_imported_bases(parse_scopes: &[AstScope], ctx: &mut Ctx) {
    for (scope_idx, ast_scope) in parse_scopes.iter().enumerate() {
        let scope = ScopeId(scope_idx as u32);
        for def in &ast_scope.defs {
            let AstItem::Def(td) = def else { continue };
            if !matches!(
                classify_def(&td.inner),
                DefKind::Apply | DefKind::ComposedShape
            ) {
                continue;
            }
            let name = &td.inner.name.inner;
            let own_shape_id = ctx.scopes[scope.0 as usize].shapes.get(name).copied();
            let fid = own_shape_id
                .and_then(|shape_id| ctx.lookup_def_typed_any(scope, name, shape_id))
                .or_else(|| ctx.lookup_def_any(scope, name));
            let Some(fid) = fid else { continue };
            if ctx.defs[fid.0 as usize].base_def.is_some() {
                continue;
            }
            let (_, base_def) = lookup_base_shape_and_def(&td.inner, ctx, scope);
            if let Some(base_def) = base_def {
                ctx.defs[fid.0 as usize].base_def = Some(base_def);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Named field resolution (shared by pass5 and inline struct values)
// ---------------------------------------------------------------------------

fn resolve_named_fields(
    ast_fields: &[AstNode<AstField>],
    field_defs: &[IrStructFieldDef],
    ctx: &mut Ctx,
    scope: ScopeId,
) -> Vec<IrField> {
    // Group named fields by field name, preserving first-occurrence order.
    // The `via` bool is taken from the first occurrence of a given field.
    let mut groups: Vec<(String, IrLoc, usize, Vec<&AstNode<AstValue>>, bool)> = Vec::new();

    for af in ast_fields {
        let AstField::Named { name, value, via } = &af.inner else {
            continue;
        };
        let field_name = &name.inner;
        let loc = ir_loc(&name.loc);

        let field_idx = match field_defs
            .iter()
            .position(|field| field.name.as_deref() == Some(field_name))
        {
            Some(idx) => idx,
            None => {
                ctx.errors.push(IrError {
                    message: format!("unknown field '{}'", field_name),
                    loc: loc.clone(),
                });
                continue;
            }
        };

        if let Some(group) = groups.iter_mut().find(|g| g.2 == field_idx) {
            group.3.push(value);
        } else {
            groups.push((field_name.clone(), loc, field_idx, vec![value], *via));
        }
    }

    // Pass 1: resolve plain (non-group) fields to build the full this_fields map.
    // Field order in instances does not affect {this:xxx} reduction.
    let mut this_fields: HashMap<String, String> = HashMap::new();
    let mut pre_resolved: Vec<Option<IrField>> = (0..groups.len()).map(|_| None).collect();

    for (i, (field_name, loc, field_idx, values, via)) in groups.iter().enumerate() {
        if values
            .iter()
            .any(|v| matches!(&v.inner, AstValue::Ref(r) if has_group(r)))
        {
            continue;
        }

        let field_type = field_defs[*field_idx].field_type.clone();
        let opt_val: Option<IrValue> = if values.len() > 1 {
            match &field_type {
                IrFieldType::List(patterns) => {
                    let items = values
                        .iter()
                        .map(|v| resolve_list_item(&v.inner, patterns, ctx, scope, &ir_loc(&v.loc)))
                        .collect();
                    Some(IrValue::List(items))
                }
                _ => {
                    ctx.errors.push(IrError {
                        message: format!(
                            "multiple values defined for a non-List field '{}'",
                            field_name
                        ),
                        loc: loc.clone(),
                    });
                    None
                }
            }
        } else {
            Some(resolve_value(
                &values[0].inner,
                &field_type,
                ctx,
                scope,
                &ir_loc(&values[0].loc),
            ))
        };

        if let Some(ir_val) = opt_val {
            if let Some(plain) = ir_value_to_plain_str(&ir_val, ctx) {
                this_fields.insert(field_name.clone(), plain);
            }
            pre_resolved[i] = Some(IrField {
                field_idx: *field_idx as u32,
                name: field_name.clone(),
                loc: loc.clone(),
                via: *via,
                value: ir_val,
            });
        }
    }

    // Pass 2: reduce group fields using the full this_fields, resolve in source order.
    groups
        .into_iter()
        .enumerate()
        .filter_map(|(i, (field_name, loc, field_idx, values, via))| {
            if let Some(cached) = pre_resolved[i].take() {
                return Some(cached);
            }

            let field_type = field_defs[field_idx].field_type.clone();
            let reduced: Vec<AstNode<AstValue>> = values
                .iter()
                .map(|v| {
                    if let AstValue::Ref(r) = &v.inner {
                        if has_group(r) {
                            return AstNode {
                                loc: v.loc.clone(),
                                inner: AstValue::Ref(reduce_ast_ref(r, &this_fields)),
                            };
                        }
                    }
                    (*v).clone()
                })
                .collect();

            let ir_val = if reduced.len() > 1 {
                match &field_type {
                    IrFieldType::List(patterns) => {
                        let items = reduced
                            .iter()
                            .map(|v| {
                                resolve_list_item(&v.inner, patterns, ctx, scope, &ir_loc(&v.loc))
                            })
                            .collect();
                        IrValue::List(items)
                    }
                    _ => {
                        ctx.errors.push(IrError {
                            message: format!(
                                "multiple values defined for a non-List field '{}'",
                                field_name
                            ),
                            loc: loc.clone(),
                        });
                        return None;
                    }
                }
            } else {
                resolve_value(
                    &reduced[0].inner,
                    &field_type,
                    ctx,
                    scope,
                    &ir_loc(&reduced[0].loc),
                )
            };

            Some(IrField {
                field_idx: field_idx as u32,
                name: field_name,
                loc,
                via,
                value: ir_val,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Value resolution (guided by IrFieldType pattern)
// ---------------------------------------------------------------------------

fn resolve_value(
    v: &AstValue,
    field_type: &IrFieldType,
    ctx: &mut Ctx,
    scope: ScopeId,
    loc: &IrLoc,
) -> IrValue {
    // Unresolved Group segments cannot be statically resolved — pass through as Ref.
    if let AstValue::Ref(r) = v {
        if has_group(r) {
            return IrValue::Ref(ref_to_repr(r));
        }
    }

    match field_type {
        IrFieldType::Primitive(IrPrimitive::Integer) => {
            let s = match v {
                AstValue::Ref(r) => r.segments[0].inner.as_plain().unwrap_or("").to_string(),
                _ => {
                    ctx.errors.push(IrError {
                        message: "expected integer".into(),
                        loc: loc.clone(),
                    });
                    return IrValue::Int(0);
                }
            };
            match s.parse::<i64>() {
                Ok(n) => IrValue::Int(n),
                Err(_) => {
                    ctx.errors.push(IrError {
                        message: format!("expected integer, got '{}'", s),
                        loc: loc.clone(),
                    });
                    IrValue::Int(0)
                }
            }
        }

        IrFieldType::Primitive(IrPrimitive::String) => match v {
            AstValue::Str(s) => IrValue::Str(s.clone()),
            _ => {
                ctx.errors.push(IrError {
                    message: "expected string".into(),
                    loc: loc.clone(),
                });
                IrValue::Str(String::new())
            }
        },

        IrFieldType::Primitive(IrPrimitive::Ipv4) => match v {
            AstValue::Str(s) => {
                if parse_ipv4(s).is_some() {
                    IrValue::Str(s.clone())
                } else {
                    ctx.errors.push(IrError {
                        message: format!("expected ipv4, got '{}'", s),
                        loc: loc.clone(),
                    });
                    IrValue::Str(String::new())
                }
            }
            _ => {
                ctx.errors.push(IrError {
                    message: "expected ipv4".into(),
                    loc: loc.clone(),
                });
                IrValue::Str(String::new())
            }
        },

        IrFieldType::Primitive(IrPrimitive::Ipv4Net) => match v {
            AstValue::Str(s) => {
                if is_valid_ipv4net(s) {
                    IrValue::Str(s.clone())
                } else {
                    ctx.errors.push(IrError {
                        message: format!("expected ipv4net, got '{}'", s),
                        loc: loc.clone(),
                    });
                    IrValue::Str(String::new())
                }
            }
            _ => {
                ctx.errors.push(IrError {
                    message: "expected ipv4net".into(),
                    loc: loc.clone(),
                });
                IrValue::Str(String::new())
            }
        },

        IrFieldType::Primitive(IrPrimitive::Boolean) => match v {
            AstValue::Ref(r) if r.segments.len() == 1 => {
                match r.segments[0].inner.as_plain().unwrap_or("") {
                    "true" => IrValue::Bool(true),
                    "false" => IrValue::Bool(false),
                    raw => {
                        ctx.errors.push(IrError {
                            message: format!("expected boolean, got '{}'", raw),
                            loc: loc.clone(),
                        });
                        IrValue::Bool(false)
                    }
                }
            }
            _ => {
                ctx.errors.push(IrError {
                    message: "expected boolean".into(),
                    loc: loc.clone(),
                });
                IrValue::Bool(false)
            }
        },

        IrFieldType::Primitive(IrPrimitive::Reference) => match v {
            AstValue::Ref(r) => IrValue::Ref(
                r.segments
                    .iter()
                    .map(|s| s.inner.as_plain().unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join(":"),
            ),
            _ => {
                ctx.errors.push(IrError {
                    message: "expected reference".into(),
                    loc: loc.clone(),
                });
                IrValue::Ref(String::new())
            }
        },

        IrFieldType::Ref(pattern) => {
            // Inline struct literal `{ field: value ... }` — allocate an anonymous instance.
            if let AstValue::Struct {
                type_hint,
                fields: ast_fields,
            } = v
            {
                if let Some(shape_id) = target_shape_from_ir_ref(pattern) {
                    match ctx.shapes.get(shape_id.0 as usize).map(|t| t.body.clone()) {
                        Some(IrShapeBody::Struct(field_defs)) => {
                            // Validate and extract shape hint if present.
                            let resolved_hint = if let Some(hint) = type_hint {
                                let hint_name = hint
                                    .inner
                                    .segments
                                    .last()
                                    .and_then(|s| s.inner.as_plain())
                                    .unwrap_or("");
                                let expected = ctx.shapes[shape_id.0 as usize]
                                    .name
                                    .clone()
                                    .unwrap_or_else(|| "?".into());
                                if let Some(hint_tid) = lookup_type_by_ref(&hint.inner, ctx, scope)
                                    .or_else(|| ctx.lookup_shape(scope, hint_name))
                                {
                                    if hint_tid != shape_id {
                                        ctx.push_error(format!(
                                            "shape hint '{}' does not match expected shape '{}'",
                                            hint_name, expected
                                        ), loc.clone());
                                    }
                                } else if hint_name != expected {
                                    ctx.push_error(format!(
                                        "unknown shape '{}' in type hint",
                                        hint_name
                                    ), loc.clone());
                                }
                                Some(hint_name.to_string())
                            } else {
                                None
                            };
                            let fid = DefId(ctx.defs.len() as u32);
                            ctx.defs.push(IrDef {
                                planned: false,
                                shape_id,
                                base_def: None,
                                name: "_".into(),
                                type_hint: resolved_hint,
                                scope,
                                loc: loc.clone(),
                                fields: vec![],
                                mapper_fn: None,
                                inputs: vec![],
                                outputs: vec![],
                            });
                            let fields = resolve_named_fields(ast_fields, &field_defs, ctx, scope);
                            ctx.defs[fid.0 as usize].fields = fields;
                            return IrValue::Inst(fid);
                        }
                        Some(IrShapeBody::Enum(variants)) => {
                            // Struct literal against an enum type — hint identifies the variant.
                            let hint = match type_hint {
                                None => {
                                    let enum_name = ctx.shapes[shape_id.0 as usize]
                                        .name
                                        .as_deref()
                                        .unwrap_or("?");
                                    ctx.push_error(format!(
                                        "shape hint required for enum '{}'",
                                        enum_name
                                    ), loc.clone());
                                    return IrValue::Ref(String::new());
                                }
                                Some(h) => h,
                            };
                            let hint_name = hint
                                .inner
                                .segments
                                .last()
                                .and_then(|s| s.inner.as_plain())
                                .unwrap_or("");
                            let hint_tid = match lookup_type_by_ref(&hint.inner, ctx, scope)
                                .or_else(|| ctx.lookup_shape(scope, hint_name))
                            {
                                None => {
                                    ctx.push_error(format!(
                                        "unknown shape '{}' in type hint",
                                        hint_name
                                    ), loc.clone());
                                    return IrValue::Ref(String::new());
                                }
                                Some(t) => t,
                            };
                            let variant_idx = variants.iter().enumerate().find_map(|(i, r)| {
                                if let [seg] = r.segments.as_slice() {
                                    if let IrRefSegValue::Shape(vt) = &seg.value {
                                        if *vt == hint_tid {
                                            return Some(i);
                                        }
                                    }
                                }
                                None
                            });
                            let idx = match variant_idx {
                                None => {
                                    let enum_name = ctx.shapes[shape_id.0 as usize]
                                        .name
                                        .as_deref()
                                        .unwrap_or("?");
                                    ctx.push_error(format!(
                                        "'{}' is not a variant of '{}'",
                                        hint_name, enum_name
                                    ), loc.clone());
                                    return IrValue::Ref(String::new());
                                }
                                Some(i) => i,
                            };
                            let inner_field_defs =
                                match ctx.shapes.get(hint_tid.0 as usize).map(|t| t.body.clone()) {
                                    Some(IrShapeBody::Struct(fields)) => fields,
                                    _ => {
                                        ctx.push_error(format!(
                                            "variant '{}' is not a struct type",
                                            hint_name
                                        ), loc.clone());
                                        return IrValue::Ref(String::new());
                                    }
                                };
                            let fid = DefId(ctx.defs.len() as u32);
                            ctx.defs.push(IrDef {
                                planned: false,
                                shape_id: hint_tid,
                                base_def: None,
                                name: "_".into(),
                                type_hint: Some(hint_name.to_string()),
                                scope,
                                loc: loc.clone(),
                                fields: vec![],
                                mapper_fn: None,
                                inputs: vec![],
                                outputs: vec![],
                            });
                            let fields =
                                resolve_named_fields(ast_fields, &inner_field_defs, ctx, scope);
                            ctx.defs[fid.0 as usize].fields = fields;
                            return IrValue::Variant(
                                shape_id,
                                idx as u32,
                                Some(Box::new(IrValue::Inst(fid))),
                            );
                        }
                        _ => {
                            ctx.push_error(
                                "inline struct value requires a struct-typed field".into(),
                                loc.clone(),
                            );
                            return IrValue::Ref(String::new());
                        }
                    };
                }
                ctx.push_error(
                    "inline struct value only valid for a single struct-typed field".into(),
                    loc.clone(),
                );
                return IrValue::Ref(String::new());
            }
            resolve_value_against_ref(v, pattern, ctx, scope, loc)
        }

        IrFieldType::List(elem_patterns) => match v {
            AstValue::List(items) => {
                let vals = items
                    .iter()
                    .map(|item| {
                        resolve_list_item(
                            &item.inner,
                            elem_patterns,
                            ctx,
                            scope,
                            &ir_loc(&item.loc),
                        )
                    })
                    .collect();
                IrValue::List(vals)
            }
            _ => {
                ctx.errors.push(IrError {
                    message: "expected list".into(),
                    loc: loc.clone(),
                });
                IrValue::List(vec![])
            }
        },
    }
}

fn resolve_value_against_ref(
    v: &AstValue,
    pattern: &IrRef,
    ctx: &mut Ctx,
    scope: ScopeId,
    loc: &IrLoc,
) -> IrValue {
    let segs = match v {
        AstValue::Ref(r) => {
            if has_group(r) {
                return IrValue::Ref(ref_to_repr(r));
            }
            &r.segments
        }
        _ => {
            ctx.errors.push(IrError {
                message: "expected ref value".into(),
                loc: loc.clone(),
            });
            return IrValue::Ref(String::new());
        }
    };
    let (value_scope, segs) = scope_and_value_segs(segs, ctx, scope);

    if pattern.segments.len() == 1 {
        if segs.len() == 1 {
            return resolve_single_seg_value(
                segs[0].inner.as_plain().unwrap_or(""),
                &pattern.segments[0],
                ctx,
                value_scope,
                loc,
            );
        }
        let is_typed_path = segs.len() > 1
            && segs[0]
                .inner
                .as_plain()
                .map_or(false, |v| ctx.lookup_shape(value_scope, v).is_some());
        if is_typed_path {
            let inst_name = segs.last().and_then(|s| s.inner.as_plain()).unwrap_or("");
            return resolve_single_seg_value(
                inst_name,
                &pattern.segments[0],
                ctx,
                value_scope,
                loc,
            );
        }
    }

    // Multi-segment typed path — segment counts must match.
    if segs.len() != pattern.segments.len() {
        ctx.errors.push(IrError {
            message: format!(
                "typed path has {} segment(s), expected {}",
                segs.len(),
                pattern.segments.len()
            ),
            loc: loc.clone(),
        });
    }

    let vals: Vec<IrValue> = pattern
        .segments
        .iter()
        .zip(segs.iter())
        .map(|(pat_seg, val_seg)| {
            resolve_single_seg_value(
                val_seg.inner.as_plain().unwrap_or(""),
                pat_seg,
                ctx,
                value_scope,
                loc,
            )
        })
        .collect();
    IrValue::Path(vals)
}

fn resolve_single_seg_value(
    raw: &str,
    pat_seg: &IrRefSeg,
    ctx: &mut Ctx,
    scope: ScopeId,
    loc: &IrLoc,
) -> IrValue {
    match &pat_seg.value {
        IrRefSegValue::Shape(tid) => {
            match ctx.shapes.get(tid.0 as usize).map(|t| t.body.clone()) {
                Some(IrShapeBody::Enum(variants)) => {
                    // Try each variant in order: plain string match, then typed (instance lookup).
                    for (idx, variant_ref) in variants.iter().enumerate() {
                        if let [seg] = variant_ref.segments.as_slice() {
                            match &seg.value {
                                IrRefSegValue::Plain(name) if name == raw => {
                                    return IrValue::Variant(*tid, idx as u32, None);
                                }
                                IrRefSegValue::Shape(inner_tid) => {
                                    // Use typed lookup to find instances of the inner type by name.
                                    let iid = ctx.lookup_def_typed(scope, raw, *inner_tid).or_else(
                                        || {
                                            ctx.lookup_def(scope, raw)
                                                .filter(|&i| ctx.def_satisfies_shape(i, *inner_tid))
                                        },
                                    );
                                    if let Some(iid) = iid {
                                        return IrValue::Variant(
                                            *tid,
                                            idx as u32,
                                            Some(Box::new(IrValue::Inst(iid))),
                                        );
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // For unit/marker shapes (e.g. `def secret`), `raw` may be a fun name.
                    if let Some(fid) = ctx.lookup_def_typed(scope, raw, *tid) {
                        return IrValue::Inst(fid);
                    }
                    let type_name = ctx.shapes[tid.0 as usize].name.as_deref().unwrap_or("?");
                    ctx.errors.push(IrError {
                        message: format!("'{}' is not a variant of '{}'", raw, type_name),
                        loc: loc.clone(),
                    });
                    IrValue::Variant(*tid, 0, None)
                }
                Some(IrShapeBody::Struct(_)) => {
                    // Use typed lookup first — handles multi-type same-name defs.
                    let fid = ctx
                        .lookup_def_typed(scope, raw, *tid)
                        .or_else(|| ctx.lookup_def(scope, raw));
                    match fid {
                        Some(fid) => IrValue::Inst(fid),
                        None => {
                            ctx.errors.push(IrError {
                                message: format!(
                                    "'{}' is not a known instance of Shape#{}",
                                    raw, tid.0
                                ),
                                loc: loc.clone(),
                            });
                            IrValue::Ref(raw.to_string())
                        }
                    }
                }
                Some(IrShapeBody::Unit) => {
                    let fid = ctx
                        .lookup_def_typed(scope, raw, *tid)
                        .or_else(|| ctx.lookup_def(scope, raw));
                    match fid {
                        Some(fid) => IrValue::Inst(fid),
                        None => {
                            ctx.errors.push(IrError {
                                message: format!(
                                    "'{}' is not a known instance of Shape#{}",
                                    raw, tid.0
                                ),
                                loc: loc.clone(),
                            });
                            IrValue::Ref(raw.to_string())
                        }
                    }
                }
                Some(IrShapeBody::Primitive(IrPrimitive::Integer)) => match raw.parse::<i64>() {
                    Ok(n) => IrValue::Int(n),
                    Err(_) => {
                        ctx.errors.push(IrError {
                            message: format!("expected integer, got '{}'", raw),
                            loc: loc.clone(),
                        });
                        IrValue::Int(0)
                    }
                },
                Some(IrShapeBody::Primitive(IrPrimitive::Boolean)) => match raw {
                    "true" => IrValue::Bool(true),
                    "false" => IrValue::Bool(false),
                    _ => {
                        ctx.errors.push(IrError {
                            message: format!("expected boolean, got '{}'", raw),
                            loc: loc.clone(),
                        });
                        IrValue::Bool(false)
                    }
                },
                Some(IrShapeBody::Primitive(IrPrimitive::Ipv4)) => {
                    if parse_ipv4(raw).is_some() {
                        IrValue::Str(raw.to_string())
                    } else {
                        ctx.errors.push(IrError {
                            message: format!("expected ipv4, got '{}'", raw),
                            loc: loc.clone(),
                        });
                        IrValue::Str(String::new())
                    }
                }
                Some(IrShapeBody::Primitive(IrPrimitive::Ipv4Net)) => {
                    if is_valid_ipv4net(raw) {
                        IrValue::Str(raw.to_string())
                    } else {
                        ctx.errors.push(IrError {
                            message: format!("expected ipv4net, got '{}'", raw),
                            loc: loc.clone(),
                        });
                        IrValue::Str(String::new())
                    }
                }
                Some(IrShapeBody::Primitive(_)) => IrValue::Str(raw.to_string()),
                None => IrValue::Ref(raw.to_string()),
            }
        }
        IrRefSegValue::Def(iid) => IrValue::Inst(*iid),
        _ => IrValue::Ref(raw.to_string()),
    }
}

fn resolve_list_item(
    v: &AstValue,
    patterns: &[IrRef],
    ctx: &mut Ctx,
    scope: ScopeId,
    loc: &IrLoc,
) -> IrValue {
    // Handle inline struct literals `{ field: val ... }` — find the first matching struct pattern.
    if let AstValue::Struct { .. } = v {
        for pattern in patterns {
            if pattern.segments.len() == 1 {
                if let IrRefSegValue::Shape(tid) = &pattern.segments[0].value {
                    if matches!(
                        ctx.shapes.get(tid.0 as usize).map(|t| &t.body),
                        Some(IrShapeBody::Struct(_))
                    ) {
                        return resolve_value(
                            v,
                            &IrFieldType::Ref(pattern.clone()),
                            ctx,
                            scope,
                            loc,
                        );
                    }
                }
            }
        }
        ctx.errors.push(IrError {
            message: "list item must be a reference".into(),
            loc: loc.clone(),
        });
        return IrValue::Ref(String::new());
    }

    let AstValue::Ref(r) = v else {
        ctx.errors.push(IrError {
            message: "list item must be a reference".into(),
            loc: loc.clone(),
        });
        return IrValue::Ref(String::new());
    };
    if has_group(r) {
        return IrValue::Ref(ref_to_repr(r));
    }
    let (value_scope, segs) = scope_and_value_segs(&r.segments, ctx, scope);

    // `def:TYPE:NAME` — explicit type-qualified instance reference.
    // Resolve directly by type+name, bypassing pattern matching.
    if segs.len() >= 2 && segs[0].inner.as_plain() == Some("def") {
        let type_name = segs[1].inner.as_plain().unwrap_or("");
        let inst_name_seg = segs.get(2);
        if let Some(shape_id) = ctx.lookup_shape(value_scope, type_name) {
            let name = inst_name_seg
                .and_then(|s| s.inner.as_plain())
                .unwrap_or(type_name);
            if let Some(fid) = ctx.lookup_def_typed(value_scope, name, shape_id) {
                return IrValue::Inst(fid);
            }
            ctx.errors.push(IrError {
                message: format!("no instance '{}' of type '{}' in scope", name, type_name),
                loc: loc.clone(),
            });
        } else {
            ctx.errors.push(IrError {
                message: format!("unknown shape '{}' in def: qualifier", type_name),
                loc: loc.clone(),
            });
        }
        return IrValue::Ref(String::new());
    }

    // For typed-path values like `service:api`, the first segment is a type qualifier
    // and the last segment is the actual instance name.
    let is_typed_path = segs.len() > 1
        && segs[0]
            .inner
            .as_plain()
            .map_or(false, |v| ctx.lookup_shape(value_scope, v).is_some());
    let inst_name = if is_typed_path {
        segs.last().unwrap().inner.as_plain().unwrap_or("")
    } else {
        segs[0].inner.as_plain().unwrap_or("")
    };

    // Find which element pattern matches this instance's type.
    let matched_pattern = patterns
        .iter()
        .find(|pattern| {
            if let Some(base_seg) = pattern.segments.first() {
                if let IrRefSegValue::Shape(tid) = &base_seg.value {
                    return ctx.lookup_def_typed(value_scope, inst_name, *tid).is_some();
                }
            }
            false
        })
        .or_else(|| patterns.first());

    // For primitive-typed patterns (e.g. `[ string ]`), join all ref segments as a string value.
    if let Some(pattern) = matched_pattern {
        if pattern.segments.len() == 1 {
            if let IrRefSegValue::Shape(tid) = &pattern.segments[0].value {
                if let Some(IrShapeBody::Primitive(_)) =
                    ctx.shapes.get(tid.0 as usize).map(|t| &t.body)
                {
                    let s = segs
                        .iter()
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
                return resolve_single_seg_value(
                    inst_name,
                    &pattern.segments[0],
                    ctx,
                    value_scope,
                    loc,
                );
            }
        }
    }

    let v_ref = AstValue::Ref(r.clone());
    match matched_pattern {
        Some(pattern) => resolve_value_against_ref(&v_ref, pattern, ctx, scope, loc),
        None => IrValue::Ref(String::new()),
    }
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
    pass4_register_defs(&res.scopes, &mut ctx);
    pass3_resolve_types_and_links(&res.scopes, &mut ctx);
    pass3b_flatten_alias_types(&mut ctx);
    pass_imports(&res.scopes, &mut ctx, ImportKind::Insts);
    pass_bind_imported_bases(&res.scopes, &mut ctx);
    pass5_resolve_def_fields(&res.scopes, &mut ctx);

    let mut errors = ctx.errors;
    errors.extend(res.errors.iter().map(|e| IrError {
        message: e.message.clone(),
        loc: IrLoc {
            unit: e.loc.unit,
            start: e.loc.start,
            end: e.loc.end,
        },
    }));

    IrRes {
        shapes: ctx.shapes,
        defs: ctx.defs,
        scopes: ctx.scopes,
        errors,
    }
}
use std::net::Ipv4Addr;
