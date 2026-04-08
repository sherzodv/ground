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

    // -- primitives ------------------------------------------------------

    fn parse_primitive(&mut self) -> Option<AstNode<AstPrimitive>> {
        let start = self.pos;
        let p = if self.at_keyword("string") {
            self.advance("string".len()); AstPrimitive::String
        } else if self.at_keyword("integer") {
            self.advance("integer".len()); AstPrimitive::Integer
        } else if self.at_keyword("reference") {
            self.advance("reference".len()); AstPrimitive::Reference
        } else {
            return None;
        };
        Some(self.node(start, p))
    }

    // -- type expressions (link bodies, list elements) -------------------

    /// `primitive | type-def | "[" type-expr "]" | ref ("|" ref)*`
    ///
    /// Always returns an anonymous `AstTypeDef`; a union of refs desugars to `Enum`.
    fn parse_type_expr(&mut self) -> Option<AstNode<AstTypeDef>> {
        let start = self.pos;

        // Primitive
        if let Some(prim) = self.parse_primitive() {
            let body = self.node(start, AstTypeDefBody::Primitive(prim.inner));
            return Some(self.node(start, AstTypeDef { name: None, params: vec![], body, scope: None }));
        }

        // Explicit type-def: "type" ident? "=" body
        if self.at_keyword("type") {
            let saved = self.pos;
            if let Some(td) = self.parse_type_def() {
                return Some(td);
            }
            self.pos = saved;
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
            let body = self.node(start, AstTypeDefBody::List(Box::new(inner)));
            return Some(self.node(start, AstTypeDef { name: None, params: vec![], body, scope: None }));
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
            let body = self.node(start, AstTypeDefBody::Ref(r.inner));
            Some(self.node(start, AstTypeDef { name: None, params: vec![], body, scope: None }))
        } else {
            let body = self.node(start, AstTypeDefBody::Enum(refs));
            Some(self.node(start, AstTypeDef { name: None, params: vec![], body, scope: None }))
        }
    }

    // -- link definition -------------------------------------------------

    /// `"link" ident? "=" type-expr`
    fn parse_link_def(&mut self) -> Option<AstNode<AstLinkDef>> {
        let start = self.pos;
        if !self.at_keyword("link") { return None; }
        self.advance("link".len());
        self.skip_ws();

        let name = if self.rest().starts_with('=') {
            None
        } else {
            let id = self.parse_ident()?;
            self.skip_ws();
            Some(id)
        };

        if !self.eat("=") {
            self.push_error(start, "expected '=' in link definition");
            return None;
        }
        self.skip_ws();

        let ty = self.parse_type_expr()?;
        Some(self.node(start, AstLinkDef { name, ty }))
    }

    // -- type body -------------------------------------------------------

    /// `ref ("|" ref)*` — items are full refs, supporting multi-segment paths such as `type:foo`.
    fn parse_enum_body(&mut self) -> Option<Vec<AstNode<AstRef>>> {
        let first = self.parse_ref()?;
        let mut items = vec![first];
        loop {
            let saved = self.pos;
            self.skip_ws();
            if self.eat("|") {
                self.skip_ws();
                if let Some(r) = self.parse_ref() {
                    items.push(r);
                    continue;
                }
            }
            self.pos = saved;
            break;
        }
        Some(items)
    }

    fn parse_struct_item(&mut self) -> Option<AstNode<AstStructItem>> {
        let start = self.pos;

        if self.at_keyword("type") {
            let saved = self.pos;
            if let Some(td) = self.parse_type_def() {
                return Some(self.node(start, AstStructItem::TypeDef(td)));
            }
            self.pos = saved;
        }

        if self.at_keyword("link") {
            let saved = self.pos;
            if let Some(ld) = self.parse_link_def() {
                return Some(self.node(start, AstStructItem::LinkDef(ld)));
            }
            self.pos = saved;
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

    // -- type function params & body -------------------------------------

    /// Try to parse a parameter list `"(" ident ":" ref ("," ident ":" ref)* ")"`.
    /// Returns empty vec and backtracks if the `(` is not followed by `ident :`.
    fn try_parse_type_params(&mut self) -> Vec<AstNode<AstTypeParam>> {
        if !self.rest().starts_with('(') { return vec![]; }
        let saved = self.pos;
        self.advance(1); // consume `(`
        self.skip_ws();

        let mut params = Vec::new();
        loop {
            let param_start = self.pos;
            let name = match self.parse_ident() {
                Some(n) => n,
                None    => { self.pos = saved; return vec![]; }
            };
            self.skip_ws();
            if !self.eat(":") { self.pos = saved; return vec![]; }
            self.skip_ws();
            let ty = match self.parse_ref() {
                Some(r) => r,
                None    => { self.pos = saved; return vec![]; }
            };
            params.push(self.node(param_start, AstTypeParam { name, ty }));
            self.skip_ws();
            if self.eat(",") { self.skip_ws(); continue; }
            break;
        }

        if !self.eat(")") { self.pos = saved; return vec![]; }
        self.skip_ws();
        params
    }

    /// Parse a type function body: `"{" (ident ":" SP inst-value)* "}"`
    /// Each entry is `alias: vendor-type { fields }`.
    fn parse_typefn_body(&mut self) -> Option<Vec<AstNode<AstTypeFnEntry>>> {
        if !self.eat("{") { return None; }
        let mut entries = Vec::new();
        loop {
            self.skip_ws();
            if self.rest().starts_with('}') || self.pos >= self.src.len() { break; }
            if let Some(entry) = self.try_parse_typefn_entry() {
                entries.push(entry);
            } else {
                self.push_error(self.pos, format!(
                    "unexpected token in type function body: {:?}", self.peek()
                ));
                self.skip_past_line();
            }
        }
        self.eat("}");
        Some(entries)
    }

    /// `ident ":" SP inst-value` — same disambiguation as `try_parse_inst_field`.
    fn try_parse_typefn_entry(&mut self) -> Option<AstNode<AstTypeFnEntry>> {
        let start = self.pos;
        let saved = self.pos;

        let alias = self.parse_ident()?;

        if !self.eat(":") {
            self.pos = saved;
            return None;
        }

        let has_ws = self.peek().map_or(false, |c| matches!(c, ' ' | '\t' | '\n' | '\r'));
        if !has_ws {
            self.pos = saved;
            return None;
        }
        self.skip_ws();

        let value = match self.parse_inst_value() {
            Some(v) => v,
            None => {
                self.push_error(self.pos, format!("expected value for type fn entry '{}'", alias.inner));
                self.pos = saved;
                return None;
            }
        };

        Some(self.node(start, AstTypeFnEntry { alias, value }))
    }

    // -- type def --------------------------------------------------------

    /// `"type" (name | anonymous) (params)? "=" body`
    ///
    /// Forms:
    ///   `type name = body`               — zero-param named type
    ///   `type = body`                    — zero-param anonymous type
    ///   `type name(p: T, ...) = { ... }` — named type function
    ///   `type (p: T, ...) = { ... }`     — anonymous type function
    ///
    /// Returns `None` and backtracks fully if the pattern doesn't match.
    fn parse_type_def(&mut self) -> Option<AstNode<AstTypeDef>> {
        let start = self.pos;
        if !self.at_keyword("type") { return None; }
        self.advance("type".len());
        self.skip_ws();

        // Determine name: skip if next char is `=` (anonymous zero-param) or `(` (anonymous fn)
        let name = if self.rest().starts_with('=') || self.rest().starts_with('(') {
            None
        } else {
            let id = self.parse_ident();
            if id.is_some() { self.skip_ws(); }
            id
        };

        // Try to parse parameter list `(ident: ref, ...)`
        let params = self.try_parse_type_params();

        if !self.eat("=") {
            // Not a type-def — backtrack so caller can try other alternatives.
            self.pos = start;
            return None;
        }
        self.skip_ws();

        let body_start = self.pos;
        let body = if !params.is_empty() {
            // Type function: body must be `{ alias: vendor-type { ... } ... }`
            if self.rest().starts_with('{') {
                let entries = self.parse_typefn_body().unwrap_or_default();
                self.node(body_start, AstTypeDefBody::TypeFn(entries))
            } else {
                self.push_error(self.pos, "expected '{' for type function body");
                self.node(body_start, AstTypeDefBody::TypeFn(vec![]))
            }
        } else if self.rest().starts_with('{') {
            let items = self.parse_struct_body().unwrap_or_default();
            self.node(body_start, AstTypeDefBody::Struct(items))
        } else {
            let items = self.parse_enum_body().unwrap_or_default();
            self.node(body_start, AstTypeDefBody::Enum(items))
        };

        Some(self.node(start, AstTypeDef { name, params, body, scope: None }))
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

        let value = match self.parse_inst_value() {
            Some(v) => v,
            None => {
                self.push_error(self.pos, format!("expected value for field '{}'", id.inner));
                self.pos = saved;
                return None;
            }
        };

        let name = AstNode::new(self.unit, id.loc.start, id.loc.end, id.inner);
        Some(self.node(start, AstField::Named { name, value }))
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

    /// `ident ident ("{" inst-item* "}")?`
    fn parse_inst(&mut self) -> Option<AstNode<AstInst>> {
        let start = self.pos;

        // Inst must not start with reserved keywords
        if self.at_keyword("type") || self.at_keyword("link") || self.at_keyword("deploy") || self.at_keyword("use") {
            return None;
        }

        let type_name = self.parse_ref()?;
        self.skip_ws();

        if self.at_keyword("type") || self.at_keyword("link") || self.at_keyword("deploy") || self.at_keyword("use") {
            self.pos = start;
            return None;
        }

        let inst_name = self.parse_ident()?;
        self.skip_ws();

        let mut fields = Vec::new();
        if self.eat("{") {
            loop {
                self.skip_ws();
                if self.rest().starts_with('}') || self.pos >= self.src.len() { break; }
                if let Some(item) = self.parse_inst_item() {
                    fields.push(item);
                } else {
                    self.push_error(self.pos, format!(
                        "unexpected token in instance body: {:?}", self.peek()
                    ));
                    self.skip_past_line();
                }
            }
            self.eat("}");
        }

        Some(self.node(start, AstInst { type_name, inst_name, fields }))
    }

    /// `"deploy" ref "to" ref "as" ref ("{" inst-field* "}")?`
    fn parse_deploy(&mut self) -> Option<AstNode<AstDeploy>> {
        let start = self.pos;
        if !self.at_keyword("deploy") { return None; }
        self.advance("deploy".len());
        self.skip_ws();

        let what = self.parse_ref()?;
        self.skip_ws();

        if !self.at_keyword("to") {
            self.push_error(self.pos, "expected 'to' after deploy target");
            return None;
        }
        self.advance("to".len());
        self.skip_ws();

        let target = self.parse_ref()?;
        self.skip_ws();

        if !self.at_keyword("as") {
            self.push_error(self.pos, "expected 'as' after deploy destination");
            return None;
        }
        self.advance("as".len());
        self.skip_ws();

        let name = self.parse_ref()?;
        self.skip_ws();

        let mut fields = Vec::new();
        if self.eat("{") {
            loop {
                self.skip_ws();
                if self.rest().starts_with('}') || self.pos >= self.src.len() { break; }
                if let Some(f) = self.try_parse_inst_field() {
                    fields.push(f);
                } else {
                    self.push_error(self.pos, format!(
                        "unexpected token in deploy body: {:?}", self.peek()
                    ));
                    self.skip_past_line();
                }
            }
            self.eat("}");
        }

        Some(self.node(start, AstDeploy { what, target, name, fields }))
    }

    fn parse_use(&mut self) -> Option<AstNode<AstUse>> {
        let start = self.pos;
        if !self.at_keyword("use") { return None; }
        self.advance("use".len());
        self.skip_ws();
        let path = self.parse_ref()?;
        Some(self.node(start, AstUse { path: path.inner }))
    }

    fn parse_def(&mut self) -> Option<AstDef> {
        if self.at_keyword("use")    { return self.parse_use().map(AstDef::Use);         }
        if self.at_keyword("type")   { return self.parse_type_def().map(AstDef::Type);   }
        if self.at_keyword("link")   { return self.parse_link_def().map(AstDef::Link);   }
        if self.at_keyword("deploy") { return self.parse_deploy().map(AstDef::Deploy);   }
        self.parse_inst().map(AstDef::Inst)
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
            if self.at_keyword("use") || self.at_keyword("type") || self.at_keyword("link") || self.at_keyword("deploy") {
                break;
            }
            if self.peek().map_or(false, |c| c.is_ascii_alphabetic()) { break; }
            self.pos += 1;
        }
    }

    // -- top-level -------------------------------------------------------

    fn parse_system(&mut self) -> Vec<AstDef> {
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

/// For a named struct `AstTypeDef` (zero-param only): create a `ScopeKind::Type` scope under
/// `parent_scope`, store its id in `td.inner.scope`, move `TypeDef` struct items into that
/// scope (removing them from the struct body), then recurse into hoisted TypeDefs and
/// remaining LinkDef types.
///
/// Type function defs (params non-empty) are skipped — their bodies are TypeFn, not Struct.
fn hoist_struct_scopes(
    td:           &mut AstNode<AstTypeDef>,
    parent_scope: AstScopeId,
    scopes:       &mut Vec<AstScope>,
) {
    let name = match td.inner.name.as_ref().map(|n| n.inner.clone()) {
        Some(n) => n,
        None    => return,
    };
    // Skip type function definitions — they have TypeFn bodies, not struct bodies.
    if !td.inner.params.is_empty() { return; }

    // Take items out first so we can freely mutate `td` below.
    let all_items = match &mut td.inner.body.inner {
        AstTypeDefBody::Struct(items) => std::mem::take(items),
        _                             => return,
    };

    let type_scope_id = AstScopeId(scopes.len() as u32);
    scopes.push(AstScope {
        kind:   ScopeKind::Type,
        name:   Some(AstNode::new(0, 0, 0, name)),
        parent: Some(parent_scope),
        defs:   vec![],
    });
    td.inner.scope = Some(type_scope_id);

    // Partition: hoist TypeDef items; keep LinkDef items.
    let mut keep:    Vec<AstNode<AstStructItem>> = Vec::new();
    let mut hoisted: Vec<AstNode<AstTypeDef>>    = Vec::new();
    for item in all_items {
        match item.inner {
            AstStructItem::TypeDef(sub_td) => hoisted.push(sub_td),
            AstStructItem::LinkDef(_)      => keep.push(item),
        }
    }

    // Recurse into hoisted TypeDefs, then collect them as defs for the new scope.
    let mut hoist_defs = Vec::new();
    for mut sub_td in hoisted {
        hoist_struct_scopes(&mut sub_td, type_scope_id, scopes);
        hoist_defs.push(AstDef::Type(sub_td));
    }
    scopes[type_scope_id.0 as usize].defs.extend(hoist_defs);

    // Recurse into LinkDef types that are named structs.
    for item in keep.iter_mut() {
        if let AstStructItem::LinkDef(ref mut ld) = item.inner {
            hoist_struct_scopes(&mut ld.inner.ty, type_scope_id, scopes);
        }
    }

    // Put the (now TypeDef-free) link items back into the struct body.
    if let AstTypeDefBody::Struct(items) = &mut td.inner.body.inner {
        *items = keep;
    }
}

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

pub fn parse(req: ParseReq) -> ParseRes {
    let root = AstScope { kind: ScopeKind::Pack, name: None, parent: None, defs: vec![] };
    let mut scopes = vec![root];
    let mut errors = Vec::new();

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

        let mut p = Parser::new(&unit.src, unit_idx as u32);
        let raw_defs = p.parse_system();
        errors.extend(p.errors);

        let mut defs = Vec::new();
        for def in raw_defs {
            match def {
                AstDef::Type(mut td) => {
                    hoist_struct_scopes(&mut td, leaf_id, &mut scopes);
                    defs.push(AstDef::Type(td));
                }
                other => defs.push(other),
            }
        }
        scopes[leaf_id.0 as usize].defs.extend(defs);
    }

    ParseRes { scopes, errors }
}
