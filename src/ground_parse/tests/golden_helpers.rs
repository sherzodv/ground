use ground_parse::ast2::{
    AstDef, AstField, AstLinkDef, AstPrimitive, AstRef,
    AstStructItem, AstTypeDef, AstTypeDefBody, AstValue,
    ParseReq,
};
use ground_parse::parse2::parse;

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
    format!("TypeDef[{}, {}]", name, show_type_def_body(&td.body.inner))
}

pub fn show_struct_item(item: &AstStructItem) -> String {
    match item {
        AstStructItem::TypeDef(td) => show_type_def(&td.inner),
        AstStructItem::LinkDef(ld) => show_link_def(&ld.inner),
    }
}

pub fn show_link_def(ld: &AstLinkDef) -> String {
    let name = ld.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
    format!("LinkDef[{}, {}]", name, show_type_def(&ld.ty.inner))
}

pub fn show_value(v: &AstValue) -> String {
    match v {
        AstValue::Str(s)      => format!("Str({:?})", s),
        AstValue::Ref(r)      => format!("Ref({})", show_ref(r)),
        AstValue::List(items) => {
            let parts: Vec<_> = items.iter().map(|i| show_value(&i.inner)).collect();
            format!("List[{}]", parts.join(", "))
        }
    }
}

pub fn show_field(f: &AstField) -> String {
    match f {
        AstField::Named { name, value } => format!("Field[{}, {}]", name.inner, show_value(&value.inner)),
        AstField::Anon(v)               => format!("Anon[{}]", show_value(&v.inner)),
    }
}

pub fn show_def(def: &AstDef) -> String {
    match def {
        AstDef::Type(td) => show_type_def(&td.inner),
        AstDef::Link(ld) => show_link_def(&ld.inner),
        AstDef::Inst(inst) => {
            let mut parts = vec![
                inst.inner.type_name.inner.clone(),
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

/// Parse `input`, format result as a compact multi-line string.
pub fn show(input: &str) -> String {
    let res = parse(ParseReq { units: vec![input.to_string()] });
    let mut lines: Vec<String> = res.units.iter()
        .flat_map(|u| u.defs.iter().map(show_def))
        .collect();
    for e in &res.errors {
        lines.push(format!("ERR: {}", e.message));
    }
    lines.join("\n")
}
