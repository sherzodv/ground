# RFC 0015 — Type Functions: Implementation Plan

## Context

RFC 0014 introduced ad-hoc `gen` blocks as a temporary solution for output structure expansion. RFC 0015 supersedes them with a principled functional model: `type` and `link` definitions are functions. Anonymous variants auto-fire during a recursive structural walk. The result eliminates parallel gen machinery (`IrGenDef`, `AsmGenDef`, `IrRefExpr`, `IrGenValue`) and deferred expression evaluation in the backend.

Full RFC: `devspec/0015-rfc-type-functions.md`

---

## Critical Files

| File | Change |
|---|---|
| `src/ground_compile/src/ast.rs` | Add params to TypeDef/LinkDef; new AstLinkFnDef; remove AstGenDef |
| `src/ground_compile/src/parse.rs` | New param parsing; new link-fn parsing; remove parse_gen_def |
| `src/ground_compile/src/ir.rs` | New IrTypeFnDef/IrLinkFnDef; remove IrGenDef/IrRefExpr/IrGenValue |
| `src/ground_compile/src/resolve.rs` | Replace pass7; update pass2/pass6; new param resolution |
| `src/ground_compile/src/asm.rs` | Recursive walk + override threading; rooted AsmExpansion tree; remove AsmGenDef |
| `src/ground_compile/src/lib.rs` | Update CompileRes/Deploy public API |
| `src/ground_be_terra/src/lib.rs` | Replace hardcoded dispatch with expansion tree walk |
| `tests/helpers/golden_parse_helpers.rs` | Show TypeFn/LinkFn; remove Gen |
| `tests/helpers/golden_ir_helpers.rs` | Show IrTypeFnDef/IrLinkFnDef; remove gen helpers |
| `tests/helpers/golden_asm_helpers.rs` | Show AsmExpansion tree; remove AsmGenDef helpers |

---

## Phase 1 — AST (`ast.rs`)

**Add:**
```rust
pub struct AstTypeParam {
    pub name: AstNode<String>,   // e.g. "this"
    pub ty:   AstNode<AstRef>,   // e.g. "service"
}

pub struct AstTypeFnEntry {
    pub alias: AstNode<String>,
    pub value: AstNode<AstValue>,   // AstValue::Struct with type_hint
}
```

**Modify `AstTypeDef`** — add optional param:
```rust
pub param: Option<AstNode<AstTypeParam>>,   // None = zero-param type
```

**Modify `AstTypeDefBody`** — add variant:
```rust
TypeFn(Vec<AstNode<AstTypeFnEntry>>),       // body of one-param type function
```

**Modify `AstLinkDef`** — extend for link functions:
```rust
pub fn_name: Option<AstNode<String>>,       // None = anonymous link fn
pub params:  Option<(AstNode<AstTypeParam>, AstNode<AstTypeParam>)>,  // (from, to)
pub fn_body: Option<Vec<AstNode<AstTypeFnEntry>>>,
```
When `params` + `fn_body` present = link function. Otherwise = zero-param link slot (unchanged).

**Remove:** `AstGenDef`, `AstGenField`, `AstDef::Gen`.

---

## Phase 2 — Parser (`parse.rs`)

**`parse_type_def()`** — after ident, check for `(`:
- `type (param: type) = { ... }` → anonymous type fn (`name = None`, `param = Some`)
- `type name(param: type) = { ... }` → named type fn (`name = Some`, `param = Some`)
- `type name = { ... }` → zero-param (unchanged)

**Add `parse_type_param()`** — parses `ident : ref` inside `()`

**`parse_link_def()`** — two dispatch paths:
- `link type:link fn_name?(from: t, to: t) = { ... }` → link function (detect by `ident:ident(` lookahead)
- `link ident? = type-expr` → zero-param link slot (unchanged)

**Add `parse_typefn_body()`** — parses `{ alias: TypeRef { ... } ... }` (same structure as current gen body)

**Remove:** `parse_gen_def()`, `try_parse_gen_field()`

**Guard in `hoist_struct_scopes()`** — skip hoisting for type defs where `param.is_some()`

---

## Phase 3 — IR (`ir.rs`)

**Add:**
```rust
pub struct TypeFnId(pub u32);
pub struct LinkFnId(pub u32);

pub struct IrTypeFnParam {
    pub name: String,
    pub ty:   TypeId,
}

pub struct IrTypeFnDef {
    pub name:   Option<String>,        // None = anonymous
    pub params: Vec<IrTypeFnParam>,    // len 0 = plain type, len N = type function
    pub scope:  ScopeId,
    pub loc:    IrLoc,
    pub body:   Vec<IrTypeFnEntry>,
}

pub struct IrTypeFnEntry {
    pub alias:       String,
    pub vendor_type: TypeId,
    pub fields:      Vec<IrFnBodyField>,
}

pub struct IrFnBodyField {
    pub name:  String,
    pub value: IrValue,   // ordinary IrValue — no special IrGenValue needed
}

pub struct IrLinkFnDef {
    pub name:    Option<String>,
    pub link_id: LinkId,
    pub params:  Vec<IrTypeFnParam>,   // arity determined by walk, not hardcoded
    pub scope:   ScopeId,
    pub loc:     IrLoc,
    pub body:    Vec<IrTypeFnEntry>,
}
```

**Modify `IrScope`** — replace `gens` with:
```rust
pub type_fns:      HashMap<String, TypeFnId>,     // named type fns by name
pub anon_type_fns: HashMap<TypeId, TypeFnId>,      // anonymous: one per param type
pub link_fns:      HashMap<String, LinkFnId>,      // named link fns by name
pub anon_link_fns: HashMap<LinkId, LinkFnId>,      // anonymous: one per link slot
```

**Modify `IrRes`** — remove `gen_defs`; add `type_fns: Vec<IrTypeFnDef>`, `link_fns: Vec<IrLinkFnDef>`

**Modify `IrDeployDef`** — remove `to_gen: Option<GenId>`; add `to_type_fn: Option<TypeFnId>`

**Remove:** `GenId`, `IrRefExprSeg`, `IrRefExpr`, `IrGenValue`, `IrGenBodyField`, `IrGenField`, `IrGenTarget`, `IrGenDef`

---

## Phase 4 — Resolver (`resolve.rs`)

**Pass 2** — register type/link functions alongside types:
- For each `AstDef::Type` with `param.is_some()`: register in `scope.type_fns` or `scope.anon_type_fns`
- For each `AstDef::Link` with `params.is_some()`: register in `scope.link_fns` or `scope.anon_link_fns`
- Error: duplicate anonymous type function for same param type in same scope
- Error: duplicate anonymous link function for same link slot in same scope

**Pass 7 — replace entirely** with `pass7_resolve_type_fns` + `pass7_resolve_link_fns`:
- Resolve `param.ty` ref → `TypeId`
- For each body entry: resolve `alias`, resolve `VendorType` → `TypeId`, validate fields against vendor type's links
- Param ref resolution: `{param_name:field}` — match `param_name` against declared param, then walk fields of that param's type. Reuse existing field validation logic.
- Trailing atom support: already handled by `AstRefSegVal::Group` — passes through unchanged into `IrValue`

**Pass 6** — `deploy X to Y as Z { fields }` is sugar — keep syntax unchanged, replace `resolve_deploy_gen` with `resolve_deploy_type_fn`:
- Walk `target` ref consuming pack segments
- Remaining tail resolves to named type function via `lookup_type_fn()`
- Populate `IrDeployDef.to_type_fn`

**`Ctx` changes:**
- Remove: `gen_defs`, `alloc_gen()`, `lookup_gen()`
- Add: `type_fns`, `link_fns`, `alloc_type_fn()`, `alloc_link_fn()`, `lookup_type_fn()`, `lookup_anon_type_fn(scope, type_id)`, `lookup_anon_link_fn(scope, link_id)`

---

## Phase 5 — ASM (`asm.rs`)

**New output types:**
```rust
pub struct AsmExpansion {
    pub inst:       AsmInst,
    pub outputs:    Vec<AsmOutput>,       // from type function firing
    pub link_outs:  Vec<AsmLinkOutput>,   // from link function firing
    pub children:   Vec<AsmExpansion>,    // recursive walk of inst links
}

pub struct AsmOutput {
    pub alias:       String,
    pub vendor_type: String,
    pub fields:      Vec<AsmField>,       // fully substituted
    pub scope:       Vec<String>,         // pack path for template lookup
}

pub struct AsmLinkOutput {
    pub from:    AsmInstRef,
    pub to:      AsmInstRef,
    pub outputs: Vec<AsmOutput>,
}
```

**Modify `AsmDeploy`:**
- Remove: `gen: Option<String>`, `members: Vec<AsmInstRef>`, `links: Vec<(AsmInstRef, AsmInstRef)>`
- Add: `expansion: Option<AsmExpansion>`, `overrides: AsmOverrides`

```rust
pub struct AsmOverrides {
    pub link_fns: HashMap<String, String>,  // link key → named fn name
    pub fields:   Vec<AsmField>,
}
```

**Recursive walk `expand(inst, scope, overrides, ir, visited) -> AsmExpansion`:**
1. Look up anonymous type function for `inst.type_id` in scope (override check first)
2. Fire: substitute `{param_name:field}` → concrete value from `inst.fields`; produce `Vec<AsmOutput>`
3. For each link in inst's type:
   - Walk link field values (InstRef/inline Inst)
   - If value type has anonymous type fn in scope → recurse, add to `children`
   - For each (from, to) pair: fire anonymous link fn (or named override) → `AsmLinkOutput`
4. Cycle guard: skip if `inst.name` in `visited`

**Override threading:** Pass `overrides` through all recursive calls unchanged.

**Remove:** `AsmGenDef`, `AsmGenValue`, `AsmGenBodyField`, `AsmGenField`, `lower_gen_def()`, `AsmRes.gen_defs`

---

## Phase 6 — Public API (`lib.rs`)

Update `Deploy` struct: remove `members`, `links`; add `expansion: Option<AsmExpansion>`, `overrides: AsmOverrides`.

---

## Phase 7 — Terra Backend (`ground_be_terra/src/lib.rs`)

Replace hardcoded `match inst_ref.type_name` dispatch with expansion tree walk:

```rust
fn walk_expansion(exp: &AsmExpansion, deploy_ctx: &Value, frags: &mut Vec<String>)
```

- For each `AsmOutput` in `exp.outputs`: load template by `scope + vendor_type`, render with fields + deploy context
- For each `AsmLinkOutput`: render link outputs similarly
- Recurse into `exp.children`

**Template lookup:** `scope + vendor_type → include_str!` map (compile-time). Add `load_template(scope: &[String], vendor_type: &str) -> Option<&'static str>` matching on the combination.

Existing 5 templates remain; dispatch mechanism changes.

---

## Implementation Order

1. **ast.rs** — new structs, modified structs, remove AstGenDef; update parse golden tests
2. **parse.rs** — new parse functions, remove parse_gen_def
3. **ir.rs** — new IR types, remove gen types
4. **resolve.rs** — replace pass7, update pass2/pass6; update IR golden tests
5. **asm.rs** — recursive walk, new expansion tree, remove AsmGenDef; update ASM golden tests
6. **lib.rs** — public API update
7. **ground_be_terra/src/lib.rs** — expansion tree walk, update template dispatch

Each step must keep `cargo test` passing before moving to the next.

---

## Test Updates

**Parse golden tests:**
- Update helpers: remove `AstDef::Gen`; add param display for `AstDef::Type`, fn display for `AstDef::Link`
- Replace all `gen_*` tests with `type_fn_*` and `link_fn_*` equivalents
- New: `parse_anon_type_fn`, `parse_named_type_fn`, `parse_anon_link_fn`, `parse_named_link_fn`

**IR golden tests:**
- Remove `show_gen_def_entry()`; add `show_type_fn_entry()`, `show_link_fn_entry()`
- Replace all `gen_*` IR tests
- New: anon registration, duplicate anon error, deploy-to-type-fn resolution

**ASM golden tests:**
- Remove `show_gen_def()`; add `show_expansion()`
- Replace all `gen_*` ASM tests
- New: `asm_anon_type_fn_fires`, `asm_link_fn_fires_per_pair`, `asm_override_named_link_fn`, `asm_cycle_stops_walk`

---

## Verification

```bash
cargo test -p ground_compile      # all parse/IR/ASM golden tests pass
cargo test -p ground_be_terra     # backend integration tests pass
cargo check --workspace           # no type errors across crates
```
