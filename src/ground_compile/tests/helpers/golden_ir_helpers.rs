use ground_compile::ast::{ParseReq, ParseUnit};
use ground_compile::ir::*;
use ground_compile::parse::parse;
use ground_compile::resolve::resolve;

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

pub fn show_primitive(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String    => "string",
        IrPrimitive::Integer   => "integer",
        IrPrimitive::Reference => "reference",
    }
}

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

pub fn show_ref_seg_value(v: &IrRefSegValue) -> String {
    match v {
        IrRefSegValue::Pack(id)  => format!("Pack#{}", id.0),
        IrRefSegValue::Type(id)  => format!("Type#{}", id.0),
        IrRefSegValue::Link(id)  => format!("Link#{}", id.0),
        IrRefSegValue::Inst(id)  => format!("Fun#{}", id.0),
        IrRefSegValue::Plain(s)  => s.clone(),
    }
}

// ---------------------------------------------------------------------------
// Link types
// ---------------------------------------------------------------------------

pub fn show_link_type(lt: &IrLinkType, ir: &IrRes) -> String {
    match lt {
        IrLinkType::Primitive(p) => format!("Prim({})", show_primitive(p)),

        IrLinkType::Ref(r) => format!("IrRef[{}]", show_link_type_ref(r, ir)),

        IrLinkType::List(patterns) => {
            let parts: Vec<_> = patterns.iter()
                .map(|p| format!("IrRef[{}]", show_link_type_ref(p, ir)))
                .collect();
            format!("List[{}]", parts.join(" | "))
        }
    }
}

fn show_link_type_ref(r: &IrRef, ir: &IrRes) -> String {
    if r.segments.len() == 1 {
        return show_link_type_seg(&r.segments[0], ir);
    }
    // Typed path — join with ':'
    r.segments.iter()
        .map(|seg| show_link_type_seg(seg, ir))
        .collect::<Vec<_>>()
        .join(":")
}

fn show_link_type_seg(seg: &IrRefSeg, ir: &IrRes) -> String {
    let inner = match &seg.value {
        IrRefSegValue::Type(tid) => {
            let kind = match &ir.types[tid.0 as usize].body {
                IrTypeBody::Enum(_)   => "Enum",
                IrTypeBody::Struct(_) => "Struct",
                IrTypeBody::Primitive(_) => "Prim",
            };
            format!("{}(Type#{})", kind, tid.0)
        }
        IrRefSegValue::Plain(s) => s.clone(),
        other => show_ref_seg_value(other),
    };
    if seg.is_opt { format!("({})", inner) } else { inner }
}

// ---------------------------------------------------------------------------
// Type bodies
// ---------------------------------------------------------------------------

pub fn show_type_body(body: &IrTypeBody, ir: &IrRes) -> String {
    match body {
        IrTypeBody::Primitive(p) => format!("Prim({})", show_primitive(p)),

        IrTypeBody::Enum(variants) => {
            let parts: Vec<_> = variants.iter().map(|r| show_link_type_ref(r, ir)).collect();
            format!("Enum[{}]", parts.join("|"))
        }

        IrTypeBody::Struct(link_ids) => {
            let parts: Vec<_> = link_ids.iter().map(|lid| {
                let ld = &ir.links[lid.0 as usize];
                let name = ld.name.as_deref().unwrap_or("_");
                format!("Link#{}[{}, {}]", lid.0, name, show_link_type(&ld.link_type, ir))
            }).collect();
            format!("Struct[{}]", parts.join(", "))
        }
    }
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

pub fn show_value(v: &IrValue, ir: &IrRes) -> String {
    match v {
        IrValue::Str(s)  => format!("Str({:?})", s),
        IrValue::Int(n)  => format!("Int({})", n),
        IrValue::Ref(s)  => format!("Ref({:?})", s),

        IrValue::Variant(tid, idx, payload) => {
            match payload {
                None => {
                    let variant = match &ir.types[tid.0 as usize].body {
                        IrTypeBody::Enum(vs) => vs[*idx as usize].segments.first()
                            .and_then(|s| if let IrRefSegValue::Plain(p) = &s.value { Some(p.as_str()) } else { None })
                            .unwrap_or("?"),
                        _ => "?",
                    };
                    format!("Variant(Type#{}, {:?})", tid.0, variant)
                }
                Some(inner) => format!("Variant(Type#{}, {})", tid.0, show_value(inner, ir)),
            }
        }

        IrValue::Inst(fid) => format!("Inst(Fun#{})", fid.0),

        IrValue::Path(segs) => {
            segs.iter().map(|v| show_value(v, ir)).collect::<Vec<_>>().join(":")
        }

        IrValue::List(items) => {
            let parts: Vec<_> = items.iter().map(|v| show_value(v, ir)).collect();
            format!("List[{}]", parts.join(", "))
        }
    }
}

pub fn show_field(f: &IrField, ir: &IrRes) -> String {
    format!("Field[Link#{}, {}]", f.link_id.0, show_value(&f.value, ir))
}

// ---------------------------------------------------------------------------
// Top-level entries
// ---------------------------------------------------------------------------

pub fn show_type_entry(idx: usize, ir: &IrRes) -> String {
    let td   = &ir.types[idx];
    let name = td.name.as_deref().unwrap_or("_");
    format!("Type#{}[{}, {}]", idx, name, show_type_body(&td.body, ir))
}

pub fn show_link_entry(idx: usize, ir: &IrRes) -> String {
    let ld   = &ir.links[idx];
    let name = ld.name.as_deref().unwrap_or("_");
    format!("Link#{}[{}, {}]", idx, name, show_link_type(&ld.link_type, ir))
}

pub fn show_fun_entry(idx: usize, ir: &IrRes) -> String {
    let fun = &ir.funs[idx];
    let mut parts = vec![format!("Type#{}", fun.type_id.0), fun.name.clone()];
    if let Some(hint) = &fun.type_hint {
        parts.push(format!("hint={}", hint));
    }
    parts.extend(fun.fields.iter().map(|f| show_field(f, ir)));
    format!("Fun#{}[{}]", idx, parts.join(", "))
}

// ---------------------------------------------------------------------------
// Type function definitions
// ---------------------------------------------------------------------------

pub fn show_type_fn_entry(idx: usize, ir: &IrRes) -> String {
    let fd = &ir.type_fns[idx];
    let name = fd.name.as_deref().unwrap_or("_");
    let params: Vec<_> = fd.params.iter()
        .map(|p| format!("{}:Type#{}", p.name, p.ty.0))
        .collect();
    let body: Vec<_> = fd.body.iter().map(|entry| {
        let fields: Vec<_> = entry.fields.iter()
            .map(|bf| format!("{}={}", bf.name, show_value(&bf.value, ir)))
            .collect();
        format!("{}:Type#{}[{}]", entry.alias, entry.vendor_type.0, fields.join(", "))
    }).collect();
    if body.is_empty() {
        format!("TypeFn#{}[{}({})]", idx, name, params.join(", "))
    } else {
        format!("TypeFn#{}[{}({}), {}]", idx, name, params.join(", "), body.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Scope tree
// ---------------------------------------------------------------------------

fn show_scope_ir(scope_id: ScopeId, ir: &IrRes) -> String {
    let scope = &ir.scopes[scope_id.0 as usize];
    let raw_name = scope.name.as_deref().unwrap_or("_");
    let kind_str = match scope.kind {
        ScopeKind::Pack => "pack",
        ScopeKind::Struct => "struct",
    };
    let name = format!("{}:{}", kind_str, raw_name);

    let mut parts: Vec<String> = Vec::new();

    // Types belonging to this scope (arena order)
    for (i, t) in ir.types.iter().enumerate() {
        if t.scope == scope_id {
            parts.push(show_type_entry(i, ir));
        }
    }

    // Pack-level links belonging to this scope.
    // Struct-body links are shown inline in their type body — exclude them here
    // regardless of which scope they landed in (old-syntax puts them in a Type
    // sub-scope, new-syntax puts them directly in the pack scope).
    if scope.kind == ScopeKind::Pack {
        let struct_link_ids: std::collections::HashSet<u32> = ir.types.iter()
            .flat_map(|t| match &t.body {
                IrTypeBody::Struct(ids) => ids.iter().map(|id| id.0).collect::<Vec<_>>(),
                _ => vec![],
            })
            .collect();
        for (i, l) in ir.links.iter().enumerate() {
            if l.scope == scope_id && !struct_link_ids.contains(&(i as u32)) {
                parts.push(show_link_entry(i, ir));
            }
        }
    }

    // Funs belonging to this scope (arena order)
    for (i, fun) in ir.funs.iter().enumerate() {
        if fun.scope == scope_id {
            parts.push(show_fun_entry(i, ir));
        }
    }

    // Type fn defs belonging to this scope
    for (i, tf) in ir.type_fns.iter().enumerate() {
        if tf.scope == scope_id {
            parts.push(show_type_fn_entry(i, ir));
        }
    }

    // Child scopes (arena insertion order)
    for (i, s) in ir.scopes.iter().enumerate() {
        if s.parent == Some(scope_id) {
            parts.push(show_scope_ir(ScopeId(i as u32), ir));
        }
    }

    if parts.is_empty() {
        format!("Scope[{}]", name)
    } else {
        format!("Scope[{},\n{},\n]", name, parts.join(",\n"))
    }
}

// ---------------------------------------------------------------------------
// Entry point used by tests
// ---------------------------------------------------------------------------

/// Normalise a raw-string expected value: strip blank lines, trim each line.
pub fn norm(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse + resolve multiple units, format as compact multi-line string.
/// Each unit is `(name, path, src)`.
pub fn show_multi(units: Vec<(&str, Vec<&str>, &str)>) -> String {
    let req = ParseReq {
        units: units.into_iter().map(|(name, path, src)| ParseUnit {
            name:   name.into(),
            path:   path.into_iter().map(|s| s.to_string()).collect(),
            src:    src.to_string(),
            ts_src: None,
        }).collect(),
    };
    let res = parse(req);
    show_ir(resolve(res))
}

/// Parse + resolve `input`, format as compact multi-line string.
pub fn show(input: &str) -> String {
    let res = parse(ParseReq {
        units: vec![ParseUnit { name: "test".into(), path: vec![], src: input.to_string(), ts_src: None }],
    });
    show_ir(resolve(res))
}

/// Like `show` but also supplies TypeScript source co-located with the Ground source.
#[allow(dead_code)]
pub fn show_with_ts(grd_src: &str, ts_src: &str) -> String {
    let res = parse(ParseReq {
        units: vec![ParseUnit {
            name:   "test".into(),
            path:   vec![],
            src:    grd_src.to_string(),
            ts_src: Some(ts_src.to_string()),
        }],
    });
    show_ir(resolve(res))
}

/// Like `show_multi` but each entry includes an optional ts_src.
/// Each unit is `(name, path, src, ts_src)`.
#[allow(dead_code)]
pub fn show_multi_ts(units: Vec<(&str, Vec<&str>, &str, Option<&str>)>) -> String {
    let req = ParseReq {
        units: units.into_iter().map(|(name, path, src, ts)| ParseUnit {
            name:   name.into(),
            path:   path.into_iter().map(|s| s.to_string()).collect(),
            src:    src.to_string(),
            ts_src: ts.map(|s| s.to_string()),
        }).collect(),
    };
    let res = parse(req);
    show_ir(resolve(res))
}

fn show_ir(ir: IrRes) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Scope tree: direct children of root (ScopeId(0))
    for (i, s) in ir.scopes.iter().enumerate().skip(1) {
        if s.parent == Some(ScopeId(0)) {
            lines.push(show_scope_ir(ScopeId(i as u32), &ir));
        }
    }

    // Plans
    for plan in &ir.plans {
        lines.push(format!("Plan[{}]", plan.name));
    }

    for e in &ir.errors {
        lines.push(format!("ERR: {}", e.message));
    }

    norm(&lines.join("\n"))
}
