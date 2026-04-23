use crate::ir::*;
use ground_ts::exec::{call_hook, call_hook_args};
/// Generation tree — output of the lowering pass over `IrRes`.
///
/// All typed indices are replaced by resolved string values.
/// ASM is plan-driven: without any `plan` declarations, lowering produces no
/// defs. ASM returns only the fully resolved planned defs.
/// Generators walk these defs directly without needing the original `IrRes`.
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;

// ---------------------------------------------------------------------------
// Data shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AsmRes {
    pub defs: Vec<AsmDef>,
}

/// A fully resolved def — no IDs, shape name inlined.
#[derive(Debug, Clone)]
pub struct AsmDef {
    pub type_name: String,
    pub name: String,
    pub type_hint: Option<String>, // explicit type annotation from source, if present
    pub fields: Vec<AsmField>,
}

#[derive(Debug, Clone)]
pub struct AsmField {
    pub name: String,
    pub value: AsmValue,
}

#[derive(Debug, Clone)]
pub enum AsmValue {
    Str(String),
    Int(i64),
    Bool(bool),
    Ref(String),         // reference primitive (opaque)
    Variant(AsmVariant), // enum variant with type context
    DefRef(AsmDefRef),   // named def ref
    Def(Box<AsmDef>),    // anonymous inline def (name == "_" in IrRes)
    Path(Vec<AsmValue>), // multi-segment typed path, e.g. Variant:Variant
    List(Vec<AsmValue>),
}

#[derive(Debug, Clone)]
pub struct AsmVariant {
    pub type_name: String,              // enum type name, e.g. "zone"
    pub value: String,                  // plain variant string or typed variant type name
    pub payload: Option<Box<AsmValue>>, // typed variant payload (def ref or inline def)
}

#[derive(Debug, Clone)]
pub struct AsmDefRef {
    pub type_name: String,
    pub name: String, // key for lookup in AsmRes::symbol
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn lower(ir: &IrRes, ts_src: &str) -> AsmRes {
    let mut cache: HashMap<DefId, AsmDef> = HashMap::new();

    let defs: Vec<AsmDef> = ir
        .defs
        .iter()
        .enumerate()
        .filter(|(_, def)| def.planned)
        .map(|(idx, def)| {
            let _ = def;
            let root_id = DefId(idx as u32);
            let mut resolving = HashSet::new();
            let mut resolved_order = Vec::new();
            resolve_named_def(
                root_id,
                ir,
                ts_src,
                &mut cache,
                &mut resolving,
                &mut resolved_order,
            )
        })
        .collect();

    AsmRes { defs }
}

// ---------------------------------------------------------------------------
// Def / field / value lowering
// ---------------------------------------------------------------------------

fn resolve_named_def(
    id: DefId,
    ir: &IrRes,
    ts_src: &str,
    cache: &mut HashMap<DefId, AsmDef>,
    resolving: &mut HashSet<DefId>,
    resolved_order: &mut Vec<DefId>,
) -> AsmDef {
    if let Some(inst) = cache.get(&id) {
        return inst.clone();
    }
    if !resolving.insert(id) {
        let def = &ir.defs[id.0 as usize];
        return AsmDef {
            type_name: ir.shapes[def.shape_id.0 as usize]
                .name
                .clone()
                .unwrap_or_else(|| "_".into()),
            name: def.name.clone(),
            type_hint: def.type_hint.clone(),
            fields: vec![],
        };
    }
    let inst = resolve_def_with_overrides(id, &[], ir, ts_src, cache, resolving, resolved_order);
    resolving.remove(&id);
    cache.insert(id, inst.clone());
    resolved_order.push(id);
    inst
}

fn resolve_def_with_overrides(
    id: DefId,
    overrides: &[IrField],
    ir: &IrRes,
    ts_src: &str,
    cache: &mut HashMap<DefId, AsmDef>,
    resolving: &mut HashSet<DefId>,
    resolved_order: &mut Vec<DefId>,
) -> AsmDef {
    let def = &ir.defs[id.0 as usize];
    let type_name = ir.shapes[def.shape_id.0 as usize]
        .name
        .clone()
        .unwrap_or_else(|| "_".into());
    let name = def.name.clone();
    let type_hint = def.type_hint.clone();

    let effective_fields = merge_ir_fields(&def.fields, overrides);
    ensure_named_deps_resolved(
        &effective_fields,
        ir,
        ts_src,
        cache,
        resolving,
        resolved_order,
    );

    let base_def = def.base_def.map(|base_id| {
        resolve_named_or_inline(
            base_id,
            &effective_fields,
            ir,
            ts_src,
            cache,
            resolving,
            resolved_order,
        )
    });

    let mut fields = base_def
        .as_ref()
        .map(|def| def.fields.clone())
        .unwrap_or_default();
    for field in effective_fields.iter().map(|f| lower_field(f, ir)) {
        merge_asm_field(&mut fields, field);
    }

    if let Some(mapper_fn) = &def.mapper_fn {
        let input_json = build_input_json(&effective_fields, &def.inputs, ir, cache);
        apply_mapper_output(&mut fields, ts_src, mapper_fn, &[input_json]);
    } else if let (Some(base_id), Some(resolved)) = (def.base_def, base_def.as_ref()) {
        let ascent_fn = ir.defs[base_id.0 as usize].name.clone();
        if has_ts_fn(ts_src, &ascent_fn) {
            let resolved_json = asm_def_to_json(resolved).to_string();
            let input_json = build_input_json(&effective_fields, &def.inputs, ir, cache);
            apply_mapper_output(
                &mut fields,
                ts_src,
                &ascent_fn,
                &[resolved_json, input_json],
            );
        }
    }

    AsmDef {
        type_name,
        name,
        type_hint,
        fields,
    }
}

fn resolve_named_or_inline(
    id: DefId,
    overrides: &[IrField],
    ir: &IrRes,
    ts_src: &str,
    cache: &mut HashMap<DefId, AsmDef>,
    resolving: &mut HashSet<DefId>,
    resolved_order: &mut Vec<DefId>,
) -> AsmDef {
    let def = &ir.defs[id.0 as usize];
    if def.name != "_" && overrides.is_empty() {
        resolve_named_def(id, ir, ts_src, cache, resolving, resolved_order)
    } else {
        resolve_def_with_overrides(id, overrides, ir, ts_src, cache, resolving, resolved_order)
    }
}

fn merge_ir_fields(base: &[IrField], overrides: &[IrField]) -> Vec<IrField> {
    let mut merged = base.to_vec();
    for field in overrides {
        if let Some(existing) = merged.iter_mut().find(|f| f.name == field.name) {
            *existing = field.clone();
        } else {
            merged.push(field.clone());
        }
    }
    merged
}

fn merge_asm_field(fields: &mut Vec<AsmField>, field: AsmField) {
    if let Some(existing) = fields.iter_mut().find(|f| f.name == field.name) {
        *existing = field;
    } else {
        fields.push(field);
    }
}

fn ensure_named_deps_resolved(
    fields: &[IrField],
    ir: &IrRes,
    ts_src: &str,
    cache: &mut HashMap<DefId, AsmDef>,
    resolving: &mut HashSet<DefId>,
    resolved_order: &mut Vec<DefId>,
) {
    for field in fields {
        ensure_value_deps_resolved(&field.value, ir, ts_src, cache, resolving, resolved_order);
    }
}

fn ensure_value_deps_resolved(
    value: &IrValue,
    ir: &IrRes,
    ts_src: &str,
    cache: &mut HashMap<DefId, AsmDef>,
    resolving: &mut HashSet<DefId>,
    resolved_order: &mut Vec<DefId>,
) {
    match value {
        IrValue::Inst(fid) => {
            let def = &ir.defs[fid.0 as usize];
            if def.name != "_" {
                let _ = resolve_named_def(*fid, ir, ts_src, cache, resolving, resolved_order);
            } else {
                ensure_named_deps_resolved(
                    &def.fields,
                    ir,
                    ts_src,
                    cache,
                    resolving,
                    resolved_order,
                );
            }
        }
        IrValue::List(items) => {
            for item in items {
                ensure_value_deps_resolved(item, ir, ts_src, cache, resolving, resolved_order);
            }
        }
        IrValue::Path(segs) => {
            for seg in segs {
                ensure_value_deps_resolved(seg, ir, ts_src, cache, resolving, resolved_order);
            }
        }
        IrValue::Variant(_, _, Some(inner)) => {
            ensure_value_deps_resolved(inner, ir, ts_src, cache, resolving, resolved_order);
        }
        _ => {}
    }
}

fn build_input_json(
    fields: &[IrField],
    input_defs: &[IrStructFieldDef],
    ir: &IrRes,
    cache: &HashMap<DefId, AsmDef>,
) -> String {
    let input_defs_by_name: HashMap<String, &IrStructFieldDef> = input_defs
        .iter()
        .filter_map(|f| f.name.as_ref().map(|name| (name.clone(), f)))
        .collect();
    let input_map: serde_json::Map<String, serde_json::Value> = fields
        .iter()
        .filter_map(|f| {
            let field_def = input_defs_by_name.get(&f.name)?;
            let json_val =
                ir_value_to_json_typed(&f.value, &field_def.field_type, ir, cache, f.via);
            Some((f.name.clone(), json_val))
        })
        .collect();
    serde_json::Value::Object(input_map).to_string()
}

fn has_ts_fn(ts_src: &str, fn_name: &str) -> bool {
    ts_src.contains(&format!("function {fn_name}"))
}

fn apply_mapper_output(fields: &mut Vec<AsmField>, ts_src: &str, fn_name: &str, args: &[String]) {
    if ts_src.is_empty() || !has_ts_fn(ts_src, fn_name) {
        return;
    }
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = if arg_refs.len() == 1 {
        call_hook(ts_src, fn_name, arg_refs[0])
    } else {
        call_hook_args(ts_src, fn_name, &arg_refs)
    };
    if let Ok(output_json) = output {
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(&output_json)
        {
            for (k, v) in map {
                merge_asm_field(
                    fields,
                    AsmField {
                        name: k,
                        value: json_val_to_asm_value(&v),
                    },
                );
            }
        }
    }
}

fn lower_field(f: &IrField, ir: &IrRes) -> AsmField {
    AsmField {
        name: f.name.clone(),
        value: lower_value(&f.value, ir),
    }
}

fn ipv4_to_json(ip: &str) -> Option<serde_json::Value> {
    let addr: Ipv4Addr = ip.parse().ok()?;
    let octets = addr.octets();
    let int = u32::from(addr);
    Some(serde_json::json!({
        "value": ip,
        "a": octets[0],
        "b": octets[1],
        "c": octets[2],
        "d": octets[3],
        "int": int,
    }))
}

fn ipv4net_to_json(net: &str) -> Option<serde_json::Value> {
    let (ip, prefix_raw) = net.split_once('/')?;
    let prefix = prefix_raw.parse::<u8>().ok()?;
    if prefix > 32 {
        return None;
    }
    Some(serde_json::json!({
        "value": net,
        "addr": ipv4_to_json(ip)?,
        "prefix": prefix,
    }))
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

fn lower_value(v: &IrValue, ir: &IrRes) -> AsmValue {
    // Note: lower_value does not execute mappers on inline anonymous defs.
    // Mappers only fire on named defs via lower() resolution.
    match v {
        IrValue::Str(s) => AsmValue::Str(s.clone()),
        IrValue::Int(n) => AsmValue::Int(*n),
        IrValue::Bool(b) => AsmValue::Bool(*b),
        IrValue::Ref(s) => AsmValue::Ref(s.clone()),

        IrValue::Variant(tid, idx, payload) => {
            let td = &ir.shapes[tid.0 as usize];
            let type_name = td.name.clone().unwrap_or_else(|| "_".into());
            let (value, asm_payload) = match &td.body {
                IrShapeBody::Enum(vs) => match vs[*idx as usize].segments.first().map(|s| &s.value)
                {
                    Some(IrRefSegValue::Plain(p)) => (p.clone(), None),
                    Some(IrRefSegValue::Shape(vtid)) => {
                        let variant_type_name = ir.shapes[vtid.0 as usize]
                            .name
                            .clone()
                            .unwrap_or_else(|| "_".into());
                        let asm_p = payload.as_deref().map(|p| Box::new(lower_value(p, ir)));
                        (variant_type_name, asm_p)
                    }
                    _ => ("?".into(), None),
                },
                _ => ("?".into(), None),
            };
            AsmValue::Variant(AsmVariant {
                type_name,
                value,
                payload: asm_payload,
            })
        }

        IrValue::Inst(fid) => {
            let mapping = &ir.defs[fid.0 as usize];
            if mapping.name == "_" {
                // Anonymous inline def — embed fully (no mapper execution for inline defs).
                let fields = mapping.fields.iter().map(|f| lower_field(f, ir)).collect();
                let type_hint = mapping.type_hint.clone();
                AsmValue::Def(Box::new(AsmDef {
                    type_name: ir.shapes[mapping.shape_id.0 as usize]
                        .name
                        .clone()
                        .unwrap_or_else(|| "_".into()),
                    name: "_".into(),
                    type_hint,
                    fields,
                }))
            } else {
                // Named def — emit a ref; full data lives in AsmSymbol.
                let type_name = ir.shapes[mapping.shape_id.0 as usize]
                    .name
                    .clone()
                    .unwrap_or_else(|| "_".into());
                AsmValue::DefRef(AsmDefRef {
                    type_name,
                    name: mapping.name.clone(),
                })
            }
        }

        IrValue::Path(segs) => AsmValue::Path(segs.iter().map(|s| lower_value(s, ir)).collect()),

        IrValue::List(items) => AsmValue::List(items.iter().map(|s| lower_value(s, ir)).collect()),
    }
}

// ---------------------------------------------------------------------------
// Mapper I/O serialisation helpers
// ---------------------------------------------------------------------------

/// Convert an `IrValue` to a `serde_json::Value` for passing to a TypeScript mapper.
fn ir_value_to_json(v: &IrValue, ir: &IrRes) -> serde_json::Value {
    match v {
        IrValue::Str(s) => serde_json::Value::String(s.clone()),
        IrValue::Int(n) => serde_json::json!(*n),
        IrValue::Bool(b) => serde_json::Value::Bool(*b),
        IrValue::Ref(s) => serde_json::Value::String(s.clone()),

        IrValue::Variant(tid, idx, payload) => {
            let td = &ir.shapes[tid.0 as usize];
            let variant_str = match &td.body {
                IrShapeBody::Enum(vs) => match vs
                    .get(*idx as usize)
                    .and_then(|r| r.segments.first())
                    .map(|s| &s.value)
                {
                    Some(IrRefSegValue::Plain(p)) => p.clone(),
                    Some(IrRefSegValue::Shape(vtid)) => ir.shapes[vtid.0 as usize]
                        .name
                        .clone()
                        .unwrap_or_else(|| "_".into()),
                    _ => "_".into(),
                },
                _ => "_".into(),
            };
            match payload {
                Some(p) => serde_json::json!({ variant_str: ir_value_to_json(p, ir) }),
                None => serde_json::Value::String(variant_str),
            }
        }

        IrValue::Inst(fid) => {
            let child = &ir.defs[fid.0 as usize];
            let mut map = serde_json::Map::new();
            // Expose the mapping name as "_name" so parent mappers can use it.
            map.insert(
                "_name".into(),
                serde_json::Value::String(child.name.clone()),
            );
            for f in &child.fields {
                map.insert(f.name.clone(), ir_value_to_json(&f.value, ir));
            }
            serde_json::Value::Object(map)
        }

        IrValue::Path(segs) => {
            serde_json::Value::Array(segs.iter().map(|s| ir_value_to_json(s, ir)).collect())
        }

        IrValue::List(items) => {
            serde_json::Value::Array(items.iter().map(|i| ir_value_to_json(i, ir)).collect())
        }
    }
}

fn ir_value_to_json_typed(
    v: &IrValue,
    field_type: &IrFieldType,
    ir: &IrRes,
    cache: &HashMap<DefId, AsmDef>,
    via: bool,
) -> serde_json::Value {
    match field_type {
        IrFieldType::Primitive(IrPrimitive::Ipv4) => match v {
            IrValue::Str(s) => {
                ipv4_to_json(s).unwrap_or_else(|| serde_json::Value::String(s.clone()))
            }
            _ => ir_value_to_json(v, ir),
        },
        IrFieldType::Primitive(IrPrimitive::Ipv4Net) => match v {
            IrValue::Str(s) => {
                ipv4net_to_json(s).unwrap_or_else(|| serde_json::Value::String(s.clone()))
            }
            _ => ir_value_to_json(v, ir),
        },
        IrFieldType::Primitive(_) => {
            if via {
                ir_value_to_json_via(v, ir, cache)
            } else {
                ir_value_to_json(v, ir)
            }
        }
        IrFieldType::Ref(r) => {
            if let Some(shape_id) = target_shape_from_ir_ref(r) {
                if let Some(shape) = ir.shapes.get(shape_id.0 as usize) {
                    return match &shape.body {
                        IrShapeBody::Primitive(p) => ir_value_to_json_typed(
                            v,
                            &IrFieldType::Primitive(p.clone()),
                            ir,
                            cache,
                            via,
                        ),
                        IrShapeBody::Struct(field_defs) => match v {
                            IrValue::Inst(fid) => {
                                let child = &ir.defs[fid.0 as usize];
                                let mut map = serde_json::Map::new();
                                map.insert(
                                    "_name".into(),
                                    serde_json::Value::String(child.name.clone()),
                                );
                                for field in &child.fields {
                                    if let Some(field_def) = field_defs
                                        .iter()
                                        .find(|fd| fd.name.as_deref() == Some(field.name.as_str()))
                                    {
                                        map.insert(
                                            field.name.clone(),
                                            ir_value_to_json_typed(
                                                &field.value,
                                                &field_def.field_type,
                                                ir,
                                                cache,
                                                field.via,
                                            ),
                                        );
                                    } else {
                                        map.insert(
                                            field.name.clone(),
                                            if field.via {
                                                ir_value_to_json_via(&field.value, ir, cache)
                                            } else {
                                                ir_value_to_json(&field.value, ir)
                                            },
                                        );
                                    }
                                }
                                serde_json::Value::Object(map)
                            }
                            _ => {
                                if via {
                                    ir_value_to_json_via(v, ir, cache)
                                } else {
                                    ir_value_to_json(v, ir)
                                }
                            }
                        },
                        _ => {
                            if via {
                                ir_value_to_json_via(v, ir, cache)
                            } else {
                                ir_value_to_json(v, ir)
                            }
                        }
                    };
                }
            }
            if via {
                ir_value_to_json_via(v, ir, cache)
            } else {
                ir_value_to_json(v, ir)
            }
        }
        IrFieldType::List(patterns) => match v {
            IrValue::List(items) => {
                let fallback = if via {
                    ir_value_to_json_via(v, ir, cache)
                } else {
                    ir_value_to_json(v, ir)
                };
                let vals = items
                    .iter()
                    .map(|item| {
                        if patterns.len() == 1 {
                            ir_value_to_json_typed(
                                item,
                                &IrFieldType::Ref(patterns[0].clone()),
                                ir,
                                cache,
                                false,
                            )
                        } else {
                            ir_value_to_json(item, ir)
                        }
                    })
                    .collect();
                match fallback {
                    serde_json::Value::Array(_) => serde_json::Value::Array(vals),
                    _ => serde_json::Value::Array(vals),
                }
            }
            _ => {
                if via {
                    ir_value_to_json_via(v, ir, cache)
                } else {
                    ir_value_to_json(v, ir)
                }
            }
        },
    }
}

/// Like `ir_value_to_json` but for `IrValue::Inst` uses the post-mapper cached `AsmDef`.
/// Used for fields marked `via` — the mapper receives the already-resolved child value.
fn ir_value_to_json_via(
    v: &IrValue,
    ir: &IrRes,
    cache: &HashMap<DefId, AsmDef>,
) -> serde_json::Value {
    match v {
        IrValue::Inst(fid) => {
            if let Some(asm_def) = cache.get(fid) {
                asm_def_to_json(asm_def)
            } else {
                // Not yet cached (shouldn't happen in bottom-up walk) — fall back to raw.
                ir_value_to_json(v, ir)
            }
        }
        // For non-Inst values, via has no special meaning.
        _ => ir_value_to_json(v, ir),
    }
}

fn asm_def_to_json(def: &AsmDef) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("_name".into(), serde_json::Value::String(def.name.clone()));
    for f in &def.fields {
        map.insert(f.name.clone(), asm_value_to_json(&f.value));
    }
    serde_json::Value::Object(map)
}

pub fn asm_value_to_json(v: &AsmValue) -> serde_json::Value {
    match v {
        AsmValue::Str(s) => serde_json::Value::String(s.clone()),
        AsmValue::Int(n) => serde_json::json!(*n),
        AsmValue::Bool(b) => serde_json::Value::Bool(*b),
        AsmValue::Ref(s) => serde_json::Value::String(s.clone()),
        AsmValue::Variant(gv) => match &gv.payload {
            Some(p) => serde_json::json!({ gv.value.clone(): asm_value_to_json(p) }),
            None => serde_json::Value::String(gv.value.clone()),
        },
        AsmValue::DefRef(r) => serde_json::json!({ "_name": r.name, "type_name": r.type_name }),
        AsmValue::Def(i) => asm_def_to_json(i),
        AsmValue::Path(segs) => {
            serde_json::Value::Array(segs.iter().map(asm_value_to_json).collect())
        }
        AsmValue::List(items) => {
            serde_json::Value::Array(items.iter().map(asm_value_to_json).collect())
        }
    }
}

/// Convert a `serde_json::Value` returned by a mapper into an `AsmValue`.
fn json_val_to_asm_value(v: &serde_json::Value) -> AsmValue {
    match v {
        serde_json::Value::String(s) => AsmValue::Str(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                AsmValue::Int(i)
            } else {
                AsmValue::Str(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => AsmValue::Bool(*b),
        serde_json::Value::Null => AsmValue::Str("null".into()),
        serde_json::Value::Array(arr) => {
            AsmValue::List(arr.iter().map(json_val_to_asm_value).collect())
        }
        serde_json::Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| AsmField {
                    name: k.clone(),
                    value: json_val_to_asm_value(v),
                })
                .collect();
            AsmValue::Def(Box::new(AsmDef {
                type_name: String::new(),
                name: "_".into(),
                type_hint: None,
                fields,
            }))
        }
    }
}
