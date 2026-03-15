# RFC 0005 — Parser rewrite

## BNF Grammar

The following grammar is descriptive, capturing the idea, not definitive

```bnf
ident            ::= [a-zA-Z][a-zA-Z0-9\-_]*
integer          ::= [0-9]+
string           ::= '"' [^"]* '"'
keyword          ::= "type" | "link" | "deploy" | "to" | "as" | "string" | "integer" | "reference"
primitive        ::= "string" | "integer" | "reference"

ref-atom         ::= [a-zA-Z\-_./][a-zA-Z0-9\-_./]* | [0-9]+[a-zA-Z\-_./][a-zA-Z0-9\-_./]*
ref-opt-atom     ::= "(" ident ")"
ref-seg          ::= ref-atom | ref-opt-atom
ref              ::= ref-seg (":" ref-seg)*

link-ref-union   ::= ref ("|" ref)*
link-scalar      ::= primitive | link-ref-union | type-def
link-list        ::= "[" link-scalar "]"
link-body        ::= link-scalar | link-list
link-def         ::= "link" ident? "=" link-body

type-link-ref    ::= ref
type-enum-item   ::= ident | integer
type-enum-body   ::= type-enum-item ("|" type-enum-item)*
type-struct-item ::= type-def | link-def | type-link-ref
type-struct-body ::= "{" type-struct-item* "}"
type-body        ::= type-enum-body | type-struct-body
type-def         ::= "type" ident? "=" type-body

inst-value       ::= string | ref | integer | "[" inst-value* "]"
inst-field       ::= ident ":" SP inst-value
inst-item        ::= inst-field | inst-value
inst             ::= ident ident ("{" inst-item* "}")?

deploy           ::= "deploy" ref "to" ref "as" ref ("{" inst-field* "}")?

def              ::= type-def | link-def | inst | deploy
system           ::= def*
```

Where `SP` denotes one or more whitespace characters (space or tab).

## Disambiguation

- *inst-field* vs *ref* — *inst-field* requires at least one whitespace after the `:` to disambiguate with ref
- *enum* vs *struct* — peek after `=`: `{` → *type-struct-body*, else → *type-enum-body*
- `link x = type foo = a | b` means link *x* has type *foo* which is enum *a | b*. Parenthesised union disambiguation may be added in a future pass.

## AST

```rust
#[derive(Debug)]
struct AstRefSeg {
    value:  String,
    is_opt: bool,
}

#[derive(Debug)]
struct AstRef {
    segments: Vec<AstNode<AstRefSeg>>,
}

#[derive(Debug)]
enum AstPrimitive { String, Integer, Reference }

#[derive(Debug)]
enum AstLinkType {
    Primitive(AstPrimitive),
    Union(Vec<AstNode<AstRef>>),
    List(Box<AstNode<AstLinkType>>),
    Inline(Box<AstNode<AstTypeDef>>),
}

#[derive(Debug)]
struct AstLinkDef {
    name: Option<AstNode<String>>,
    ty:   AstNode<AstLinkType>,
}

#[derive(Debug)]
enum AstEnumItem { Name(String), Int(u64) }

#[derive(Debug)]
enum AstStructItem {
    TypeDef(AstNode<AstTypeDef>),
    LinkDef(AstNode<AstLinkDef>),
    Ref(AstNode<AstRef>),
}

#[derive(Debug)]
enum AstTypeDefBody {
    Enum(Vec<AstNode<AstEnumItem>>),
    Struct(Vec<AstNode<AstStructItem>>),
}

#[derive(Debug)]
struct AstTypeDef {
    name: Option<AstNode<String>>,
    body: AstNode<AstTypeDefBody>,
}

#[derive(Debug)]
enum AstValue {
    Str(String),
    Int(u64),
    Ref(AstRef),
    List(Vec<AstNode<AstValue>>),
}

#[derive(Debug)]
enum AstField {
    Named { name: AstNode<String>, value: AstNode<AstValue> },
    Anon(AstNode<AstValue>),
}

#[derive(Debug)]
struct AstInst {
    type_name: AstNode<String>,
    inst_name: AstNode<String>,
    fields:    Vec<AstNode<AstField>>,
}

#[derive(Debug)]
struct AstDeploy {
    what:   AstNode<AstRef>,
    target: AstNode<AstRef>,
    name:   AstNode<AstRef>,
    fields: Vec<AstNode<AstField>>,
}

#[derive(Debug)]
enum AstDef {
    Type(AstNode<AstTypeDef>),
    Link(AstNode<AstLinkDef>),
    Inst(AstNode<AstInst>),
    Deploy(AstNode<AstDeploy>),
}

#[derive(Debug)]
struct AstUnit {
    unit: u32,
    defs: Vec<AstDef>,
}

#[derive(Debug)]
struct AstParseError {
    message: String,
    loc:     AstNodeLoc,
}

#[derive(Debug)]
struct AstNodeLoc {
    unit:  u32,
    start: u32,
    end:   u32,
}

#[derive(Debug)]
struct AstNode<T> {
    loc:   AstNodeLoc,
    inner: T,
}

#[derive(Debug)]
struct ParseReq {
    units: Vec<String>,
}

#[derive(Debug)]
struct ParseRes {
    units:  Vec<AstUnit>,
    errors: Vec<AstParseError>,
}
```

## Out of scope

Resolve and semantic validation is out of scope and will be implemented in the following layers.
