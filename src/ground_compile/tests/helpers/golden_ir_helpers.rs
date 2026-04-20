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
        IrPrimitive::Boolean   => "boolean",
        IrPrimitive::Reference => "reference",
    }
}

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

pub fn show_ref_seg_value(v: &IrRefSegValue) -> String {
    match v {
        IrRefSegValue::Pack(id)  => format!("Pack#{}", id.0),
        IrRefSegValue::Shape(id)  => format!("Shape#{}", id.0),
        IrRefSegValue::Def(id)  => format!("Def#{}", id.0),
        IrRefSegValue::Plain(s)  => s.clone(),
    }
}

// ---------------------------------------------------------------------------
// Field shapes
// ---------------------------------------------------------------------------

pub fn show_field_type(lt: &IrFieldType, ir: &IrRes) -> String {
    match lt {
        IrFieldType::Primitive(p) => format!("Prim({})", show_primitive(p)),

        IrFieldType::Ref(r) => format!("IrRef[{}]", show_field_type_ref(r, ir)),

        IrFieldType::List(patterns) => {
            let parts: Vec<_> = patterns.iter()
                .map(|p| format!("IrRef[{}]", show_field_type_ref(p, ir)))
                .collect();
            format!("List[{}]", parts.join(" | "))
        }
    }
}

fn show_field_type_ref(r: &IrRef, ir: &IrRes) -> String {
    if r.segments.len() == 1 {
        return show_field_type_seg(&r.segments[0], ir);
    }
    // Typed path — join with ':'
    r.segments.iter()
        .map(|seg| show_field_type_seg(seg, ir))
        .collect::<Vec<_>>()
        .join(":")
}

fn show_field_type_seg(seg: &IrRefSeg, ir: &IrRes) -> String {
    let inner = match &seg.value {
        IrRefSegValue::Shape(tid) => {
            let kind = match &ir.shapes[tid.0 as usize].body {
                IrShapeBody::Unit => "Unit",
                IrShapeBody::Enum(_)   => "Enum",
                IrShapeBody::Struct(_) => "Struct",
                IrShapeBody::Primitive(_) => "Prim",
            };
            format!("{}(Shape#{})", kind, tid.0)
        }
        IrRefSegValue::Plain(s) => s.clone(),
        other => show_ref_seg_value(other),
    };
    if seg.is_opt { format!("({})", inner) } else { inner }
}

// ---------------------------------------------------------------------------
// Shape bodies
// ---------------------------------------------------------------------------

pub fn show_type_body(body: &IrShapeBody, ir: &IrRes) -> String {
    match body {
        IrShapeBody::Unit => "Unit".to_string(),
        IrShapeBody::Primitive(p) => format!("Prim({})", show_primitive(p)),

        IrShapeBody::Enum(variants) => {
            let parts: Vec<_> = variants.iter().map(|r| show_field_type_ref(r, ir)).collect();
            format!("Enum[{}]", parts.join("|"))
        }

        IrShapeBody::Struct(fields) => {
            let parts: Vec<_> = fields.iter().enumerate().map(|(idx, field)| {
                let name = field.name.as_deref().unwrap_or("_");
                format!("Field#{}[{}, {}]", idx, name, show_field_type(&field.field_type, ir))
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
        IrValue::Bool(b) => format!("Bool({})", b),
        IrValue::Ref(s)  => format!("Ref({})", s),

        IrValue::Variant(tid, idx, payload) => {
            match payload {
                None => {
                    let variant = match &ir.shapes[tid.0 as usize].body {
                        IrShapeBody::Enum(vs) => vs[*idx as usize].segments.first()
                            .and_then(|s| if let IrRefSegValue::Plain(p) = &s.value { Some(p.as_str()) } else { None })
                            .unwrap_or("?"),
                        _ => "?",
                    };
                    format!("Variant(Shape#{}, {:?})", tid.0, variant)
                }
                Some(inner) => format!("Variant(Shape#{}, {})", tid.0, show_value(inner, ir)),
            }
        }

        IrValue::Inst(fid) => format!("Inst(Def#{})", fid.0),

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
    format!("Set[Field#{}, {}]", f.field_idx, show_value(&f.value, ir))
}

pub fn show_shape_entry(idx: usize, ir: &IrRes) -> String {
    let td   = &ir.shapes[idx];
    let name = td.name.as_deref().unwrap_or("_");
    format!("Shape#{}[{}, {}]", idx, name, show_type_body(&td.body, ir))
}

pub fn show_def_entry(idx: usize, ir: &IrRes) -> String {
    let def = &ir.defs[idx];
    let mut parts = vec![def.name.clone(), format!("Shape#{}", def.shape_id.0)];
    if def.planned {
        parts.push("planned".into());
    }
    if let Some(base_def) = def.base_def {
        parts.push(format!("base=Def#{}", base_def.0));
    }
    if let Some(hint) = &def.type_hint {
        parts.push(format!("hint={}", hint));
    }
    parts.extend(def.fields.iter().map(|f| show_field(f, ir)));
    format!("Def#{}[{}]", idx, parts.join(", "))
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

    // Shapes belonging to this scope (arena order)
    for (i, t) in ir.shapes.iter().enumerate() {
        if t.scope == scope_id {
            parts.push(show_shape_entry(i, ir));
        }
    }

    // Defs belonging to this scope (arena order)
    for (i, def) in ir.defs.iter().enumerate() {
        if def.scope == scope_id {
            parts.push(show_def_entry(i, ir));
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
#[allow(dead_code)]
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

    for e in &ir.errors {
        lines.push(format!("ERR: {}", e.message));
    }

    norm(&lines.join("\n"))
}
