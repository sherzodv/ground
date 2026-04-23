/// Resolved IR — output of the resolve pass over `ParseRes`.
///
/// All symbolic names are replaced by typed indices into flat arenas.
/// After this pass no string-based symbol lookup is needed by any consumer.
///
///   `ShapeId`  → `IrRes::shapes[id.0]`
///   `DefId`    → `IrRes::defs[id.0]` (resolved Ground def)
///   `ScopeId`  → `IrRes::scopes[id.0]`  (`ScopeId(0)` is always the root)
use std::collections::{HashMap, HashSet};

pub use crate::ast::UnitId;

// ---------------------------------------------------------------------------
// Typed indices
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct IrLoc {
    pub unit: UnitId,
    pub start: u32,
    pub end: u32,
}

// ---------------------------------------------------------------------------
// Refs — the core resolved reference type
// ---------------------------------------------------------------------------

/// A resolved ref: flat list of resolved-or-plain segments.
/// Keywords (`pack` / `def`) are consumed during resolution and not stored.
#[derive(Debug, Clone, PartialEq)]
pub struct IrRef {
    pub segments: Vec<IrRefSeg>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrRefSeg {
    pub value: IrRefSegValue,
    pub is_opt: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrRefSegValue {
    Pack(ScopeId),
    Shape(ShapeId),
    Def(DefId),
    /// Could not be resolved in lexical scope — kept verbatim.
    Plain(String),
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScopeKind {
    Pack,
    Struct,
}

/// `ScopeId(0)` is always the root scope.
/// `ambiguous` tracks names that have been marked as conflicting (same-kind duplicates
/// from imports or local-vs-import collisions). Lookups for ambiguous names return None
/// and stop parent-chain traversal; callers must use a qualified prefix to disambiguate.
#[derive(Debug, Clone)]
pub struct IrScope {
    pub kind: ScopeKind,
    pub name: Option<String>,
    pub parent: Option<ScopeId>,
    pub shapes: HashMap<String, ShapeId>,
    pub defs: HashMap<String, Vec<DefId>>,
    pub packs: HashMap<String, ScopeId>,
    pub ambiguous: HashSet<String>,
    /// TypeScript function names exported from the co-located `.ts` file
    /// in this pack scope.
    pub ts_fns: HashSet<String>,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum IrPrimitive {
    String,
    Integer,
    Boolean,
    Reference,
    Ipv4,
    Ipv4Net,
}

/// What a named type IS.
#[derive(Debug, Clone)]
pub enum IrShapeBody {
    Unit,
    Primitive(IrPrimitive),
    Enum(Vec<IrRef>), // variant refs, order preserved; plain atom → Plain seg, typed → Shape seg
    Struct(Vec<IrStructFieldDef>), // ordered fields owned by the shape
}

#[derive(Debug, Clone)]
pub struct IrShapeDef {
    pub name: Option<String>, // None for anonymous inline shapes
    pub scope: ScopeId,
    pub loc: IrLoc,
    pub body: IrShapeBody,
}

// ---------------------------------------------------------------------------
// Field shapes
// ---------------------------------------------------------------------------

/// What a field ACCEPTS — its resolved type expression.
#[derive(Debug, Clone)]
pub enum IrFieldType {
    Primitive(IrPrimitive),
    Ref(IrRef),       // resolved type ref: single type, enum, or typed path
    List(Vec<IrRef>), // [ type1:(opt) | type2 ] — one IrRef per element pattern
    Optional(Box<IrFieldType>),
}

#[derive(Debug, Clone)]
pub struct IrStructFieldDef {
    pub name: Option<String>,
    pub field_type: IrFieldType,
}

// ---------------------------------------------------------------------------
// Values  (instance fields — validated against IrFieldType patterns)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum IrValue {
    Str(String),
    Int(i64),
    Bool(bool),
    Ref(String), // reference primitive (opaque) OR unresolved param ref like "{this:name}-sg"
    Variant(ShapeId, u32, Option<Box<IrValue>>), // enum shape + variant index + optional typed payload
    Inst(DefId),
    Path(Vec<IrValue>), // multi-segment typed path
    List(Vec<IrValue>), // list of validated values
}

// ---------------------------------------------------------------------------
// Instances & deploys
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IrField {
    pub field_idx: u32,
    pub name: String,
    pub loc: IrLoc,
    pub via: bool, // true → pass pre-resolved (post-mapper) value to enclosing mapper
    pub value: IrValue,
}

/// A named mapping instance — unified representation for both root definitions and instances.
///
/// `def boo`  → `base_def: None`,           `shape_id`: boo's own ShapeId
/// `boo b`    → `base_def: Some(boo_id)`,   `shape_id`: boo's ShapeId
/// `{ ... }`  → anonymous inline def with a struct `shape_id`, `name`: "_"
///
/// Mapper fields are only populated for root definitions (`base_def.is_none()`) that
/// carry a TypeScript transformation: `def name { inputs } = mapper_fn { outputs }`.
#[derive(Debug, Clone)]
pub struct IrDef {
    pub planned: bool, // true when declared with `plan`
    pub shape_id: ShapeId,
    pub base_def: Option<DefId>, // None → root def; Some → derived def/anonymous
    pub name: String,
    pub type_hint: Option<String>, // explicit type annotation from source, if present
    pub scope: ScopeId,
    pub loc: IrLoc,
    pub fields: Vec<IrField>,
    pub mapper_fn: Option<String>, // TS function name; None for non-mapper defs
    pub inputs: Vec<IrStructFieldDef>, // input fields (before `=` in mapper def)
    pub outputs: Vec<IrStructFieldDef>, // output fields (after `=` in mapper def)
}

// ---------------------------------------------------------------------------
// Errors & result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IrError {
    pub message: String,
    pub loc: IrLoc,
}

#[derive(Debug)]
pub struct IrRes {
    pub shapes: Vec<IrShapeDef>,
    pub defs: Vec<IrDef>,
    pub scopes: Vec<IrScope>,
    pub errors: Vec<IrError>,
}
