use ground_compile::ast::{
    AstDef, AstDefO, AstField, AstItem, AstNode, AstRef, AstRefSegVal, AstStructFieldBody,
    AstStructItem, AstTypeExpr, AstUse, AstValue, UnitId,
};
use serde_json::{json, Value};

use crate::util::offset_to_line_col;
use crate::workspace::{lookup_def_visible, lookup_pack_path, lookup_shape_visible, Workspace};

pub fn semantic_tokens(workspace: &Workspace, params: &Value) -> Value {
    let Some(uri) = params
        .get("textDocument")
        .and_then(|v| v.get("uri"))
        .and_then(Value::as_str)
    else {
        return json!({ "data": [] });
    };
    let Some(src) = workspace.text_for_uri(uri) else {
        return json!({ "data": [] });
    };
    if uri.ends_with(".ts") {
        return json!({ "data": build_lexical_tokens(src, true) });
    }
    let Some((analysis, scope, unit)) = workspace.analysis_scope_and_unit_for_uri(uri) else {
        return json!({ "data": build_lexical_tokens(src, false) });
    };
    let mut tokens = vec![];
    push_lexical_ground_tokens(src, &mut tokens);
    if let Some(pu) = analysis.res.parse.units.get(unit.as_usize()) {
        collect_scope_ref_tokens(
            &analysis.res.parse.scopes,
            pu.scope_id.0 as usize,
            unit,
            src,
            &analysis.res.ir,
            scope,
            &mut tokens,
        );
    }
    json!({ "data": encode_tokens(src, tokens) })
}

#[derive(Clone, Copy)]
enum RefContext {
    Use,
    Mapper,
    Type,
    Value,
}

#[derive(Clone, Copy)]
struct Token {
    line: u32,
    start: u32,
    len: u32,
    token_type: u32,
}

const TOK_KEYWORD: u32 = 0;
const TOK_NAMESPACE: u32 = 1;
const TOK_TYPE: u32 = 2;
const TOK_FUNCTION: u32 = 3;
const TOK_PROPERTY: u32 = 4;
const TOK_ENUM_MEMBER: u32 = 5;
const TOK_VARIABLE: u32 = 6;
const TOK_STRING: u32 = 7;
const TOK_NUMBER: u32 = 8;
const TOK_COMMENT: u32 = 9;
const TOK_CLASS: u32 = 10;

fn build_lexical_tokens(src: &str, is_ts: bool) -> Vec<u32> {
    let mut tokens = vec![];
    for (line_idx, line) in src.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                tokens.push(Token {
                    line: line_idx as u32,
                    start: i as u32,
                    len: (chars.len() - i) as u32,
                    token_type: TOK_COMMENT,
                });
                break;
            }
            if c == '"' {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(Token {
                    line: line_idx as u32,
                    start: start as u32,
                    len: (i - start) as u32,
                    token_type: TOK_STRING,
                });
                continue;
            }
            if c.is_ascii_digit() {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                tokens.push(Token {
                    line: line_idx as u32,
                    start: start as u32,
                    len: (i - start) as u32,
                    token_type: TOK_NUMBER,
                });
                continue;
            }
            if c.is_ascii_alphabetic() || c == '_' {
                let start = i;
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '-'))
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let token_type = if is_ts {
                    if matches!(
                        word.as_str(),
                        "export"
                            | "function"
                            | "return"
                            | "const"
                            | "let"
                            | "if"
                            | "else"
                            | "for"
                            | "while"
                            | "interface"
                            | "type"
                    ) {
                        Some(TOK_KEYWORD)
                    } else {
                        None
                    }
                } else if matches!(word.as_str(), "def" | "plan" | "pack" | "use" | "via") {
                    Some(TOK_KEYWORD)
                } else if matches!(
                    word.as_str(),
                    "string" | "integer" | "boolean" | "reference" | "ipv4" | "ipv4net"
                ) {
                    Some(TOK_TYPE)
                } else {
                    None
                };
                if let Some(token_type) = token_type {
                    tokens.push(Token {
                        line: line_idx as u32,
                        start: start as u32,
                        len: (i - start) as u32,
                        token_type,
                    });
                }
                continue;
            }
            i += 1;
        }
    }
    encode_tokens(src, tokens)
}

fn push_lexical_ground_tokens(src: &str, tokens: &mut Vec<Token>) {
    for (line_idx, line) in src.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if c == '#' {
                tokens.push(Token {
                    line: line_idx as u32,
                    start: i as u32,
                    len: (chars.len() - i) as u32,
                    token_type: TOK_COMMENT,
                });
                break;
            }
            if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                tokens.push(Token {
                    line: line_idx as u32,
                    start: i as u32,
                    len: (chars.len() - i) as u32,
                    token_type: TOK_COMMENT,
                });
                break;
            }
            if c == '"' {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(Token {
                    line: line_idx as u32,
                    start: start as u32,
                    len: (i - start) as u32,
                    token_type: TOK_STRING,
                });
                continue;
            }
            if c.is_ascii_digit() {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                tokens.push(Token {
                    line: line_idx as u32,
                    start: start as u32,
                    len: (i - start) as u32,
                    token_type: TOK_NUMBER,
                });
                continue;
            }
            if c.is_ascii_alphabetic() || c == '_' {
                let start = i;
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '-'))
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if matches!(word.as_str(), "def" | "plan" | "pack" | "use" | "via") {
                    tokens.push(Token {
                        line: line_idx as u32,
                        start: start as u32,
                        len: (i - start) as u32,
                        token_type: TOK_KEYWORD,
                    });
                } else if matches!(
                    word.as_str(),
                    "string" | "integer" | "boolean" | "reference" | "ipv4" | "ipv4net"
                ) {
                    tokens.push(Token {
                        line: line_idx as u32,
                        start: start as u32,
                        len: (i - start) as u32,
                        token_type: TOK_TYPE,
                    });
                }
                continue;
            }
            i += 1;
        }
    }
}

fn collect_scope_ref_tokens(
    scopes: &[ground_compile::ast::AstScope],
    scope_idx: usize,
    unit: UnitId,
    src: &str,
    ir: &ground_compile::ir::IrRes,
    visible_scope: ground_compile::ir::ScopeId,
    out: &mut Vec<Token>,
) {
    let Some(scope) = scopes.get(scope_idx) else {
        return;
    };
    for item in &scope.defs {
        collect_item_tokens(item, unit, src, ir, visible_scope, out);
    }
    for (child_idx, child) in scopes.iter().enumerate() {
        if child.parent.map(|p| p.0 as usize) == Some(scope_idx) {
            collect_scope_ref_tokens(scopes, child_idx, unit, src, ir, visible_scope, out);
        }
    }
}

fn collect_item_tokens(
    item: &AstItem,
    unit: UnitId,
    src: &str,
    ir: &ground_compile::ir::IrRes,
    scope: ground_compile::ir::ScopeId,
    out: &mut Vec<Token>,
) {
    match item {
        AstItem::Def(def) => collect_def_tokens(&def.inner, unit, src, ir, scope, out),
        AstItem::Pack(pack) => collect_ref_tokens(
            &pack.inner.path.inner,
            RefContext::Use,
            unit,
            src,
            ir,
            scope,
            out,
        ),
        AstItem::Use(use_) => collect_use_tokens(&use_.inner, unit, src, ir, scope, out),
        AstItem::Comment(_) => {}
    }
}

fn collect_use_tokens(
    use_: &AstUse,
    unit: UnitId,
    src: &str,
    ir: &ground_compile::ir::IrRes,
    scope: ground_compile::ir::ScopeId,
    out: &mut Vec<Token>,
) {
    collect_ref_tokens(&use_.path, RefContext::Use, unit, src, ir, scope, out);
}

fn collect_def_tokens(
    def: &AstDef,
    unit: UnitId,
    src: &str,
    ir: &ground_compile::ir::IrRes,
    scope: ground_compile::ir::ScopeId,
    out: &mut Vec<Token>,
) {
    push_name_token(&def.name, unit, src, TOK_CLASS, out);
    for input in &def.input {
        if let Some(name) = &input.inner.name {
            push_name_token(name, unit, src, TOK_PROPERTY, out);
        }
        collect_type_tokens(&input.inner.ty.inner, unit, src, ir, scope, out);
    }
    if let Some(mapper) = &def.mapper {
        collect_ref_tokens(&mapper.inner, RefContext::Mapper, unit, src, ir, scope, out);
    }
    match def.output.as_ref().map(|o| &o.inner) {
        None => {}
        Some(AstDefO::TypeExpr(ty)) => collect_type_tokens(&ty.inner, unit, src, ir, scope, out),
        Some(AstDefO::Struct(items)) => {
            for item in items {
                collect_struct_item_tokens(&item.inner, unit, src, ir, scope, out);
            }
        }
    }
}

fn collect_type_tokens(
    ty: &AstTypeExpr,
    unit: UnitId,
    src: &str,
    ir: &ground_compile::ir::IrRes,
    scope: ground_compile::ir::ScopeId,
    out: &mut Vec<Token>,
) {
    match ty {
        AstTypeExpr::Unit | AstTypeExpr::Primitive(_) => {}
        AstTypeExpr::Ref(r) => collect_ref_tokens(r, RefContext::Type, unit, src, ir, scope, out),
        AstTypeExpr::Enum(refs) => {
            for r in refs {
                collect_ref_tokens(&r.inner, RefContext::Type, unit, src, ir, scope, out);
            }
        }
        AstTypeExpr::Struct(items) => {
            for item in items {
                collect_struct_item_tokens(&item.inner, unit, src, ir, scope, out);
            }
        }
        AstTypeExpr::List(inner) => collect_type_tokens(&inner.inner, unit, src, ir, scope, out),
        AstTypeExpr::Tuple(items) => {
            for item in items {
                collect_type_tokens(&item.inner, unit, src, ir, scope, out);
            }
        }
        AstTypeExpr::Optional(inner) => {
            collect_type_tokens(&inner.inner, unit, src, ir, scope, out)
        }
    }
}

fn collect_struct_item_tokens(
    item: &AstStructItem,
    unit: UnitId,
    src: &str,
    ir: &ground_compile::ir::IrRes,
    scope: ground_compile::ir::ScopeId,
    out: &mut Vec<Token>,
) {
    match item {
        AstStructItem::Field(field) => {
            if let Some(name) = &field.inner.name {
                push_name_token(name, unit, src, TOK_PROPERTY, out);
            }
            match &field.inner.body {
                AstStructFieldBody::Type(ty) => {
                    collect_type_tokens(&ty.inner, unit, src, ir, scope, out)
                }
                AstStructFieldBody::Value(value) => {
                    collect_value_tokens(&value.inner, unit, src, ir, scope, out)
                }
            }
        }
        AstStructItem::Anon(value) => collect_value_tokens(&value.inner, unit, src, ir, scope, out),
        AstStructItem::Def(def) => collect_def_tokens(&def.inner, unit, src, ir, scope, out),
        AstStructItem::Comment(_) => {}
    }
}

fn collect_value_tokens(
    value: &AstValue,
    unit: UnitId,
    src: &str,
    ir: &ground_compile::ir::IrRes,
    scope: ground_compile::ir::ScopeId,
    out: &mut Vec<Token>,
) {
    match value {
        AstValue::Str(_) => {}
        AstValue::Ref(r) => collect_ref_tokens(r, RefContext::Value, unit, src, ir, scope, out),
        AstValue::List(items) => {
            for item in items {
                collect_value_tokens(&item.inner, unit, src, ir, scope, out);
            }
        }
        AstValue::Tuple(items) => {
            for item in items {
                collect_value_tokens(&item.inner, unit, src, ir, scope, out);
            }
        }
        AstValue::Struct { type_hint, fields } => {
            if let Some(hint) = type_hint {
                collect_ref_tokens(&hint.inner, RefContext::Type, unit, src, ir, scope, out);
            }
            for field in fields {
                match &field.inner {
                    AstField::Named { name, value, .. } => {
                        push_name_token(name, unit, src, TOK_PROPERTY, out);
                        collect_value_tokens(&value.inner, unit, src, ir, scope, out);
                    }
                    AstField::Anon(value) => {
                        collect_value_tokens(&value.inner, unit, src, ir, scope, out)
                    }
                    AstField::Comment(_) => {}
                }
            }
        }
    }
}

fn collect_ref_tokens(
    r: &AstRef,
    ctx: RefContext,
    unit: UnitId,
    src: &str,
    ir: &ground_compile::ir::IrRes,
    scope: ground_compile::ir::ScopeId,
    out: &mut Vec<Token>,
) {
    let mut plain_parts = vec![];
    for seg in &r.segments {
        match &seg.inner.value {
            AstRefSegVal::Plain(s) => plain_parts.push(Some(s.as_str())),
            AstRefSegVal::Group(inner, trailing) => {
                collect_ref_tokens(inner, ctx, unit, src, ir, scope, out);
                if let Some(trailing) = trailing {
                    plain_parts.push(Some(trailing.as_str()));
                } else {
                    plain_parts.push(None);
                }
            }
        }
    }

    let mut pack_prefix = 0usize;
    let mut prefix_parts = vec![];
    for part in &plain_parts {
        let Some(part) = *part else {
            break;
        };
        if matches!(part, "pack" | "def") {
            prefix_parts.push(part);
            pack_prefix += 1;
            continue;
        }
        prefix_parts.push(part);
        if lookup_pack_path(ir, scope, &prefix_parts).is_some() {
            pack_prefix = prefix_parts.len();
            continue;
        }
        prefix_parts.pop();
        break;
    }

    let remaining_plain: Vec<&str> = plain_parts
        .iter()
        .skip(pack_prefix)
        .filter_map(|p| *p)
        .collect();
    let typed_value_head_is_shape = matches!(ctx, RefContext::Value)
        && remaining_plain.len() >= 2
        && lookup_shape_visible(ir, scope, remaining_plain[0]).is_some();

    let last_plain_idx = plain_parts
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, p)| p.is_some().then_some(i));
    for (idx, seg) in r.segments.iter().enumerate() {
        if seg.loc.unit != unit {
            continue;
        }
        let token_type = match &seg.inner.value {
            AstRefSegVal::Group(_, _) => Some(TOK_VARIABLE),
            AstRefSegVal::Plain(s) => {
                if matches!(s.as_str(), "pack" | "def") {
                    Some(TOK_KEYWORD)
                } else if s == "*" {
                    Some(TOK_KEYWORD)
                } else if idx < pack_prefix {
                    Some(TOK_NAMESPACE)
                } else {
                    let remaining_idx = idx.saturating_sub(pack_prefix);
                    match ctx {
                        RefContext::Use => Some(use_token_type(
                            &plain_parts,
                            idx,
                            s,
                            ir,
                            scope,
                            last_plain_idx,
                        )),
                        RefContext::Mapper => Some(if idx == last_plain_idx.unwrap_or(idx) {
                            TOK_FUNCTION
                        } else {
                            TOK_NAMESPACE
                        }),
                        RefContext::Type => Some(TOK_TYPE),
                        RefContext::Value => {
                            if typed_value_head_is_shape && remaining_idx == 0 {
                                Some(TOK_TYPE)
                            } else if typed_value_head_is_shape
                                && idx == last_plain_idx.unwrap_or(idx)
                            {
                                Some(TOK_VARIABLE)
                            } else if remaining_idx == 0 && last_plain_idx == Some(idx) {
                                if lookup_def_visible(ir, scope, s).is_some() {
                                    Some(TOK_VARIABLE)
                                } else {
                                    Some(TOK_ENUM_MEMBER)
                                }
                            } else if idx == last_plain_idx.unwrap_or(idx) {
                                Some(TOK_ENUM_MEMBER)
                            } else {
                                Some(TOK_VARIABLE)
                            }
                        }
                    }
                }
            }
        };
        if let Some(token_type) = token_type {
            let (line, start) = offset_to_line_col(src, seg.loc.start as usize);
            let (_, end_col) = offset_to_line_col(src, seg.loc.end as usize);
            if end_col > start {
                out.push(Token {
                    line: line as u32,
                    start: start as u32,
                    len: (end_col - start) as u32,
                    token_type,
                });
            }
        }
    }
}

fn encode_tokens(_src: &str, mut tokens: Vec<Token>) -> Vec<u32> {
    tokens.sort_by_key(|t| (t.line, t.start, t.len, t.token_type));
    tokens.dedup_by_key(|t| (t.line, t.start, t.len));

    let mut data = vec![];
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for token in tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start - prev_start
        } else {
            token.start
        };
        data.extend([delta_line, delta_start, token.len, token.token_type, 0]);
        prev_line = token.line;
        prev_start = token.start;
    }
    data
}

fn use_token_type(
    plain_parts: &[Option<&str>],
    idx: usize,
    seg: &str,
    ir: &ground_compile::ir::IrRes,
    scope: ground_compile::ir::ScopeId,
    last_plain_idx: Option<usize>,
) -> u32 {
    if matches!(seg, "pack" | "def" | "*") {
        return TOK_KEYWORD;
    }

    let plain: Vec<&str> = plain_parts.iter().filter_map(|p| *p).collect();
    let mut cursor = 0usize;
    if plain.first() == Some(&"pack") {
        cursor += 1;
    }

    let def_kw_at = plain.iter().position(|p| *p == "def");
    let target_end = def_kw_at.unwrap_or(plain.len());
    let path_parts = &plain[cursor..target_end];

    let mut pack_prefix_len = 0usize;
    for i in 1..=path_parts.len() {
        if lookup_pack_path(ir, scope, &path_parts[..i]).is_some() {
            pack_prefix_len = i;
        } else {
            break;
        }
    }

    let namespace_count = cursor + pack_prefix_len;
    let plain_idx = plain_parts[..=idx]
        .iter()
        .filter(|p| p.is_some())
        .count()
        .saturating_sub(1);

    if plain_idx < namespace_count {
        return TOK_NAMESPACE;
    }

    if let Some(def_at) = def_kw_at {
        if plain_idx == def_at {
            return TOK_KEYWORD;
        }
        if plain_idx > def_at {
            return TOK_VARIABLE;
        }
    }

    if Some(idx) == last_plain_idx {
        if plain_idx >= namespace_count && namespace_count < path_parts.len() + cursor {
            if lookup_shape_visible(ir, scope, seg).is_some() {
                TOK_TYPE
            } else if lookup_def_visible(ir, scope, seg).is_some() {
                TOK_VARIABLE
            } else {
                TOK_NAMESPACE
            }
        } else {
            TOK_NAMESPACE
        }
    } else {
        TOK_NAMESPACE
    }
}

fn push_name_token(
    name: &AstNode<String>,
    unit: UnitId,
    src: &str,
    token_type: u32,
    out: &mut Vec<Token>,
) {
    if name.loc.unit != unit {
        return;
    }
    let (line, start) = offset_to_line_col(src, name.loc.start as usize);
    let (_, end_col) = offset_to_line_col(src, name.loc.end as usize);
    if end_col > start {
        out.push(Token {
            line: line as u32,
            start: start as u32,
            len: (end_col - start) as u32,
            token_type,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ground_compile::ast::{ParseReq, ParseUnit};
    use ground_compile::parse::parse;
    use ground_compile::resolve::resolve;

    #[test]
    fn semantic_tokens_cover_optional_list_inner_type() {
        let src = "pack std:aws:tf\naws_tag = { key = string  value = string }\nsvc = { tags = ([ aws_tag ]) }";
        let parse_res = parse(ParseReq {
            units: vec![ParseUnit {
                name: "test".into(),
                path: vec![],
                declared_pack: None,
                src: src.into(),
                ts_src: None,
            }],
        });
        let ir = resolve(parse_res.clone());
        let unit = UnitId(0);
        let scope = ground_compile::ir::ScopeId(parse_res.units[0].scope_id.0);
        let mut tokens = vec![];
        collect_scope_ref_tokens(&parse_res.scopes, 0, unit, src, &ir, scope, &mut tokens);

        let needle = "aws_tag";
        let offset = src.rfind(needle).expect("aws_tag occurrence missing");
        let (line, start) = offset_to_line_col(src, offset);
        assert!(tokens.iter().any(|t| {
            t.line == line as u32
                && t.start == start as u32
                && t.len == needle.len() as u32
                && t.token_type == TOK_TYPE
        }));
    }

    #[test]
    fn semantic_tokens_cover_aligned_optional_list_inner_type() {
        let src = "pack std:aws:tf\n\naws_tag = {\n  key   = string\n  value = string\n}\n\naws_vpc = {\n  cidr_block = string\n  tags       = ([ aws_tag ])\n}";
        let parse_res = parse(ParseReq {
            units: vec![ParseUnit {
                name: "test".into(),
                path: vec![],
                declared_pack: None,
                src: src.into(),
                ts_src: None,
            }],
        });
        let ir = resolve(parse_res.clone());
        let unit = UnitId(0);
        let scope = ground_compile::ir::ScopeId(parse_res.units[0].scope_id.0);
        let mut tokens = vec![];
        collect_scope_ref_tokens(&parse_res.scopes, 0, unit, src, &ir, scope, &mut tokens);

        let needle = "aws_tag";
        let offset = src.rfind(needle).expect("aws_tag occurrence missing");
        let (line, start) = offset_to_line_col(src, offset);
        assert!(tokens.iter().any(|t| {
            t.line == line as u32
                && t.start == start as u32
                && t.len == needle.len() as u32
                && t.token_type == TOK_TYPE
        }));
    }
}
