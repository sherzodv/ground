/// Ground parser level AST  — every node is wrapped in `AstNode<T>` carrying byte-offset location.

// ---------------------------------------------------------------------------
// Ids
// ---------------------------------------------------------------------------

/// Opaque handle for a source unit within a single compilation.
/// Assigned by `parse()` in input order — `UnitId(i)` refers to `ParseReq::units[i]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitId(pub u32);

impl UnitId {
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

// ---------------------------------------------------------------------------
// Location & node wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AstNodeLoc {
    pub unit: UnitId,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode<T> {
    pub loc: AstNodeLoc,
    pub inner: T,
}

impl<T> AstNode<T> {
    pub fn new(unit: UnitId, start: u32, end: u32, inner: T) -> Self {
        AstNode {
            loc: AstNodeLoc { unit, start, end },
            inner,
        }
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
    pub value: AstRefSegVal,
    pub is_opt: bool,
}

impl AstRefSeg {
    /// Returns the plain string for Plain segments; None for Group segments.
    pub fn as_plain(&self) -> Option<&str> {
        match &self.value {
            AstRefSegVal::Plain(s) => Some(s.as_str()),
            AstRefSegVal::Group(..) => None,
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
pub enum AstPrimitive {
    String,
    Integer,
    Boolean,
    Reference,
    Ipv4,
    Ipv4Net,
}

// ---------------------------------------------------------------------------
// Unified def node — the central construct of the language
// ---------------------------------------------------------------------------

/// A named mapping — covers type aliases, bare unit shapes, mapper-backed transforms, and instances.
///
/// Forms:
///   `name = type_expr`                         — type alias
///   `def name`                                 — bare entity def
///   `def name { input } = { output }`          — def with implicit mapper name = `name`
///   `def name { input } = mapper { output }`   — def with explicit mapper name
///   `name = mapper_ref { output }`             — def with explicit mapper and output body
///   `name mapper { output }`                   — shorthand def with explicit mapper
///   `name mapper`                              — shorthand def with explicit mapper and unit output
#[derive(Debug, Clone, PartialEq)]
pub struct AstDef {
    pub planned: bool, // true when declared with `plan`
    pub name: AstNode<String>,
    pub input: Vec<AstNode<AstDefI>>, // fields before `=`; empty for simple defs
    pub mapper: Option<AstNode<AstRef>>, // explicit mapper ref when it appears in source
    pub output: Option<AstNode<AstDefO>>,
}

/// Output side of a def — what appears after `=`, when an output body or type is
/// explicitly present. `None` means no output body was written.
#[derive(Debug, Clone, PartialEq)]
pub enum AstDefO {
    TypeExpr(AstNode<AstTypeExpr>),      // `= type_expr`
    Struct(Vec<AstNode<AstStructItem>>), // `= mapper? { struct_items }`
}

/// Input field declaration inside a def input block or struct output body.
#[derive(Debug, Clone, PartialEq)]
pub struct AstDefI {
    pub name: Option<AstNode<String>>, // None for anonymous `= type_expr`
    pub ty: AstNode<AstTypeExpr>,
}

// ---------------------------------------------------------------------------
// Pack declarations
// ---------------------------------------------------------------------------

/// `pack ref` or `pack ref { defs... }` — namespace declaration (like Scala packages).
#[derive(Debug, Clone, PartialEq)]
pub struct AstPack {
    pub path: AstNode<AstRef>,
    pub defs: Option<Vec<AstItem>>, // None for bare file-level `pack std:aws`
}

// ---------------------------------------------------------------------------
// Type definitions — the universal type expression
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstTypeExpr {
    /// Unit: bare `def name` or `name` with no rhs
    Unit,
    /// Built-in scalar: `string` | `integer` | `boolean` | `reference` | `ipv4` | `ipv4net`
    Primitive(AstPrimitive),
    /// Single reference to an existing type: `postgresql`, `type:region:type:zone`
    Ref(AstRef),
    /// Union of refs: `self | provider | cloud`
    Enum(Vec<AstNode<AstRef>>),
    /// Struct body: `{ field … }` — field items
    Struct(Vec<AstNode<AstStructItem>>),
    /// List whose element type is described by the inner `AstTypeExpr`: `[ … ]`
    List(Box<AstNode<AstTypeExpr>>),
    /// Optional field type: `( type_expr )`
    Optional(Box<AstNode<AstTypeExpr>>),
}

// ---------------------------------------------------------------------------
// Struct items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AstComment {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstStructItem {
    /// `name = type_expr`, `= type_expr`, or `name: value`
    Field(AstNode<AstStructField>),
    /// Anonymous value item inside a struct body.
    Anon(AstNode<AstValue>),
    /// Temporary nested named def inside a struct body; the parse pass hoists these
    /// into an adjacent `ScopeKind::Struct` scope before returning the final AST.
    Def(AstNode<AstDef>),
    Comment(AstNode<AstComment>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstStructFieldKind {
    Def,
    Set,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstStructFieldBody {
    Type(AstNode<AstTypeExpr>),
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
    Struct {
        type_hint: Option<AstNode<AstRef>>,
        fields: Vec<AstNode<AstField>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstField {
    Named {
        name: AstNode<String>,
        value: AstNode<AstValue>,
        via: bool,
    },
    /// Anonymous value (only valid inside `inst`, not `deploy`)
    Anon(AstNode<AstValue>),
    Comment(AstNode<AstComment>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AstScopeId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum ScopeKind {
    Pack,
    Struct,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstScope {
    pub kind: ScopeKind,
    pub name: Option<AstNode<String>>,
    pub parent: Option<AstScopeId>,
    pub defs: Vec<AstItem>,
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
    Use(AstNode<AstUse>),
    Comment(AstNode<AstComment>),
}

// ---------------------------------------------------------------------------
// Parse request / response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AstParseError {
    pub message: String,
    pub loc: AstNodeLoc,
}

/// One source file. `path` is the chain of parent namespace names (from folder
/// structure); `name` is the leaf namespace for this file's contents.
/// `ts_src` is the co-located TypeScript source (same pack scope as `src`).
#[derive(Debug)]
pub struct ParseUnit {
    pub name: String,
    pub path: Vec<String>,
    pub declared_pack: Option<Vec<String>>,
    pub src: String,
    pub ts_src: Option<String>,
}

#[derive(Debug)]
pub struct ParseReq {
    pub units: Vec<ParseUnit>,
}

/// Per-unit parse output. `ParseRes::units[UnitId.as_usize()]` is everything
/// the rest of the compiler needs to know about one input unit.
#[derive(Debug, Clone)]
pub struct ParseUnitRes {
    pub scope_id: AstScopeId,
    pub ts_src: Option<String>,
}

/// Flat scope arena. `scopes[0]` is the synthetic root scope (unnamed, no parent).
/// `units[UnitId.as_usize()]` has per-unit output, in input order.
#[derive(Debug, Clone)]
pub struct ParseRes {
    pub scopes: Vec<AstScope>,
    pub errors: Vec<AstParseError>,
    pub units: Vec<ParseUnitRes>,
}
