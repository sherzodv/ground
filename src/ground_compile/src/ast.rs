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
// Unified def node — the central construct of the language
// ---------------------------------------------------------------------------

/// A named def — covers type aliases, bare unit types, hook transformations, and instances.
///
/// Forms:
///   `name = type_expr`                         — type alias
///   `def name`                                 — bare entity def
///   `def name { input } = { output }`          — def with canonical hook name = `name`
///   `def name { input } = hookname { output }` — def with explicit hook name
///   `name = hook_ref { output }`               — def with explicit hook and output body
///   `name hook { output }`                     — shorthand def with explicit hook
///   `name hook`                                — shorthand def with explicit hook and unit output
#[derive(Debug, Clone, PartialEq)]
pub struct AstDef {
    pub name:    AstNode<String>,
    pub input:   Vec<AstNode<AstDefI>>,        // fields before `=`; empty for simple defs
    pub hook:    AstNode<AstRef>,              // canonical hook ref; defaults to the def name
    pub output:  AstNode<AstDefO>,
}

/// Output side of a def — what appears after `=`.
#[derive(Debug, Clone, PartialEq)]
pub enum AstDefO {
    Unit,                                     // bare `def name` or `name` with no `=`
    TypeExpr(AstNode<AstTypeDef>),            // `= type_expr`
    Struct(Vec<AstNode<AstStructItem>>),      // `= hook? { struct_items }`
}

/// Input field declaration inside a def input block or struct output body.
#[derive(Debug, Clone, PartialEq)]
pub struct AstDefI {
    pub name: Option<AstNode<String>>,        // None for anonymous `= type_expr`
    pub ty:   AstNode<AstTypeDef>,
}

// ---------------------------------------------------------------------------
// Pack and Plan declarations
// ---------------------------------------------------------------------------

/// `pack ref` or `pack ref { defs... }` — namespace declaration (like Scala packages).
#[derive(Debug, Clone, PartialEq)]
pub struct AstPack {
    pub path: AstNode<AstRef>,
    pub defs: Vec<AstItem>,                   // empty for bare file-level `pack std:aws`
}

/// `plan name` or `plan name { fields }` — resolution trigger.
#[derive(Debug, Clone, PartialEq)]
pub struct AstPlan {
    pub name:   AstNode<String>,
    pub fields: Vec<AstNode<AstField>>,
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
    /// Struct body: `{ field … }` — field items
    Struct(Vec<AstNode<AstStructItem>>),
    /// List whose element type is described by the inner `AstTypeDef`: `[ … ]`
    List(Box<AstNode<AstTypeDef>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstTypeDef {
    pub name:   Option<AstNode<String>>,
    pub body:   AstNode<AstTypeDefBody>,
    /// Populated by the parse pass for named struct types: the `ScopeKind::Type`
    /// scope that holds this struct's inline type definitions.
    pub scope:  Option<AstScopeId>,
}

// ---------------------------------------------------------------------------
// Struct items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstStructItem {
    /// `name = type_expr`, `= type_expr`, or `name: value`
    Field(AstNode<AstStructField>),
    /// Anonymous value item inside a struct body.
    Anon(AstNode<AstValue>),
    /// `def name = type_expr` — nested named def inside a struct body
    Def(AstNode<AstDef>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstStructFieldKind {
    Def,
    Set,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstStructFieldBody {
    Type(AstNode<AstTypeDef>),
    Value(AstNode<AstValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstStructField {
    pub name: Option<AstNode<String>>,
    pub kind: AstStructFieldKind,
    pub body: AstStructFieldBody,
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


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AstScopeId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum ScopeKind { Pack, Type }

#[derive(Debug, Clone, PartialEq)]
pub struct AstScope {
    pub kind:   ScopeKind,
    pub name:   Option<AstNode<String>>,
    pub parent: Option<AstScopeId>,
    pub defs:   Vec<AstItem>,
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
// Top-level scope items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstItem {
    /// `name = …`, `def name …`, bare `name` / `def name`, `name = type { values }`
    Def(AstNode<AstDef>),
    /// `pack ref { … }` namespace declaration
    Pack(AstNode<AstPack>),
    /// `plan name { … }` resolution trigger
    Plan(AstNode<AstPlan>),
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
/// `ts_src` is the co-located TypeScript source (same pack scope as `src`).
#[derive(Debug)]
pub struct ParseUnit {
    pub name:   String,
    pub path:   Vec<String>,
    pub src:    String,
    pub ts_src: Option<String>,
}

#[derive(Debug)]
pub struct ParseReq {
    pub units: Vec<ParseUnit>,
}

/// Flat scope arena. `scopes[0]` is the synthetic root scope (unnamed, no parent).
/// `unit_scope_ids[i]` is the leaf pack `AstScopeId` for `ParseReq::units[i]`.
/// `unit_ts_srcs[i]` is the TypeScript source for `ParseReq::units[i]` (if any).
/// Both vecs have the same length as `ParseReq::units`.
#[derive(Debug)]
pub struct ParseRes {
    pub scopes:         Vec<AstScope>,
    pub errors:         Vec<AstParseError>,
    pub unit_scope_ids: Vec<AstScopeId>,
    pub unit_ts_srcs:   Vec<Option<String>>,
}
