use std::collections::HashMap;
use ground_parse::*;
use ground_core::*;

const STDLIB: &str = include_str!("ground_stdlib.grd");

// ---------------------------------------------------------------------------
// Symbol table types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum PrimitiveKind {
    Integer,
    Float,
    Boolean,
    String,
}

#[derive(Debug, Clone)]
pub(crate) enum TypeDef {
    Primitive,
    Enum(Vec<std::string::String>),
    Composite(Vec<MemberDef>),
}

#[derive(Debug, Clone)]
pub(crate) struct MemberDef {
    pub link_name:   std::string::String,
    pub required:    bool,
    pub default:     Option<DefaultValue>,
    pub link_type:   LinkDef,
    pub is_type_ref: bool,  // true if declared as `type T` in composite body, not a link
}

#[derive(Debug, Clone)]
pub(crate) enum DefaultValue {
    Composite(Vec<(std::string::String, std::string::String)>),
    Scalar(std::string::String),
}

#[derive(Debug, Clone)]
pub(crate) enum LinkDef {
    Primitive(PrimitiveKind),
    Ref,
    Enum(std::string::String),          // reference to enum type by name
    TypeRef(std::string::String),       // reference to composite type by name
    TypedPath(Vec<PathSegment>),        // e.g. type:region:type:zone
    List(Vec<ShapeDef>),
}

#[derive(Debug, Clone)]
pub(crate) struct PathSegment {
    pub kind: SegmentKind,
    pub name: std::string::String,
}

#[derive(Debug, Clone)]
pub(crate) enum SegmentKind {
    Type,
    Link,
}

#[derive(Debug, Clone)]
pub(crate) struct ShapeDef {
    pub type_name:        std::string::String,
    pub optional_segment: Option<std::string::String>,
}

// ---------------------------------------------------------------------------
// Symbol table
// ---------------------------------------------------------------------------

struct SymbolTable {
    types: HashMap<std::string::String, TypeDef>,
    links: HashMap<std::string::String, LinkDef>,
}

impl SymbolTable {
    fn new() -> Self {
        SymbolTable {
            types: HashMap::new(),
            links: HashMap::new(),
        }
    }

    fn add_type(&mut self, name: std::string::String, def: TypeDef) {
        self.types.insert(name, def);
    }

    fn add_link(&mut self, name: std::string::String, def: LinkDef) {
        self.links.insert(name, def);
    }
}

// ---------------------------------------------------------------------------
// Parse a raw link type expression string into a LinkDef
// ---------------------------------------------------------------------------

fn parse_link_type_expr(
    expr: &str,
    symbols: &SymbolTable,
    path: &str,
    line: usize,
    col: usize,
) -> Result<LinkDef, ParseError> {
    let expr = expr.trim();

    // List form: [ ... ]
    if expr.starts_with('[') && expr.ends_with(']') {
        let inner = &expr[1..expr.len() - 1];
        let shapes = parse_list_shape(inner);
        return Ok(LinkDef::List(shapes));
    }

    // typed path: contains "type:" or multiple colon-separated segments
    // e.g. type:scaling, type:region:type:zone, engine, integer, reference
    if expr.starts_with("type:") {
        // Could be type:name or type:name:type:name
        let parts: Vec<&str> = expr.split(':').collect();
        if parts.len() == 2 && parts[0] == "type" {
            // type:scaling → TypeRef
            return Ok(LinkDef::TypeRef(parts[1].to_string()));
        }
        // multiple segments → TypedPath
        let mut segments = Vec::new();
        let mut i = 0;
        while i < parts.len() {
            let kind_str = parts[i];
            let name_str = if i + 1 < parts.len() { parts[i + 1] } else { "" };
            let kind = match kind_str {
                "type" => SegmentKind::Type,
                "link" => SegmentKind::Link,
                _ => {
                    return Err(ParseError {
                        path: path.to_string(),
                        line,
                        col,
                        message: format!("unknown path segment kind: {}", kind_str),
                    });
                }
            };
            segments.push(PathSegment { kind, name: name_str.to_string() });
            i += 2;
        }
        return Ok(LinkDef::TypedPath(segments));
    }

    // primitive kinds
    match expr {
        "integer"   => return Ok(LinkDef::Primitive(PrimitiveKind::Integer)),
        "float"     => return Ok(LinkDef::Primitive(PrimitiveKind::Float)),
        "boolean"   => return Ok(LinkDef::Primitive(PrimitiveKind::Boolean)),
        "string"    => return Ok(LinkDef::Primitive(PrimitiveKind::String)),
        "reference" => return Ok(LinkDef::Ref),
        _ => {}
    }

    // Check if it's a known enum type name
    if let Some(TypeDef::Enum(_)) = symbols.types.get(expr) {
        return Ok(LinkDef::Enum(expr.to_string()));
    }

    // Check if it's a known composite type name
    if let Some(TypeDef::Composite(_)) = symbols.types.get(expr) {
        return Ok(LinkDef::TypeRef(expr.to_string()));
    }

    // Fallback: treat as enum reference (will be validated at resolve time)
    Ok(LinkDef::Enum(expr.to_string()))
}

/// Parse a list-shape inner string like " service:(port)?  | database "
/// into a Vec<ShapeDef>
fn parse_list_shape(inner: &str) -> Vec<ShapeDef> {
    let mut shapes = Vec::new();
    for part in inner.split('|') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // part could be "service:(port)?" or "database"
        if let Some(colon_pos) = part.find(':') {
            let type_name = part[..colon_pos].trim().to_string();
            let rest = part[colon_pos + 1..].trim();
            // rest might be "(port)?"
            let optional_segment = if rest.starts_with('(') && rest.ends_with(')') {
                Some(rest[1..rest.len() - 1].to_string())
            } else if rest.ends_with(")?") && rest.starts_with('(') {
                Some(rest[1..rest.len() - 2].to_string())
            } else if !rest.is_empty() {
                Some(rest.to_string())
            } else {
                None
            };
            shapes.push(ShapeDef { type_name, optional_segment });
        } else {
            shapes.push(ShapeDef { type_name: part.to_string(), optional_segment: None });
        }
    }
    shapes
}

// ---------------------------------------------------------------------------
// Build symbol table from AST items
// ---------------------------------------------------------------------------

fn build_symbol_table(
    items: &[AstItem],
    symbols: &mut SymbolTable,
    errors: &mut Vec<ParseError>,
    path: &str,
) {
    for item in items {
        match item {
            AstItem::TypeDef(td) => {
                let def = match &td.body {
                    AstTypeBody::Primitive(_) => TypeDef::Primitive,
                    AstTypeBody::Enum(variants) => TypeDef::Enum(variants.clone()),
                    AstTypeBody::Composite(members) => {
                        let mut member_defs = Vec::new();
                        for member in members {
                            match member {
                                AstCompositeMember::LinkRef { link_name, line, col } => {
                                    member_defs.push(MemberDef {
                                        link_name: link_name.clone(),
                                        required:     true,
                                        default:      None,
                                        link_type:    LinkDef::Ref,
                                        is_type_ref:  false,
                                    });
                                    let _ = (line, col);
                                }
                                AstCompositeMember::LinkInline { link_name, type_expr, line, col } => {
                                    member_defs.push(MemberDef {
                                        link_name:    link_name.clone(),
                                        required:     false,
                                        default:      None,
                                        link_type:    LinkDef::Ref,
                                        is_type_ref:  false,
                                    });
                                    let _ = (type_expr, line, col);
                                }
                                AstCompositeMember::LinkDefault { link_name, default, line, col } => {
                                    let dv = match default {
                                        AstDefaultVal::Block(entries) => {
                                            DefaultValue::Composite(entries.clone())
                                        }
                                        AstDefaultVal::Single(s) => DefaultValue::Scalar(s.clone()),
                                    };
                                    member_defs.push(MemberDef {
                                        link_name:    link_name.clone(),
                                        required:     false,
                                        default:      Some(dv),
                                        link_type:    LinkDef::Ref,
                                        is_type_ref:  false,
                                    });
                                    let _ = (line, col);
                                }
                                AstCompositeMember::TypeRef { type_name, .. } => {
                                    member_defs.push(MemberDef {
                                        link_name:    type_name.clone(),
                                        required:     false,
                                        default:      None,
                                        link_type:    LinkDef::Ref,
                                        is_type_ref:  true,
                                    });
                                }
                                AstCompositeMember::TypeInline { type_name, .. } => {
                                    member_defs.push(MemberDef {
                                        link_name:    type_name.clone(),
                                        required:     false,
                                        default:      None,
                                        link_type:    LinkDef::Ref,
                                        is_type_ref:  true,
                                    });
                                }
                            }
                        }
                        TypeDef::Composite(member_defs)
                    }
                };
                symbols.add_type(td.name.clone(), def);
            }
            AstItem::LinkDef(ld) => {
                match parse_link_type_expr(&ld.type_expr, symbols, path, ld.line, ld.col) {
                    Ok(def) => symbols.add_link(ld.name.clone(), def),
                    Err(e) => errors.push(e),
                }
            }
            _ => {}
        }
    }

    // Second pass: resolve placeholder link types in composite type members
    // We need to do this after all types and links are registered
    let type_names: Vec<std::string::String> = symbols.types.keys().cloned().collect();
    for type_name in type_names {
        let type_def = symbols.types.get(&type_name).unwrap().clone();
        if let TypeDef::Composite(_) = &type_def {
            // Re-build the members with proper link types
        }
    }
}

/// Second pass: fill in proper link types for composite type members
fn resolve_composite_link_types(
    items: &[AstItem],
    symbols: &mut SymbolTable,
    errors: &mut Vec<ParseError>,
    path: &str,
) {
    for item in items {
        if let AstItem::TypeDef(td) = item {
            if let AstTypeBody::Composite(members) = &td.body {
                let mut new_members = Vec::new();
                for member in members {
                    match member {
                        AstCompositeMember::LinkRef { link_name, line, col } => {
                            // Look up in links table
                            let link_type = if let Some(ld) = symbols.links.get(link_name) {
                                ld.clone()
                            } else {
                                // might be an enum type reference
                                if let Some(TypeDef::Enum(_)) = symbols.types.get(link_name.as_str()) {
                                    LinkDef::Enum(link_name.clone())
                                } else {
                                    errors.push(ParseError {
                                        path: path.to_string(),
                                        line: *line,
                                        col: *col,
                                        message: format!("unknown bare link: {}", link_name),
                                    });
                                    LinkDef::Ref
                                }
                            };
                            new_members.push(MemberDef {
                                link_name:   link_name.clone(),
                                required:    true,
                                default:     None,
                                link_type,
                                is_type_ref: false,
                            });
                        }
                        AstCompositeMember::LinkInline { link_name, type_expr, line, col } => {
                            let link_type = match parse_link_type_expr(type_expr, symbols, path, *line, *col) {
                                Ok(lt) => lt,
                                Err(e) => { errors.push(e); LinkDef::Ref }
                            };
                            new_members.push(MemberDef {
                                link_name:   link_name.clone(),
                                required:    false,
                                default:     None,
                                link_type,
                                is_type_ref: false,
                            });
                        }
                        AstCompositeMember::LinkDefault { link_name, default, line, col } => {
                            let link_type = if let Some(ld) = symbols.links.get(link_name.as_str()) {
                                ld.clone()
                            } else {
                                LinkDef::Ref
                            };
                            let dv = match default {
                                AstDefaultVal::Block(entries) => DefaultValue::Composite(entries.clone()),
                                AstDefaultVal::Single(s) => DefaultValue::Scalar(s.clone()),
                            };
                            let _ = (line, col);
                            new_members.push(MemberDef {
                                link_name:   link_name.clone(),
                                required:    false,
                                default:     Some(dv),
                                link_type,
                                is_type_ref: false,
                            });
                        }
                        AstCompositeMember::TypeRef { type_name, line, col } => {
                            if !symbols.types.contains_key(type_name.as_str()) {
                                errors.push(ParseError {
                                    path: path.to_string(),
                                    line: *line,
                                    col:  *col,
                                    message: format!("unknown type '{}' in type body", type_name),
                                });
                            }
                            new_members.push(MemberDef {
                                link_name:   type_name.clone(),
                                required:    false,
                                default:     None,
                                link_type:   LinkDef::TypeRef(type_name.clone()),
                                is_type_ref: true,
                            });
                        }
                        AstCompositeMember::TypeInline { line, col, .. } => {
                            errors.push(ParseError {
                                path: path.to_string(),
                                line: *line,
                                col:  *col,
                                message: "inline type definitions inside type bodies are not supported".into(),
                            });
                        }
                    }
                }
                symbols.types.insert(td.name.clone(), TypeDef::Composite(new_members));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Resolve field values
// ---------------------------------------------------------------------------

fn resolve_field_value(
    link_name: &str,
    ast_value: &AstFieldValue,
    link_def: &LinkDef,
    symbols: &SymbolTable,
    path: &str,
    line: usize,
    col: usize,
    errors: &mut Vec<ParseError>,
) -> ResolvedValue {
    match link_def {
        LinkDef::Ref => {
            match ast_value {
                AstFieldValue::Single(tok) => ResolvedValue::Scalar(ScalarValue::Ref(tok.clone())),
                _ => {
                    errors.push(ParseError {
                        path: path.to_string(), line, col,
                        message: format!("field '{}': expected a reference value", link_name),
                    });
                    ResolvedValue::Scalar(ScalarValue::Ref(std::string::String::new()))
                }
            }
        }

        LinkDef::Primitive(kind) => {
            match kind {
                PrimitiveKind::Integer => {
                    match ast_value {
                        AstFieldValue::Single(tok) => {
                            match tok.parse::<i64>() {
                                Ok(n)  => ResolvedValue::Scalar(ScalarValue::Int(n)),
                                Err(_) => {
                                    errors.push(ParseError {
                                        path: path.to_string(), line, col,
                                        message: format!("field '{}': expected integer, got '{}'", link_name, tok),
                                    });
                                    ResolvedValue::Scalar(ScalarValue::Int(0))
                                }
                            }
                        }
                        _ => {
                            errors.push(ParseError {
                                path: path.to_string(), line, col,
                                message: format!("field '{}': expected integer value", link_name),
                            });
                            ResolvedValue::Scalar(ScalarValue::Int(0))
                        }
                    }
                }
                PrimitiveKind::Float => {
                    match ast_value {
                        AstFieldValue::Single(tok) => ResolvedValue::Scalar(ScalarValue::Str(tok.clone())),
                        _ => ResolvedValue::Scalar(ScalarValue::Str(std::string::String::new())),
                    }
                }
                PrimitiveKind::Boolean => {
                    match ast_value {
                        AstFieldValue::Single(tok) => {
                            let b = tok == "true";
                            ResolvedValue::Scalar(ScalarValue::Bool(b))
                        }
                        _ => ResolvedValue::Scalar(ScalarValue::Bool(false)),
                    }
                }
                PrimitiveKind::String => {
                    match ast_value {
                        AstFieldValue::Single(tok) => ResolvedValue::Scalar(ScalarValue::Str(tok.clone())),
                        _ => ResolvedValue::Scalar(ScalarValue::Str(std::string::String::new())),
                    }
                }

            }
        }

        LinkDef::Enum(type_name) => {
            match ast_value {
                AstFieldValue::Single(tok) => {
                    // Validate the token is a known variant of the enum type
                    if let Some(TypeDef::Enum(variants)) = symbols.types.get(type_name.as_str()) {
                        if !variants.contains(tok) {
                            errors.push(ParseError {
                                path: path.to_string(), line, col,
                                message: format!(
                                    "field '{}': '{}' is not a valid variant of '{}' (expected one of: {})",
                                    link_name, tok, type_name,
                                    variants.join(", ")
                                ),
                            });
                        }
                    }
                    ResolvedValue::Scalar(ScalarValue::Enum(tok.clone()))
                }
                _ => {
                    errors.push(ParseError {
                        path: path.to_string(), line, col,
                        message: format!("field '{}': expected enum value", link_name),
                    });
                    ResolvedValue::Scalar(ScalarValue::Enum(std::string::String::new()))
                }
            }
        }

        LinkDef::TypeRef(type_name) => {
            match ast_value {
                AstFieldValue::Block(sub_fields) => {
                    // Look up the composite type definition
                    let member_defs = if let Some(TypeDef::Composite(members)) = symbols.types.get(type_name.as_str()) {
                        members.clone()
                    } else {
                        vec![]
                    };
                    let resolved = resolve_fields(sub_fields, &member_defs, symbols, path, errors);
                    ResolvedValue::Composite(resolved)
                }
                AstFieldValue::Single(tok) => {
                    // Could be an instance reference name
                    ResolvedValue::Scalar(ScalarValue::InstanceRef {
                        type_name: type_name.clone(),
                        name:      tok.clone(),
                    })
                }
                _ => {
                    errors.push(ParseError {
                        path: path.to_string(), line, col,
                        message: format!("field '{}': expected block or reference for type '{}'", link_name, type_name),
                    });
                    ResolvedValue::Composite(vec![])
                }
            }
        }

        LinkDef::TypedPath(segments) => {
            // Each list entry has N segments corresponding to the path segments
            // e.g. type:region:type:zone → each entry has [region_enum, zone_enum]
            // The token for each list entry is like "us-east:1" (colon-separated)
            let entries = match ast_value {
                AstFieldValue::List(tokens) => {
                    tokens.iter().map(|tok| {
                        resolve_typed_path_entry(tok, segments, symbols, path, line, col, errors)
                    }).collect()
                }
                AstFieldValue::Single(tok) => {
                    vec![resolve_typed_path_entry(tok, segments, symbols, path, line, col, errors)]
                }
                _ => vec![],
            };
            ResolvedValue::List(entries)
        }

        LinkDef::List(shapes) => {
            // Polymorphic list of instances
            // Each token can be "name:name" (type:instance) or just "name"
            let entries = match ast_value {
                AstFieldValue::List(tokens) => {
                    tokens.iter().map(|tok| {
                        resolve_list_entry(tok, shapes, path, line, col, errors)
                    }).collect()
                }
                AstFieldValue::Single(tok) => {
                    vec![resolve_list_entry(tok, shapes, path, line, col, errors)]
                }
                _ => vec![],
            };
            ResolvedValue::List(entries)
        }
    }
}

fn resolve_typed_path_entry(
    tok: &str,
    segments: &[PathSegment],
    symbols: &SymbolTable,
    path: &str,
    line: usize,
    col: usize,
    errors: &mut Vec<ParseError>,
) -> ListEntry {
    // Split the token by ':'
    let parts: Vec<&str> = tok.split(':').collect();
    let mut scalar_segs = Vec::new();

    for (i, seg) in segments.iter().enumerate() {
        let raw = parts.get(i).copied().unwrap_or("");
        match &seg.kind {
            SegmentKind::Type => {
                // Validate against enum type
                if let Some(TypeDef::Enum(variants)) = symbols.types.get(&seg.name) {
                    if !variants.contains(&raw.to_string()) {
                        errors.push(ParseError {
                            path: path.to_string(), line, col,
                            message: format!(
                                "typed path: '{}' is not a valid variant of '{}' (expected one of: {})",
                                raw, seg.name, variants.join(", ")
                            ),
                        });
                    }
                }
                scalar_segs.push(ScalarValue::Enum(raw.to_string()));
            }
            SegmentKind::Link => {
                scalar_segs.push(ScalarValue::Str(raw.to_string()));
            }
        }
    }

    ListEntry { segments: scalar_segs }
}

fn resolve_list_entry(
    tok: &str,
    shapes: &[ShapeDef],
    _path: &str,
    _line: usize,
    _col: usize,
    _errors: &mut Vec<ParseError>,
) -> ListEntry {
    // Split the token by ':' to get parts
    let parts: Vec<&str> = tok.splitn(2, ':').collect();
    let part_count = parts.len(); // 1 or 2

    // Try each shape in order; first match wins
    for shape in shapes {
        let matches = if shape.optional_segment.is_some() {
            // Shape allows 1 or 2 colon-separated segments
            part_count == 1 || part_count == 2
        } else {
            // Shape requires exactly 1 segment (no colon)
            part_count == 1
        };

        if matches {
            let name = parts[0].to_string();
            let mut segments = vec![ScalarValue::InstanceRef {
                type_name: shape.type_name.clone(),
                name,
            }];
            // If there's a second part and the shape allows it, add it as Str
            if part_count == 2 && shape.optional_segment.is_some() {
                segments.push(ScalarValue::Str(parts[1].to_string()));
            }
            return ListEntry { segments };
        }
    }

    // Fallback: no shape matched; store as bare InstanceRef with empty type
    let name = parts[0].to_string();
    let mut segments = vec![ScalarValue::InstanceRef {
        type_name: std::string::String::new(),
        name,
    }];
    if part_count == 2 {
        segments.push(ScalarValue::Str(parts[1].to_string()));
    }
    ListEntry { segments }
}

// ---------------------------------------------------------------------------
// Resolve a set of AST fields against a member definition list
// ---------------------------------------------------------------------------

fn resolve_fields(
    ast_fields: &[AstField],
    member_defs: &[MemberDef],
    symbols: &SymbolTable,
    path: &str,
    errors: &mut Vec<ParseError>,
) -> Vec<ResolvedField> {
    let mut resolved = Vec::new();
    let mut provided: HashMap<std::string::String, bool> = HashMap::new();

    for ast_field in ast_fields {
        provided.insert(ast_field.link_name.clone(), true);

        // Only link-declared members are settable via field entry syntax
        let link_def = match member_defs.iter().find(|m| m.link_name == ast_field.link_name) {
            Some(md) if md.is_type_ref => {
                errors.push(ParseError {
                    path: path.to_string(),
                    line: ast_field.line,
                    col:  ast_field.col,
                    message: format!("'{}' is a type member, not a link — cannot be set as a field", ast_field.link_name),
                });
                continue;
            }
            Some(md) => md.link_type.clone(),
            None if member_defs.is_empty() => LinkDef::Ref,
            None => {
                errors.push(ParseError {
                    path: path.to_string(),
                    line: ast_field.line,
                    col:  ast_field.col,
                    message: format!("unknown field '{}'", ast_field.link_name),
                });
                continue;
            }
        };

        let value = resolve_field_value(
            &ast_field.link_name,
            &ast_field.value,
            &link_def,
            symbols,
            path,
            ast_field.line,
            ast_field.col,
            errors,
        );
        resolved.push(ResolvedField { link_name: ast_field.link_name.clone(), value });
    }

    // Apply defaults for unset optional fields
    for md in member_defs {
        if provided.contains_key(&md.link_name) {
            continue;
        }
        if md.required {
            // Required field missing — emit an error only at the top level (not sub-blocks)
            // We skip this here and do it at the instance level
            continue;
        }
        if let Some(default) = &md.default {
            let value = match default {
                DefaultValue::Scalar(s) => {
                    resolve_field_value(
                        &md.link_name,
                        &AstFieldValue::Single(s.clone()),
                        &md.link_type,
                        symbols,
                        path,
                        0,
                        0,
                        errors,
                    )
                }
                DefaultValue::Composite(entries) => {
                    let sub_fields: Vec<AstField> = entries.iter().map(|(k, v)| AstField {
                        link_name: k.clone(),
                        value:     AstFieldValue::Single(v.clone()),
                        line:      0,
                        col:       0,
                    }).collect();
                    // Get sub-member defs
                    let sub_members = if let LinkDef::TypeRef(tn) = &md.link_type {
                        if let Some(TypeDef::Composite(m)) = symbols.types.get(tn.as_str()) {
                            m.clone()
                        } else { vec![] }
                    } else { vec![] };
                    let sub_resolved = resolve_fields(&sub_fields, &sub_members, symbols, path, errors);
                    ResolvedValue::Composite(sub_resolved)
                }
            };
            resolved.push(ResolvedField { link_name: md.link_name.clone(), value });
        }
    }

    resolved
}

// ---------------------------------------------------------------------------
// Deploy field resolution
// ---------------------------------------------------------------------------

fn resolve_deploy_field(
    ast_field: &AstField,
    symbols: &SymbolTable,
    path: &str,
    errors: &mut Vec<ParseError>,
) -> ResolvedField {
    // For deploy fields, we look up in the global links table first
    // Special case: "stack" field → instance reference
    // Special case: "region" field → typed path list
    let link_def = if let Some(ld) = symbols.links.get(&ast_field.link_name) {
        ld.clone()
    } else {
        // unknown deploy field, store as ref
        LinkDef::Ref
    };

    let value = resolve_field_value(
        &ast_field.link_name,
        &ast_field.value,
        &link_def,
        symbols,
        path,
        ast_field.line,
        ast_field.col,
        errors,
    );
    ResolvedField { link_name: ast_field.link_name.clone(), value }
}

// ---------------------------------------------------------------------------
// Main resolve entry point
// ---------------------------------------------------------------------------

pub(crate) fn resolve_file(items: Vec<AstItem>) -> Result<Spec, Vec<ParseError>> {
    let mut errors = Vec::new();
    let mut symbols = SymbolTable::new();

    // 1. Parse and load stdlib
    let stdlib_items = ground_parse::parse_to_items("<stdlib>", STDLIB)
        .map_err(|es| es)?;

    build_symbol_table(&stdlib_items, &mut symbols, &mut errors, "<stdlib>");
    resolve_composite_link_types(&stdlib_items, &mut symbols, &mut errors, "<stdlib>");

    // 2. Add user items to symbol table
    build_symbol_table(&items, &mut symbols, &mut errors, "<user>");
    resolve_composite_link_types(&items, &mut symbols, &mut errors, "<user>");

    if !errors.is_empty() {
        return Err(errors);
    }

    // 3. Resolve instances and deploys
    let declared_stacks: std::collections::HashSet<std::string::String> = items.iter()
        .filter_map(|item| {
            if let AstItem::TypeDecl(inst) = item {
                if inst.type_name == "stack" { Some(inst.name.clone()) } else { None }
            } else { None }
        })
        .collect();

    let mut instances = Vec::new();
    let mut deploys   = Vec::new();

    for item in &items {
        match item {
            AstItem::TypeDecl(inst) => {
                // Look up the type
                let member_defs = if let Some(TypeDef::Composite(members)) = symbols.types.get(&inst.type_name) {
                    members.clone()
                } else {
                    errors.push(ParseError {
                        path: "<user>".to_string(),
                        line: inst.line,
                        col:  inst.col,
                        message: format!("unknown type '{}'", inst.type_name),
                    });
                    continue;
                };

                // Check required fields
                for md in &member_defs {
                    if md.required && !inst.fields.iter().any(|f| f.link_name == md.link_name) {
                        errors.push(ParseError {
                            path: "<user>".to_string(),
                            line: inst.line,
                            col:  inst.col,
                            message: format!(
                                "instance '{} {}': missing required field '{}'",
                                inst.type_name, inst.name, md.link_name
                            ),
                        });
                    }
                }

                let fields = resolve_fields(&inst.fields, &member_defs, &symbols, "<user>", &mut errors);
                instances.push(Instance {
                    type_name: inst.type_name.clone(),
                    name:      inst.name.clone(),
                    fields,
                });
            }

            AstItem::Deploy(dep) => {
                // dep.name is the stack reference from "deploy <stack> to <provider> as <alias>"
                if !declared_stacks.contains(dep.name.as_str()) {
                    errors.push(ParseError {
                        path: "<user>".to_string(),
                        line: dep.line,
                        col:  dep.col,
                        message: format!("deploy references unknown stack '{}'", dep.name),
                    });
                }
                let fields = dep.fields.iter()
                    .map(|f| resolve_deploy_field(f, &symbols, "<user>", &mut errors))
                    .collect();
                deploys.push(DeployInstance {
                    name:     dep.name.clone(),
                    provider: dep.provider.clone(),
                    alias:    dep.alias.clone(),
                    fields,
                });
            }

            _ => {}
        }
    }

    // 4. Validate TypeRef field values against declared instances
    // Build name → type_name map from resolved instances
    let instance_types: std::collections::HashMap<std::string::String, std::string::String> = instances.iter()
        .map(|i| (i.name.clone(), i.type_name.clone()))
        .collect();

    for item in &items {
        if let AstItem::TypeDecl(inst) = item {
            let member_defs = match symbols.types.get(&inst.type_name) {
                Some(TypeDef::Composite(members)) => members.clone(),
                _ => continue,
            };
            for field in &inst.fields {
                if let Some(md) = member_defs.iter().find(|m| m.link_name == field.link_name) {
                    if let LinkDef::TypeRef(ref_type) = &md.link_type {
                        if let AstFieldValue::Single(tok) = &field.value {
                            match instance_types.get(tok.as_str()) {
                                None => errors.push(ParseError {
                                    path: "<user>".to_string(),
                                    line: field.line,
                                    col:  field.col,
                                    message: format!("'{}' references unknown instance '{}'", field.link_name, tok),
                                }),
                                Some(actual) if actual != ref_type => errors.push(ParseError {
                                    path: "<user>".to_string(),
                                    line: field.line,
                                    col:  field.col,
                                    message: format!("'{}' expects a {} but '{}' is a {}", field.link_name, ref_type, tok, actual),
                                }),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(Spec { instances, deploys })
    }
}
