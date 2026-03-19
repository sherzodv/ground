/// Resolved IR — output of the resolve pass over `ParseRes`.
///
/// All symbolic names are replaced by typed indices into flat arenas.
/// After this pass no string-based symbol lookup is needed by any consumer.
///
///   `TypeId`  → `IrRes::types[id.0]`
///   `LinkId`  → `IrRes::links[id.0]`
///   `InstId`  → `IrRes::insts[id.0]`
///   `ScopeId` → `IrRes::scopes[id.0]`  (`ScopeId(0)` is always the root)

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Typed indices
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

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
    Inst(InstId),
    /// Could not be resolved in lexical scope — kept verbatim.
    Plain(String),
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScopeKind { Pack, Type }

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
    pub insts:     HashMap<String, InstId>,
    pub packs:     HashMap<String, ScopeId>,
    pub ambiguous: std::collections::HashSet<String>,
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
    Enum(Vec<String>),         // variant names, order preserved
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
// Links
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

#[derive(Debug, Clone)]
pub enum IrValue {
    Str(String),
    Int(i64),
    Ref(String),               // reference primitive (opaque)
    Variant(TypeId, u32),      // enum type + variant index
    Inst(InstId),
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
    pub value:   IrValue,
}

#[derive(Debug, Clone)]
pub struct IrInstDef {
    pub type_id: TypeId,
    pub name:    String,
    pub scope:   ScopeId,
    pub loc:     IrLoc,
    pub fields:  Vec<IrField>,
}

#[derive(Debug, Clone)]
pub struct IrDeployDef {
    pub what:   IrRef,
    pub target: IrRef,
    pub name:   IrRef,
    pub loc:    IrLoc,
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
    pub types:   Vec<IrTypeDef>,
    pub links:   Vec<IrLinkDef>,
    pub insts:   Vec<IrInstDef>,
    pub deploys: Vec<IrDeployDef>,
    pub scopes:  Vec<IrScope>,
    pub errors:  Vec<IrError>,
}
