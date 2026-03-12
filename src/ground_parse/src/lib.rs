pub mod ast;

pub use ground_core::ParseError;
pub use ast::*;

use pest::iterators::Pair;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "src/ground.pest"]
struct GroundParser;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn parse_to_items(path: &str, content: &str) -> Result<Vec<AstItem>, Vec<ParseError>> {
    let pairs = <GroundParser as pest::Parser<Rule>>::parse(Rule::file, content)
        .map_err(|e| {
            let (line, col) = match e.line_col {
                pest::error::LineColLocation::Pos(lc)     => lc,
                pest::error::LineColLocation::Span(lc, _) => lc,
            };
            vec![ParseError {
                path:    path.to_string(),
                line,
                col,
                message: format!("parse error: {}", e),
            }]
        })?;

    let mut items  = Vec::new();
    let mut errors = Vec::new();

    // The outer rule is `file`; iterate its inner pairs
    for pair in pairs {
        if pair.as_rule() == Rule::file {
            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::type_decl    => match convert_type_decl(path, inner) {
                        Ok(td)     => items.push(AstItem::TypeDecl(td)),
                        Err(mut e) => errors.append(&mut e),
                    },
                    Rule::link_decl    => match convert_link_decl(path, inner) {
                        Ok(ld)     => items.push(AstItem::LinkDecl(ld)),
                        Err(mut e) => errors.append(&mut e),
                    },
                    Rule::instance_def => match convert_instance_def(path, inner) {
                        Ok(inst)   => items.push(AstItem::Instance(inst)),
                        Err(mut e) => errors.append(&mut e),
                    },
                    Rule::deploy_def   => match convert_deploy_def(path, inner) {
                        Ok(dep)    => items.push(AstItem::Deploy(dep)),
                        Err(mut e) => errors.append(&mut e),
                    },
                    Rule::EOI          => {}
                    r => errors.push(ParseError {
                        path: path.to_string(),
                        line: 1,
                        col:  1,
                        message: format!("unexpected top-level rule: {:?}", r),
                    }),
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(items)
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// CST → AST converters
// ---------------------------------------------------------------------------

fn pair_pos(pair: &Pair<Rule>) -> (usize, usize) {
    pair.as_span().start_pos().line_col()
}

fn convert_type_decl(path: &str, pair: Pair<Rule>) -> Result<AstTypeDecl, Vec<ParseError>> {
    let (line, col) = pair_pos(&pair);
    let mut inner   = pair.into_inner();

    let name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected type name".into() }]),
    };

    let type_body_pair = match inner.next() {
        Some(p) => p,
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected type body".into() }]),
    };

    let body = convert_type_body(path, type_body_pair)?;
    Ok(AstTypeDecl { name, body, line, col })
}

fn convert_type_body(path: &str, pair: Pair<Rule>) -> Result<AstTypeBody, Vec<ParseError>> {
    // pair is `type_body` which has one inner child: composite_body | enum_body | primitive_kind
    let inner = match pair.into_inner().next() {
        Some(p) => p,
        None    => return Err(vec![ParseError { path: path.to_string(), line: 1, col: 1, message: "empty type body".into() }]),
    };

    match inner.as_rule() {
        Rule::primitive_kind => Ok(AstTypeBody::Primitive(inner.as_str().to_string())),
        Rule::enum_body      => {
            let variants = inner.into_inner()
                .filter(|p| p.as_rule() == Rule::enum_variant)
                .map(|p| p.as_str().to_string())
                .collect();
            Ok(AstTypeBody::Enum(variants))
        }
        Rule::composite_body => {
            let members = convert_composite_body(path, inner)?;
            Ok(AstTypeBody::Composite(members))
        }
        r => Err(vec![ParseError {
            path: path.to_string(), line: 1, col: 1,
            message: format!("unexpected type body rule: {:?}", r),
        }]),
    }
}

fn convert_composite_body(path: &str, pair: Pair<Rule>) -> Result<Vec<AstCompositeMember>, Vec<ParseError>> {
    let mut members = Vec::new();
    let mut errors  = Vec::new();

    for member_pair in pair.into_inner() {
        if member_pair.as_rule() != Rule::composite_member {
            continue;
        }
        let (ml, mc) = pair_pos(&member_pair);
        let inner = match member_pair.into_inner().next() {
            Some(p) => p,
            None    => continue,
        };
        match inner.as_rule() {
            Rule::composite_bare => {
                let link_name = inner.into_inner().next()
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_default();
                members.push(AstCompositeMember::Bare { link_name, line: ml, col: mc });
            }
            Rule::composite_inline => {
                let mut ci = inner.into_inner();
                let link_name = ci.next().map(|p| p.as_str().to_string()).unwrap_or_default();
                let type_expr = ci.next().map(|p| convert_link_type_raw(p)).unwrap_or_default();
                members.push(AstCompositeMember::Inline { link_name, type_expr, line: ml, col: mc });
            }
            Rule::composite_default => {
                let mut cd = inner.into_inner();
                let link_name = cd.next().map(|p| p.as_str().to_string()).unwrap_or_default();
                let def_val_pair = cd.next();
                let default = if let Some(dvp) = def_val_pair {
                    convert_composite_default_val(path, dvp)?
                } else {
                    AstDefaultVal::Single(String::new())
                };
                members.push(AstCompositeMember::Default { link_name, default, line: ml, col: mc });
            }
            r => errors.push(ParseError {
                path: path.to_string(), line: ml, col: mc,
                message: format!("unexpected composite member rule: {:?}", r),
            }),
        }
    }

    if errors.is_empty() { Ok(members) } else { Err(errors) }
}

fn convert_composite_default_val(path: &str, pair: Pair<Rule>) -> Result<AstDefaultVal, Vec<ParseError>> {
    // pair is composite_default_val which has: comp_def_block | value_token
    let inner = match pair.into_inner().next() {
        Some(p) => p,
        None    => return Ok(AstDefaultVal::Single(String::new())),
    };
    match inner.as_rule() {
        Rule::comp_def_block => {
            let entries = inner.into_inner()
                .filter(|p| p.as_rule() == Rule::comp_def_entry)
                .map(|entry| {
                    let mut ei = entry.into_inner();
                    let k = ei.next().map(|p| p.as_str().to_string()).unwrap_or_default();
                    let v = ei.next().map(|p| p.as_str().to_string()).unwrap_or_default();
                    (k, v)
                })
                .collect();
            Ok(AstDefaultVal::Block(entries))
        }
        Rule::value_token => Ok(AstDefaultVal::Single(inner.as_str().to_string())),
        r => Err(vec![ParseError {
            path: path.to_string(), line: 1, col: 1,
            message: format!("unexpected default val rule: {:?}", r),
        }]),
    }
}

fn convert_link_type_raw(pair: Pair<Rule>) -> String {
    // pair is link_type_raw
    // It's either "[" ~ link_list_inner ~ "]" or link_shape
    let raw = pair.as_str().trim().to_string();
    raw
}

fn convert_link_decl(path: &str, pair: Pair<Rule>) -> Result<AstLinkDecl, Vec<ParseError>> {
    let (line, col) = pair_pos(&pair);
    let mut inner   = pair.into_inner();

    let name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected link name".into() }]),
    };

    let type_raw = match inner.next() {
        Some(p) => convert_link_type_raw(p),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected link type".into() }]),
    };

    Ok(AstLinkDecl { name, type_expr: type_raw, line, col })
}

fn convert_instance_def(path: &str, pair: Pair<Rule>) -> Result<AstInstance, Vec<ParseError>> {
    let (line, col) = pair_pos(&pair);
    let mut inner   = pair.into_inner();

    let type_name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected type name".into() }]),
    };
    let name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected instance name".into() }]),
    };

    let mut fields = Vec::new();
    let mut errors = Vec::new();
    for field_pair in inner {
        if field_pair.as_rule() == Rule::field_entry {
            match convert_field_entry(path, field_pair) {
                Ok(f)      => fields.push(f),
                Err(mut e) => errors.append(&mut e),
            }
        }
    }

    if errors.is_empty() {
        Ok(AstInstance { type_name, name, fields, line, col })
    } else {
        Err(errors)
    }
}

fn convert_deploy_def(path: &str, pair: Pair<Rule>) -> Result<AstDeploy, Vec<ParseError>> {
    let (line, col) = pair_pos(&pair);
    let mut inner   = pair.into_inner();

    // deploy NAME to PROVIDER as ALIAS { ... }
    let name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected deploy name".into() }]),
    };
    let provider = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected provider".into() }]),
    };
    let alias = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected alias".into() }]),
    };

    let mut fields = Vec::new();
    let mut errors = Vec::new();
    for field_pair in inner {
        if field_pair.as_rule() == Rule::field_entry {
            match convert_field_entry(path, field_pair) {
                Ok(f)      => fields.push(f),
                Err(mut e) => errors.append(&mut e),
            }
        }
    }

    if errors.is_empty() {
        Ok(AstDeploy { name, provider, alias, fields, line, col })
    } else {
        Err(errors)
    }
}

fn convert_field_entry(path: &str, pair: Pair<Rule>) -> Result<AstField, Vec<ParseError>> {
    let (line, col) = pair_pos(&pair);
    let mut inner   = pair.into_inner();

    let link_name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "expected field name".into() }]),
    };

    let val_pair = match inner.next() {
        Some(p) => p,
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: format!("expected value for field '{}'", link_name) }]),
    };

    // val_pair is field_value which has one child: block_value | list_value | single_value
    let val_inner = match val_pair.into_inner().next() {
        Some(p) => p,
        None    => return Err(vec![ParseError { path: path.to_string(), line, col, message: "empty field value".into() }]),
    };

    let value = match val_inner.as_rule() {
        Rule::single_value => {
            let tok = val_inner.as_str().trim().to_string();
            AstFieldValue::Single(tok)
        }
        Rule::list_value => {
            let entries = val_inner.into_inner()
                .filter(|p| p.as_rule() == Rule::list_entry)
                .map(|p| p.as_str().to_string())
                .collect();
            AstFieldValue::List(entries)
        }
        Rule::block_value => {
            let mut sub_fields = Vec::new();
            let mut errors     = Vec::new();
            for sub_pair in val_inner.into_inner() {
                if sub_pair.as_rule() == Rule::field_entry {
                    match convert_field_entry(path, sub_pair) {
                        Ok(f)      => sub_fields.push(f),
                        Err(mut e) => errors.append(&mut e),
                    }
                }
            }
            if !errors.is_empty() {
                return Err(errors);
            }
            AstFieldValue::Block(sub_fields)
        }
        r => return Err(vec![ParseError {
            path: path.to_string(), line, col,
            message: format!("unexpected field value rule: {:?}", r),
        }]),
    };

    Ok(AstField { link_name, value, line, col })
}
