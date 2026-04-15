/// Ground parser level AST  — every node is wrapped in `AstNode<T>` carrying byte-offset location.

// ---------------------------------------------------------------------------
// Location & node wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AstNodeLoc {
    pub unit:  u32,
    pub start: u32,
    pub end:   u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode<T> {
    pub loc:   AstNodeLoc,
    pub inner: T,
}

impl<T> AstNode<T> {
    pub fn new(unit: u32, start: u32, end: u32, inner: T) -> Self {
        AstNode { loc: AstNodeLoc { unit, start, end }, inner }
    }
}

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

/// The value of one ref segment: either a plain atom or a brace-group `{inner:ref}`.
///
/// `Group(inner, trailing)` — `{inner}` followed immediately (no `:`) by an optional
/// plain atom, e.g. `{this:id}-sg` → `Group(this:id, Some("-sg"))`.
/// This is distinct from `{this:id}:seg` which produces `[Group(this:id, None), Plain("seg")]`.
#[derive(Debug, Clone, PartialEq)]
pub enum AstRefSegVal {
    Plain(String),
    Group(AstRef, Option<String>),
}

/// One segment of a colon-separated reference.
/// `is_opt = true` when the segment was written `(ident)`.
#[derive(Debug, Clone, PartialEq)]
pub struct AstRefSeg {
    pub value:  AstRefSegVal,
    pub is_opt: bool,
}

impl AstRefSeg {
    /// Returns the plain string for Plain segments; None for Group segments.
    pub fn as_plain(&self) -> Option<&str> {
        match &self.value {
            AstRefSegVal::Plain(s)    => Some(s.as_str()),
            AstRefSegVal::Group(..)   => None,
        }
    }
}

/// A colon-separated reference: `seg0 ":" seg1 ":" …`
#[derive(Debug, Clone, PartialEq)]
pub struct AstRef {
    pub segments: Vec<AstNode<AstRefSeg>>,
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstPrimitive { String, Integer, Reference }

// ---------------------------------------------------------------------------
// New unified def node (replaces AstDef::Type + AstDef::Link at top level)
// ---------------------------------------------------------------------------

/// A named def — covers simple type defs, bare unit types, and hook transformations.
///
/// Forms:
///   `name = type_expr`                         — type alias (has_def=false, input empty, hook None)
///   `def name`                                 — bare entity def (has_def=true, output Unit)
///   `def name { input } = { output }`          — hook def (has_def=true, has input)
///   `def name { input } = hookname { output }` — hook def with explicit TS name
#[derive(Debug, Clone, PartialEq)]
pub struct AstTopDef {
    pub name:    AstNode<String>,
    pub has_def: bool,                           // true when the `def` keyword was written
    pub input:   Vec<AstNode<AstFieldDef>>,      // fields before `=`; empty for simple defs
    pub hook:    Option<AstNode<String>>,         // TS function name between `=` and output body
    pub output:  AstNode<AstTopDefOutput>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstTopDefOutput {
    Unit,                                       // bare `def name` or `name` with no `=`
    TypeExpr(AstNode<AstTypeDef>),              // `= type_expr`
    Struct(Vec<AstNode<AstStructItem>>),        // `= hook? { struct_items }`
}

/// A named or anonymous field declaration inside a struct/output body.
/// Replaces `AstLinkDef` in struct item context.
#[derive(Debug, Clone, PartialEq)]
pub struct AstFieldDef {
    pub name: Option<AstNode<String>>,          // None for anonymous `= type_expr`
    pub ty:   AstNode<AstTypeDef>,
}

// ---------------------------------------------------------------------------
// Pack and Plan declarations
// ---------------------------------------------------------------------------

/// `pack ref` or `pack ref { defs... }` — namespace declaration (like Scala packages).
#[derive(Debug, Clone, PartialEq)]
pub struct AstPack {
    pub path: AstNode<AstRef>,
    pub defs: Vec<AstDef>,                      // empty for bare file-level `pack std:aws`
}

/// `plan name` or `plan name { fields }` — resolution trigger.
#[derive(Debug, Clone, PartialEq)]
pub struct AstPlan {
    pub name:   AstNode<String>,
    pub fields: Vec<AstNode<AstField>>,
}

// ---------------------------------------------------------------------------
// Type function parameters & entries
// ---------------------------------------------------------------------------

/// One parameter in a type function definition: `name: type-ref`
#[derive(Debug, Clone, PartialEq)]
pub struct AstTypeParam {
    pub name: AstNode<String>,
    pub ty:   AstNode<AstRef>,
}

/// One entry in a type function body: `alias: vendor-type { fields... }`
#[derive(Debug, Clone, PartialEq)]
pub struct AstTypeFnEntry {
    pub alias: AstNode<String>,
    pub value: AstNode<AstValue>,  // AstValue::Struct with type_hint
}

// ---------------------------------------------------------------------------
// Type definitions — the universal type expression
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstTypeDefBody {
    /// Unit: bare `def name` or `name` with no rhs
    Unit,
    /// Built-in scalar: `string` | `integer` | `reference`
    Primitive(AstPrimitive),
    /// Single reference to an existing type: `postgresql`, `type:region:type:zone`
    Ref(AstRef),
    /// Union of refs: `self | provider | cloud`
    Enum(Vec<AstNode<AstRef>>),
    /// Struct body: `{ field … }` — new-syntax field items or legacy `link`/`type` items
    Struct(Vec<AstNode<AstStructItem>>),
    /// List whose element type is described by the inner `AstTypeDef`: `[ … ]`
    List(Box<AstNode<AstTypeDef>>),
    /// Type function body: `{ alias: vendor-type { fields } … }` — only valid when params non-empty
    TypeFn(Vec<AstNode<AstTypeFnEntry>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstTypeDef {
    pub name:   Option<AstNode<String>>,
    /// Non-empty for type function definitions: `type name(param: type) = { ... }`
    pub params: Vec<AstNode<AstTypeParam>>,
    pub body:   AstNode<AstTypeDefBody>,
    /// Populated by the parse pass for named struct types: the `ScopeKind::Type`
    /// scope that holds this struct's inline type definitions.
    pub scope:  Option<AstScopeId>,
}

// ---------------------------------------------------------------------------
// Struct items & link definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstStructItem {
    /// New syntax: `name = type_expr` or `= type_expr` (anonymous)
    Field(AstNode<AstFieldDef>),
    /// New syntax: `def name = type_expr` — nested named type inside a struct body
    Def(AstNode<AstTopDef>),
    /// Legacy (kept for backward compat while resolver migrates): `type name = …`
    TypeDef(AstNode<AstTypeDef>),
    /// Legacy (kept for backward compat while resolver migrates): `link name = …`
    LinkDef(AstNode<AstLinkDef>),
}

/// A link slot: `link name? = type-expr`
/// Anonymous links enumerate refs for composition; named links are typed fields.
#[derive(Debug, Clone, PartialEq)]
pub struct AstLinkDef {
    pub name: Option<AstNode<String>>,
    pub ty:   AstNode<AstTypeDef>,
}

// ---------------------------------------------------------------------------
// Instances & values
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstValue {
    Str(String),
    /// Integers are left as single-segment refs; the resolve pass interprets them.
    Ref(AstRef),
    List(Vec<AstNode<AstValue>>),
    /// Inline struct literal: `{ field: value ... }`
    /// `type_hint` is present when written as `type:scaling { ... }`.
    Struct { type_hint: Option<AstNode<AstRef>>, fields: Vec<AstNode<AstField>> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstField {
    Named { name: AstNode<String>, value: AstNode<AstValue>, via: bool },
    /// Anonymous value (only valid inside `inst`, not `deploy`)
    Anon(AstNode<AstValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstInst {
    pub type_name: AstNode<AstRef>,
    pub inst_name: AstNode<String>,
    pub fields:    Vec<AstNode<AstField>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AstScopeId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum ScopeKind { Pack, Type }

#[derive(Debug, Clone, PartialEq)]
pub struct AstScope {
    pub kind:   ScopeKind,
    pub name:   Option<AstNode<String>>,
    pub parent: Option<AstScopeId>,
    pub defs:   Vec<AstDef>,
}

// ---------------------------------------------------------------------------
// Use statements
// ---------------------------------------------------------------------------

/// `use <ref>` — imports names from another pack into the current scope.
/// The `path` ref is parsed using the standard ref grammar; `*` is allowed
/// as a wildcard terminal segment.
#[derive(Debug, Clone, PartialEq)]
pub struct AstUse {
    pub path: AstRef,
}

// ---------------------------------------------------------------------------
// Top-level definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstDef {
    /// New syntax: `name = type_expr`, `def name …`, bare `name` / `def name`
    Def(AstNode<AstTopDef>),
    /// New syntax: `pack ref { … }` namespace declaration
    Pack(AstNode<AstPack>),
    /// New syntax: `plan name { … }` resolution trigger
    Plan(AstNode<AstPlan>),
    /// Legacy (kept for resolver backward compat): `type name = …`
    Type(AstNode<AstTypeDef>),
    /// Legacy (kept for resolver backward compat): `link name = …`
    Link(AstNode<AstLinkDef>),
    Inst(AstNode<AstInst>),
    Scope(AstNode<AstScope>),
    Use(AstNode<AstUse>),
}

// ---------------------------------------------------------------------------
// Parse request / response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AstParseError {
    pub message: String,
    pub loc:     AstNodeLoc,
}

/// One source file. `path` is the chain of parent namespace names (from folder
/// structure); `name` is the leaf namespace for this file's contents.
#[derive(Debug)]
pub struct ParseUnit {
    pub name: String,
    pub path: Vec<String>,
    pub src:  String,
}

#[derive(Debug)]
pub struct ParseReq {
    pub units: Vec<ParseUnit>,
}

/// Flat scope arena. `scopes[0]` is the synthetic root scope (unnamed, no parent).
#[derive(Debug)]
pub struct ParseRes {
    pub scopes: Vec<AstScope>,
    pub errors: Vec<AstParseError>,
}
