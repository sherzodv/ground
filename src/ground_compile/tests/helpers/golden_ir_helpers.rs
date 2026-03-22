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

pub fn show_ref(r: &IrRef) -> String {
    r.segments.iter().map(|seg| {
        let val = show_ref_seg_value(&seg.value);
        if seg.is_opt { format!("({})", val) } else { val }
    }).collect::<Vec<_>>().join(":")
}

pub fn show_ref_seg_value(v: &IrRefSegValue) -> String {
    match v {
        IrRefSegValue::Pack(id)  => format!("Pack#{}", id.0),
        IrRefSegValue::Type(id)  => format!("Type#{}", id.0),
        IrRefSegValue::Link(id)  => format!("Link#{}", id.0),
        IrRefSegValue::Inst(id)  => format!("Inst#{}", id.0),
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

        IrTypeBody::Enum(variants) => format!("Enum[{}]", variants.join("|")),

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

        IrValue::Variant(tid, idx) => {
            let variant = match &ir.types[tid.0 as usize].body {
                IrTypeBody::Enum(vs) => vs[*idx as usize].as_str(),
                _                    => "?",
            };
            format!("Variant(Type#{}, {:?})", tid.0, variant)
        }

        IrValue::Inst(iid) => format!("Inst(Inst#{})", iid.0),

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

pub fn show_inst_entry(idx: usize, ir: &IrRes) -> String {
    let inst = &ir.insts[idx];
    let mut parts = vec![format!("Type#{}", inst.type_id.0), inst.name.clone()];
    if let Some(hint) = &inst.type_hint {
        parts.push(format!("hint={}", hint));
    }
    parts.extend(inst.fields.iter().map(|f| show_field(f, ir)));
    format!("Inst#{}[{}]", idx, parts.join(", "))
}

pub fn show_deploy_entry(dep: &IrDeployDef, ir: &IrRes) -> String {
    let mut parts = vec![show_ref(&dep.what), show_ref(&dep.target), show_ref(&dep.name)];
    parts.extend(dep.fields.iter().map(|f| show_field(f, ir)));
    format!("Deploy[{}]", parts.join(", "))
}

// ---------------------------------------------------------------------------
// Scope tree
// ---------------------------------------------------------------------------

fn show_scope_ir(scope_id: ScopeId, ir: &IrRes) -> String {
    let scope = &ir.scopes[scope_id.0 as usize];
    let raw_name = scope.name.as_deref().unwrap_or("_");
    let kind_str = match scope.kind {
        ScopeKind::Pack => "pack",
        ScopeKind::Type => "type",
    };
    let name = format!("{}:{}", kind_str, raw_name);

    let mut parts: Vec<String> = Vec::new();

    // Types belonging to this scope (arena order)
    for (i, t) in ir.types.iter().enumerate() {
        if t.scope == scope_id {
            parts.push(show_type_entry(i, ir));
        }
    }

    // Pack-level links belonging to this scope (struct links are shown inline in type bodies)
    if scope.kind == ScopeKind::Pack {
        for (i, l) in ir.links.iter().enumerate() {
            if l.scope == scope_id {
                parts.push(show_link_entry(i, ir));
            }
        }
    }

    // Instances belonging to this scope (arena order)
    for (i, inst) in ir.insts.iter().enumerate() {
        if inst.scope == scope_id {
            parts.push(show_inst_entry(i, ir));
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
            name: name.into(),
            path: path.into_iter().map(|s| s.to_string()).collect(),
            src:  src.to_string(),
        }).collect(),
    };
    let res = parse(req);
    show_ir(resolve(res))
}

/// Parse + resolve `input`, format as compact multi-line string.
pub fn show(input: &str) -> String {
    let res = parse(ParseReq {
        units: vec![ParseUnit { name: "test".into(), path: vec![], src: input.to_string() }],
    });
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

    // Deploys are flat for now (IrDeployDef has no scope field yet)
    for dep in &ir.deploys {
        lines.push(show_deploy_entry(dep, &ir));
    }

    for e in &ir.errors {
        lines.push(format!("ERR: {}", e.message));
    }

    norm(&lines.join("\n"))
}
