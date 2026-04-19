/// Generate TypeScript interface declarations for mapper I/O from the IR.
///
/// For every root def that carries a mapper (parent=None, mapper_fn=Some), we emit:
///
///   interface MakeLabelInput  { key: string; }
///   interface MakeLabelOutput { value: string; }
///
/// The naming convention is `PascalCase(mapper_fn) + "Input"` / `"Output"`.
/// These interfaces are prepended to the TypeScript blob before execution so
/// mapper authors can reference them for type safety; the transpiler erases them
/// at runtime so they have no runtime cost.

use crate::ir::{IrLinkType, IrPrimitive, IrRef, IrRefSegValue, IrRes, IrTypeBody, LinkId};

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn gen_mapper_interfaces(ir: &IrRes) -> String {
    let mut parts: Vec<String> = Vec::new();

    for fun in &ir.funs {
        if fun.parent.is_some() { continue; }
        let Some(mapper_fn) = &fun.mapper_fn else { continue; };

        let prefix = to_pascal_case(mapper_fn);
        parts.push(gen_interface(&format!("{}Input",  prefix), &fun.inputs,  ir));
        parts.push(gen_interface(&format!("{}Output", prefix), &fun.outputs, ir));
    }

    parts.join("\n")
}

/// Generate type-compatibility assertions to append to `user_ts` before type-checking.
///
/// For each mapper whose function is defined in `user_ts` (detected via
/// `function <name>` pattern), emit:
///
///   const __tc0: (i: MakeLabelInput) => MakeLabelOutput = make_label; void __tc0;
///
/// This causes TypeScript to verify that the user's implementation is compatible
/// with the generated I/O interfaces, even when the function has no type annotations.
pub fn gen_typecheck_assertions(ir: &IrRes, user_ts: &str) -> String {
    let mut parts: Vec<String> = Vec::new();

    for (idx, fun) in ir.funs.iter().enumerate() {
        if fun.parent.is_some() { continue; }
        let Some(mapper_fn) = &fun.mapper_fn else { continue; };

        // Only assert for functions that are actually defined in user_ts.
        let fn_decl = format!("function {mapper_fn}");
        if !user_ts.contains(&fn_decl) { continue; }

        let prefix   = to_pascal_case(mapper_fn);
        let var_name = format!("__tc{idx}");
        parts.push(format!(
            "const {var_name}: (i: {prefix}Input) => {prefix}Output = {mapper_fn}; void {var_name};"
        ));
    }

    parts.join("\n")
}

// ---------------------------------------------------------------------------
// Per-interface generation
// ---------------------------------------------------------------------------

fn gen_interface(name: &str, link_ids: &[LinkId], ir: &IrRes) -> String {
    if link_ids.is_empty() {
        return format!("interface {} {{}}", name);
    }
    let fields: Vec<String> = link_ids.iter().map(|&lid| {
        let link    = &ir.links[lid.0 as usize];
        let fname   = link.name.as_deref().unwrap_or("_");
        let ts_type = link_type_to_ts(&link.link_type, ir);
        format!("  {}: {};", fname, ts_type)
    }).collect();
    format!("interface {} {{\n{}\n}}", name, fields.join("\n"))
}

// ---------------------------------------------------------------------------
// Ground link type → TypeScript type string
// ---------------------------------------------------------------------------

fn link_type_to_ts(lt: &IrLinkType, ir: &IrRes) -> String {
    match lt {
        IrLinkType::Primitive(p)  => prim_to_ts(p).to_string(),
        IrLinkType::Ref(r)        => ref_to_ts(r, ir),
        IrLinkType::List(refs)    => {
            if refs.is_empty() { return "unknown[]".to_string(); }
            // Use the first ref's element type (simple case).
            format!("{}[]", ref_to_ts(&refs[0], ir))
        }
    }
}

fn prim_to_ts(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String    => "string",
        IrPrimitive::Integer   => "number",
        IrPrimitive::Reference => "string",
    }
}

fn ref_to_ts(r: &IrRef, ir: &IrRes) -> String {
    // Walk segments and find the resolved Type.
    // Only map primitive-wrapping Ground types to a concrete TypeScript type name;
    // for struct/enum Ground types there is no corresponding TypeScript declaration,
    // so we emit `unknown` rather than a name that TypeScript can't resolve.
    for seg in r.segments.iter().rev() {
        if let IrRefSegValue::Type(tid) = &seg.value {
            let ty = &ir.types[tid.0 as usize];
            return match &ty.body {
                IrTypeBody::Primitive(p) => prim_to_ts(p).to_string(),
                // Complex Ground types (struct, enum) have no TypeScript counterpart here.
                _ => "unknown".to_string(),
            };
        }
    }
    // Fallback: plain segments (shouldn't appear in mapper I/O normally).
    "unknown".to_string()
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
                None    => String::new(),
                Some(f) => f.to_uppercase().to_string() + cs.as_str(),
            }
        })
        .collect()
}
