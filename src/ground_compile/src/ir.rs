/// Resolved IR — output of the resolve pass over `ParseRes`.
///
/// All symbolic names are replaced by typed indices into flat arenas.
/// After this pass no string-based symbol lookup is needed by any consumer.
///
///   `TypeId`   → `IrRes::types[id.0]`
///   `LinkId`   → `IrRes::links[id.0]`
///   `FunId`    → `IrRes::funs[id.0]` (resolved mapping entry)
///   `ScopeId`  → `IrRes::scopes[id.0]`  (`ScopeId(0)` is always the root)
///   `TypeFnId` → `IrRes::type_fns[id.0]`

use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Typed indices
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeFnId(pub u32);

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct IrLoc {
    pub unit:  u32,
    pub start: u32,
    pub end:   u32,
}

// ---------------------------------------------------------------------------
// Refs — the core resolved reference type
// ---------------------------------------------------------------------------

/// A resolved ref: flat list of resolved-or-plain segments.
/// Keywords (`pack` / `type` / `link`) are consumed during resolution and not stored.
#[derive(Debug, Clone, PartialEq)]
pub struct IrRef {
    pub segments: Vec<IrRefSeg>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrRefSeg {
    pub value:  IrRefSegValue,
    pub is_opt: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrRefSegValue {
    Pack(ScopeId),
    Type(TypeId),
    Link(LinkId),
    Inst(FunId),
    /// Could not be resolved in lexical scope — kept verbatim.
    Plain(String),
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScopeKind { Pack, Struct }

/// `ScopeId(0)` is always the root scope.
/// Separate maps per kind so types and links can share names in the same scope.
/// `ambiguous` tracks names that have been marked as conflicting (same-kind duplicates
/// from imports or local-vs-import collisions). Lookups for ambiguous names return None
/// and stop parent-chain traversal; callers must use a qualified prefix to disambiguate.
#[derive(Debug, Clone)]
pub struct IrScope {
    pub kind:      ScopeKind,
    pub name:      Option<String>,
    pub parent:    Option<ScopeId>,
    pub types:     HashMap<String, TypeId>,
    pub links:     HashMap<String, LinkId>,
    pub funs:      HashMap<String, Vec<FunId>>,
    pub packs:     HashMap<String, ScopeId>,
    /// Named type functions in this scope (1-param or N-param), keyed by function name.
    pub type_fns:      HashMap<String, TypeFnId>,
    /// Anonymous 1-param type functions, keyed by the param's TypeId.
    pub anon_type_fns: HashMap<TypeId, TypeFnId>,
    /// Anonymous 2-param pair functions, keyed by (from TypeId, to TypeId).
    pub anon_pair_fns: HashMap<(TypeId, TypeId), TypeFnId>,
    pub ambiguous:     HashSet<String>,
    /// TypeScript function names exported from the co-located `.ts` file
    /// (same pack scope). Cross-pack imports via `use pack:fn` or `use pack:*`
    /// copy names here.
    pub ts_fns:        HashSet<String>,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum IrPrimitive { String, Integer, Reference }

/// What a named type IS.
#[derive(Debug, Clone)]
pub enum IrTypeBody {
    Primitive(IrPrimitive),
    Enum(Vec<IrRef>),          // variant refs, order preserved; plain atom → Plain seg, typed → Type seg
    Struct(Vec<LinkId>),       // ordered named links; inline types are hoisted
}

#[derive(Debug, Clone)]
pub struct IrTypeDef {
    pub name:  Option<String>, // None for anonymous inline types
    pub scope: ScopeId,
    pub loc:   IrLoc,
    pub body:  IrTypeBody,
}

// ---------------------------------------------------------------------------
// Type function definitions
// ---------------------------------------------------------------------------

/// One parameter of a type function: `name: TypeId`
#[derive(Debug, Clone, PartialEq)]
pub struct IrTypeFnParam {
    pub name: String,
    pub ty:   TypeId,
}

/// One entry in a type function body: `alias: vendor_type { fields... }`
#[derive(Debug, Clone, PartialEq)]
pub struct IrTypeFnEntry {
    pub alias:       String,
    pub vendor_type: TypeId,
    pub fields:      Vec<IrFnBodyField>,
}

/// A single field assignment in a type function entry body.
/// Values use ordinary `IrValue` — group refs that reference params (e.g. `{this:name}`)
/// are stored as `IrValue::Ref("{this:name}")` strings and substituted at ASM expand time.
#[derive(Debug, Clone, PartialEq)]
pub struct IrFnBodyField {
    pub name:  String,
    pub value: IrValue,
}

/// A resolved type function definition.
/// - `params.len() == 1` → 1-param type function (fires per matching instance)
/// - `params.len() == 2` → 2-param pair function (fires per (from, to) pair)
/// - `name.is_none()` → anonymous (auto-fires during walk)
/// - `name.is_some()` → named (explicit application via deploy `to` target or override)
#[derive(Debug, Clone)]
pub struct IrTypeFnDef {
    pub name:   Option<String>,
    pub params: Vec<IrTypeFnParam>,
    pub scope:  ScopeId,
    pub loc:    IrLoc,
    pub body:   Vec<IrTypeFnEntry>,
}

// ---------------------------------------------------------------------------
// Link types
// ---------------------------------------------------------------------------

/// What a link ACCEPTS — its resolved type expression.
#[derive(Debug, Clone)]
pub enum IrLinkType {
    Primitive(IrPrimitive),
    Ref(IrRef),                // resolved type ref: single type, enum, or typed path
    List(Vec<IrRef>),          // [ type1:(opt) | type2 ] — one IrRef per element pattern
}

#[derive(Debug, Clone)]
pub struct IrLinkDef {
    pub name:      Option<String>,
    pub scope:     ScopeId,
    pub loc:       IrLoc,
    pub link_type: IrLinkType,
}

// ---------------------------------------------------------------------------
// Values  (instance fields — validated against IrLinkType patterns)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum IrValue {
    Str(String),
    Int(i64),
    Ref(String),               // reference primitive (opaque) OR unresolved param ref like "{this:name}-sg"
    Variant(TypeId, u32, Option<Box<IrValue>>), // enum type + variant index + optional typed payload
    Inst(FunId),
    Path(Vec<IrValue>),        // multi-segment typed path
    List(Vec<IrValue>),        // list of validated values
}

// ---------------------------------------------------------------------------
// Instances & deploys
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IrField {
    pub link_id: LinkId,
    pub name:    String,
    pub loc:     IrLoc,
    pub via:     bool,   // true → pass pre-resolved (post-mapper) value to enclosing mapper
    pub value:   IrValue,
}

/// A named mapping instance — unified representation for both root definitions and instances.
///
/// `def boo`  → `parent: None`,          `type_id`: boo's own TypeId
/// `boo b`    → `parent: Some(boo_id)`,  `type_id`: boo's TypeId
/// `{ ... }`  → `parent: Some(type_id)`, `type_id`: the struct TypeId, `name`: "_"
///
/// Mapper fields are only populated for root definitions (`parent.is_none()`) that
/// carry a TypeScript transformation: `def name { inputs } = mapper_fn { outputs }`.
#[derive(Debug, Clone)]
pub struct IrFunDef {
    pub type_id:   TypeId,
    pub parent:    Option<TypeId>,       // None → root def; Some → instance/anonymous
    pub name:      String,
    pub type_hint: Option<String>,       // explicit type annotation from source, if present
    pub scope:     ScopeId,
    pub loc:       IrLoc,
    pub fields:    Vec<IrField>,
    pub mapper_fn: Option<String>,       // TS function name; None for non-mapper defs
    pub inputs:    Vec<LinkId>,          // input links (before `=` in mapper def)
    pub outputs:   Vec<LinkId>,          // output links (after `=` in mapper def)
}

// ---------------------------------------------------------------------------
// Plan declarations (new: `plan name`)
// ---------------------------------------------------------------------------

/// A `plan name` declaration — triggers Terraform generation for instance `name`.
#[derive(Debug, Clone)]
pub struct IrPlanDef {
    pub name:   String,
    pub loc:    IrLoc,
    /// Fields provided directly on the `plan` declaration (e.g. `plan prd-eu { region: [...] }`).
    /// These supplement or override the named deploy instance's own fields at generation time.
    pub fields: Vec<IrField>,
}

// ---------------------------------------------------------------------------
// Errors & result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IrError {
    pub message: String,
    pub loc:     IrLoc,
}

#[derive(Debug)]
pub struct IrRes {
    pub types:    Vec<IrTypeDef>,
    pub links:    Vec<IrLinkDef>,
    pub funs:     Vec<IrFunDef>,
    pub plans:    Vec<IrPlanDef>,
    pub type_fns: Vec<IrTypeFnDef>,
    pub scopes:   Vec<IrScope>,
    pub errors:   Vec<IrError>,
}
