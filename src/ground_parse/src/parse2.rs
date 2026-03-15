/// Hand-written recursive-descent parser for RFC 0005.
/// Full grammar is documented in `grammar.md`.
///
/// ```bnf
/// ident          ::= [a-zA-Z][a-zA-Z0-9\-_]*
/// comment        ::= "#" [^\n]*
/// string         ::= '"' [^"]* '"'
/// ref-atom       ::= [a-zA-Z0-9\-_./]+
/// ref-opt-atom   ::= "(" ident ")"
/// ref-seg        ::= ref-opt-atom | ref-atom
/// ref            ::= ref-seg (":" ref-seg)*
///
/// primitive      ::= "string" | "integer" | "reference"
///
/// type-expr      ::= primitive
///                  | type-def
///                  | "[" type-expr "]"
///                  | ref ("|" ref)*        (single ref or sugar enum)
///
/// type-enum-item ::= ref-atom              (simple atom in explicit type body)
/// type-enum-body ::= type-enum-item ("|" type-enum-item)*
/// type-struct-item ::= type-def | link-def
/// type-struct-body ::= "{" type-struct-item* "}"
/// type-body        ::= type-struct-body | type-enum-body
/// type-def         ::= "type" ident? "=" type-body
///
/// link-def       ::= "link" ident? "=" type-expr
///
/// inst-value     ::= string | "[" inst-value* "]" | ref
/// inst-field     ::= ident ":" SP inst-value      (SP = 1+ whitespace)
/// inst-item      ::= inst-field | inst-value
/// inst           ::= ident ident ("{" inst-item* "}")?
/// deploy         ::= "deploy" ref "to" ref "as" ref ("{" inst-field* "}")?
///
/// def            ::= type-def | link-def | deploy | inst
/// system         ::= def*
/// ```
use crate::ast2::*;

// ---------------------------------------------------------------------------
// Parser state
// ---------------------------------------------------------------------------

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
            return Some(self.node(start, AstRefSeg { value: v, is_opt: true }));
        }
        if let Some(v) = self.parse_ref_atom() {
            return Some(self.node(start, AstRefSeg { value: v, is_opt: false }));
        }
        None
    }

    /// `ref-seg (":" ref-seg)*`
    fn parse_ref(&mut self) -> Option<AstNode<AstRef>> {
        let start = self.pos;
        let first = self.parse_ref_seg()?;
        let mut segments = vec![first];
        loop {
            let saved = self.pos;
            if !self.eat(":") { break; }
            if let Some(seg) = self.parse_ref_seg() {
                segments.push(seg);
            } else {
                self.pos = saved;
                break;
            }
        }
        Some(self.node(start, AstRef { segments }))
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
            return Some(self.node(start, AstTypeDef { name: None, body }));
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
            return Some(self.node(start, AstTypeDef { name: None, body }));
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
            Some(self.node(start, AstTypeDef { name: None, body }))
        } else {
            let body = self.node(start, AstTypeDefBody::Enum(refs));
            Some(self.node(start, AstTypeDef { name: None, body }))
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

    /// `ref-atom ("|" ref-atom)*` — items are plain single-segment refs.
    fn parse_enum_body(&mut self) -> Option<Vec<AstNode<AstRef>>> {
        let first = self.parse_atom_as_ref()?;
        let mut items = vec![first];
        loop {
            let saved = self.pos;
            self.skip_ws();
            if self.eat("|") {
                self.skip_ws();
                if let Some(r) = self.parse_atom_as_ref() {
                    items.push(r);
                    continue;
                }
            }
            self.pos = saved;
            break;
        }
        Some(items)
    }

    /// Parse a single `ref-atom` and wrap it as a single-segment `AstRef`.
    fn parse_atom_as_ref(&mut self) -> Option<AstNode<AstRef>> {
        let start = self.pos;
        let atom = self.parse_ref_atom()?;
        let seg = AstNode::new(self.unit, start as u32, self.pos as u32,
            AstRefSeg { value: atom, is_opt: false });
        Some(self.node(start, AstRef { segments: vec![seg] }))
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

    /// `"type" ident? "=" type-body`
    ///
    /// Returns `None` and backtracks fully if the pattern doesn't match
    /// (allows callers to try other alternatives).
    fn parse_type_def(&mut self) -> Option<AstNode<AstTypeDef>> {
        let start = self.pos;
        if !self.at_keyword("type") { return None; }
        self.advance("type".len());
        self.skip_ws();

        let name = if !self.rest().starts_with('=') {
            let id = self.parse_ident();
            if id.is_some() { self.skip_ws(); }
            id
        } else {
            None
        };

        if !self.eat("=") {
            // Not a type-def — backtrack so caller can try other alternatives.
            self.pos = start;
            return None;
        }
        self.skip_ws();

        let body_start = self.pos;
        let body = if self.rest().starts_with('{') {
            let items = self.parse_struct_body().unwrap_or_default();
            self.node(body_start, AstTypeDefBody::Struct(items))
        } else {
            let items = self.parse_enum_body().unwrap_or_default();
            self.node(body_start, AstTypeDefBody::Enum(items))
        };

        Some(self.node(start, AstTypeDef { name, body }))
    }

    // -- instance values & fields ----------------------------------------

    /// `string | "[" inst-value* "]" | ref`
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
            return Some(self.node(start, AstValue::Ref(r.inner)));
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
        if self.at_keyword("type") || self.at_keyword("link") || self.at_keyword("deploy") {
            return None;
        }

        let type_name = self.parse_ident()?;
        self.skip_ws();

        if self.at_keyword("type") || self.at_keyword("link") || self.at_keyword("deploy") {
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

    fn parse_def(&mut self) -> Option<AstDef> {
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
            if self.at_keyword("type") || self.at_keyword("link") || self.at_keyword("deploy") {
                break;
            }
            if self.peek().map_or(false, |c| c.is_ascii_alphabetic()) { break; }
            self.pos += 1;
        }
    }

    // -- top-level -------------------------------------------------------

    fn parse_system(&mut self) -> AstUnit {
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
        AstUnit { unit: self.unit, defs }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn parse(req: ParseReq) -> ParseRes {
    let mut units  = Vec::new();
    let mut errors = Vec::new();

    for (idx, src) in req.units.iter().enumerate() {
        let mut p = Parser::new(src, idx as u32);
        units.push(p.parse_system());
        errors.extend(p.errors);
    }

    ParseRes { units, errors }
}
