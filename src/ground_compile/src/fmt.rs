use crate::ast::{
    AstComment, AstDef, AstDefI, AstDefO, AstField, AstItem, AstPrimitive, AstRef, AstRefSegVal,
    AstStructField, AstStructFieldBody, AstStructFieldKind, AstStructItem, AstTypeExpr, AstValue,
    AstUse,
};
use crate::parse::parse_file_items;

pub fn format_source(src: &str) -> Result<String, Vec<String>> {
    let (items, errors) = parse_file_items(src, 0);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(|e| e.message).collect());
    }
    Ok(render_items(&items, 0, true))
}

fn render_items(items: &[AstItem], indent: usize, top_level: bool) -> String {
    if top_level {
        let mut uses: Vec<String> = items.iter()
            .filter_map(|item| match item {
                AstItem::Use(_) => Some(render_item(item, indent)),
                _ => None,
            })
            .collect();
        uses.sort();

        let others: Vec<String> = items.iter()
            .filter(|item| !matches!(item, AstItem::Use(_)))
            .map(|item| render_item(item, indent))
            .collect();

        let mut blocks = vec![];
        if !uses.is_empty() {
            blocks.push(uses.join("\n"));
        }
        if !others.is_empty() {
            blocks.push(others.join("\n\n"));
        }
        return blocks.join("\n\n");
    }

    let rendered: Vec<String> = items.iter().map(|item| render_item(item, indent)).collect();
    rendered.join("\n")
}

fn render_item(item: &AstItem, indent: usize) -> String {
    match item {
        AstItem::Def(def) => render_def(&def.inner, indent),
        AstItem::Pack(pack) => {
            let head = format!("{}pack {}", spaces(indent), render_ref(&pack.inner.path.inner));
            match &pack.inner.defs {
                Some(defs) if !defs.is_empty() => format!("{head} {{\n{}\n{}}}", render_items(defs, indent + 2, false), spaces(indent)),
                Some(_) => format!("{head} {{}}"),
                None => head,
            }
        }
        AstItem::Use(use_) => render_use(&use_.inner, indent),
        AstItem::Comment(c) => render_comment(&c.inner, indent),
    }
}

fn render_use(use_: &AstUse, indent: usize) -> String {
    format!("{}use {}", spaces(indent), render_ref(&use_.path))
}

fn render_def(def: &AstDef, indent: usize) -> String {
    let head = if def.planned {
        format!("{}plan {}", spaces(indent), def.name.inner)
    } else if def.input.is_empty() && def.mapper.is_none() {
        match &def.output.inner {
            AstDefO::Unit => format!("{}def {}", spaces(indent), def.name.inner),
            _ => format!("{}{} =", spaces(indent), def.name.inner),
        }
    } else if def.input.is_empty() {
        format!("{}{} =", spaces(indent), def.name.inner)
    } else {
        let input = render_input_block(&def.input, indent);
        format!("{}def {} {}", spaces(indent), def.name.inner, input)
    };

    match &def.output.inner {
        AstDefO::Unit => {
            if let Some(mapper) = &def.mapper {
                format!("{head} {}", render_ref(&mapper.inner))
            } else {
                head
            }
        }
        AstDefO::TypeExpr(ty) => {
            if matches!(head.trim_end(), x if x.ends_with('=')) {
                format!("{head} {}", render_type_expr(&ty.inner, indent, false))
            } else if let Some(mapper) = &def.mapper {
                format!("{head} = {} {}", render_ref(&mapper.inner), render_type_expr(&ty.inner, indent, false))
            } else {
                format!("{head} = {}", render_type_expr(&ty.inner, indent, false))
            }
        }
        AstDefO::Struct(items) => {
            let body = render_struct_items(items, indent);
            if let Some(mapper) = &def.mapper {
                if matches!(head.trim_end(), x if x.ends_with('=')) {
                    format!("{head} {} {}", render_ref(&mapper.inner), body)
                } else {
                    format!("{head} = {} {}", render_ref(&mapper.inner), body)
                }
            } else if matches!(head.trim_end(), x if x.ends_with('=')) {
                format!("{head} {}", body)
            } else {
                format!("{head} = {}", body)
            }
        }
    }
}

fn render_input_block(fields: &[crate::ast::AstNode<AstDefI>], indent: usize) -> String {
    if fields.is_empty() {
        return "{}".into();
    }
    let parts: Vec<String> = fields.iter().map(|f| {
        let name = f.inner.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
        format!("{}{} = {}", spaces(indent + 2), name, render_type_expr(&f.inner.ty.inner, indent + 2, false))
    }).collect();
    format!("{{\n{}\n{}}}", parts.join("\n"), spaces(indent))
}

fn render_type_expr(ty: &AstTypeExpr, indent: usize, nested: bool) -> String {
    match ty {
        AstTypeExpr::Unit => "unit".into(),
        AstTypeExpr::Primitive(p) => render_primitive(p).into(),
        AstTypeExpr::Ref(r) => render_ref(r),
        AstTypeExpr::Enum(items) => items.iter().map(|r| render_ref(&r.inner)).collect::<Vec<_>>().join(" | "),
        AstTypeExpr::Struct(items) => {
            if items.is_empty() {
                "{}".into()
            } else if can_inline_struct_items(items) && nested {
                format!("{{ {} }}", items.iter().map(|i| render_struct_item(&i.inner, indent + 2)).collect::<Vec<_>>().join(" "))
            } else {
                render_struct_items(items, indent)
            }
        }
        AstTypeExpr::List(inner) => render_list_type(&inner.inner, indent),
    }
}

fn render_list_type(inner: &AstTypeExpr, indent: usize) -> String {
    let inner_s = render_type_expr(inner, indent, false);
    if inner_s.contains('\n') {
        format!("[\n{}{}\n{}]", spaces(indent + 2), inner_s.replace('\n', &format!("\n{}", spaces(indent + 2))), spaces(indent))
    } else {
        format!("[ {} ]", inner_s)
    }
}

fn render_struct_items(items: &[crate::ast::AstNode<AstStructItem>], indent: usize) -> String {
    if items.is_empty() {
        return "{}".into();
    }
    let inline = can_inline_struct_items(items);
    if inline {
        return format!("{{ {} }}", items.iter().map(|i| render_struct_item(&i.inner, indent + 2)).collect::<Vec<_>>().join(" "));
    }
    let parts: Vec<String> = items.iter().map(|i| render_struct_item_line(&i.inner, indent + 2)).collect();
    format!("{{\n{}\n{}}}", parts.join("\n"), spaces(indent))
}

fn can_inline_struct_items(items: &[crate::ast::AstNode<AstStructItem>]) -> bool {
    items.len() == 1 && !matches!(items[0].inner, AstStructItem::Comment(_))
}

fn render_struct_item(item: &AstStructItem, indent: usize) -> String {
    match item {
        AstStructItem::Field(f) => render_struct_field(&f.inner, indent),
        AstStructItem::Anon(v) => render_value(&v.inner, indent),
        AstStructItem::Def(d) => render_def(&d.inner, indent),
        AstStructItem::Comment(c) => render_comment(&c.inner, indent),
    }
}

fn render_struct_item_line(item: &AstStructItem, indent: usize) -> String {
    match item {
        AstStructItem::Def(d) => render_def(&d.inner, indent),
        AstStructItem::Comment(c) => render_comment(&c.inner, indent),
        _ => format!("{}{}", spaces(indent), render_struct_item(item, indent)),
    }
}

fn render_struct_field(field: &AstStructField, indent: usize) -> String {
    let name = field.name.as_ref().map(|n| n.inner.as_str()).unwrap_or("_");
    match (&field.kind, &field.body) {
        (AstStructFieldKind::Def, AstStructFieldBody::Type(ty)) => {
            format!("{name} = {}", render_type_expr(&ty.inner, indent, true))
        }
        (AstStructFieldKind::Set, AstStructFieldBody::Value(v)) => {
            format!("{name}: {}", render_value(&v.inner, indent))
        }
        (AstStructFieldKind::Def, AstStructFieldBody::Value(v)) => {
            format!("{name} = {}", render_value(&v.inner, indent))
        }
        (AstStructFieldKind::Set, AstStructFieldBody::Type(ty)) => {
            format!("{name}: {}", render_type_expr(&ty.inner, indent, true))
        }
    }
}

fn render_value(value: &AstValue, indent: usize) -> String {
    match value {
        AstValue::Str(s) => format!("{:?}", s),
        AstValue::Ref(r) => render_ref(r),
        AstValue::List(items) => render_value_list(items, indent),
        AstValue::Struct { type_hint, fields } => {
            let head = type_hint.as_ref().map(|h| format!("{} ", render_ref(&h.inner))).unwrap_or_default();
            let body = render_value_fields(fields, indent);
            format!("{head}{body}")
        }
    }
}

fn render_value_list(items: &[crate::ast::AstNode<AstValue>], indent: usize) -> String {
    if items.is_empty() {
        return "[]".into();
    }
    if items.len() == 1 {
        return format!("[ {} ]", render_value(&items[0].inner, indent));
    }
    let parts: Vec<String> = items.iter().map(|i| format!("{}{}", spaces(indent + 2), render_value(&i.inner, indent + 2))).collect();
    format!("[\n{}\n{}]", parts.join("\n"), spaces(indent))
}

fn render_value_fields(fields: &[crate::ast::AstNode<AstField>], indent: usize) -> String {
    if fields.is_empty() {
        return "{}".into();
    }
    let inline = fields.len() == 1 && !matches!(fields[0].inner, AstField::Comment(_));
    if inline {
        return format!("{{ {} }}", render_field(&fields[0].inner, indent + 2));
    }
    let parts: Vec<String> = fields.iter().map(|f| render_field_line(&f.inner, indent + 2)).collect();
    format!("{{\n{}\n{}}}", parts.join("\n"), spaces(indent))
}

fn render_field(field: &AstField, indent: usize) -> String {
    match field {
        AstField::Named { name, value, via } => {
            if *via {
                format!("{}: via {}", name.inner, render_value(&value.inner, indent))
            } else {
                format!("{}: {}", name.inner, render_value(&value.inner, indent))
            }
        }
        AstField::Anon(v) => render_value(&v.inner, indent),
        AstField::Comment(c) => render_comment(&c.inner, indent),
    }
}

fn render_field_line(field: &AstField, indent: usize) -> String {
    match field {
        AstField::Comment(c) => render_comment(&c.inner, indent),
        _ => format!("{}{}", spaces(indent), render_field(field, indent)),
    }
}

fn render_comment(comment: &AstComment, indent: usize) -> String {
    if comment.text.is_empty() {
        format!("{}#", spaces(indent))
    } else {
        format!("{}# {}", spaces(indent), comment.text)
    }
}

fn render_ref(r: &AstRef) -> String {
    r.segments.iter().map(render_ref_seg).collect::<Vec<_>>().join(":")
}

fn render_ref_seg(seg: &crate::ast::AstNode<crate::ast::AstRefSeg>) -> String {
    let mut inner = match &seg.inner.value {
        AstRefSegVal::Plain(s) => s.clone(),
        AstRefSegVal::Group(r, trailing) => format!("{{{}}}{}", render_ref(r), trailing.as_deref().unwrap_or("")),
    };
    if seg.inner.is_opt {
        inner.push('?');
    }
    inner
}

fn render_primitive(p: &AstPrimitive) -> &'static str {
    match p {
        AstPrimitive::String => "string",
        AstPrimitive::Integer => "integer",
        AstPrimitive::Boolean => "boolean",
        AstPrimitive::Reference => "reference",
    }
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

#[cfg(test)]
mod tests {
    use super::format_source;

    #[test]
    fn fmt_comments_and_structs() {
        let src = r#"
# top
service={# doc
port=grpc|http
}
"#;
        let got = format_source(src).unwrap();
        assert_eq!(got, "# top\n\nservice = {\n  # doc\n  port = grpc | http\n}");
    }

    #[test]
    fn fmt_values_and_lists() {
        let src = r#"api=service{access: [media:http database:main] scaling: scaling{min: 1 max: 2}}"#;
        let got = format_source(src).unwrap();
        assert_eq!(got, "api = service {\n  access: [\n    media:http\n    database:main\n  ]\n  scaling: scaling {\n    min: 1\n    max: 2\n  }\n}");
    }

    #[test]
    fn fmt_sorts_and_groups_imports() {
        let src = r#"
service = { port = grpc | http }
use pack:ops
use pack:app
use pack:std
"#;
        let got = format_source(src).unwrap();
        assert_eq!(got, "use pack:app\nuse pack:ops\nuse pack:std\n\nservice = { port = grpc | http }");
    }
}
