# RFC 0004 — Semantic Core

## Premise

Two primitives — `type` and `link` — form the semantic core. The ground DSL
(`service`, `database`, `deploy`, `access`) is an instance of this core.
The core defines both semantics and syntactic shape of all constructs.

---

## Identifiers

```
ident      = ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "-" | "_" | ":" | "/")*
enum_value = ident | integer
```

- First character of `ident` must be alpha.
- Enum variants may be integers (`1 | 2 | 3`) — `enum_value` extends `ident` with the `integer` built-in.
- `key: value` fields require at least one space after `:` to disambiguate
  from identifiers containing `:`.
- `/` permitted for values like `payments/prod:latest`. No structural meaning.

---

## Built-in primitives

```
integer    — whole number
float      — decimal number
boolean    — true | false
string     — quoted text
reference  — opaque identifier, `:` and `/` valid, segments not resolved
```

---

## `type`

All forms use `=`.

**Primitive alias:**
```ground
type port    = integer
type weight  = float
type enabled = boolean
type label   = string
```

**Enum:**
```ground
type size   = small | medium | large | xlarge
type engine = postgres | mysql
type cloud  = aws | gcp | azure
type region = eu-central | eu-west | us-east | us-west | ap-southeast
type zone   = 1 | 2 | 3
```

**Composite** — lists which links apply, with optional inline declarations and defaults:
```ground
type scaling = {
  link min = integer
  link max = integer
}

type service = {
  image
  access
  scaling = { min = 1  max = 1 }
  compute
}
```

- Plain link name — required, no default.
- `link name = ...` — inline link declaration, scoped to this type. Not visible outside.
  Same name in different types are independent declarations.
- `link_name = value` — default value, resolved against the link's declared type.

---

## `link`

All forms use `=`.

**Primitive:**
```ground
link min     = integer
link max     = integer
link storage = integer
link ratio   = float
link active  = boolean
```

**Reference** — opaque identifier:
```ground
link image = reference
```

**Unit** — reference to an instance of a declared type:
```ground
link env     = env
link compute = reference
```

When a type and link share the same name, use `type:` or `link:` prefix to disambiguate:
```ground
link scaling = type:scaling   // holds an instance of type scaling
link engine  = link:engine    // holds the value of link engine (the enum type)
```

Prefixes can be chained to form typed paths:
```ground
link region = type:region:type:zone
```

Each segment is validated against its declared type or link. `region: us-east:1` resolves
`us-east` against `type region` and `1` against `type zone`.

Without a prefix, bare names are resolved unambiguously — error if a name exists as both
a type and a link. Use `type:` or `link:` to resolve the conflict explicitly.

**List / polymorphic / optional** — inline expressions:
```ground
link access = [ service:(port)?  | database ]
```

- `[ ]` — list. No brackets — single entry.
- `(segment)?` — optional segment.
- `|` — polymorphic targets, entry may match any of the listed shapes.
- Each segment is a type name, link name, or built-in primitive. Use `type:` or `link:`
  prefix to disambiguate when names collide.
- A bare type name with no further segments means "reference to an instance of that type".

**Inline grouped syntax for list links:**
```ground
// multiple separate entries
access: api:http
access: main

// grouped — equivalent
access: [ api:http  main ]

// mixed optional segment
access: [ api:http  api:grpc  main ]
```

---

## All ground primitives declared

```ground
// --- primitive links (global) ---
link env     = reference
link image   = reference
link compute = reference

// --- primitive types ---
type port    = integer
type storage = integer
type size    = small | medium | large | xlarge
type engine  = postgres | mysql
type cloud   = aws | gcp | azure
type region  = eu-central | eu-west | us-east | us-west | ap-southeast
type zone    = 1 | 2 | 3

// --- structured types (links scoped within) ---
type scaling = {
  link min = integer
  link max = integer
}

// --- links ---
link image   = reference
link access  = [ service:(port)?  | database ]
link scaling = type:scaling
link engine  = engine
link size    = size
link storage = storage
link compute = reference
link region  = type:region:type:zone
link env     = reference

// --- composite types ---
type service = {
  image
  access
  scaling = { min = 1  max = 1 }
  compute
}

type database = {
  engine
  size
  storage
  compute
}

type stack = {
  service
  database
}

type deploy = {
  stack
  env
  region
}
```

---

## Instantiation examples

`=` is only used in `type` and `link` declarations — the core. Instances use
the bare `name { }` form, clearly distinguishing declarations from instances.

```ground
service payments {
  image:   payments/prod:latest
  access:  [ api:http  api:grpc  main ]
}

service api {
  image:    api/prod:latest
  scaling:  { min = 2  max = 10 }
}

database main {
  engine:  postgres
  size:    small
  storage: 20
}

stack backend {
  service:  [ payments  api ]
  database: main
}

deploy backend to aws as prod {
  env:    prod
  region: [ us-east:1  us-east:2 ]
}
```

### Reading link entries

`access: [ api:http  api:grpc  main ]` resolves against `link access = [ service:(port)?  | database ]`:
- `api:http` — service instance `api`, port `http`
- `api:grpc` — service instance `api`, port `grpc`
- `main` — database instance `main`, port segment absent (optional)

`scaling: { min = 2  max = 10 }` resolves against `link scaling = type:scaling` where `type scaling = { min  max }`:
- `min` resolves against `link min = integer` → value `2`
- `max` resolves against `link max = integer` → value `10`

`image: payments/prod:latest` resolves against `link image = reference`:
- entire value is opaque, no segment resolution

`region: [ us-east:1  us-east:2 ]` resolves against `link region = type:region:type:zone`:
- `us-east` validated against `type region` enum
- `1` validated against `type zone` enum

---

## Hook templates

Templates attach to nodes and edges of the resolved instance tree:

```
deploy: prod
├── type: service:payments
│   ├── link: access → service:api:http
│   ├── link: access → service:api:grpc
│   └── link: access → database:main
├── type: service:api
└── type: database:main
```

Three attachment points:

- **root** — fires once per deploy, full context. Emits shared infrastructure.
- **type** — fires per type instance. `on service` → ECS resources. `on database` → RDS resources.
- **link** — fires per edge. `on access service→service` → ingress rule. `on access service→database` → ingress rule + env vars.

Hooks are additive — multiple hooks on the same node merge outputs.
A provider backend is a set of hooks over the resolved tree.

Template context:
- root → full resolved tree as JSON
- type → instance + root context
- link → source instance + target instance + resolved scalar segments

Template engine: Tera or Handlebars. Naming is deterministic from instance
idents — no explicit output declarations needed for cross-template references.

---

## Gen layer

### Position in pipeline

```
Spec  (resolved instance tree)
  │
  ▼
ground_gen
  hook_registry   — backend-owned hook set, matched by pattern
  dispatcher      — walks tree, fires matching hooks, collects fragments
  merger          — merges fragments per attachment point into final output
  │
  ▼
provider output   — e.g. Terraform JSON, Pulumi YAML, CDK
```

`ground_gen` is a crate. It has no knowledge of any provider. Provider backends
are separate crates (`ground_be_terra`, `ground_be_cdk`, …) that register hooks
into the engine.

---

### Hook patterns

A hook matches an attachment point on the resolved tree. Three pattern kinds:

```
Root
Type  { type_name: String }
Link  { link_name: String, source_type: String, target_type: String }
```

Pattern matching is exact on names. No wildcards — if multiple hooks match the
same node (e.g. two backends both handle `type service`) their outputs are
merged.

---

### Hook registration (Rust API)

```rust
pub enum HookPattern {
    Root,
    Type { type_name: String },
    Link { link_name: String, source: String, target: String },
}

pub struct Hook {
    pub pattern:  HookPattern,
    pub template: String,       // Tera template source
}

pub struct Backend {
    pub name:  String,
    pub hooks: Vec<Hook>,
}
```

A backend is pure data — no trait objects, no dynamic dispatch. The engine
owns the registry; backends are registered before dispatch runs.

---

### Dispatch algorithm

```
fn dispatch(spec: &Spec, backends: &[Backend]) -> Fragments

  for each deploy D in spec:
    ctx_root = RootCtx { deploy: D, tree: resolved_tree(D, spec) }

    // root hooks
    for each backend B:
      for each hook H in B where H.pattern == Root:
        fragments.push(render(H.template, ctx_root))

    // type hooks
    for each instance I reachable from D:
      ctx_type = TypeCtx { instance: I, root: ctx_root }
      for each backend B:
        for each hook H in B where H.pattern == Type { I.type_name }:
          fragments.push(render(H.template, ctx_type))

    // link hooks
    for each edge E reachable from D (source I_s, target I_t, segments S):
      ctx_link = LinkCtx { source: I_s, target: I_t, segments: S, root: ctx_root }
      for each backend B:
        for each hook H in B where H.pattern == Link { E.link_name, I_s.type_name, I_t.type_name }:
          fragments.push(render(H.template, ctx_link))
```

Hooks within the same backend fire in declaration order. Hooks across backends
fire in backend registration order.

---

### Context shapes

```rust
// root — fires once per deploy
struct RootCtx {
    deploy:  DeployInstance,  // name, env, region, zones
    tree:    Value,           // full resolved instance tree as serde_json::Value
}

// type — fires per instance
struct TypeCtx {
    instance: TypeInstance,   // name, type_name, resolved fields as Value
    root:     RootCtx,
}

// link — fires per directed edge
struct LinkCtx {
    source:   TypeInstance,
    target:   TypeInstance,
    segments: IndexMap<String, ScalarValue>,  // e.g. { "port": "http" }
    root:     RootCtx,
}
```

All context types implement `Serialize`. Tera receives them as a flat JSON
object via `Context::from_serialize`.

---

### Template engine — Tera

Tera (Jinja2-like, pure Rust, no FFI). Runtime rendering is intentional:

- ground ships core templates for each built-in backend
- users supply override templates and additional hooks for their own infra
- both are registered into the same `tera::Tera` instance at engine init —
  user templates can shadow or extend core ones without recompiling ground

Other reasons:
- native Rust, no FFI, no runtime binary dependency
- Jinja2 syntax is broadly familiar
- template inheritance and macros — core templates expose blocks users can
  override selectively rather than replacing a whole hook
- structured output (JSON/HCL blocks) renders cleanly without escaping issues

Templates are stored as strings in the `Backend` struct and registered into a
`tera::Tera` instance at engine init time. Template names are namespaced by
backend: `{backend_name}/{hook_index}`. User-supplied templates registered last
take priority (Tera's last-write-wins for same-name templates).

---

### Naming convention — deterministic resource identifiers

Output artifact names are derived from instance idents only. A backend must
follow the scheme; no explicit naming declarations are needed in `.grd` files.

Example scheme for `ground_be_terra`:

```
service  {name}  →  aws_ecs_service.{name}
database {name}  →  aws_db_instance.{name}
```

Because names are deterministic, a `link access service→database` hook can
reference the RDS instance directly:

```hcl
{{ source.name }}_to_{{ target.name }} = aws_db_instance.{{ target.name }}.endpoint
```

No cross-template coordination needed. The `resolve` layer guarantees instance
names are unique within a deploy scope.

---

### Fragment merging

Each hook emits a string fragment. Merging strategy is backend-defined and
provided as a `MergeStrategy` value alongside the backend:

```rust
pub enum MergeStrategy {
    Concat { separator: String },   // plain text / HCL: join with separator
    JsonMerge,                      // deep-merge JSON objects, concat arrays
}
```

`ground_be_terra` uses `JsonMerge` — each hook emits a partial Terraform JSON
object; the engine deep-merges all fragments into a single valid `.tf.json`.

---

### Provider backend example sketch

```rust
// ground_be_terra/src/hooks.rs

pub fn backend() -> Backend {
    Backend {
        name: "terra_aws".into(),
        hooks: vec![

            // root — VPC, cluster, shared networking
            Hook {
                pattern:  HookPattern::Root,
                template: include_str!("templates/root.json.tera").into(),
            },

            // type service — ECS task definition + service
            Hook {
                pattern:  HookPattern::Type { type_name: "service".into() },
                template: include_str!("templates/type_service.json.tera").into(),
            },

            // type database — RDS instance
            Hook {
                pattern:  HookPattern::Type { type_name: "database".into() },
                template: include_str!("templates/type_database.json.tera").into(),
            },

            // link access service→service — security group ingress
            Hook {
                pattern:  HookPattern::Link {
                    link_name:   "access".into(),
                    source:      "service".into(),
                    target:      "service".into(),
                },
                template: include_str!("templates/link_access_svc_svc.json.tera").into(),
            },

            // link access service→database — sg ingress + connection env var
            Hook {
                pattern:  HookPattern::Link {
                    link_name:   "access".into(),
                    source:      "service".into(),
                    target:      "database".into(),
                },
                template: include_str!("templates/link_access_svc_db.json.tera").into(),
            },
        ],
    }
}
```

---

### Updated full pipeline

```
.grd files
  │
  ▼
ground_parse
  grammar   — pest CST
  ast       — typed AST
  resolve   — validates, resolves refs, expands optionals, applies defaults
  Spec      — resolved instance tree
  │
  ▼
ground_gen
  dispatcher   — walks Spec, fires hooks
  merger       — merges fragments (strategy per backend)
  │
  ▼
ground_be_terra / ground_be_cdk / …
  hooks        — registered Backend with Hook set and MergeStrategy
  templates/   — *.tera files (one per hook)
  │
  ▼
provider output (Terraform JSON, CDK, …)
```

Layer rules carry forward: no layer imports from above. `ground_gen` depends on
`ground_parse` output types only. Backend crates depend on `ground_gen` traits
only — not on each other.

---

## Parser layers

```
text
  │
  ▼
grammar   — pest, CST only, no interpretation
  │
  ▼
ast       — CST → typed AST, link entries parsed into structured segments
  │
  ▼
resolve   — validates segments against declared types/links,
            resolves instance refs, checks enum values and primitive types,
            expands optional segments, applies link defaults from type bodies
  │
  ▼
Spec      — public output
```

- No layer imports from above.
- Pest types do not leak outside `grammar`.
- `ast` types are internal to `ground_parse`.
- Existing code audited for layer leaks before new layers introduced.
