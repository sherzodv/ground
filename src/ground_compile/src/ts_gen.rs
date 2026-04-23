/// Generate TypeScript interface declarations for mapper I/O from the IR.
///
/// For every root def that carries a mapper (base_def=None, mapper_fn=Some), we emit:
///
///   interface MakeLabelI { key: string; }
///   interface MakeLabelO { value: string; }
///
/// The naming convention is `PascalCase(mapper_fn) + "I"` / `"O"`.
/// These interfaces are prepended to the TypeScript blob before execution so
/// mapper authors can reference them for type safety; the transpiler erases them
/// at runtime so they have no runtime cost.
use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{
    IrDef, IrFieldType, IrPrimitive, IrRef, IrRefSegValue, IrRes, IrShapeBody, IrStructFieldDef,
    UnitId,
};

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn gen_mapper_interfaces(ir: &IrRes) -> String {
    let mut parts: Vec<String> = vec![ground_builtin_ts_types()];
    let mut shape_ids = BTreeSet::new();
    let preferred_names = preferred_shape_names(ir);

    for fun in &ir.defs {
        if fun.base_def.is_some() {
            continue;
        }
        let Some(mapper_fn) = &fun.mapper_fn else {
            continue;
        };
        let prefix = to_pascal_case(mapper_fn);
        collect_shapes_from_fields(&fun.inputs, ir, &mut shape_ids);
        collect_shapes_from_fields(&fun.outputs, ir, &mut shape_ids);
        parts.push(gen_interface(&format!("{}I", prefix), &fun.inputs, ir));
        parts.push(gen_interface(&format!("{}O", prefix), &fun.outputs, ir));
    }

    let mut out: Vec<String> = shape_ids
        .into_iter()
        .filter_map(|tid| gen_shape_decl(crate::ir::ShapeId(tid), ir, &preferred_names))
        .collect();
    out.extend(parts);
    out.join("\n\n")
}

pub fn gen_mapper_interfaces_by_unit(ir: &IrRes) -> Vec<(UnitId, String)> {
    let mut by_unit: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut seen: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    let mut shapes_by_unit: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let preferred_names = preferred_shape_names(ir);

    for fun in &ir.defs {
        let Some((fn_name, decl)) = gen_mapper_dts_decl_for_unit(fun, ir) else {
            continue;
        };
        let unit = fun.loc.unit.0;
        let seen_unit = seen.entry(unit).or_default();
        if seen_unit.insert(fn_name) {
            by_unit.entry(unit).or_default().push(decl);
        }
        let shape_ids = shapes_by_unit.entry(unit).or_default();
        collect_shapes_from_fields(&fun.inputs, ir, shape_ids);
        collect_shapes_from_fields(&fun.outputs, ir, shape_ids);
        if fun.mapper_fn.is_none() && fun.base_def.is_some() {
            if let Some(base_id) = fun.base_def {
                if let Some(shape) = ir
                    .shapes
                    .get(ir.defs[base_id.0 as usize].shape_id.0 as usize)
                {
                    if let IrShapeBody::Struct(fields) = &shape.body {
                        collect_shapes_from_fields(fields, ir, shape_ids);
                    }
                }
            }
        }
    }

    by_unit
        .into_iter()
        .map(|(unit, parts)| {
            let mut out: Vec<String> = vec![ground_builtin_ts_types()];
            out.extend(
                shapes_by_unit
                    .remove(&unit)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|tid| gen_shape_decl(crate::ir::ShapeId(tid), ir, &preferred_names))
                    .collect::<Vec<_>>(),
            );
            out.extend(parts);
            (UnitId(unit), out.join("\n\n"))
        })
        .collect()
}

/// Generate type-compatibility assertions to append to `user_ts` before type-checking.
///
/// For each mapper whose function is defined in `user_ts` (detected via
/// `function <name>` pattern), emit:
///
///   const __tc0: (i: MakeLabelI) => MakeLabelO = make_label; void __tc0;
///
/// This causes TypeScript to verify that the user's implementation is compatible
/// with the generated I/O interfaces, even when the function has no type annotations.
pub fn gen_typecheck_assertions(ir: &IrRes, user_ts: &str) -> String {
    let mut parts: Vec<String> = Vec::new();

    for (idx, fun) in ir.defs.iter().enumerate() {
        if fun.base_def.is_some() {
            continue;
        }
        let Some(mapper_fn) = &fun.mapper_fn else {
            continue;
        };

        // Only assert for functions that are actually defined in user_ts.
        let fn_decl = format!("function {mapper_fn}");
        if !user_ts.contains(&fn_decl) {
            continue;
        }

        let prefix = to_pascal_case(mapper_fn);
        let var_name = format!("__tc{idx}");
        parts.push(format!(
            "const {var_name}: (i: {prefix}I) => {prefix}O = {mapper_fn}; void {var_name};"
        ));
    }

    parts.join("\n")
}

// ---------------------------------------------------------------------------
// Per-interface generation
// ---------------------------------------------------------------------------

fn gen_interface(name: &str, fields_def: &[IrStructFieldDef], ir: &IrRes) -> String {
    let mut seen = BTreeSet::new();
    let fields: Vec<String> = fields_def
        .iter()
        .filter_map(|field| {
            let fname = field.name.as_deref().unwrap_or("_");
            if !seen.insert(fname.to_string()) {
                return None;
            }
            let ts_type = field_type_to_ts(&field.field_type, ir);
            let opt = if is_optional_field_type(&field.field_type) {
                "?"
            } else {
                ""
            };
            Some(format!("  {}{}: {};", fname, opt, ts_type))
        })
        .collect();

    if fields.is_empty() {
        return format!("interface {} {{}}", name);
    }
    format!("interface {} {{\n{}\n}}", name, fields.join("\n"))
}

fn gen_mapper_dts_decl(fun: &IrDef, ir: &IrRes) -> String {
    let mapper_fn = fun.mapper_fn.as_deref().unwrap_or("_");
    let prefix = to_pascal_case(mapper_fn);
    let input_name = format!("{prefix}I");
    let output_name = format!("{prefix}O");
    let iface_in = gen_interface(&input_name, &fun.inputs, ir);
    let iface_out = gen_interface(&output_name, &fun.outputs, ir);
    let decl = format!("declare function {mapper_fn}(i: {input_name}): {output_name};");
    format!("{iface_in}\n\n{iface_out}\n\n{decl}")
}

fn gen_mapper_dts_decl_for_unit(fun: &IrDef, ir: &IrRes) -> Option<(String, String)> {
    if let Some(mapper_fn) = &fun.mapper_fn {
        return Some((mapper_fn.clone(), gen_mapper_dts_decl(fun, ir)));
    }

    if fun.base_def.is_some() && ir.scopes[fun.scope.0 as usize].ts_fns.contains(&fun.name) {
        return Some((fun.name.clone(), gen_ascent_mapper_dts_decl(fun, ir)));
    }

    None
}

fn gen_ascent_mapper_dts_decl(fun: &IrDef, ir: &IrRes) -> String {
    let fn_name = &fun.name;
    let prefix = to_pascal_case(fn_name);
    let resolved_name = format!("{prefix}Resolved");
    let input_name = format!("{prefix}I");
    let output_name = format!("{prefix}O");

    let resolved_fields = fun
        .base_def
        .and_then(|base_id| {
            ir.shapes
                .get(ir.defs[base_id.0 as usize].shape_id.0 as usize)
        })
        .and_then(|shape| match &shape.body {
            IrShapeBody::Struct(fields) => Some(fields.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let iface_resolved = gen_interface(&resolved_name, &resolved_fields, ir);
    let iface_in = gen_interface(&input_name, &fun.inputs, ir);
    let iface_out = gen_interface(&output_name, &fun.outputs, ir);
    let decl = format!(
        "declare function {fn_name}(resolved: {resolved_name}, input: {input_name}): {output_name};"
    );
    format!("{iface_resolved}\n\n{iface_in}\n\n{iface_out}\n\n{decl}")
}

fn preferred_shape_names(ir: &IrRes) -> BTreeMap<u32, String> {
    let mut names = BTreeMap::new();
    for (idx, shape) in ir.shapes.iter().enumerate() {
        let IrShapeBody::Struct(fields) = &shape.body else {
            continue;
        };
        let parent_name = shape_ts_name(crate::ir::ShapeId(idx as u32), ir, &BTreeMap::new());
        for field in fields {
            let Some(field_name) = field.name.as_deref() else {
                continue;
            };
            collect_preferred_names_from_field_type(
                &field.field_type,
                ir,
                &mut names,
                &format!("{}{}", parent_name, to_pascal_case(field_name)),
            );
        }
    }
    names
}

fn collect_preferred_names_from_field_type(
    field_type: &IrFieldType,
    ir: &IrRes,
    out: &mut BTreeMap<u32, String>,
    candidate: &str,
) {
    match field_type {
        IrFieldType::Primitive(_) => {}
        IrFieldType::Ref(r) => collect_preferred_names_from_ref(r, ir, out, candidate),
        IrFieldType::List(rs) => {
            for r in rs {
                collect_preferred_names_from_ref(r, ir, out, candidate);
            }
        }
        IrFieldType::Optional(inner) => {
            collect_preferred_names_from_field_type(inner, ir, out, candidate)
        }
    }
}

fn collect_preferred_names_from_ref(
    r: &IrRef,
    ir: &IrRes,
    out: &mut BTreeMap<u32, String>,
    candidate: &str,
) {
    for seg in &r.segments {
        let IrRefSegValue::Shape(tid) = seg.value else {
            continue;
        };
        let Some(shape) = ir.shapes.get(tid.0 as usize) else {
            continue;
        };
        if shape.name.is_none() {
            out.entry(tid.0).or_insert_with(|| candidate.to_string());
        }
    }
}

fn collect_shapes_from_fields(fields: &[IrStructFieldDef], ir: &IrRes, out: &mut BTreeSet<u32>) {
    for field in fields {
        collect_shapes_from_field_type(&field.field_type, ir, out);
    }
}

fn collect_shapes_from_field_type(field_type: &IrFieldType, ir: &IrRes, out: &mut BTreeSet<u32>) {
    match field_type {
        IrFieldType::Primitive(_) => {}
        IrFieldType::Ref(r) => collect_shapes_from_ref(r, ir, out),
        IrFieldType::List(rs) => {
            for r in rs {
                collect_shapes_from_ref(r, ir, out);
            }
        }
        IrFieldType::Optional(inner) => collect_shapes_from_field_type(inner, ir, out),
    }
}

fn collect_shapes_from_ref(r: &IrRef, ir: &IrRes, out: &mut BTreeSet<u32>) {
    for seg in &r.segments {
        if let IrRefSegValue::Shape(tid) = seg.value {
            if !out.insert(tid.0) {
                continue;
            }
            if let Some(shape) = ir.shapes.get(tid.0 as usize) {
                match &shape.body {
                    IrShapeBody::Struct(fields) => collect_shapes_from_fields(fields, ir, out),
                    IrShapeBody::Enum(variants) => {
                        for variant in variants {
                            collect_shapes_from_ref(variant, ir, out);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn gen_shape_decl(
    tid: crate::ir::ShapeId,
    ir: &IrRes,
    preferred_names: &BTreeMap<u32, String>,
) -> Option<String> {
    let shape = ir.shapes.get(tid.0 as usize)?;
    match &shape.body {
        IrShapeBody::Primitive(_) => None,
        IrShapeBody::Unit => Some(format!(
            "interface {} {{}}",
            shape_ts_name(tid, ir, preferred_names)
        )),
        IrShapeBody::Struct(fields) => Some(gen_interface(
            &shape_ts_name(tid, ir, preferred_names),
            fields,
            ir,
        )),
        IrShapeBody::Enum(variants) => {
            let variants = variants
                .iter()
                .map(|r| ref_to_ts(r, ir, preferred_names))
                .collect::<Vec<_>>();
            Some(format!(
                "type {} = {};",
                shape_ts_name(tid, ir, preferred_names),
                if variants.is_empty() {
                    "never".into()
                } else {
                    variants.join(" | ")
                }
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Ground field type → TypeScript type string
// ---------------------------------------------------------------------------

fn field_type_to_ts(lt: &IrFieldType, ir: &IrRes) -> String {
    let preferred_names = preferred_shape_names(ir);
    field_type_to_ts_with_names(lt, ir, &preferred_names)
}

fn field_type_to_ts_with_names(
    lt: &IrFieldType,
    ir: &IrRes,
    preferred_names: &BTreeMap<u32, String>,
) -> String {
    match lt {
        IrFieldType::Primitive(p) => prim_to_ts(p).to_string(),
        IrFieldType::Ref(r) => ref_to_ts(r, ir, preferred_names),
        IrFieldType::List(refs) => {
            if refs.is_empty() {
                return "unknown[]".to_string();
            }
            let mut item_types: Vec<String> = refs
                .iter()
                .map(|r| ref_to_ts(r, ir, preferred_names))
                .collect();
            item_types.dedup();
            let union = if item_types.len() == 1 {
                item_types[0].clone()
            } else {
                item_types.join(" | ")
            };
            format!("({union})[]")
        }
        IrFieldType::Optional(inner) => field_type_to_ts_with_names(inner, ir, preferred_names),
    }
}

fn is_optional_field_type(field_type: &IrFieldType) -> bool {
    matches!(field_type, IrFieldType::Optional(_))
}

fn prim_to_ts(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String => "string",
        IrPrimitive::Integer => "number",
        IrPrimitive::Boolean => "boolean",
        IrPrimitive::Reference => "string",
        IrPrimitive::Ipv4 => "GroundIpv4",
        IrPrimitive::Ipv4Net => "GroundIpv4Net",
    }
}

fn ground_builtin_ts_types() -> String {
    [
        "interface GroundIpv4 {",
        "  value: string;",
        "  a: number;",
        "  b: number;",
        "  c: number;",
        "  d: number;",
        "  int: number;",
        "}",
        "",
        "interface GroundIpv4Net {",
        "  value: string;",
        "  addr: GroundIpv4;",
        "  prefix: number;",
        "}",
    ]
    .join("\n")
}

fn ref_to_ts(r: &IrRef, ir: &IrRes, preferred_names: &BTreeMap<u32, String>) -> String {
    let parts: Vec<String> = r
        .segments
        .iter()
        .filter_map(|seg| match &seg.value {
            IrRefSegValue::Shape(tid) => {
                let ty = &ir.shapes[tid.0 as usize];
                Some(match &ty.body {
                    IrShapeBody::Primitive(p) => prim_to_ts(p).to_string(),
                    _ => shape_ts_name(*tid, ir, preferred_names),
                })
            }
            IrRefSegValue::Plain(s) => Some(format!("{s:?}")),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        "unknown".to_string()
    } else if parts.len() == 1 {
        parts[0].clone()
    } else {
        format!("[{}]", parts.join(", "))
    }
}

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| c == '_' || c == '-')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut cs = p.chars();
            match cs.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + cs.as_str(),
            }
        })
        .collect()
}

fn shape_ts_name(
    tid: crate::ir::ShapeId,
    ir: &IrRes,
    preferred_names: &BTreeMap<u32, String>,
) -> String {
    let shape = &ir.shapes[tid.0 as usize];
    if let Some(name) = preferred_names.get(&tid.0) {
        return name.clone();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut cur = Some(shape.scope);
    while let Some(scope_id) = cur {
        let scope = &ir.scopes[scope_id.0 as usize];
        if let Some(name) = &scope.name {
            parts.push(to_pascal_case(name));
        }
        cur = scope.parent;
    }
    parts.reverse();
    parts.push(to_pascal_case(
        shape.name.as_deref().unwrap_or(&format!("shape_{}", tid.0)),
    ));
    parts.join("")
}
