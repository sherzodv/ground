/// Generation tree — output of the lowering pass over `IrRes`.
///
/// All typed indices are replaced by resolved string values.
/// `AsmSymbol` holds every named instance in the program.
/// `AsmDeploy` is self-contained except for `InstRef` values,
/// which are resolved by name against `AsmRes::symbol`.
/// Generators walk `AsmDeploy` + `AsmSymbol` without needing a global arena or `IrRes`.

use std::collections::{HashMap, HashSet};
use crate::ir::*;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AsmRes {
    pub deploys:  Vec<AsmDeploy>,
    pub symbol:   AsmSymbol,
    pub type_fns: Vec<AsmTypeFnDef>,
}

/// Global symbol table — all named instances in the program.
#[derive(Debug, Clone)]
pub struct AsmSymbol {
    pub insts: Vec<AsmInst>,
}

/// A fully resolved, self-contained deployment context.
#[derive(Debug, Clone)]
pub struct AsmDeploy {
    pub target:    Vec<String>,          // scope path segments, e.g. ["aws", "ecs", "stack"]
    pub name:      String,               // deployment name, e.g. "prod"
    pub inst:      AsmInst,              // the instance being deployed
    pub fields:    Vec<AsmField>,        // deploy-specific fields (separate from inst fields)
    pub type_fn:   Option<String>,       // named type fn resolved from deploy target tail
    pub expansion: Option<AsmExpansion>, // recursive structural walk result
    pub overrides: AsmOverrides,         // link-slot and field overrides threaded through walk
}

/// Override set threaded through the recursive expansion walk.
#[derive(Debug, Clone, Default)]
pub struct AsmOverrides {
    /// Link slot key → named type fn name; selects an explicit fn for pair expansion.
    pub link_fns: HashMap<String, String>,
}

/// Result of recursively expanding one instance through type functions.
#[derive(Debug, Clone)]
pub struct AsmExpansion {
    pub inst:      AsmInst,
    pub outputs:   Vec<AsmOutput>,       // from 1-param type function firing
    pub link_outs: Vec<AsmLinkOutput>,   // from 2-param pair function firing
    pub children:  Vec<AsmExpansion>,    // recursive walk of inst links
}

/// One fired type-function entry — all `{param:field}` refs substituted.
#[derive(Debug, Clone)]
pub struct AsmOutput {
    pub alias:       String,
    pub vendor_type: String,
    pub fields:      Vec<AsmField>,   // fully substituted
    pub scope:       Vec<String>,     // pack path for template lookup
}

/// Outputs produced by firing a 2-param pair function on a (from, to) instance pair.
#[derive(Debug, Clone)]
pub struct AsmLinkOutput {
    pub from:    AsmInstRef,
    pub to:      AsmInstRef,
    pub outputs: Vec<AsmOutput>,
}

// ---------------------------------------------------------------------------
// Type fn ASM types
// ---------------------------------------------------------------------------

/// One entry in a type function at ASM level: `alias: VendorType { fields... }`
/// Field values may contain param placeholder strings like `{param_name:field_name}`.
#[derive(Debug, Clone)]
pub struct AsmTypeFnEntry {
    pub alias:       String,
    pub vendor_type: String,
    pub fields:      Vec<AsmField>,
}

/// A single named parameter in a type function def.
#[derive(Debug, Clone)]
pub struct AsmTypeFnParam {
    pub name:      String,   // e.g. "this", "from", "to"
    pub type_name: String,   // e.g. "service"
}

/// Type function def at ASM level — IDs replaced by strings.
/// - `params.len() == 1` → fires per matching instance
/// - `params.len() == 2` → fires per (from, to) pair
/// - `name.is_none()`   → anonymous (auto-fires during walk)
#[derive(Debug, Clone)]
pub struct AsmTypeFnDef {
    pub name:   Option<String>,
    pub params: Vec<AsmTypeFnParam>,
    pub scope:  Vec<String>,    // pack path segments
    pub body:   Vec<AsmTypeFnEntry>,
}

/// A fully resolved instance — no IDs, type name inlined.
#[derive(Debug, Clone)]
pub struct AsmInst {
    pub type_name: String,
    pub name:      String,
    pub type_hint: Option<String>, // explicit type annotation from source, if present
    pub fields:    Vec<AsmField>,
}

#[derive(Debug, Clone)]
pub struct AsmField {
    pub name:  String,
    pub value: AsmValue,
}

#[derive(Debug, Clone)]
pub enum AsmValue {
    Str(String),
    Int(i64),
    Ref(String),                 // reference primitive (opaque)
    Variant(AsmVariant),         // enum variant with type context
    InstRef(AsmInstRef),         // named instance — full data available in AsmRes::symbol
    Inst(Box<AsmInst>),          // anonymous inline instance (name == "_" in IrRes)
    Path(Vec<AsmValue>),         // multi-segment typed path, e.g. Variant:Variant
    List(Vec<AsmValue>),
}

#[derive(Debug, Clone)]
pub struct AsmVariant {
    pub type_name: String,              // enum type name, e.g. "zone"
    pub value:     String,              // plain variant string or typed variant type name
    pub payload:   Option<Box<AsmValue>>, // typed variant payload (inst ref or inline inst)
}

#[derive(Debug, Clone)]
pub struct AsmInstRef {
    pub type_name: String,
    pub name:      String,       // key for lookup in AsmRes::symbol
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn lower(ir: &IrRes) -> AsmRes {
    // Symbol and type_fns must be computed first — deploy expansion needs both.
    let symbol   = lower_symbol(ir);
    let type_fns: Vec<AsmTypeFnDef> = ir.type_fns.iter().map(|f| lower_type_fn_def(f, ir)).collect();
    let deploys  = ir.deploys.iter().map(|d| lower_deploy(d, ir, &symbol, &type_fns)).collect();
    AsmRes { deploys, symbol, type_fns }
}

// ---------------------------------------------------------------------------
// Symbol lowering
// ---------------------------------------------------------------------------

fn lower_symbol(ir: &IrRes) -> AsmSymbol {
    let insts = ir.insts.iter().enumerate()
        .filter(|(_, i)| i.name != "_")
        .map(|(idx, _)| lower_inst(InstId(idx as u32), ir))
        .collect();
    AsmSymbol { insts }
}

// ---------------------------------------------------------------------------
// Deploy lowering
// ---------------------------------------------------------------------------

fn lower_deploy(dep: &IrDeployDef, ir: &IrRes, symbol: &AsmSymbol, type_fns: &[AsmTypeFnDef]) -> AsmDeploy {
    let target = ref_to_strings(&dep.target, ir);
    let name   = ref_to_strings(&dep.name, ir).join(":");

    let type_fn = dep.to_type_fn.map(|fid| {
        ir.type_fns[fid.0 as usize].name.clone().unwrap_or_default()
    });

    let what_id = match find_inst_in_ref(&dep.what, ir) {
        Some(id) => id,
        None     => {
            let placeholder = AsmInst { type_name: String::new(), name: String::new(), type_hint: None, fields: vec![] };
            let overrides   = AsmOverrides::default();
            return AsmDeploy { target, name, inst: placeholder, fields: vec![], type_fn, expansion: None, overrides };
        }
    };

    let inst   = lower_inst(what_id, ir);
    let fields = dep.fields.iter().map(|f| lower_field(f, ir)).collect();

    let overrides = AsmOverrides::default();
    let mut visited = HashSet::new();
    // Named type fn from deploy target takes priority over anonymous fn for the root inst.
    let explicit_fn = dep.to_type_fn.map(|fid| fid.0 as usize);
    let exp = expand_with_explicit_fn(&inst, explicit_fn, type_fns, symbol, &overrides, &mut visited);
    let expansion = if exp.outputs.is_empty() && exp.link_outs.is_empty() && exp.children.is_empty() {
        None
    } else {
        Some(exp)
    };

    AsmDeploy { target, name, inst, fields, type_fn, expansion, overrides }
}

// ---------------------------------------------------------------------------
// Recursive expansion walk
// ---------------------------------------------------------------------------

/// Recursively expand `inst` through matching anonymous type functions.
/// Returns an `AsmExpansion` tree; empty branches are still returned (callers collapse if needed).
pub fn expand_inst(
    inst:      &AsmInst,
    type_fns:  &[AsmTypeFnDef],
    symbol:    &AsmSymbol,
    overrides: &AsmOverrides,
    visited:   &mut HashSet<String>,
) -> AsmExpansion {
    expand_with_explicit_fn(inst, None, type_fns, symbol, overrides, visited)
}

/// Like `expand_inst` but allows specifying an explicit type fn index for the root node.
/// `explicit_fn = Some(idx)` → fire `type_fns[idx]` on `inst` instead of the anonymous fn.
/// Children always use anonymous fn lookup.
fn expand_with_explicit_fn(
    inst:        &AsmInst,
    explicit_fn: Option<usize>,
    type_fns:    &[AsmTypeFnDef],
    symbol:      &AsmSymbol,
    overrides:   &AsmOverrides,
    visited:     &mut HashSet<String>,
) -> AsmExpansion {
    // Cycle guard — only named instances can form cycles.
    if inst.name != "_" && !visited.insert(inst.name.clone()) {
        return AsmExpansion { inst: inst.clone(), outputs: vec![], link_outs: vec![], children: vec![] };
    }

    // 1. Fire named (explicit) or anonymous 1-param type fn for this instance's type.
    let outputs = if let Some(idx) = explicit_fn {
        fire_1param(&type_fns[idx], inst)
    } else {
        find_anon_1param_fn(&inst.type_name, type_fns)
            .map(|tf| fire_1param(tf, inst))
            .unwrap_or_default()
    };

    // 2. Collect all InstRef targets from this instance's fields.
    let mut refs: Vec<AsmInstRef> = Vec::new();
    for f in &inst.fields {
        collect_inst_refs_in_value(&f.value, &mut refs);
    }

    // 3. Recurse into referenced instances that have a matching type fn.
    let mut children = Vec::new();
    for r in &refs {
        if has_any_fn_for_type(&r.type_name, type_fns) {
            if let Some(child) = symbol.insts.iter().find(|i| i.name == r.name) {
                children.push(expand_inst(child, type_fns, symbol, overrides, visited));
            }
        }
    }

    // 4. Fire anonymous 2-param pair fns for each (inst → ref) pair.
    let mut link_outs = Vec::new();
    for r in &refs {
        if let Some(tf) = find_anon_2param_fn(&inst.type_name, &r.type_name, type_fns) {
            if let Some(to_inst) = symbol.insts.iter().find(|i| i.name == r.name) {
                let from_ref = AsmInstRef { type_name: inst.type_name.clone(), name: inst.name.clone() };
                let to_ref   = AsmInstRef { type_name: r.type_name.clone(),    name: r.name.clone() };
                let outputs  = fire_2param(tf, inst, to_inst);
                link_outs.push(AsmLinkOutput { from: from_ref, to: to_ref, outputs });
            }
        }
    }

    AsmExpansion { inst: inst.clone(), outputs, link_outs, children }
}

// ---------------------------------------------------------------------------
// Type fn firing
// ---------------------------------------------------------------------------

fn fire_1param(tf: &AsmTypeFnDef, inst: &AsmInst) -> Vec<AsmOutput> {
    debug_assert_eq!(tf.params.len(), 1);
    let param_name = &tf.params[0].name;
    fire_entries(&tf.body, &[(param_name.as_str(), inst)], &tf.scope)
}

fn fire_2param(tf: &AsmTypeFnDef, from: &AsmInst, to: &AsmInst) -> Vec<AsmOutput> {
    debug_assert_eq!(tf.params.len(), 2);
    let from_name = &tf.params[0].name;
    let to_name   = &tf.params[1].name;
    fire_entries(&tf.body, &[(from_name.as_str(), from), (to_name.as_str(), to)], &tf.scope)
}

fn fire_entries(
    body:     &[AsmTypeFnEntry],
    bindings: &[(&str, &AsmInst)],
    scope:    &[String],
) -> Vec<AsmOutput> {
    body.iter().map(|entry| {
        let fields = entry.fields.iter().map(|f| AsmField {
            name:  f.name.clone(),
            value: substitute_value(&f.value, bindings),
        }).collect();
        AsmOutput { alias: entry.alias.clone(), vendor_type: entry.vendor_type.clone(), fields, scope: scope.to_vec() }
    }).collect()
}

// ---------------------------------------------------------------------------
// Param substitution
// ---------------------------------------------------------------------------

/// Substitute `{param_name:field_name}` placeholders in a value.
/// If the whole `Ref` string is consumed by one substitution, the inner value type is preserved.
/// If it's embedded in a larger string, the result is `Str` (string interpolation).
fn substitute_value(v: &AsmValue, bindings: &[(&str, &AsmInst)]) -> AsmValue {
    match v {
        AsmValue::Ref(s) => {
            // Check for exact single-placeholder match: "{param:field}"
            if let Some(inner) = s.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                if let Some((param_name, field_name)) = inner.split_once(':') {
                    if let Some((_, inst)) = bindings.iter().find(|(n, _)| *n == param_name) {
                        // Special intrinsic: {param:name} → the instance name.
                        if field_name == "name" {
                            return AsmValue::Str(inst.name.clone());
                        }
                        if let Some(field) = inst.fields.iter().find(|f| f.name == field_name) {
                            return field.value.clone();
                        }
                    }
                }
            }
            // String interpolation: replace all {param:field} occurrences in the string.
            let mut result = s.clone();
            let mut changed = false;
            for (param_name, inst) in bindings {
                // Special intrinsic: {param:name} → the instance name.
                let name_placeholder = format!("{{{param_name}:name}}");
                if result.contains(&name_placeholder) {
                    result = result.replace(&name_placeholder, &inst.name);
                    changed = true;
                }
                for field in &inst.fields {
                    let placeholder = format!("{{{param_name}:{}}}", field.name);
                    if result.contains(&placeholder) {
                        result = result.replace(&placeholder, &value_to_str(&field.value));
                        changed = true;
                    }
                }
            }
            if changed { AsmValue::Str(result) } else { AsmValue::Ref(result) }
        }
        AsmValue::List(items) => AsmValue::List(items.iter().map(|i| substitute_value(i, bindings)).collect()),
        AsmValue::Path(segs)  => AsmValue::Path(segs.iter().map(|s| substitute_value(s, bindings)).collect()),
        _ => v.clone(),
    }
}

fn value_to_str(v: &AsmValue) -> String {
    match v {
        AsmValue::Str(s)      => s.clone(),
        AsmValue::Ref(s)      => s.clone(),
        AsmValue::Int(n)      => n.to_string(),
        AsmValue::Variant(v)  => v.value.clone(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Type fn lookup helpers
// ---------------------------------------------------------------------------

fn find_anon_1param_fn<'a>(type_name: &str, type_fns: &'a [AsmTypeFnDef]) -> Option<&'a AsmTypeFnDef> {
    type_fns.iter().find(|f| {
        f.name.is_none() && f.params.len() == 1 && f.params[0].type_name == type_name
    })
}

fn find_anon_2param_fn<'a>(from_type: &str, to_type: &str, type_fns: &'a [AsmTypeFnDef]) -> Option<&'a AsmTypeFnDef> {
    type_fns.iter().find(|f| {
        f.name.is_none()
            && f.params.len() == 2
            && f.params[0].type_name == from_type
            && f.params[1].type_name == to_type
    })
}

fn has_any_fn_for_type(type_name: &str, type_fns: &[AsmTypeFnDef]) -> bool {
    type_fns.iter().any(|f| {
        f.params.first().map_or(false, |p| p.type_name == type_name)
    })
}

// ---------------------------------------------------------------------------
// InstRef collection from values
// ---------------------------------------------------------------------------

fn collect_inst_refs_in_value(v: &AsmValue, out: &mut Vec<AsmInstRef>) {
    match v {
        AsmValue::InstRef(r) => out.push(r.clone()),
        AsmValue::Path(segs) => { for s in segs { collect_inst_refs_in_value(s, out); } }
        AsmValue::List(items) => { for i in items { collect_inst_refs_in_value(i, out); } }
        AsmValue::Inst(i) => { for f in &i.fields { collect_inst_refs_in_value(&f.value, out); } }
        AsmValue::Variant(v) => {
            if let Some(p) = &v.payload { collect_inst_refs_in_value(p, out); }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Return path segment names up to (and including) `scope`.
// ---------------------------------------------------------------------------

fn scope_path_names(scope: ScopeId, ir: &IrRes) -> Vec<String> {
    let mut path = Vec::new();
    let mut cur  = scope;
    loop {
        let s = &ir.scopes[cur.0 as usize];
        if let Some(n) = &s.name { path.push(n.clone()); }
        match s.parent {
            Some(p) if p != cur => cur = p,
            _                   => break,
        }
    }
    path.reverse();
    path
}

// ---------------------------------------------------------------------------
// Instance / field / value lowering
// ---------------------------------------------------------------------------

fn lower_inst(id: InstId, ir: &IrRes) -> AsmInst {
    let inst      = &ir.insts[id.0 as usize];
    let type_name = ir.types[inst.type_id.0 as usize].name.clone().unwrap_or_else(|| "_".into());
    let name      = inst.name.clone();
    let type_hint = inst.type_hint.clone();
    let fields    = inst.fields.iter().map(|f| lower_field(f, ir)).collect();
    AsmInst { type_name, name, type_hint, fields }
}

fn lower_field(f: &IrField, ir: &IrRes) -> AsmField {
    AsmField { name: f.name.clone(), value: lower_value(&f.value, ir) }
}

fn lower_value(v: &IrValue, ir: &IrRes) -> AsmValue {
    match v {
        IrValue::Str(s) => AsmValue::Str(s.clone()),
        IrValue::Int(n) => AsmValue::Int(*n),
        IrValue::Ref(s) => AsmValue::Ref(s.clone()),

        IrValue::Variant(tid, idx, payload) => {
            let td        = &ir.types[tid.0 as usize];
            let type_name = td.name.clone().unwrap_or_else(|| "_".into());
            let (value, asm_payload) = match &td.body {
                IrTypeBody::Enum(vs) => match vs[*idx as usize].segments.first().map(|s| &s.value) {
                    Some(IrRefSegValue::Plain(p)) => (p.clone(), None),
                    Some(IrRefSegValue::Type(vtid)) => {
                        let variant_type_name = ir.types[vtid.0 as usize].name.clone().unwrap_or_else(|| "_".into());
                        let asm_p = payload.as_deref().map(|p| Box::new(lower_value(p, ir)));
                        (variant_type_name, asm_p)
                    }
                    _ => ("?".into(), None),
                },
                _ => ("?".into(), None),
            };
            AsmValue::Variant(AsmVariant { type_name, value, payload: asm_payload })
        }

        IrValue::Inst(iid) => {
            let inst = &ir.insts[iid.0 as usize];
            if inst.name == "_" {
                // Anonymous inline instance — embed fully.
                AsmValue::Inst(Box::new(lower_inst(*iid, ir)))
            } else {
                // Named instance — emit a ref; full data lives in AsmSymbol.
                let type_name = ir.types[inst.type_id.0 as usize].name.clone().unwrap_or_else(|| "_".into());
                AsmValue::InstRef(AsmInstRef { type_name, name: inst.name.clone() })
            }
        }

        IrValue::Path(segs) => {
            AsmValue::Path(segs.iter().map(|s| lower_value(s, ir)).collect())
        }

        IrValue::List(items) => {
            AsmValue::List(items.iter().map(|s| lower_value(s, ir)).collect())
        }
    }
}

// ---------------------------------------------------------------------------
// Type fn def lowering
// ---------------------------------------------------------------------------

fn lower_type_fn_def(f: &IrTypeFnDef, ir: &IrRes) -> AsmTypeFnDef {
    let name   = f.name.clone();
    let params = f.params.iter()
        .map(|p| AsmTypeFnParam {
            name:      p.name.clone(),
            type_name: ir.types[p.ty.0 as usize].name.clone().unwrap_or_default(),
        })
        .collect();
    let scope  = scope_path_names(f.scope, ir);
    let body   = f.body.iter().map(|entry| {
        let vendor_type = ir.types[entry.vendor_type.0 as usize].name.clone().unwrap_or_default();
        let fields = entry.fields.iter().map(|bf| AsmField {
            name:  bf.name.clone(),
            value: lower_type_fn_field_value(&bf.value),
        }).collect();
        AsmTypeFnEntry { alias: entry.alias.clone(), vendor_type, fields }
    }).collect();
    AsmTypeFnDef { name, params, scope, body }
}

/// Lower a type fn body field value. Param placeholder refs like `{this:name}-sg`
/// are kept as-is (`AsmValue::Ref`) for expand-time substitution.
fn lower_type_fn_field_value(v: &IrValue) -> AsmValue {
    match v {
        IrValue::Str(s) => AsmValue::Str(s.clone()),
        IrValue::Ref(s) => AsmValue::Ref(s.clone()),
        IrValue::Int(n) => AsmValue::Int(*n),
        _               => AsmValue::Ref(String::new()),
    }
}

// ---------------------------------------------------------------------------
// Ref helpers
// ---------------------------------------------------------------------------

/// Flatten an IrRef to a list of name strings.
fn ref_to_strings(r: &IrRef, ir: &IrRes) -> Vec<String> {
    r.segments.iter().map(|seg| match &seg.value {
        IrRefSegValue::Pack(id)  => ir.scopes[id.0 as usize].name.clone().unwrap_or_else(|| "_".into()),
        IrRefSegValue::Type(id)  => ir.types[id.0 as usize].name.clone().unwrap_or_else(|| "_".into()),
        IrRefSegValue::Link(id)  => ir.links[id.0 as usize].name.clone().unwrap_or_else(|| "_".into()),
        IrRefSegValue::Inst(id)  => ir.insts[id.0 as usize].name.clone(),
        IrRefSegValue::Plain(s)  => s.clone(),
    }).collect()
}

/// Find the first InstId in an IrRef, falling back to name-lookup for Plain segments.
fn find_inst_in_ref(r: &IrRef, ir: &IrRes) -> Option<InstId> {
    for seg in &r.segments {
        match &seg.value {
            IrRefSegValue::Inst(id) => return Some(*id),
            IrRefSegValue::Plain(name) => {
                // Deploy `what` may be unresolved if the instance was declared after
                // the deploy in source; fall back to a linear name scan.
                if let Some(idx) = ir.insts.iter().position(|i| &i.name == name) {
                    return Some(InstId(idx as u32));
                }
            }
            _ => {}
        }
    }
    None
}
