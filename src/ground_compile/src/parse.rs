/// Recursive-descent parser ground language

use crate::ast::*;


struct Parser<'a> {
    src:    &'a str,
    pos:    usize,
    unit:   u32,
    errors: Vec<AstParseError>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, unit: u32) -> Self {
        Parser { src, pos: 0, unit, errors: Vec::new() }
    }

    // -- location helpers ------------------------------------------------

    fn node<T>(&self, start: usize, inner: T) -> AstNode<T> {
        AstNode::new(self.unit, start as u32, self.pos as u32, inner)
    }

    fn push_error(&mut self, start: usize, msg: impl Into<String>) {
        self.errors.push(AstParseError {
            message: msg.into(),
            loc: AstNodeLoc { unit: self.unit, start: start as u32, end: self.pos as u32 },
        });
    }

    // -- low-level navigation -------------------------------------------

    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn advance(&mut self, bytes: usize) {
        self.pos += bytes;
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.rest().starts_with(s) {
            self.advance(s.len());
            true
        } else {
            false
        }
    }

    /// True if we are at `kw` followed by a non-ident character (or end of input).
    fn at_keyword(&self, kw: &str) -> bool {
        let rest = self.rest();
        if !rest.starts_with(kw) { return false; }
        rest[kw.len()..].chars().next()
            .map_or(true, |c| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
    }

    // -- whitespace / comments -------------------------------------------

    fn skip_ws(&mut self) {
        loop {
            let before = self.pos;
            while self.pos < self.src.len() {
                let b = self.src.as_bytes()[self.pos];
                if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.rest().starts_with('#') {
                while self.pos < self.src.len() && self.src.as_bytes()[self.pos] != b'\n' {
                    self.pos += 1;
                }
            }
            if self.pos == before { break; }
        }
    }

    // -- lexemes ---------------------------------------------------------

    /// `[a-zA-Z][a-zA-Z0-9\-_]*`
    fn parse_ident(&mut self) -> Option<AstNode<String>> {
        let start = self.pos;
        let rest = self.rest();
        let first = rest.chars().next()?;
        if !first.is_ascii_alphabetic() { return None; }
        let mut len = 1usize;
        for c in rest[1..].chars() {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' { len += 1; } else { break; }
        }
        let value = rest[..len].to_string();
        self.advance(len);
        Some(self.node(start, value))
    }

    /// `'"' [^"]* '"'`
    fn parse_string_lit(&mut self) -> Option<AstNode<String>> {
        let start = self.pos;
        if !self.eat("\"") { return None; }
        let s_start = self.pos;
        while self.pos < self.src.len() && self.src.as_bytes()[self.pos] != b'"' {
            self.pos += 1;
        }
        let value = self.src[s_start..self.pos].to_string();
        self.eat("\"");
        Some(self.node(start, value))
    }

    // -- refs ------------------------------------------------------------

    /// `[a-zA-Z0-9\-_./]+`  — integers are valid atoms; distinction is deferred to resolve.
    fn parse_ref_atom(&mut self) -> Option<String> {
        let atom_start = self.pos;
        let rest = self.rest();
        let is_ref_char = |c: char| c.is_ascii_alphanumeric() || "-_./".contains(c);
        let first = rest.chars().next()?;
        if !is_ref_char(first) { return None; }
        let len = rest.chars().take_while(|&c| is_ref_char(c)).count();
        self.advance(len);
        Some(self.src[atom_start..self.pos].to_string())
    }

    /// `"(" ident ")"`
    fn parse_ref_opt_atom(&mut self) -> Option<String> {
        if !self.rest().starts_with('(') { return None; }
        let saved = self.pos;
        self.advance(1);
        let id = self.parse_ident();
        if id.is_none() || !self.eat(")") {
            self.pos = saved;
            return None;
        }
        Some(id.unwrap().inner)
    }

    fn parse_ref_seg(&mut self) -> Option<AstNode<AstRefSeg>> {
        let start = self.pos;
        if let Some(v) = self.parse_ref_opt_atom() {
            return Some(self.node(start, AstRefSeg { value: AstRefSegVal::Plain(v), is_opt: true }));
        }
        if let Some(v) = self.parse_ref_atom() {
            return Some(self.node(start, AstRefSeg { value: AstRefSegVal::Plain(v), is_opt: false }));
        }
        // Wildcard segment — only semantically valid as final segment of a `use` path.
        if self.rest().starts_with('*') {
            self.advance(1);
            return Some(self.node(start, AstRefSeg { value: AstRefSegVal::Plain("*".to_string()), is_opt: false }));
        }
        None
    }

    /// `ref-part ((":" | "") ref-part)*`
    /// ref-part = `"{" inner-ref "}"` trailing-atom? | ref-seg
    ///
    /// `{this:name}-sg` → [Seg("this"), Seg("name"), Seg("-sg")]
    fn parse_ref(&mut self) -> Option<AstNode<AstRef>> {
        let start = self.pos;
        let mut segments = Vec::new();

        if !self.collect_ref_part(&mut segments) { return None; }

        loop {
            // Adjacent brace group continues this ref without a colon.
            if self.rest().starts_with('{') {
                if !self.collect_ref_part(&mut segments) { break; }
                continue;
            }
            let saved = self.pos;
            if !self.eat(":") { break; }
            if !self.collect_ref_part(&mut segments) {
                self.pos = saved;
                break;
            }
        }

        Some(self.node(start, AstRef { segments }))
    }

    /// Append one ref part to `out`. Returns true if anything was appended.
    /// A brace group `{inner-ref}` produces a single Group segment, followed
    /// by an optional trailing plain atom as its own segment (`-sg`, `_v2`, …).
    fn collect_ref_part(&mut self, out: &mut Vec<AstNode<AstRefSeg>>) -> bool {
        if self.rest().starts_with('{') {
            let saved = self.pos;
            self.advance(1);
            if let Some(inner) = self.parse_ref() {
                if self.eat("}") {
                    let seg_start = saved;
                    let trailing  = self.parse_ref_atom();
                    out.push(self.node(seg_start, AstRefSeg {
                        value: AstRefSegVal::Group(inner.inner, trailing),
                        is_opt: false,
                    }));
                    return true;
                }
            }
            self.pos = saved;
            return false;
        }
        if let Some(seg) = self.parse_ref_seg() {
            out.push(seg);
            return true;
        }
        false
    }

    // -- type expressions (link bodies, list elements) -------------------

    /// `type-def | "[" type-expr "]" | ref ("|" ref)*`
    ///
    /// Always returns a pure type expression; a union of refs desugars to `Enum`.
    fn parse_type_expr(&mut self) -> Option<AstNode<AstTypeExpr>> {
        let start = self.pos;

        // `unit` is syntax sugar for the empty struct.
        if self.at_keyword("unit") {
            self.advance("unit".len());
            return Some(self.node(start, AstTypeExpr::Unit));
        }

        // List: "[" type-expr "]"
        if self.rest().starts_with('[') {
            self.advance(1);
            self.skip_ws();
            let inner = self.parse_type_expr()?;
            self.skip_ws();
            if !self.eat("]") {
                self.push_error(self.pos, "expected ']' after list type");
            }
            return Some(self.node(start, AstTypeExpr::List(Box::new(inner))));
        }

        // Struct body: "{" struct_items "}" — used for anonymous inline struct shapes in fields
        if self.rest().starts_with('{') {
            let items = self.parse_struct_body().unwrap_or_default();
            let expr = if items.is_empty() { AstTypeExpr::Unit } else { AstTypeExpr::Struct(items) };
            return Some(self.node(start, expr));
        }

        // Ref or sugar enum: ref ("|" ref)*
        let first = self.parse_ref()?;
        let mut refs = vec![first];
        loop {
            let saved = self.pos;
            self.skip_ws();
            if self.eat("|") {
                self.skip_ws();
                if let Some(r) = self.parse_ref() {
                    refs.push(r);
                    continue;
                }
            }
            self.pos = saved;
            break;
        }

        if refs.len() == 1 {
            let r = refs.remove(0);
            Some(self.node(start, AstTypeExpr::Ref(r.inner)))
        } else {
            Some(self.node(start, AstTypeExpr::Enum(refs)))
        }
    }

    fn parse_struct_item(&mut self) -> Option<AstNode<AstStructItem>> {
        let start = self.pos;

        // def keyword → nested named def inside struct body
        if self.at_keyword("def") {
            if let Some(td) = self.parse_top_def_with_keyword("def", false) {
                return Some(self.node(start, AstStructItem::Def(td)));
            }
        }

        // Anonymous field `= type_expr`
        if self.rest().starts_with('=') {
            let eq_pos = self.pos;
            self.advance(1); // consume `=`
            self.skip_ws();
            if let Some(ty) = self.parse_type_expr() {
                let fd = self.node(eq_pos, AstStructField {
                    name: None,
                    kind: AstStructFieldKind::Def,
                    body: AstStructFieldBody::Type(ty),
                });
                return Some(self.node(start, AstStructItem::Field(fd)));
            }
            self.pos = eq_pos;
            return None;
        }

        // Named field `ident = type_expr`
        let saved = self.pos;
        if let Some(name) = self.parse_ident() {
            self.skip_ws();
            if self.eat("=") {
                self.skip_ws();
                if let Some(ty) = self.parse_type_expr() {
                    let fd = self.node(saved, AstStructField {
                        name: Some(name),
                        kind: AstStructFieldKind::Def,
                        body: AstStructFieldBody::Type(ty),
                    });
                    return Some(self.node(start, AstStructItem::Field(fd)));
                }
            }
        }
        self.pos = saved;

        // Named field `ident : value`
        let saved = self.pos;
        if let Some(name) = self.parse_ident() {
            if self.eat(":") {
                let has_ws = self.peek().map_or(false, |c| matches!(c, ' ' | '\t' | '\n' | '\r'));
                if !has_ws {
                    self.pos = saved;
                } else {
                    self.skip_ws();

                    if let Some(value) = self.parse_inst_value() {
                        let fd = self.node(saved, AstStructField {
                            name: Some(name),
                            kind: AstStructFieldKind::Set,
                            body: AstStructFieldBody::Value(value),
                        });
                        return Some(self.node(start, AstStructItem::Field(fd)));
                    }
                }
            }
        }
        self.pos = saved;

        if let Some(value) = self.parse_inst_value() {
            return Some(self.node(start, AstStructItem::Anon(value)));
        }

        None
    }

    /// `"{" type-struct-item* "}"`
    fn parse_struct_body(&mut self) -> Option<Vec<AstNode<AstStructItem>>> {
        if !self.eat("{") { return None; }
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.rest().starts_with('}') || self.pos >= self.src.len() { break; }
            if let Some(item) = self.parse_struct_item() {
                items.push(item);
            } else {
                self.push_error(self.pos, format!(
                    "unexpected token in struct body: {:?}", self.peek()
                ));
                self.skip_past_line();
            }
        }
        self.eat("}");
        Some(items)
    }

    // -- instance values & fields ----------------------------------------

    /// `string | "[" inst-value* "]" | ref | "{" inst-item* "}"`
    ///
    /// Ref is tried before bare struct so that `{this:name}-sg` is parsed as a
    /// brace-group ref rather than a struct body.  A bare `{` that fails the ref
    /// parse (e.g. `{ field: value }`) falls through to the struct branch.
    fn parse_inst_value(&mut self) -> Option<AstNode<AstValue>> {
        let start = self.pos;

        if let Some(s) = self.parse_string_lit() {
            return Some(self.node(start, AstValue::Str(s.inner)));
        }

        if self.eat("[") {
            let mut values = Vec::new();
            loop {
                self.skip_ws();
                if self.rest().starts_with(']') || self.pos >= self.src.len() { break; }
                match self.parse_inst_value() {
                    Some(v) => values.push(v),
                    None    => break,
                }
            }
            self.skip_ws();
            if !self.eat("]") {
                self.push_error(self.pos, "expected ']' after list value");
            }
            return Some(self.node(start, AstValue::List(values)));
        }

        if let Some(r) = self.parse_ref() {
            self.skip_ws();
            if self.rest().starts_with('{') {
                self.advance(1);
                let mut fields = Vec::new();
                loop {
                    self.skip_ws();
                    if self.rest().starts_with('}') || self.pos >= self.src.len() { break; }
                    if let Some(item) = self.parse_inst_item() {
                        fields.push(item);
                    } else {
                        self.push_error(self.pos, format!(
                            "unexpected token in inline struct value: {:?}", self.peek()
                        ));
                        self.skip_past_line();
                    }
                }
                self.eat("}");
                return Some(self.node(start, AstValue::Struct { type_hint: Some(r), fields }));
            }
            return Some(self.node(start, AstValue::Ref(r.inner)));
        }

        // Bare struct body — `{` that did not parse as a brace-group ref.
        if self.rest().starts_with('{') {
            self.advance(1);
            let mut fields = Vec::new();
            loop {
                self.skip_ws();
                if self.rest().starts_with('}') || self.pos >= self.src.len() { break; }
                if let Some(item) = self.parse_inst_item() {
                    fields.push(item);
                } else {
                    self.push_error(self.pos, format!(
                        "unexpected token in inline struct value: {:?}", self.peek()
                    ));
                    self.skip_past_line();
                }
            }
            self.eat("}");
            return Some(self.node(start, AstValue::Struct { type_hint: None, fields }));
        }

        None
    }

    /// Try to parse `ident ":" SP inst-value`.
    /// Returns `None` and backtracks if the `:` is not followed by whitespace
    /// (that indicates a ref separator, not a field separator).
    fn try_parse_inst_field(&mut self) -> Option<AstNode<AstField>> {
        let start = self.pos;
        let saved = self.pos;

        let id = self.parse_ident()?;

        if !self.eat(":") {
            self.pos = saved;
            return None;
        }

        // Disambiguation: field requires at least one whitespace after ':'
        let has_ws = self.peek().map_or(false, |c| matches!(c, ' ' | '\t' | '\n' | '\r'));
        if !has_ws {
            self.pos = saved;
            return None;
        }
        self.skip_ws();

        // Detect `via` keyword: the word "via" followed by whitespace.
        let via = {
            let rest = &self.src[self.pos..];
            let is_via = rest.starts_with("via") && rest[3..].chars().next()
                .map_or(true, |c| !c.is_alphanumeric() && c != '_' && c != '-');
            if is_via { self.pos += 3; self.skip_ws(); }
            is_via
        };

        let value = match self.parse_inst_value() {
            Some(v) => v,
            None => {
                self.push_error(self.pos, format!("expected value for field '{}'", id.inner));
                self.pos = saved;
                return None;
            }
        };

        let name = AstNode::new(self.unit, id.loc.start, id.loc.end, id.inner);
        Some(self.node(start, AstField::Named { name, value, via }))
    }

    fn parse_inst_item(&mut self) -> Option<AstNode<AstField>> {
        let start = self.pos;
        if let Some(f) = self.try_parse_inst_field() { return Some(f); }
        if let Some(v) = self.parse_inst_value() {
            return Some(self.node(start, AstField::Anon(v)));
        }
        None
    }

    // -- top-level defs --------------------------------------------------

    fn parse_use(&mut self) -> Option<AstNode<AstUse>> {
        let start = self.pos;
        if !self.at_keyword("use") { return None; }
        self.advance("use".len());
        self.skip_ws();
        let path = self.parse_ref()?;
        Some(self.node(start, AstUse { path: path.inner }))
    }

    // -- new keyword: pack -----------------------------------------------

    /// `"pack" ref ("{" def* "}")?`
    fn parse_pack(&mut self) -> Option<AstNode<AstPack>> {
        let start = self.pos;
        if !self.at_keyword("pack") { return None; }
        let saved = self.pos;
        self.advance("pack".len());
        self.skip_ws();
        let path = match self.parse_ref() {
            Some(r) => r,
            None    => { self.pos = saved; return None; }
        };
        self.skip_ws();
        let defs = if self.eat("{") {
            let mut defs = Vec::new();
            loop {
                self.skip_ws();
                if self.rest().starts_with('}') || self.pos >= self.src.len() { break; }
                if let Some(def) = self.parse_def() {
                    defs.push(def);
                } else {
                    self.push_error(self.pos, format!(
                        "unexpected token in pack body: {:?}", self.peek()
                    ));
                    self.skip_to_next_def();
                    if self.pos >= self.src.len() { break; }
                }
            }
            self.eat("}");
            Some(defs)
        } else {
            None
        };
        Some(self.node(start, AstPack { path, defs }))
    }

    // -- new unified def forms ------------------------------------------

    /// `ident "=" type_expr` — a single named field def.
    /// Returns None (backtracking fully) when not followed by `=`.
    fn parse_named_field_def(&mut self) -> Option<AstNode<AstDefI>> {
        let start = self.pos;
        let saved = self.pos;
        let name = self.parse_ident()?;
        self.skip_ws();
        if !self.eat("=") { self.pos = saved; return None; }
        self.skip_ws();
        let ty = match self.parse_type_expr() {
            Some(t) => t,
            None => {
                self.push_error(self.pos, format!(
                    "expected type expression for field '{}'", name.inner
                ));
                return None;
            }
        };
        Some(self.node(start, AstDefI { name: Some(name), ty }))
    }

    /// `"{" (ident "=" type_expr)* "}"` — input field block in `def name { … } = …`
    fn parse_field_block(&mut self) -> Vec<AstNode<AstDefI>> {
        if !self.eat("{") { return vec![]; }
        let mut fields = Vec::new();
        loop {
            self.skip_ws();
            if self.rest().starts_with('}') || self.pos >= self.src.len() { break; }
            if let Some(f) = self.parse_named_field_def() {
                fields.push(f);
            } else {
                self.push_error(self.pos, format!(
                    "unexpected token in def input block: {:?}", self.peek()
                ));
                self.skip_past_line();
            }
        }
        self.eat("}");
        fields
    }

    /// After consuming `=` in a `def`-keyword form, parse the output:
    ///   - `{` struct body → `Struct`
    ///   - `ref {` mapper ref + struct body → `Struct` (with mapper ref)
    ///   - anything else → `TypeExpr`
    fn parse_after_eq(&mut self) -> (Option<AstNode<AstRef>>, AstNode<AstDefO>) {
        let start = self.pos;

        // Direct struct body
        if self.rest().starts_with('{') {
            let items = self.parse_struct_body().unwrap_or_default();
            let output = if items.is_empty() { AstDefO::Unit } else { AstDefO::Struct(items) };
            return (None, self.node(start, output));
        }

        // Try mapper ref immediately followed (with optional ws) by `{`
        let saved = self.pos;
        if let Some(mapper_ref) = self.parse_ref() {
            self.skip_ws();
            if self.rest().starts_with('{') {
                let items = self.parse_struct_body().unwrap_or_default();
                let output = if items.is_empty() { AstDefO::Unit } else { AstDefO::Struct(items) };
                return (Some(mapper_ref), self.node(start, output));
            }
            self.pos = saved; // not a mapper — backtrack
        }

        // Type expression (ref, enum, list, primitive)
        if let Some(te) = self.parse_type_expr() {
            return (None, self.node(start, AstDefO::TypeExpr(te)));
        }

        self.push_error(self.pos, "expected type expression or struct body after '='");
        (None, self.node(start, AstDefO::Struct(vec![])))
    }

    /// `"def" ident input? ("=" mapper? output)?`
    /// `"plan" ident input? ("=" mapper? output)?`
    fn parse_top_def_with_keyword(&mut self, keyword: &str, planned: bool) -> Option<AstNode<AstDef>> {
        let start = self.pos;
        if !self.at_keyword(keyword) { return None; }
        let saved = self.pos;
        self.advance(keyword.len());
        self.skip_ws();
        let name = match self.parse_ident() {
            Some(n) => n,
            None    => { self.pos = saved; return None; }
        };
        self.skip_ws();

        // Optional input block: `{ field* }` before `=`
        let input = if self.rest().starts_with('{') {
            self.parse_field_block()
        } else {
            vec![]
        };
        self.skip_ws();

        let output_start = self.pos;
        let (mapper_opt, output) = if self.eat("=") {
            self.skip_ws();
            self.parse_after_eq()
        } else {
            (None, self.node(output_start, AstDefO::Unit))
        };

        Some(self.node(start, AstDef {
            planned,
            name,
            input,
            mapper: mapper_opt,
            output,
        }))
    }

    /// Keyword-free def.
    ///
    /// `ident "=" rhs` forms:
    ///   - `{` struct body `}` → `Struct` (type alias with inline struct)
    ///   - `ref "{" struct-items "}"` → `Def[name, ref, unit, Struct[...]]`
    ///   - type-expr (ref, enum, list, primitive) → `TypeExpr` (type alias)
    ///
    /// Shorthand explicit-mapper def forms:
    ///   - `ident mapper "{" struct-items "}"` → `Def[name, mapper, unit, Struct[...]]`
    ///   - `ident mapper` → `Def[name, mapper, unit, unit]`
    ///
    /// Shorthand initial def form:
    ///   - `ident "{" field-def* "}"` → `Def[name, name, Input[...], unit]`
    ///
    /// Returns None (backtracking fully) when the input is not a keyword-free def.
    fn parse_top_def_no_keyword(&mut self) -> Option<AstNode<AstDef>> {
        let start = self.pos;
        let saved = self.pos;

        // Skip if this looks like a keyword
        if self.at_keyword("use")  || self.at_keyword("pack") || self.at_keyword("plan")
        || self.at_keyword("def") {
            return None;
        }

        let name = match self.parse_ident() {
            Some(n) => n,
            None    => return None,
        };
        self.skip_ws();

        if self.rest().starts_with('{') {
            let input = self.parse_field_block();
            let output = self.node(self.pos, AstDefO::Unit);
            return Some(self.node(start, AstDef {
                planned: false,
                name,
                input,
                mapper: None,
                output,
            }));
        }

        if !self.eat("=") {
            let mapper_saved = self.pos;
            if let Some(mapper_name) = self.parse_ref() {
                self.skip_ws();
                let out_start = self.pos;
                let output = if self.rest().starts_with('{') {
                    let items = self.parse_struct_body().unwrap_or_default();
                    let output = if items.is_empty() { AstDefO::Unit } else { AstDefO::Struct(items) };
                    self.node(out_start, output)
                } else {
                    self.node(out_start, AstDefO::Unit)
                };
                return Some(self.node(start, AstDef {
                    planned: false,
                    name,
                    input: vec![],
                    mapper: Some(mapper_name),
                    output,
                }));
            }
            self.pos = mapper_saved;
            self.pos = saved;
            return None;
        }
        self.skip_ws();

        let out_start = self.pos;

        // Direct struct body: `name = { field = type ... }` — type alias
        if self.rest().starts_with('{') {
            let items = self.parse_struct_body().unwrap_or_default();
            let output = self.node(out_start, if items.is_empty() { AstDefO::Unit } else { AstDefO::Struct(items) });
            return Some(self.node(start, AstDef {
                planned: false,
                name,
                input: vec![],
                mapper: None,
                output,
            }));
        }

        // Try explicit mapper ref: `name = ref_expr { ... }`
        let ref_saved = self.pos;
        if let Some(ref_node) = self.parse_ref() {
            self.skip_ws();
            // If followed by `{`, this is a def with explicit mapper ref and struct body.
            if self.rest().starts_with('{') {
                let items = self.parse_struct_body().unwrap_or_default();
                let output = self.node(out_start, if items.is_empty() { AstDefO::Unit } else { AstDefO::Struct(items) });
                return Some(self.node(start, AstDef {
                    planned: false,
                    name,
                    input: vec![],
                    mapper: Some(ref_node),
                    output,
                }));
            }
            self.pos = ref_saved; // backtrack — fall through to type expr
        }

        // Type expression (ref, enum, list, primitive): `name = type_expr`
        if let Some(te) = self.parse_type_expr() {
            let output = self.node(out_start, AstDefO::TypeExpr(te));
            return Some(self.node(start, AstDef {
                planned: false,
                name,
                input: vec![],
                mapper: None,
                output,
            }));
        }

        self.push_error(self.pos, "expected type expression or struct body after '='");
        let output = self.node(out_start, AstDefO::Struct(vec![]));
        Some(self.node(start, AstDef {
            planned: false,
            name,
            input: vec![],
            mapper: None,
            output,
        }))
    }

    fn parse_def(&mut self) -> Option<AstItem> {
        if self.at_keyword("pack") {
            if let Some(p) = self.parse_pack() { return Some(AstItem::Pack(p)); }
        }
        if self.at_keyword("use")  { return self.parse_use().map(AstItem::Use); }
        if self.at_keyword("plan") {
            if let Some(td) = self.parse_top_def_with_keyword("plan", true) { return Some(AstItem::Def(td)); }
        }
        if self.at_keyword("def") {
            if let Some(td) = self.parse_top_def_with_keyword("def", false) { return Some(AstItem::Def(td)); }
        }
        if let Some(td) = self.parse_top_def_no_keyword() { return Some(AstItem::Def(td)); }
        None
    }

    // -- error recovery --------------------------------------------------

    /// Skip to just past the next newline (or EOF).
    fn skip_past_line(&mut self) {
        while self.pos < self.src.len() {
            let b = self.src.as_bytes()[self.pos];
            self.pos += 1;
            if b == b'\n' { break; }
        }
    }

    /// Skip past the current bad token, then advance until we are at a
    /// position that can start a top-level def.
    fn skip_to_next_def(&mut self) {
        match self.peek() {
            None => return,
            Some(c) if c.is_ascii_alphanumeric() || "-_./".contains(c) => {
                while self.pos < self.src.len() {
                    let ch = self.rest().chars().next().unwrap();
                    if ch.is_ascii_alphanumeric() || "-_./".contains(ch) {
                        self.pos += ch.len_utf8();
                    } else {
                        break;
                    }
                }
            }
            Some(_) => { self.pos += 1; }
        }

        loop {
            self.skip_ws();
            if self.pos >= self.src.len() { break; }
            if self.at_keyword("use")  || self.at_keyword("type")  || self.at_keyword("link")
            || self.at_keyword("pack") || self.at_keyword("plan")
            || self.at_keyword("def") {
                break;
            }
            if self.peek().map_or(false, |c| c.is_ascii_alphabetic()) { break; }
            self.pos += 1;
        }
    }

    // -- top-level -------------------------------------------------------

    fn parse_system(&mut self) -> Vec<AstItem> {
        let mut defs = Vec::new();
        loop {
            self.skip_ws();
            if self.pos >= self.src.len() { break; }
            if let Some(def) = self.parse_def() {
                defs.push(def);
            } else {
                let err_pos = self.pos;
                self.push_error(err_pos, format!(
                    "unexpected token at top level: {:?}", self.peek()
                ));
                self.skip_to_next_def();
                if self.pos >= self.src.len() { break; }
            }
        }
        defs
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------


fn find_or_create_scope(scopes: &mut Vec<AstScope>, name: &str, parent: AstScopeId) -> AstScopeId {
    for (i, scope) in scopes.iter().enumerate() {
        if scope.parent == Some(parent) {
            if scope.name.as_ref().map(|n| n.inner.as_str()) == Some(name) {
                return AstScopeId(i as u32);
            }
        }
    }
    let id = AstScopeId(scopes.len() as u32);
    let name_node = AstNode::new(0, 0, 0, name.to_string());
    scopes.push(AstScope { kind: ScopeKind::Pack, name: Some(name_node), parent: Some(parent), defs: vec![] });
    id
}

fn find_or_create_struct_scope(scopes: &mut Vec<AstScope>, name: &AstNode<String>, parent: AstScopeId) -> AstScopeId {
    for (i, scope) in scopes.iter().enumerate() {
        if scope.parent == Some(parent)
            && scope.kind == ScopeKind::Struct
            && scope.name.as_ref().map(|n| n.inner.as_str()) == Some(name.inner.as_str())
        {
            return AstScopeId(i as u32);
        }
    }
    let id = AstScopeId(scopes.len() as u32);
    scopes.push(AstScope {
        kind: ScopeKind::Struct,
        name: Some(name.clone()),
        parent: Some(parent),
        defs: vec![],
    });
    id
}

fn hoist_nested_defs_from_items(items: &mut Vec<AstNode<AstStructItem>>) -> Vec<AstItem> {
    let mut hoisted = Vec::new();
    items.retain(|item| {
        if let AstStructItem::Def(td) = &item.inner {
            hoisted.push(AstItem::Def(td.clone()));
            false
        } else {
            true
        }
    });
    hoisted
}

fn hoist_nested_defs_in_scope(scopes: &mut Vec<AstScope>, scope_id: AstScopeId) {
    let defs_len = scopes[scope_id.0 as usize].defs.len();
    for def_idx in 0..defs_len {
        let mut hoisted = Vec::new();
        let mut parent_name: Option<AstNode<String>> = None;

        {
            let scope = &mut scopes[scope_id.0 as usize];
            if let Some(AstItem::Def(td)) = scope.defs.get_mut(def_idx) {
                parent_name = Some(td.inner.name.clone());
                if let AstDefO::Struct(items) = &mut td.inner.output.inner {
                    hoisted = hoist_nested_defs_from_items(items);
                    if items.is_empty() {
                        td.inner.output.inner = AstDefO::Unit;
                    }
                }
            }
        }

        if !hoisted.is_empty() {
            let struct_scope_id = find_or_create_struct_scope(scopes, &parent_name.unwrap(), scope_id);
            scopes[struct_scope_id.0 as usize].defs.extend(hoisted);
        }
    }

    let child_scope_ids: Vec<AstScopeId> = scopes.iter().enumerate()
        .filter_map(|(i, s)| (s.parent == Some(scope_id)).then_some(AstScopeId(i as u32)))
        .collect();
    for child_id in child_scope_ids {
        hoist_nested_defs_in_scope(scopes, child_id);
    }
}

fn find_or_create_pack_path(scopes: &mut Vec<AstScope>, path: &AstRef, parent: AstScopeId) -> AstScopeId {
    let mut cur = parent;
    for seg in &path.segments {
        let Some(name) = seg.inner.as_plain() else { continue; };
        cur = find_or_create_scope(scopes, name, cur);
    }
    cur
}

fn materialize_items_into_scope(scopes: &mut Vec<AstScope>, scope_id: AstScopeId, items: Vec<AstItem>) {
    let mut active_scope = scope_id;
    for item in items {
        match item {
            AstItem::Pack(pack_node) => {
                let pack_scope = find_or_create_pack_path(scopes, &pack_node.inner.path.inner, active_scope);
                if let Some(defs) = pack_node.inner.defs {
                    materialize_items_into_scope(scopes, pack_scope, defs);
                } else {
                    active_scope = pack_scope;
                }
            }
            other => scopes[active_scope.0 as usize].defs.push(other),
        }
    }
}

pub fn parse(req: ParseReq) -> ParseRes {
    let root = AstScope { kind: ScopeKind::Pack, name: None, parent: None, defs: vec![] };
    let mut scopes = vec![root];
    let mut errors = Vec::new();
    let mut unit_scope_ids = Vec::with_capacity(req.units.len());
    let mut unit_ts_srcs   = Vec::with_capacity(req.units.len());

    for (unit_idx, unit) in req.units.iter().enumerate() {
        let mut parent_id = AstScopeId(0);
        for seg in &unit.path {
            parent_id = find_or_create_scope(&mut scopes, seg, parent_id);
        }
        let leaf_id = if unit.name.is_empty() {
            parent_id
        } else {
            find_or_create_scope(&mut scopes, &unit.name, parent_id)
        };
        unit_scope_ids.push(leaf_id);
        unit_ts_srcs.push(unit.ts_src.clone());

        let mut p = Parser::new(&unit.src, unit_idx as u32);
        let items = p.parse_system();
        errors.extend(p.errors);
        materialize_items_into_scope(&mut scopes, leaf_id, items);
        hoist_nested_defs_in_scope(&mut scopes, leaf_id);
    }

    ParseRes { scopes, errors, unit_scope_ids, unit_ts_srcs }
}
