use ground_compile::ast::{
    AstComment, AstItem, AstDef, AstDefO, AstDefI, AstField, AstPack, AstPrimitive, AstRef,
    AstRefSegVal, AstScope, AstScopeId, AstStructField, AstStructFieldBody, AstStructFieldKind,
    AstStructItem, AstTypeExpr, AstValue, ScopeKind, ParseReq, ParseUnit, AstUse,
};
use ground_compile::parse::parse;

/// Parse multiple units, return the scope tree.
/// Each entry is `(name, path_segs, src)`.
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

    let mut lines: Vec<String> = res.scopes.iter().enumerate().skip(1)
        .filter(|(_, s)| s.parent == Some(AstScopeId(0)))
        .map(|(i, _)| show_scope(&res.scopes, AstScopeId(i as u32)))
        .collect();

    for e in &res.errors {
        lines.push(format!("ERR: {}", e.message));
    }

    norm(&lines.join("\n"))
}

pub fn show_ref(r: &AstRef) -> String {
    r.segments.iter().map(|s| {
        let inner = match &s.inner.value {
            AstRefSegVal::Plain(v)         => v.clone(),
            AstRefSegVal::Group(g, trail)  => format!(
                "{{{}}}{}",
                show_ref(g),
                trail.as_deref().unwrap_or(""),
            ),
        };
        if s.inner.is_opt { format!("{}?", inner) } else { inner }
    }).collect::<Vec<_>>().join(":")
}

pub fn show_primitive(p: &AstPrimitive) -> &'static str {
    match p {
        AstPrimitive::String    => "string",
        AstPrimitive::Integer   => "integer",
        AstPrimitive::Boolean   => "boolean",
        AstPrimitive::Reference => "reference",
    }
}

pub fn show_type_expr(body: &AstTypeExpr) -> String {
    match body {
        AstTypeExpr::Unit         => "unit".to_string(),
        AstTypeExpr::Primitive(p) => format!("Primitive({})", show_primitive(p)),
        AstTypeExpr::Ref(r)       => format!("Ref({})", show_ref(r)),
        AstTypeExpr::Enum(items)  => {
            let parts: Vec<_> = items.iter().map(|i| format!("Ref({})", show_ref(&i.inner))).collect();
            format!("Enum[{}]", parts.join(" | "))
        }
        AstTypeExpr::Struct(items) => {
            let parts: Vec<_> = items.iter().map(|i| show_struct_item(&i.inner)).collect();
            format!("Struct[{}]", parts.join(", "))
        }
        AstTypeExpr::List(inner) => {
            format!("List[{}]", show_nested_type_expr(&inner.inner))
        }
    }
}

pub fn show_nested_type_expr(td: &AstTypeExpr) -> String {
    format!("Type[_, {}]", show_type_expr(td))
}

pub fn show_field_def(f: &AstDefI) -> String {
    let name = f.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
    format!("Field[{}, {}]", name, show_nested_type_expr(&f.ty.inner))
}

pub fn show_struct_field(f: &AstStructField) -> String {
    let name = f.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
    match (&f.kind, &f.body) {
        (AstStructFieldKind::Def, AstStructFieldBody::Type(ty)) => {
            format!("FieldDef[{}, {}]", name, show_nested_type_expr(&ty.inner))
        }
        (AstStructFieldKind::Set, AstStructFieldBody::Value(value)) => {
            format!("FieldSet[{}, {}]", name, show_value(&value.inner))
        }
        (AstStructFieldKind::Def, AstStructFieldBody::Value(value)) => {
            format!("FieldDef[{}, {}]", name, show_value(&value.inner))
        }
        (AstStructFieldKind::Set, AstStructFieldBody::Type(ty)) => {
            format!("FieldSet[{}, {}]", name, show_nested_type_expr(&ty.inner))
        }
    }
}

pub fn show_top_def_output(output: &AstDefO) -> String {
    match output {
        AstDefO::Unit => "unit".to_string(),
        AstDefO::TypeExpr(td) => show_type_expr(&td.inner),
        AstDefO::Struct(items) => {
            let parts: Vec<_> = items.iter().map(|i| show_struct_item(&i.inner)).collect();
            format!("Struct[{}]", parts.join(", "))
        }
    }
}

pub fn show_top_def(td: &AstDef) -> String {
    let name = td.name.inner.clone();
    let mapper = td.mapper.as_ref().map(|m| show_ref(&m.inner)).unwrap_or_else(|| "_".to_string());
    let input = if td.input.is_empty() {
        "unit".to_string()
    } else {
        let input_parts: Vec<_> = td.input.iter()
            .map(|f| show_field_def(&f.inner))
            .collect();
        format!("Input[{}]", input_parts.join(", "))
    };
    let output = show_top_def_output(&td.output.inner);
    let head = if td.planned { "Plan" } else { "Def" };
    format!("{head}[{name}, {mapper}, {input}, {output}]")
}

pub fn show_pack(p: &AstPack) -> String {
    let path = show_ref(&p.path.inner);
    if let Some(defs) = &p.defs {
        if defs.is_empty() {
            format!("Pack[{},\n,\n]", path)
        } else {
            let parts: Vec<_> = defs.iter().map(show_def).collect();
            format!("Pack[{},\n{},\n]", path, parts.join(",\n"))
        }
    } else {
        format!("Pack[{}]", path)
    }
}

pub fn show_struct_item(item: &AstStructItem) -> String {
    match item {
        AstStructItem::Field(fd)    => show_struct_field(&fd.inner),
        AstStructItem::Anon(v)      => format!("Anon[{}]", show_value(&v.inner)),
        AstStructItem::Def(td)      => show_top_def(&td.inner),
        AstStructItem::Comment(c)   => show_comment(&c.inner),
    }
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
        AstField::Named { name, value, .. } => format!("Field[{}, {}]", name.inner, show_value(&value.inner)),
        AstField::Anon(v)                   => format!("Anon[{}]", show_value(&v.inner)),
        AstField::Comment(c)                => show_comment(&c.inner),
    }
}

pub fn show_use(u: &AstUse) -> String {
    format!("Use[{}]", show_ref(&u.path))
}

pub fn show_def(def: &AstItem) -> String {
    match def {
        AstItem::Def(td)      => show_top_def(&td.inner),
        AstItem::Pack(p)      => show_pack(&p.inner),
        AstItem::Use(u)       => show_use(&u.inner),
        AstItem::Comment(c)   => show_comment(&c.inner),
    }
}

pub fn show_comment(c: &AstComment) -> String {
    format!("Comment({})", c.text)
}

pub fn show_scope(scopes: &[AstScope], id: AstScopeId) -> String {
    let scope = &scopes[id.0 as usize];
    let raw_name = scope.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
    let kind_str = match scope.kind {
        ScopeKind::Pack => "pack",
        ScopeKind::Struct => "struct",
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
        units: vec![ParseUnit { name: "test".into(), path: vec![], src: input.to_string(), ts_src: None }],
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
