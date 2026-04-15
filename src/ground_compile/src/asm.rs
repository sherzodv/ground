/// Generation tree — output of the lowering pass over `IrRes`.
///
/// All typed indices are replaced by resolved string values.
/// `AsmSymbol` holds every named instance in the program.
/// `AsmPlan` is a self-contained plan context created from a `plan name` declaration.
/// Generators walk `AsmPlan` + `AsmSymbol` without needing a global arena or `IrRes`.

use std::collections::{HashMap, HashSet, VecDeque};
use crate::ir::*;
use ground_ts::exec::call_hook;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AsmRes {
    pub plans:  Vec<AsmPlan>,
    pub symbol: AsmSymbol,
}

/// Global symbol table — all named instances in the program.
#[derive(Debug, Clone)]
pub struct AsmSymbol {
    pub insts: Vec<AsmInst>,
}

/// A fully resolved plan context — created from a `plan name` declaration.
/// `reachable` is topo-sorted (leaves first) so generators can walk dependencies in order.
#[derive(Debug, Clone)]
pub struct AsmPlan {
    pub name:      String,        // plan name (e.g. "prd-eu")
    pub root:      AsmInst,       // the root instance (lowered + hooks fired)
    pub fields:    Vec<AsmField>, // root's fields + plan-level overrides
    pub reachable: Vec<AsmInst>,  // all reachable named instances, topo-sorted (leaves first)
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

pub fn lower(ir: &IrRes, ts_src: &str) -> AsmRes {
    // Build hook lookup: TypeId.0 → index into ir.hooks.
    let hook_map: HashMap<u32, usize> = ir.hooks.iter().enumerate()
        .map(|(idx, h)| (h.type_id.0, idx))
        .collect();

    // Shared resolution cache — populated bottom-up as plans are resolved.
    let mut cache: HashMap<InstId, AsmInst> = HashMap::new();

    // Resolve each plan: topo-sort reachable instances, resolve leaves first.
    let plans: Vec<AsmPlan> = ir.plans.iter().filter_map(|plan| {
        // Find the root instance by name.
        let root_id = ir.insts.iter().enumerate()
            .find(|(_, i)| i.name == plan.name)
            .map(|(idx, _)| InstId(idx as u32))?;

        // Collect all named instances reachable from root_id.
        let reachable_ids = collect_reachable(root_id, ir);

        // Topo-sort: leaves (no deps in reachable set) come first.
        let ordered = topo_sort(&reachable_ids, ir);

        // Resolve bottom-up, caching results so via fields can look up post-hook values.
        for &iid in &ordered {
            if !cache.contains_key(&iid) {
                let inst = lower_inst(iid, ir, &hook_map, ts_src, &cache);
                cache.insert(iid, inst);
            }
        }

        let root = cache.get(&root_id)?.clone();

        // Apply plan-level field overrides on top of the root's resolved fields.
        let mut fields = root.fields.clone();
        for plan_field in plan.fields.iter().map(|f| lower_field(f, ir)) {
            if let Some(existing) = fields.iter_mut().find(|f| f.name == plan_field.name) {
                *existing = plan_field;
            } else {
                fields.push(plan_field);
            }
        }

        let reachable: Vec<AsmInst> = ordered.iter()
            .filter_map(|iid| cache.get(iid))
            .cloned()
            .collect();

        Some(AsmPlan { name: plan.name.clone(), root, fields, reachable })
    }).collect();

    // Build symbol table from all named instances; use cache where already resolved.
    let symbol = build_symbol(ir, &hook_map, ts_src, &cache);

    AsmRes { plans, symbol }
}

// ---------------------------------------------------------------------------
// Graph helpers: reachability + topo-sort
// ---------------------------------------------------------------------------

/// Collect all named instance IDs reachable from `root` via `IrValue::Inst` references.
/// Anonymous instances (name == "_") are traversed but not collected.
fn collect_reachable(root: InstId, ir: &IrRes) -> HashSet<InstId> {
    let mut visited: HashSet<InstId> = HashSet::new();
    let mut queue: VecDeque<InstId> = VecDeque::new();
    queue.push_back(root);
    while let Some(id) = queue.pop_front() {
        if !visited.insert(id) { continue; }
        let inst = &ir.insts[id.0 as usize];
        for f in &inst.fields {
            enqueue_named_inst_refs(&f.value, ir, &mut queue);
        }
    }
    visited
}

fn enqueue_named_inst_refs(v: &IrValue, ir: &IrRes, queue: &mut VecDeque<InstId>) {
    match v {
        IrValue::Inst(iid) => {
            let child = &ir.insts[iid.0 as usize];
            if child.name != "_" {
                queue.push_back(*iid);
            } else {
                // Anonymous: traverse its fields to find named refs within.
                for f in &child.fields {
                    enqueue_named_inst_refs(&f.value, ir, queue);
                }
            }
        }
        IrValue::List(items) => { for i in items { enqueue_named_inst_refs(i, ir, queue); } }
        IrValue::Path(segs)  => { for s in segs  { enqueue_named_inst_refs(s, ir, queue); } }
        _ => {}
    }
}

/// Kahn's topo-sort over a set of named instance IDs.
/// Returns them in dependency order — leaves (no deps) first, root last.
fn topo_sort(ids: &HashSet<InstId>, ir: &IrRes) -> Vec<InstId> {
    // For each id, compute its direct named-instance deps (within ids).
    let mut deps_of: HashMap<InstId, HashSet<InstId>> = HashMap::new();
    let mut dependents_of: HashMap<InstId, Vec<InstId>> = HashMap::new();

    for &id in ids {
        let mut deps: HashSet<InstId> = HashSet::new();
        let inst = &ir.insts[id.0 as usize];
        for f in &inst.fields {
            collect_named_deps_in_value(&f.value, ids, ir, &mut deps);
        }
        // Remove self-loops (can arise from self-referential field values in the source).
        deps.remove(&id);
        for &dep in &deps {
            dependents_of.entry(dep).or_default().push(id);
        }
        deps_of.insert(id, deps);
    }

    let mut in_degree: HashMap<InstId, usize> = deps_of.iter()
        .map(|(&id, deps)| (id, deps.len()))
        .collect();

    let mut queue: VecDeque<InstId> = in_degree.iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut result = Vec::with_capacity(ids.len());
    while let Some(id) = queue.pop_front() {
        result.push(id);
        if let Some(dependents) = dependents_of.get(&id) {
            for &dep in dependents {
                let d = in_degree.get_mut(&dep).unwrap();
                *d -= 1;
                if *d == 0 {
                    queue.push_back(dep);
                }
            }
        }
    }

    result
}

fn collect_named_deps_in_value(
    v:   &IrValue,
    ids: &HashSet<InstId>,
    ir:  &IrRes,
    out: &mut HashSet<InstId>,
) {
    match v {
        IrValue::Inst(iid) => {
            let child = &ir.insts[iid.0 as usize];
            if child.name != "_" {
                if ids.contains(iid) { out.insert(*iid); }
            } else {
                // Anonymous: look through its fields for named deps.
                for f in &child.fields {
                    collect_named_deps_in_value(&f.value, ids, ir, out);
                }
            }
        }
        IrValue::List(items) => { for i in items { collect_named_deps_in_value(i, ids, ir, out); } }
        IrValue::Path(segs)  => { for s in segs  { collect_named_deps_in_value(s, ids, ir, out); } }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Symbol building
// ---------------------------------------------------------------------------

/// Build the global symbol table from all named instances.
/// Uses the cache for instances already resolved during plan walks;
/// resolves fresh (with empty cache context) for any remaining.
fn build_symbol(
    ir:       &IrRes,
    hook_map: &HashMap<u32, usize>,
    ts_src:   &str,
    cache:    &HashMap<InstId, AsmInst>,
) -> AsmSymbol {
    let insts = ir.insts.iter().enumerate()
        .filter(|(_, i)| i.name != "_")
        .map(|(idx, _)| {
            let iid = InstId(idx as u32);
            cache.get(&iid).cloned()
                .unwrap_or_else(|| lower_inst(iid, ir, hook_map, ts_src, cache))
        })
        .collect();
    AsmSymbol { insts }
}

// ---------------------------------------------------------------------------
// Instance / field / value lowering
// ---------------------------------------------------------------------------

fn lower_inst(
    id:       InstId,
    ir:       &IrRes,
    hook_map: &HashMap<u32, usize>,
    ts_src:   &str,
    cache:    &HashMap<InstId, AsmInst>,
) -> AsmInst {
    let inst      = &ir.insts[id.0 as usize];
    let type_name = ir.types[inst.type_id.0 as usize].name.clone().unwrap_or_else(|| "_".into());
    let name      = inst.name.clone();
    let type_hint = inst.type_hint.clone();

    // Lower all user-provided fields to AsmValue (raw, for ASM output).
    let mut fields: Vec<AsmField> = inst.fields.iter().map(|f| lower_field(f, ir)).collect();

    // If this type has a hook def and we have TypeScript source, execute the hook.
    if !ts_src.is_empty() {
        if let Some(&hook_idx) = hook_map.get(&inst.type_id.0) {
            let hook_def = &ir.hooks[hook_idx];

            // Build input JSON from the instance's user-provided fields (the input links).
            // Fields marked `via` use the pre-resolved (post-hook) cached AsmInst as their value.
            let input_link_set: HashSet<u32> = hook_def.inputs.iter().map(|l| l.0).collect();
            let input_map: serde_json::Map<String, serde_json::Value> = inst.fields.iter()
                .filter(|f| input_link_set.contains(&f.link_id.0))
                .map(|f| {
                    let json_val = if f.via {
                        ir_value_to_json_via(&f.value, ir, cache)
                    } else {
                        ir_value_to_json(&f.value, ir)
                    };
                    (f.name.clone(), json_val)
                })
                .collect();
            let input_json = serde_json::Value::Object(input_map).to_string();

            // Call the TypeScript hook and merge output fields into the instance.
            match call_hook(ts_src, &hook_def.hook_fn, &input_json) {
                Ok(output_json) => {
                    if let Ok(serde_json::Value::Object(map)) =
                        serde_json::from_str::<serde_json::Value>(&output_json)
                    {
                        for (k, v) in map {
                            fields.push(AsmField { name: k, value: json_val_to_asm_value(&v) });
                        }
                    }
                }
                Err(_) => {
                    // Hook execution errors surface at generation time when expected
                    // fields are missing. Silently continue so other instances resolve.
                }
            }
        }
    }

    AsmInst { type_name, name, type_hint, fields }
}

fn lower_field(f: &IrField, ir: &IrRes) -> AsmField {
    AsmField { name: f.name.clone(), value: lower_value(&f.value, ir) }
}

fn lower_value(v: &IrValue, ir: &IrRes) -> AsmValue {
    // Note: lower_value does not execute hooks on inline anonymous instances.
    // Hooks only fire on named instances via lower_inst (hook_map/ts_src path).
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
                // Anonymous inline instance — embed fully (no hook execution for inline insts).
                let fields    = inst.fields.iter().map(|f| lower_field(f, ir)).collect();
                let type_hint = inst.type_hint.clone();
                AsmValue::Inst(Box::new(AsmInst {
                    type_name: ir.types[inst.type_id.0 as usize].name.clone().unwrap_or_else(|| "_".into()),
                    name: "_".into(),
                    type_hint,
                    fields,
                }))
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
// Hook I/O serialisation helpers
// ---------------------------------------------------------------------------

/// Convert an `IrValue` to a `serde_json::Value` for passing to a TypeScript hook.
fn ir_value_to_json(v: &IrValue, ir: &IrRes) -> serde_json::Value {
    match v {
        IrValue::Str(s)  => serde_json::Value::String(s.clone()),
        IrValue::Int(n)  => serde_json::json!(*n),
        IrValue::Ref(s)  => serde_json::Value::String(s.clone()),

        IrValue::Variant(tid, idx, payload) => {
            let td = &ir.types[tid.0 as usize];
            let variant_str = match &td.body {
                IrTypeBody::Enum(vs) => match vs.get(*idx as usize)
                    .and_then(|r| r.segments.first())
                    .map(|s| &s.value)
                {
                    Some(IrRefSegValue::Plain(p)) => p.clone(),
                    Some(IrRefSegValue::Type(vtid)) =>
                        ir.types[vtid.0 as usize].name.clone().unwrap_or_else(|| "_".into()),
                    _ => "_".into(),
                },
                _ => "_".into(),
            };
            match payload {
                Some(p) => serde_json::json!({ variant_str: ir_value_to_json(p, ir) }),
                None    => serde_json::Value::String(variant_str),
            }
        }

        IrValue::Inst(iid) => {
            let child = &ir.insts[iid.0 as usize];
            let mut map = serde_json::Map::new();
            // Expose the instance name as "_name" so hooks can use it.
            map.insert("_name".into(), serde_json::Value::String(child.name.clone()));
            for f in &child.fields {
                map.insert(f.name.clone(), ir_value_to_json(&f.value, ir));
            }
            serde_json::Value::Object(map)
        }

        IrValue::Path(segs)  => serde_json::Value::Array(
            segs.iter().map(|s| ir_value_to_json(s, ir)).collect()),

        IrValue::List(items) => serde_json::Value::Array(
            items.iter().map(|i| ir_value_to_json(i, ir)).collect()),
    }
}

/// Like `ir_value_to_json` but for `IrValue::Inst` uses the post-hook cached `AsmInst`.
/// Used for fields marked `via` — the hook receives the already-resolved child value.
fn ir_value_to_json_via(v: &IrValue, ir: &IrRes, cache: &HashMap<InstId, AsmInst>) -> serde_json::Value {
    match v {
        IrValue::Inst(iid) => {
            if let Some(asm_inst) = cache.get(iid) {
                asm_inst_to_json(asm_inst)
            } else {
                // Not yet cached (shouldn't happen in bottom-up walk) — fall back to raw.
                ir_value_to_json(v, ir)
            }
        }
        // For non-Inst values, via has no special meaning.
        _ => ir_value_to_json(v, ir),
    }
}

fn asm_inst_to_json(inst: &AsmInst) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("_name".into(), serde_json::Value::String(inst.name.clone()));
    for f in &inst.fields {
        map.insert(f.name.clone(), asm_value_to_json(&f.value));
    }
    serde_json::Value::Object(map)
}

pub fn asm_value_to_json(v: &AsmValue) -> serde_json::Value {
    match v {
        AsmValue::Str(s)      => serde_json::Value::String(s.clone()),
        AsmValue::Int(n)      => serde_json::json!(*n),
        AsmValue::Ref(s)      => serde_json::Value::String(s.clone()),
        AsmValue::Variant(gv) => match &gv.payload {
            Some(p) => serde_json::json!({ gv.value.clone(): asm_value_to_json(p) }),
            None    => serde_json::Value::String(gv.value.clone()),
        },
        AsmValue::InstRef(r)  => serde_json::json!({ "_name": r.name, "type_name": r.type_name }),
        AsmValue::Inst(i)     => asm_inst_to_json(i),
        AsmValue::Path(segs)  => serde_json::Value::Array(segs.iter().map(asm_value_to_json).collect()),
        AsmValue::List(items) => serde_json::Value::Array(items.iter().map(asm_value_to_json).collect()),
    }
}

/// Convert a `serde_json::Value` returned by a hook into an `AsmValue`.
fn json_val_to_asm_value(v: &serde_json::Value) -> AsmValue {
    match v {
        serde_json::Value::String(s)  => AsmValue::Str(s.clone()),
        serde_json::Value::Number(n)  => {
            if let Some(i) = n.as_i64() { AsmValue::Int(i) }
            else { AsmValue::Str(n.to_string()) }
        }
        serde_json::Value::Bool(b)    => AsmValue::Str(b.to_string()),
        serde_json::Value::Null       => AsmValue::Str("null".into()),
        serde_json::Value::Array(arr) =>
            AsmValue::List(arr.iter().map(json_val_to_asm_value).collect()),
        serde_json::Value::Object(map) => {
            let fields = map.iter().map(|(k, v)| AsmField {
                name:  k.clone(),
                value: json_val_to_asm_value(v),
            }).collect();
            AsmValue::Inst(Box::new(AsmInst {
                type_name: String::new(),
                name:      "_".into(),
                type_hint: None,
                fields,
            }))
        }
    }
}
