# RFC 0007 — Generation Tree (gen2)

## Context

`IrRes` is an arena-based IR where all cross-references use typed numeric IDs (`TypeId`,
`LinkId`, `InstId`, `ScopeId`). Every consumer must carry the full `IrRes` and perform arena
lookups to recover names and values. Generators should not need to do this — they need a
walk-friendly tree with all IDs replaced by resolved data.

The existing terra backend (`ground_be_terra`) reveals the concrete usage pattern:
- A deploy selects a set of member instances (found by traversing the deploy instance's fields
  for `InstanceRef` values and resolving them from a global flat list).
- Templates for link hooks (e.g. `access`) need the full data of *both* source and target
  instances — not just their names.
- Instances can reference each other (e.g. two services with mutual `access` entries), so
  full recursive inlining of named instances in field values is not possible.

## Scope

Strictly: `IrRes` → lower pass → `GenRes`.
Out of scope: actual code generation, template rendering, deploy execution.

## Goals

1. No global arena lookups in generators — the deploy context is self-contained.
2. All enum variants resolved to their string names (with type context retained).
3. Named instance references stay as `InstRef { type_name, name }`; the full instance data
   is always reachable within `GenDeploy.members` — no cross-deploy lookup needed.
4. Anonymous inline instances fully embedded in the tree.
5. Deploy field overrides merged into the base instance — generator sees one flat `GenInst`.
6. Member instance set pre-computed by the lowering pass (eliminates `stack_members()` logic
   from the generator).

## Decisions

**On named instance references:** keep as `InstRef { type_name, name }`. Cross-references
between instances (e.g. mutual `access` entries) make full recursive inlining impossible.
Full data is available via lookup in `GenDeploy.members`.

**On `GenDeploy.members`:** the lowering pass pre-computes which instances belong to a deploy
by walking the deploy instance's fields for `InstRef` values (the logic currently in
`stack_members()` in `ground_be_terra`). This makes each `GenDeploy` self-contained.

**On anonymous instances:** fully inline as `GenInst` — they have no identity outside their
containing field.

**On deploy fields:** deploy fields (top-level links set on the `deploy` statement) are kept
separate from the instance's own fields — they appear on `GenDeploy.fields`, not on
`GenDeploy.inst.fields`. The terra backend treats them as separate template contexts
(`deploy.field` vs `instance.field`), so merging would lose that distinction.

**On `Path` values:** each segment retains its type name and the resolved string value.

**On `type_path`:** omitted. The terra backend only ever uses `type_name` to select templates;
it does not need scope-qualified paths. Can be added later if a multi-provider generator
requires distinguishing `aws:database` from `gcp:database`.

**On error handling:** lowering is infallible — `IrRes` is assumed valid (no errors). If
`IrRes` has errors, lowering is not invoked.

## Data Structures

```rust
// gen2.rs

pub struct GenRes {
    pub deploys: Vec<GenDeploy>,
}

/// A fully resolved, self-contained deployment context.
pub struct GenDeploy {
    pub target:  Vec<String>,    // scope path segments (e.g. ["aws"])
    pub name:    String,         // deployment name string
    pub inst:    GenInst,        // the instance being deployed
    pub fields:  Vec<GenField>,  // deploy-specific fields (separate from inst fields)
    pub members: Vec<GenInst>,   // all instances referenced from inst's fields (pre-resolved)
}

/// A fully resolved instance — no IDs, all names inlined.
pub struct GenInst {
    pub type_name: String,       // e.g. "database"
    pub name:      String,
    pub fields:    Vec<GenField>,
}

pub struct GenField {
    pub name:  String,           // link name
    pub value: GenValue,
}

pub enum GenValue {
    Str(String),
    Int(i64),
    Ref(String),                 // reference primitive (opaque)
    Variant(GenVariant),
    InstRef(GenInstRef),         // named instance — full data in GenDeploy.members
    Inst(Box<GenInst>),          // anonymous inline instance
    Path(Vec<GenPathSeg>),       // multi-segment typed path
    List(Vec<GenValue>),
}

pub struct GenVariant {
    pub type_name: String,       // enum type name
    pub value:     String,       // variant string
}

pub struct GenInstRef {
    pub type_name: String,
    pub name:      String,       // key for lookup in GenDeploy.members
}

pub struct GenPathSeg {
    pub type_name: String,       // the type this segment belongs to
    pub value:     String,       // variant name or instance name
    pub is_opt:    bool,
}
```

## Lowering Pass

Single pass over `IrRes::deploys`:

```
for each IrDeployDef:
  1. resolve target IrRef       → Vec<String>  (scope names)
  2. resolve name IrRef         → String        (last Plain/Inst segment)
  3. resolve what IrRef         → InstId
  4. lower IrInstDef[what]      → GenInst       (base instance)
  5. apply deploy field overrides on top of GenInst.fields
  6. collect members: walk inst.fields, gather all InstRef names,
     lower each referenced IrInstDef → GenInst, deduplicate by name
  7. emit GenDeploy { target, name, inst, members }
```

Helpers:
- `lower_inst(InstId, &IrRes) → GenInst` — resolves type name, lowers all fields
- `lower_value(IrValue, &IrRes) → GenValue` — dispatches on variant; anonymous `Inst(id)`
  inlined, named `Inst(id)` becomes `InstRef { type_name, name }`
- `lower_ref_to_path(IrRef, &IrRes) → Vec<String>` — flattens IrRef segments to strings
- `collect_inst_refs(fields: &[GenField]) → Vec<InstId>` — walks field values for InstRef ids,
  used to build `members`

## Open Questions

1. **`what` resolution** — `IrDeployDef.what` can be any `IrRef`, not always a direct `InstId`.
   Should lowering require it to be a single `Inst` segment (error otherwise), or handle
   more complex cases?

2. **Members depth** — `collect_inst_refs` currently walks only the top-level deploy instance's
   fields. Should it recurse into member instances' fields to pull in transitively referenced
   instances? The terra backend does not need this today, but deeper graphs may arise.

3. **Anonymous instance identity** — the resolver allocates `IrInstDef` entries for anonymous
   inline instances (with auto-generated names). The lowering pass needs a reliable way to
   distinguish anonymous from named instances to decide inline vs `InstRef`. Current plan:
   check `IrInstDef.name` for a sentinel prefix set by the resolver.
