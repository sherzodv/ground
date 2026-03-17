# RFC 0006 — Resolve Pass

## Context

After `parse2` produces a merged `ParseRes` (flat scope arena across all `ParseUnit`s), all
refs and names are still raw strings. The resolve pass walks the AST and produces a fully
validated, name-resolved representation.

## Scope

Strictly: `parse2` → `ParseRes` → resolve → `IrRes`.
Out of scope: backend codegen, deploy execution.

## Decisions

- **Scope lookup**: lexical — walk current scope → parent chain only.
- **Cross-unit refs**: not implicitly visible; a ref to another unit's type must use the full
  scope path (e.g. `pack:infra:type:database`).
- **Error handling**: accumulate all errors, continue resolving as far as possible.

## What needs to be resolved / verified

1. **Name uniqueness** — within each scope no two defs share the same name (types, links, instances).
2. **Type ref resolution** — `AstTypeDefBody::Ref` is a raw string; resolve it via lexical scope lookup.
3. **Enum variant binding** — when a value is assigned to an enum-typed link the value must be a valid variant of that enum.
4. **Link type resolution** — each `AstLinkDef.ty` is an unresolved type expression; resolve it to `IrLinkType`.
5. **Instance type resolution** — `AstInst.type_name` must resolve to a struct-kinded type in scope.
6. **Field name validation** — each field name in `AstInst.fields` must match a named link in the resolved struct type.
7. **Value resolution** — `AstValue::Ref` segments are resolved and validated against the link's `IrLinkType` pattern.
8. **Deploy ref resolution** — `AstDeploy.what`, `.target`, `.name` are resolved as `IrRef`s.

---

## Ref Resolution Rules

### Keywords

`pack`, `type`, `link` are keywords when they appear as ref segments. They act as kind-filters:
the following segment is looked up in the lexical scope restricted to that kind.

```
pack:infra       → find Scope  named "infra"  in lexical scope
type:database    → find Type   named "database" in lexical scope
link:access      → find Link   named "access" in lexical scope
```

Non-keyword segments are looked up as any symbol in the lexical scope.

### In definitions (`=` right-hand side)

Refs are resolved **segment by segment, always in the lexical scope** (from use-site, walking
up parent chain). Each segment resolves independently — there is no scope navigation between
segments. A segment that cannot be resolved in the lexical scope is kept as `Plain(String)`.

```
type:region:type:zone
  → [Type(region_id), Type(zone_id)]

service:(port)                         # in a list element type
  → [Type(service_id), Plain("port")]  # "port" not in lexical scope

pack:infra:type:database
  → [Scope(infra_id), Type(database_id)]
```

The result is an `IrRef` — a flat list of resolved-or-plain segments.

### In instances (field values)

Field value refs are **validated against the link def's `IrRef` pattern**. The link def's
resolved `IrRef` acts as a type pattern: each segment of the value ref is checked against the
corresponding segment of the pattern.

```
link zone    = type:region:type:zone   → pattern [Type(region_id), Type(zone_id)]
field zone: eu-central:3
  → segment "eu-central" validated as variant of region → Variant(region_id, 0)
  → segment "3"          validated as variant of zone   → Variant(zone_id, 2)
  → IrValue::Path([Variant(region_id,0), Variant(zone_id,2)])

link engine = postgresql | mongodb     → pattern [Type(anon_enum_id)]
field engine: postgresql
  → segment "postgresql" validated as variant of engine → Variant(anon_enum_id, 0)

link scaling = type scaling = { ... } → pattern [Type(scaling_id)]
field scaling: my-scaling-inst
  → segment "my-scaling-inst" validated as instance of scaling → Inst(inst_id)

link access = [ service:(port) | database ]
field access: [ svc-a:grpc  db-1 ]
  → each list item matched against element patterns:
      "svc-a:grpc" → Inst(svc-a_id) + Plain("grpc")
      "db-1"       → Inst(db-1_id)
```

---

## IR AST

All nodes live in flat arenas; every cross-reference uses a typed numeric ID.

```rust
// IDs
pub struct TypeId(pub u32);
pub struct LinkId(pub u32);
pub struct InstId(pub u32);
pub struct ScopeId(pub u32);

pub struct IrLoc { pub unit: u32, pub start: u32, pub end: u32 }

// ---------------------------------------------------------------------------
// Refs — the core resolved reference type
// ---------------------------------------------------------------------------

/// A resolved ref: flat list of resolved-or-plain segments.
/// Keywords (pack/type/link) are consumed during resolution and not stored.
pub struct IrRef {
    pub segments: Vec<IrRefSeg>,
}

pub struct IrRefSeg {
    pub value:  IrRefSegValue,
    pub is_opt: bool,           // true when written as (ident) in source
}

pub enum IrRefSegValue {
    Pack(ScopeId),
    Type(TypeId),
    Link(LinkId),
    Inst(InstId),
    Plain(String),              // could not be resolved in lexical scope
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

pub struct IrScope {
    pub kind:    ScopeKind,     // Pack | Type
    pub name:    Option<String>,
    pub parent:  Option<ScopeId>,
    pub symbols: HashMap<String, IrSymbol>,
}

pub enum IrSymbol {
    Pack(ScopeId),
    Type(TypeId),
    Link(LinkId),
    Inst(InstId),
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub enum IrPrimitive { String, Integer, Reference }

/// What a named type IS.
pub enum IrTypeBody {
    Primitive(IrPrimitive),
    Enum(Vec<String>),          // variant names, order preserved
    Struct(Vec<LinkId>),        // ordered named links; inline types are hoisted
}

pub struct IrTypeDef {
    pub name:  Option<String>,  // None for anonymous inline types
    pub scope: ScopeId,
    pub loc:   IrLoc,
    pub body:  IrTypeBody,
}

// ---------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------

/// What a link ACCEPTS — its resolved type expression.
pub enum IrLinkType {
    Primitive(IrPrimitive),     // string | integer | reference
    Ref(IrRef),                 // resolved type ref: single type, enum, or typed path
    List(Vec<IrRef>),           // [ type1:(opt) | type2 ] — one IrRef per element pattern
}

pub struct IrLinkDef {
    pub name:      Option<String>,
    pub scope:     ScopeId,
    pub loc:       IrLoc,
    pub link_type: IrLinkType,
}

// ---------------------------------------------------------------------------
// Values  (instance fields — validated against IrLinkType patterns)
// ---------------------------------------------------------------------------

pub enum IrValue {
    Str(String),                          // string literal
    Int(i64),                             // integer primitive
    Ref(String),                          // reference primitive (opaque string)
    Variant(TypeId, u32),                 // enum type + variant index
    Inst(InstId),                         // instance of a struct type
    Path(Vec<IrValue>),                   // multi-segment typed path
    List(Vec<IrValue>),                   // list of validated values
}

// ---------------------------------------------------------------------------
// Instances & deploys
// ---------------------------------------------------------------------------

pub struct IrField {
    pub link_id: LinkId,
    pub name:    String,
    pub loc:     IrLoc,
    pub value:   IrValue,
}

pub struct IrInstDef {
    pub type_id: TypeId,
    pub name:    String,
    pub scope:   ScopeId,
    pub loc:     IrLoc,
    pub fields:  Vec<IrField>,
}

pub struct IrDeployDef {
    pub what:   IrRef,          // resolved ref to what is being deployed
    pub target: IrRef,          // resolved ref to deployment target
    pub name:   IrRef,          // resolved ref for deployment name
    pub loc:    IrLoc,
    pub fields: Vec<IrField>,
}

// ---------------------------------------------------------------------------
// Errors & result
// ---------------------------------------------------------------------------

pub struct IrError {
    pub message: String,
    pub loc:     IrLoc,
}

pub struct IrRes {
    pub types:   Vec<IrTypeDef>,
    pub links:   Vec<IrLinkDef>,
    pub insts:   Vec<IrInstDef>,
    pub deploys: Vec<IrDeployDef>,
    pub scopes:  Vec<IrScope>,
    pub errors:  Vec<IrError>,
}
```

---

## Resolve Passes

```
Pass 1 — mirror scope tree
  For each AstScope: create IrScope (symbols empty).
  IrScopeId corresponds 1-to-1 with AstScopeId.

Pass 2 — register names
  For each AstDef::Type: alloc IrTypeDef (body placeholder), register in scope.
  For each AstDef::Link: alloc IrLinkDef (type placeholder), register in scope.
  Duplicate name in same scope → error, skip.

Pass 3 — resolve type bodies & link types
  For each IrTypeDef: resolve AstTypeDefBody → IrTypeBody.
  For each IrLinkDef: resolve AstLinkDef.ty → IrLinkType.
  Both use def-style ref resolution (lexical, Plain for unresolved).

Pass 4 — register instance names
  For each AstDef::Inst: alloc IrInstDef (fields placeholder), register in scope.
  Pre-registration enables forward instance references.

Pass 5 — resolve instance fields
  For each IrInstDef:
    resolve type_name → TypeId (must be Struct).
    for each AstField::Named:
      look up link name in struct's LinkId list,
      validate AstValue → IrValue against the link's IrLinkType pattern.

Pass 6 — resolve deploys
  For each AstDef::Deploy:
    resolve what/target/name → IrRef (def-style).
    resolve fields same as instance fields.
```

Each pass is a standalone function over `&mut Ctx` — new passes can be appended without
touching existing ones.
