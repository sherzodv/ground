# Plan 0004 — Semantic Core Implementation

RFC: `0004-rfc-semantic-core.md`

---

## Overview

This plan replaces the current hard-coded parse/compile/generate pipeline with the
generic semantic core. Every phase is self-contained and leaves tests passing before
the next begins.

Current pipeline:
```
.grd → ground_parse (hard-coded rules) → high::Spec → compile → low::Plan → terra_gen (imperative)
```

Target pipeline:
```
.grd → ground_parse (grammar→ast→resolve) → Spec → ground_gen (hooks+tera) → ground_be_terra (templates)
```

**Crates deleted:** `ground_core` (high, low, compile — all replaced).
**Crates added:** `ground_gen`.
**Crates rewritten:** `ground_parse`, `ground_be_terra`.

---

## Dependency graph (after)

```
ground_parse   — no upstream ground deps
ground_gen     — depends on ground_parse (Spec types only)
ground_be_terra — depends on ground_gen (Backend/Hook/MergeStrategy)
ground_run     — depends on ground_parse + ground_gen + ground_be_terra
ground         — depends on ground_run
ground_test    — depends on ground_parse + ground_gen + ground_be_terra
```

---

## Phase 1 — New grammar (`ground_parse/src/ground.pest`)

**Goal:** replace all domain-specific rules with three generic constructs:
`type_decl`, `link_decl`, `instance_def`. Grammar stays purely structural — no
semantic interpretation.

### 1.1 Identifiers

`ident` gains `:` and `/`:
```
ident      = @{ ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "-" | "_" | ":" | "/")* }
enum_value = @{ ASCII_DIGIT+ | ident }
```
`value` (opaque reference) stays as-is — used only for reference-typed fields.

### 1.2 Type declarations

Three forms, all `type <name> = <body>`:

- **Primitive alias** — body is a primitive keyword (`integer`, `float`, `boolean`,
  `string`, `reference`)
- **Enum** — body is `variant | variant | …`; variants are `ident` or `integer`
- **Composite** — body is `{ composite_member* }` where each member is one of:
  - `link <name> = <link_type_expr>` — inline link declaration
  - `<link_name> = <default_value>` — link name with default
  - `<link_name>` — bare required link reference

### 1.3 Link declarations

`link <name> = <link_type_expr>`

`link_type_expr` grammar:
- `[ shape ("|" shape)* ]` — list with optional polymorphic alternatives
- `shape` alone — single-entry (no brackets)

A `shape` is a chain of segments separated by `:`. Each segment is an ident,
optionally wrapped in `( segment )?` for optional, and optionally prefixed with
`type:` or `link:`.

Examples:
```
link image  = reference
link scaling = type:scaling
link access  = [ service:(port)?  | database ]
link region  = type:region:type:zone
```

Grammar rule should capture the raw token string of `link_type_expr` — full
parsing of shapes happens in the AST layer, not in pest.

### 1.4 Instances

Generic form: `<type_name> <instance_name> { field_entry* }`

Deploy is a special form: `deploy <name> to <provider> as <alias> { field_entry* }`

Field entry: `<link_name> ":" <field_value>`

`field_value`:
- single token (enum value, integer, reference) — e.g. `postgres`, `20`,
  `payments/prod:latest`
- inline block `{ <field_entry>* }` — for composite links like `scaling`
- list `[ <entry>* ]` — for list links, entries space-separated

A list entry is a value token (may contain `:` for path segments like `api:http`
or `us-east:1`).

### 1.5 File rule

```
file = { SOI ~ (type_decl | link_decl | instance_def | deploy_def)* ~ EOI }
```

Drop all old domain rules (`service_def`, `rdb_def`, etc.).

---

## Phase 2 — AST layer (`ground_parse/src/ast.rs`)

**Goal:** convert the pest CST into typed Rust structs. All types are
`pub(crate)` — not visible outside `ground_parse`.

### 2.1 Top-level

```
AstFile      { items: Vec<AstItem> }
AstItem      = TypeDecl | LinkDecl | Instance | Deploy
```

### 2.2 Type declaration

```
AstTypeDecl  { name, span, body: AstTypeBody }
AstTypeBody  = Primitive(PrimitiveKind)
             | Enum(Vec<AstEnumVariant>)
             | Composite(Vec<AstCompositeMember>)

AstEnumVariant  { value: String, span }
AstCompositeMember = Bare    { link_name, span }
                   | Inline  { link_name, type_expr: AstLinkTypeExpr, span }
                   | Default { link_name, value: AstRawValue, span }
```

### 2.3 Link declaration

```
AstLinkDecl  { name, span, type_expr: AstLinkTypeExpr }

AstLinkTypeExpr { is_list: bool, shapes: Vec<AstShape> }
AstShape        { segments: Vec<AstSegment> }
AstSegment      { prefix: Option<Prefix>, name: String, optional: bool, span }
Prefix          = Type | Link
```

Parsing `type:region:type:zone` produces two segments: `[{prefix:Type, name:"region"}, {prefix:Type, name:"zone"}]`.

Parsing `service:(port)?` produces two segments: `[{prefix:None, name:"service"}, {prefix:None, name:"port", optional:true}]`.

### 2.4 Instances

```
AstInstance  { type_name, name, span, fields: Vec<AstField> }
AstDeploy    { name, provider, alias, span, fields: Vec<AstField> }
AstField     { link_name, span, value: AstFieldValue }
AstFieldValue = Single(AstRawValue)
              | List(Vec<AstRawValue>)
              | Block(Vec<AstField>)
AstRawValue  { raw: String, span }   // unparsed token, resolved later
```

`AstRawValue.raw` for a list entry like `us-east:1` is the literal string
`"us-east:1"`. Segment splitting happens in the resolve layer.

### 2.5 CST → AST conversion

One function per rule: `fn ast_type_decl(pair: Pair<Rule>) -> Result<AstTypeDecl, ParseError>`.

Follow existing `Parsed<T>` = `(Option<T>, Vec<ParseError>)` convention from
current `ground_parse`. Collect all errors, don't stop at first.

---

## Phase 3 — Resolve layer (`ground_parse/src/resolve.rs`)

**Goal:** validate the AST against the symbol table, produce the public `Spec`.
No AST types leak out of this module.

### 3.1 Symbol table

Built from all `type_decl` and `link_decl` items in the file, plus the built-in
declarations loaded first (see 3.2).

```
SymbolTable {
    types: HashMap<String, TypeDef>,
    links: HashMap<String, LinkDef>,
}
```

`TypeDef` mirrors `AstTypeBody` but with resolved references:
```
TypeDef = Primitive(PrimitiveKind)
        | Enum { variants: Vec<String> }
        | Composite { members: Vec<MemberDef> }

MemberDef { link_name: String, default: Option<ResolvedValue>, required: bool }
```

`LinkDef` holds the resolved `LinkTypeExpr` (segments replaced with resolved
`TypeDef`/`LinkDef` references, optional flags preserved).

### 3.2 Built-in declarations

All declarations from the "All ground primitives declared" section of the RFC
are embedded as a string constant in `ground_parse`:

```rust
const GROUND_STDLIB: &str = include_str!("ground_stdlib.grd");
```

`ground_stdlib.grd` lives at `ground_parse/src/ground_stdlib.grd` and contains
the full block from the RFC. It is parsed and loaded into the symbol table
before any user file.

User files may not re-declare built-in names (error if they try).

### 3.3 Resolve pass

For each `AstInstance`:

1. Look up `type_name` in symbol table → `TypeDef::Composite`. Error if not
   found or not composite.
2. For each `MemberDef` in the composite:
   - Find matching `AstField` or use default, error if required and absent.
3. For each present `AstField`, call `resolve_field_value(link_name, ast_value, symbol_table)`.
4. Collect into `Instance { type_name, name, fields: Vec<ResolvedField> }`.

`resolve_field_value` dispatches on the link's `LinkTypeExpr`:

- **reference** — value is opaque, wrap in `ScalarValue::Ref`
- **primitive** — parse the raw token as integer/float/bool/string, error on mismatch
- **enum** — validate raw token is a known variant
- **composite (type:T)** — expect a `Block` field value, recurse
- **list** — expect a `List` or single value, validate each entry against shapes
- **typed path (type:region:type:zone)** — split raw token on `:`, validate each
  segment against its declared type in order

For **polymorphic list entries** (`service:(port)? | database`): try each shape
in order, take first match. An entry matches a shape if its segments are
consistent with the shape's segment types. E.g. `api:http` matches
`service:(port)?` because `api` resolves to a `service` instance and `http`
resolves to a valid port on that service.

Instance reference resolution: when a shape starts with a type name (e.g.
`service`), the first segment of the entry must be an instance name of that type
that exists in the file. Collect a second-pass list of unresolved refs during
first pass; resolve them after all instances are registered.

### 3.4 Deploy resolution

`AstDeploy` becomes `DeployInstance { name, provider, alias, fields }` following
the same field-resolution logic as instances. `provider` is validated against
known providers (`aws`, `gcp`, `azure`). `alias` is stored as-is (used as env
name in gen context).

### 3.5 Error strategy

Same as current: collect all errors, return `Err(Vec<ParseError>)` if non-empty.
Position info comes from `AstRawValue.span`.

---

## Phase 4 — New `Spec` (public output of `ground_parse`)

`Spec` moves from `ground_core::high` into `ground_parse/src/spec.rs` and becomes
the crate's public output type.

```
pub struct Spec {
    pub instances: Vec<Instance>,
    pub deploys:   Vec<DeployInstance>,
}

pub struct Instance {
    pub type_name: String,
    pub name:      String,
    pub fields:    Vec<ResolvedField>,
}

pub struct ResolvedField {
    pub link_name: String,
    pub value:     ResolvedValue,
}

pub enum ResolvedValue {
    Scalar(ScalarValue),
    Ref(InstanceRef),
    Composite(Vec<ResolvedField>),
    List(Vec<ListEntry>),
}

pub struct ListEntry {
    pub target:   InstanceRef,
    pub segments: Vec<ScalarValue>,   // resolved optional/path segments
}

pub struct InstanceRef {
    pub type_name: String,
    pub name:      String,
}

pub enum ScalarValue {
    Int(i64), Float(f64), Bool(bool), Str(String),
    Ref(String),     // opaque reference value
    Enum(String),    // validated enum variant
}

pub struct DeployInstance {
    pub name:     String,
    pub provider: String,
    pub alias:    String,
    pub fields:   Vec<ResolvedField>,
}
```

`ground_parse::parse(req) -> Result<Spec, Vec<ParseError>>` — public API is
unchanged in shape, only the output type changes.

Delete `ground_core` crate entirely once `ground_be_terra` is migrated off it.

---

## Phase 5 — `ground_gen` crate (new)

New crate at `src/ground_gen/`. No domain knowledge. Depends only on
`ground_parse` (for `Spec`) and `tera` + `serde_json`.

### 5.1 Public types

```
pub enum HookPattern {
    Root,
    Type { type_name: String },
    Link { link_name: String, source: String, target: String },
}

pub struct Hook {
    pub pattern:  HookPattern,
    pub template: String,      // Tera template source
}

pub enum MergeStrategy {
    Concat { separator: String },
    JsonMerge,
}

pub struct Backend {
    pub name:     String,
    pub hooks:    Vec<Hook>,
    pub strategy: MergeStrategy,
}
```

### 5.2 Engine

`Engine` owns a `tera::Tera` instance.

`Engine::new(backends: Vec<Backend>) -> Result<Engine, GenError>`:
- iterates backends in order, registers each hook's template as
  `"{backend_name}/{hook_idx}"` into Tera
- later-registered backends win on same-name collisions (user overrides)

`Engine::dispatch(spec: &Spec) -> Result<String, GenError>`:
- for each `DeployInstance D` in spec, builds `RootCtx`
- fires Root hooks → collect fragments
- for each `Instance I` reachable from D via field refs, fires `Type { I.type_name }` hooks
- for each `ListEntry` on list-typed fields, fires `Link { link_name, source.type_name, target.type_name }` hooks
- merges all fragments using the backend's `MergeStrategy`
- returns final output string

"Reachable from D" means: walk D's fields recursively. If a field value is a
`ResolvedValue::Ref(r)`, find the instance `r` in spec and include it. List
entries yield `InstanceRef` targets. Stop at already-visited instances.

### 5.3 Context serialization

Three context structs implement `serde::Serialize`:

**RootCtx** — passed to `Root` hooks:
```
deploy:    { name, provider, alias }
instances: [ { type_name, name, fields_as_json } ]
```

**TypeCtx** — passed to `Type` hooks:
```
instance:  { name, type_name, <field_name>: <field_value>, … }
deploy:    (same as RootCtx.deploy)
```
Fields are flattened into key-value pairs for easy template access:
`{{ instance.name }}`, `{{ instance.image }}`, `{{ instance.scaling.min }}`.

**LinkCtx** — passed to `Link` hooks:
```
source:    { name, type_name, …fields }
target:    { name, type_name, …fields }
segments:  { <seg_name>: <seg_value>, … }   // e.g. { "port": "http" }
deploy:    (same as RootCtx.deploy)
```

`Context::from_serialize` (Tera API) converts these to Tera's context map.

### 5.4 Merging

`JsonMerge`: parse each fragment as `serde_json::Value`, deep-merge objects
(right wins on scalar conflict), concat arrays. Start with `{}` as accumulator.

`Concat`: join fragment strings with separator. Simple string concatenation,
useful for plain text or HCL output.

### 5.5 Error type

```
pub enum GenError {
    TemplateRegister { name: String, cause: String },
    TemplateRender   { template: String, cause: String },
    MergeError       { cause: String },
}
```

---

## Phase 6 — `ground_be_terra` rewrite

Replace imperative `terra_gen/aws.rs` with a hook-based backend.

### 6.1 Directory layout

```
src/ground_be_terra/
  src/
    lib.rs          — pub fn backend() -> Backend
    templates/
      root.json.tera
      type_service.json.tera
      type_database.json.tera
      link_access_svc_svc.json.tera
      link_access_svc_db.json.tera
```

### 6.2 `backend()` function

Returns a `Backend { name: "terra_aws", hooks: [...], strategy: MergeStrategy::JsonMerge }`.
Each hook uses `include_str!("templates/…")` to embed its template at compile time.

### 6.3 Template responsibilities

**root.json.tera** — emits:
- `terraform.required_providers` block
- `provider.aws` with `region: {{ deploy.region }}`
- `resource.aws_ecs_cluster` named `ground-{{ deploy.name }}`
- VPC, subnets (one pub+priv per zone from `deploy.region`), IGW, NAT, route tables

Region/zone → AWS AZ name mapping is done inside the template using Tera
conditionals, since the mapping is provider-specific knowledge that belongs in
the backend template, not in Spec.

Example fragment (abbreviated):
```json
{
  "provider": { "aws": { "region": "{{ deploy.region }}" } },
  "resource": {
    "aws_vpc": {
      "ground_{{ deploy.name }}": { "cidr_block": "10.0.0.0/16" }
    }
  }
}
```

**type_service.json.tera** — emits ECS task definition + ECS service + IAM task
role + CloudWatch log group + security group for `{{ instance.name }}`.

Uses `{{ instance.image }}`, `{{ instance.scaling.min }}`, `{{ instance.scaling.max }}`,
`{{ instance.compute }}` (reference to compute profile — resolved to cpu/memory by a
Tera lookup or default values).

**type_database.json.tera** — emits RDS instance + subnet group + security group
for `{{ instance.name }}`. Uses `{{ instance.engine }}`, `{{ instance.size }}`,
`{{ instance.storage }}`.

**link_access_svc_svc.json.tera** — emits `aws_vpc_security_group_ingress_rule`
from source SG to target SG. Uses `{{ source.name }}`, `{{ target.name }}`,
`{{ segments.port }}` (if present, restrict to that port number looked up from
target's ports field; if absent, allow all).

**link_access_svc_db.json.tera** — emits ingress rule (source SG → db SG on
port 5432/3306 depending on `{{ target.engine }}`) and an ECS task environment
variable `{{ source.name | upper }}_DB_URL` referencing the RDS endpoint.

### 6.4 Template inheritance for user overrides

Each template exposes named Tera blocks at logical boundaries so users can
override just a section:

Example in `type_service.json.tera`:
```
{% block task_cpu %}{{ instance.compute.cpu | default(value=256) }}{% endblock %}
```

A user override template that extends the core:
```
{% extends "terra_aws/2" %}
{% block task_cpu %}512{% endblock %}
```

Users register their override backend after the core backend — Tera
last-write-wins ensures the override takes effect.

### 6.5 Remove old code

Once templates cover the same fixture cases as current `terra_gen/aws.rs` output
(verified by running the golden tests), delete:
- `src/ground_be_terra/src/terra_gen/`
- `src/ground_core/` (entire crate)
- Update `Cargo.toml` workspace members accordingly

---

## Phase 7 — Wire-up (`ground_run`, `ground` CLI)

### 7.1 `ground_run/src/lib.rs`

Current flow: `parse → compile → generate → write`.

New flow:
1. `ground_parse::parse(sources)` → `Spec`
2. `ground_gen::Engine::new(vec![ground_be_terra::backend()])` → `Engine`
3. `engine.dispatch(&spec)` → `String` (Terraform JSON)
4. Write to output file

User-supplied template directories (from CLI flag or config) are loaded and
registered as an additional backend after the core one, so they override/extend.

### 7.2 `ground` CLI (`src/ground/src/main.rs`)

Add optional `--templates <dir>` flag. If provided, scan for `*.tera` files,
load them as a second `Backend { name: "user", … }` registered after the core
backend.

Error display (`ops_display.rs`) stays unchanged — `GenError` variants are
formatted the same way as `ParseError`.

---

## Phase 8 — Tests and fixtures

### 8.1 Update existing fixtures

Three existing fixtures in `ground_test/fixtures/` use old syntax. Each must be
updated to new `.grd` syntax:

**0001-minimal-service.md** — translate `service` block to new syntax. The
`service` type is now declared via the stdlib, so instances use:
```
service payments {
  image: payments/prod:latest
}
```
Expected JSON output changes to reflect template-generated Terraform (same
resources, different generation path).

**0002-scaling.md** — add `scaling: { min = 2  max = 10 }` field.

**0003-database.md** — translate `database` block.

Use `UPDATE_FIXTURES=1 cargo test -- files` to regenerate expected JSON after
templates stabilise.

### 8.2 New fixtures to add

- `0004-access-svc-svc.md` — two services, one accessing the other via named port
- `0005-access-svc-db.md` — service accessing a database
- `0006-deploy-multi-zone.md` — deploy with `region: [ us-east:1  us-east:2 ]`
- `0007-user-template-override.md` — verifies user template block override
  produces modified output

### 8.3 Unit tests for resolve layer

Add unit tests inside `ground_parse` (not in `ground_test`) for the resolve
layer in isolation:

- unknown type in instance → error
- missing required field → error
- polymorphic list entry matched to correct shape
- optional segment present vs absent
- default value applied when field omitted
- ambiguous name (exists as both type and link, no prefix) → error
- `type:` prefix disambiguates correctly

---

## Order of work

```
Phase 1  grammar rewrite         — ground.pest
Phase 2  AST layer               — ast.rs + cst_to_ast.rs
Phase 3  Resolve layer           — resolve.rs + ground_stdlib.grd
Phase 4  New Spec                — spec.rs, update lib.rs public API
         → run ground_test: expect parse errors (old fixtures); fix fixtures
Phase 5  ground_gen crate        — new crate, Engine, dispatch, merge
Phase 6  ground_be_terra rewrite — hooks.rs + templates/
         → run ground_test: golden output should match
Phase 7  Wire-up                 — ground_run, CLI
Phase 8  Tests                   — new fixtures, resolve unit tests
         → delete ground_core, old terra_gen
```

Each phase ends with `cargo build` succeeding and existing tests not regressing
before the next phase starts.

---

## Key invariants to maintain throughout

- No layer imports from above (grammar ← ast ← resolve ← Spec; gen ← be_terra)
- Pest types (`Pair<Rule>`) never leak outside the grammar module
- AST types never leak outside `ground_parse`
- `ground_gen` has zero knowledge of AWS, Terraform, or any domain concept
- Templates are the only place provider-specific resource names appear
- All errors collected and returned, never panic on bad input
