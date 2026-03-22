use ground_compile::ast::{
    AstDef, AstField, AstLinkDef, AstPrimitive, AstRef, AstScope, AstScopeId,
    AstStructItem, AstTypeDef, AstTypeDefBody, AstValue, ScopeKind,
    ParseReq, ParseUnit,
    AstUse,
};
use ground_compile::parse::parse;

pub fn show_ref(r: &AstRef) -> String {
    r.segments.iter().map(|s| {
        if s.inner.is_opt { format!("{}?", s.inner.value) }
        else              { s.inner.value.clone() }
    }).collect::<Vec<_>>().join(":")
}

pub fn show_primitive(p: &AstPrimitive) -> &'static str {
    match p {
        AstPrimitive::String    => "string",
        AstPrimitive::Integer   => "integer",
        AstPrimitive::Reference => "reference",
    }
}

pub fn show_type_def_body(body: &AstTypeDefBody) -> String {
    match body {
        AstTypeDefBody::Primitive(p) => format!("Primitive({})", show_primitive(p)),
        AstTypeDefBody::Ref(r)       => format!("Ref({})", show_ref(r)),
        AstTypeDefBody::Enum(items)  => {
            let parts: Vec<_> = items.iter().map(|i| format!("Ref({})", show_ref(&i.inner))).collect();
            format!("Enum[{}]", parts.join(" | "))
        }
        AstTypeDefBody::Struct(items) => {
            let parts: Vec<_> = items.iter().map(|i| show_struct_item(&i.inner)).collect();
            format!("Struct[{}]", parts.join(", "))
        }
        AstTypeDefBody::List(inner) => {
            format!("List[{}]", show_type_def(&inner.inner))
        }
    }
}

pub fn show_type_def(td: &AstTypeDef) -> String {
    let name = td.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
    format!("Type[{}, {}]", name, show_type_def_body(&td.body.inner))
}

pub fn show_struct_item(item: &AstStructItem) -> String {
    match item {
        AstStructItem::TypeDef(td) => show_type_def(&td.inner),
        AstStructItem::LinkDef(ld) => show_link_def(&ld.inner),
    }
}

pub fn show_link_def(ld: &AstLinkDef) -> String {
    let name = ld.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
    format!("Link[{}, {}]", name, show_type_def(&ld.ty.inner))
}

pub fn show_value(v: &AstValue) -> String {
    match v {
        AstValue::Str(s)        => format!("Str({:?})", s),
        AstValue::Ref(r)        => format!("Ref({})", show_ref(r)),
        AstValue::List(items)   => {
            let parts: Vec<_> = items.iter().map(|i| show_value(&i.inner)).collect();
            format!("List[{}]", parts.join(", "))
        }
        AstValue::Struct { type_hint, fields } => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(hint) = type_hint {
                parts.push(format!("Hint({})", show_ref(&hint.inner)));
            }
            parts.extend(fields.iter().map(|f| show_field(&f.inner)));
            format!("Struct[{}]", parts.join(", "))
        }
    }
}

pub fn show_field(f: &AstField) -> String {
    match f {
        AstField::Named { name, value } => format!("Field[{}, {}]", name.inner, show_value(&value.inner)),
        AstField::Anon(v)               => format!("Anon[{}]", show_value(&v.inner)),
    }
}

pub fn show_use(u: &AstUse) -> String {
    format!("Use[{}]", show_ref(&u.path))
}

pub fn show_def(def: &AstDef) -> String {
    match def {
        AstDef::Use(u)   => show_use(&u.inner),
        AstDef::Type(td) => show_type_def(&td.inner),
        AstDef::Link(ld) => show_link_def(&ld.inner),
        AstDef::Inst(inst) => {
            let mut parts = vec![
                show_ref(&inst.inner.type_name.inner),
                inst.inner.inst_name.inner.clone(),
            ];
            parts.extend(inst.inner.fields.iter().map(|f| show_field(&f.inner)));
            format!("Inst[{}]", parts.join(", "))
        }
        AstDef::Deploy(dep) => {
            let mut parts = vec![
                show_ref(&dep.inner.what.inner),
                show_ref(&dep.inner.target.inner),
                show_ref(&dep.inner.name.inner),
            ];
            parts.extend(dep.inner.fields.iter().map(|f| show_field(&f.inner)));
            format!("Deploy[{}]", parts.join(", "))
        }
        AstDef::Scope(s) => {
            let name = s.inner.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
            let parts: Vec<_> = s.inner.defs.iter().map(show_def).collect();
            format!("Scope[{}, {}]", name, parts.join(", "))
        }
    }
}

pub fn show_scope(scopes: &[AstScope], id: AstScopeId) -> String {
    let scope = &scopes[id.0 as usize];
    let raw_name = scope.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
    let kind_str = match scope.kind {
        ScopeKind::Pack => "pack",
        ScopeKind::Type => "type",
    };
    let name = format!("{}:{}", kind_str, raw_name);

    let mut parts: Vec<String> = scope.defs.iter().map(show_def).collect();

    // Append child scopes from arena (preserving insertion order)
    for (i, s) in scopes.iter().enumerate() {
        if s.parent == Some(id) {
            parts.push(show_scope(scopes, AstScopeId(i as u32)));
        }
    }

    if parts.is_empty() {
        format!("Scope[{}]", name)
    } else {
        format!("Scope[{},\n{},\n]", name, parts.join(",\n"))
    }
}

/// Normalise a raw-string expected value: strip blank lines, trim each line.
pub fn norm(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse `input` as a single unit named "test", return the scope tree rooted
/// at the direct children of the synth root.
pub fn show(input: &str) -> String {
    let req = ParseReq {
        units: vec![ParseUnit { name: "test".into(), path: vec![], src: input.to_string() }],
    };
    let res = parse(req);

    let mut lines: Vec<String> = res.scopes.iter().enumerate().skip(1)
        .filter(|(_, s)| s.parent == Some(AstScopeId(0)))
        .map(|(i, _)| show_scope(&res.scopes, AstScopeId(i as u32)))
        .collect();

    for e in &res.errors {
        lines.push(format!("ERR: {}", e.message));
    }

    norm(&lines.join("\n"))
}
