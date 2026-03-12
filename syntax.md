# Ground DSL

`.grd` files, merged per directory. Comments: `//`.

Whitespace is significant in one place: `key: value` field assignments require at least one space after `:` to disambiguate from identifiers that contain `:` (e.g. `svc:http`). Elsewhere — indentation, newlines, spacing between tokens — whitespace is insignificant.

---

## Services

```ground
service <name> {
  image:   <image>:<tag>             // required — container image reference
  access:  [ <target>  … ]          // optional — services and databases this service may reach
  scaling: { min: <n>  max: <n> }   // optional — CPU-based autoscaling; default min:1 max:1
  compute: <reference>              // optional — compute profile reference
}
```

Example:

```ground
service api {
  image:   api:prod
  scaling: { min: 2  max: 10 }
  access:  [ svc-core:http  db-main ]
}

service svc-core {
  image: svc-core:prod
}
```

---

## Databases

```ground
database <name> {
  engine:  postgres | mysql          // required
  size:    small | medium | large | xlarge
  storage: <integer>                 // GB
}
```

Example:

```ground
database db-main {
  engine:  postgres
  size:    medium
  storage: 20
}
```

---

## Access rules

The `access` field declares which services and databases a service can reach. Ground derives all security group rules automatically.

```ground
// reach a service on a specific port
access: svc-core:http

// reach a service with no port qualifier
access: svc-core

// reach a database
access: db-main

// multiple targets in one list
access: [ svc-core:http  db-main ]
```

When a service accesses a database, ground also injects the connection endpoint as an environment variable into the container.

---

## Deploy

```ground
deploy <name> to <provider> as <alias> {
  region: [ <region>:<zone>  … ]
}
```

Example:

```ground
deploy prod to aws as prod {
  region: [ us-east:1  us-east:2 ]
}
```

Supported regions: `eu-central`, `eu-west`, `us-east`, `us-west`, `ap-southeast`
Supported zones: `1`, `2`, `3`

---

## Full example

```ground
service api {
  image:   api:prod
  scaling: { min: 2  max: 10 }
  access:  [ svc-core:http  db-main ]
}

service svc-core {
  image: svc-core:prod
}

database db-main {
  engine:  postgres
  size:    medium
  storage: 20
}

deploy prod to aws as prod {
  region: [ us-east:1  us-east:2 ]
}
```

---

## CLI

```
ground init [--git-ignore]   create .ground/ + settings.json; patch .gitignore
ground gen terra             write .ground/terra/<deploy>/main.tf.json
ground plan                  plan changes per Ground entity (no apply)
```

### ground plan output

```
running in plan mode, no changes will be made
plan to deploy prod stack to aws /

  terraform 1.9.0 ready
  starting state refresh
  ↻ refreshing aws_vpc.ground_prod
  ↻ refreshing aws_ecs_service.api
  …
  state refresh complete
  computing plan
  running terraform show -json .tfplan

stack prod → aws

create service:api
  + aws_ecs_service:svc_api
  + aws_ecs_task_definition:svc_api
  + aws_iam_role:svc_api_task
  + aws_iam_role:svc_api_exec
  + aws_iam_role_policy_attachment:svc_api_exec
  + aws_security_group:svc_api
  + aws_vpc_security_group_egress_rule:svc_api_all
  + aws_cloudwatch_log_group:_ground_svc_api

create database:db-main
  + aws_db_instance:db_main
  + aws_db_subnet_group:db_main
  + aws_security_group:db_main_db
  + aws_vpc_security_group_egress_rule:db_main_db_all
  + aws_vpc_security_group_ingress_rule:svc_api_to_db_main_db

create prod
  + aws_ecs_cluster:ground_prod
  + aws_vpc:ground_prod
  + aws_subnet:prod_pub_1
  + aws_subnet:prod_priv_1
  + aws_subnet:prod_pub_2
  + aws_subnet:prod_priv_2
  + aws_internet_gateway:ground_prod
  + aws_eip:ground_prod_eip
  + aws_nat_gateway:ground_prod
  + aws_route_table:rt_prod_pub_1
  + aws_route_table:rt_prod_priv_1
  + aws_route_table:rt_prod_pub_2
  + aws_route_table:rt_prod_priv_2
  …

create 31  modify 0  delete 0
```

---

## AWS resource mapping

**Per service**

| Ground | AWS |
|--------|-----|
| workload | `aws_ecs_task_definition`, `aws_ecs_service` |
| identity | `aws_iam_role` ×2 (task + exec), `aws_iam_role_policy_attachment` |
| network | `aws_security_group`, `aws_vpc_security_group_egress_rule` |
| logs | `aws_cloudwatch_log_group` |
| scaling | `aws_appautoscaling_target`, `aws_appautoscaling_policy` |
| access svc→svc | `aws_vpc_security_group_ingress_rule` on target |
| access svc→db | `aws_vpc_security_group_ingress_rule` on target + connection env var |

**Per database**

| Ground | AWS |
|--------|-----|
| instance | `aws_db_instance`, `random_password` |
| network | `aws_security_group`, `aws_db_subnet_group`, `aws_vpc_security_group_egress_rule` |

**Per deploy (shared infra)**

| Ground | AWS |
|--------|-----|
| cluster | `aws_ecs_cluster` |
| network | `aws_vpc`, `aws_subnet` ×2 per zone, `aws_internet_gateway`, `aws_eip`, `aws_nat_gateway`, `aws_route_table` ×2 per zone, `aws_route` ×2 per zone, `aws_route_table_association` ×2 per zone |

---

---

# Advanced: Semantic Core

Ground's type system is built on two primitives — `type` and `link`. All built-in constructs (`service`, `database`, `deploy`, `access`, `scaling`) are declared in the stdlib using these same primitives. There is no privileged syntax: user declarations are first-class.

`type` introduces a named type into the type environment. `link` introduces a typed field declaration — essentially a named, typed projection — into the link environment. Instances inhabit types; field values inhabit the types their links declare.

---

## Identifiers

```
ident      = ASCII_ALPHA ~ (ASCII_ALPHANUMERIC | "-" | "_" | ":" | "/")*
enum_value = ident | integer
```

- `:` and `/` are valid within identifiers and values — `payments/prod:latest`, `us-east:1`.
- `key: value` requires at least one space after `:` to disambiguate field syntax from infix `:` in identifiers.
- `=` is used exclusively in declarations (`type`, `link`). Instances use `:` for field assignment.

---

## Built-in base types

Five base types are pre-declared and not expressible in ground syntax:

```
integer    — ℤ, whole numbers
float      — ℝ, decimal numbers
boolean    — 𝔹, true | false
string     — quoted text
reference  — opaque token; `:` and `/` permitted, no segment structure imposed
```

All user-declared types are aliases, enumerations, or products over these bases and other declared types.

---

## `type` — type introduction

`type T = ...` introduces `T` into the type environment. Three forms:

**Alias** — `T` is a new name for a base type. Values of `T` are validated as that base:
```ground
type port    = integer
type weight  = float
type enabled = boolean
```

**Enumeration (sum type)** — `T` is a finite inhabited set; `|` separates constructors. Each constructor is a unit — no payload:
```ground
type size   = small | medium | large | xlarge
type engine = postgres | mysql
type region = eu-central | eu-west | us-east | us-west | ap-southeast
type zone   = 1 | 2 | 3
```

Variants may be integers (`zone = 1 | 2 | 3`) — `enum_value` extends `ident` to cover the integer base type.

**Composite (product type)** — `T` is a record; the body declares which links are in scope for instances of `T`, with optional defaults:
```ground
type scaling = {
  link min = integer
  link max = integer
}

type service = {
  image
  access
  scaling = { min: 1  max: 1 }
  compute
}
```

- Bare link name — required field; the link must be declared in the link environment.
- `link name = expr` — inline link declaration, scoped to this type only. The same name in two types are independent declarations.
- `link_name = value` — field has a default; the value is resolved against the link's declared type at instantiation.

---

## `link` — typed field declaration

`link L = expr` introduces `L` into the link environment with a declared type. The type expression determines how field values are parsed and validated.

**Primitive** — field values must be parseable as the base type:
```ground
link min     = integer
link storage = integer
link ratio   = float
link active  = boolean
```

**Reference** — field value is an opaque token; no segment structure is imposed:
```ground
link image   = reference
link compute = reference
```

**TypeRef** — field value must be an instance of the named type. Use `type:` prefix to refer to a type when a link of the same name also exists:
```ground
link scaling = type:scaling   // value inhabits type scaling
link engine  = engine         // bare name — unambiguous, resolves to type engine
```

Disambiguation prefixes:
```ground
link engine  = link:engine    // refers to the link named engine (its declared enum type)
link scaling = type:scaling   // refers to the type named scaling (the product)
```

Without a prefix, bare names must be unambiguous in the environment — error if a name exists as both a type and a link.

**Typed path** — a sequence of alternating type/link projections, each segment validated against the type or link it names. Syntax: `type:T:type:U` or `type:T:link:L`:
```ground
link region = type:region:type:zone
```

A value like `us-east:1` is split on `:` and each token validated left-to-right: `us-east` checked against `type region` (must be a valid constructor), `1` checked against `type zone`. The full path forms a dependent-style composite value.

**List / sum / optional** — a link whose values are lists of entries, each entry matched against one of several shapes:
```ground
link access = [ service:(port)?  | database ]
```

- `[ ]` — value is a list; each element is matched independently.
- `T:(L)?` — entry is an instance reference of type `T`, optionally followed by a segment satisfying link `L`.
- `| T` — additional shape in the sum; entry may match any listed shape (first match wins).
- A bare type name `T` with no further segments denotes a reference to an instance of `T`.

The list type is therefore `List(Shape₁ | Shape₂ | …)` where each shape is a partial path expression.

---

## Term syntax (instantiation)

`=` is restricted to the declaration language. Instances — terms inhabiting a type — use the bare `name { fields }` form, where fields assign values via `:`:

```ground
service payments {
  image:   payments/prod:latest
  access:  [ api:http  api:grpc  main ]
}

database main {
  engine:  postgres
  size:    small
  storage: 20
}

deploy backend to aws as prod {
  region: [ us-east:1  us-east:2 ]
}
```

### Type-directed value resolution

Each field value is resolved by dispatching on the link's declared type:

| Link type | Resolution |
|-----------|------------|
| `integer` / `float` / `boolean` | Parse literal. |
| `string` | Parse quoted literal. |
| `reference` | Store token as-is; `:` and `/` permitted, no structure imposed. |
| Enum `T` | Token must be a declared constructor of `T`. |
| `type:T` (product) | Value must be a block `{ … }`; fields resolved recursively against `T`'s link declarations. |
| Typed path `type:T:type:U` | Token split on `:`; each segment resolved against the corresponding type in the path. |
| `List(shapes)` | Value is `[ … ]` or a single entry; each entry matched against shapes left-to-right. |

**Examples:**

`access: [ api:http  main ]` against `link access = [ service:(port)?  | database ]`:
- `api:http` — shape `service:(port)?`: instance `api` of type `service`, segment `http` satisfies `link port`
- `main` — shape `database`: instance `main` of type `database`, optional port segment absent

`scaling: { min: 2  max: 10 }` against `link scaling = type:scaling` where `type scaling = { link min = integer  link max = integer }`:
- block resolved as product; `min: 2` and `max: 10` checked against `integer`

`region: [ us-east:1  us-east:2 ]` against `link region = type:region:type:zone`:
- each entry split: `us-east` ∈ constructors of `type region`, `1` ∈ constructors of `type zone`

---

## All built-in declarations (stdlib)

```ground
// --- primitive types ---
type zone   = 1 | 2 | 3
type region = eu-central | eu-west | us-east | us-west | ap-southeast
type engine = postgres | mysql
type size   = small | medium | large | xlarge

// --- primitive links ---
link image   = reference
link compute = reference
link version = integer
link storage = integer

// --- structured types ---
type scaling = {
  link min = integer
  link max = integer
}

// --- link declarations ---
link scaling = type:scaling
link engine  = engine
link size    = size
link access  = [ service:(port)?  | database ]
link region  = type:region:type:zone

// --- composite types ---
type service = {
  image
  link access  = [ service:(port)?  | database ]
  scaling = { min: 1  max: 1 }
  link compute = reference
}

type database = {
  engine
  link version = integer
  link size    = size
  link storage = integer
  link compute = reference
}

type stack = {
  link service  = service
  link database = database
}
```

---

## Hook system

After parsing and resolution, the output is a `Spec` — a resolved instance tree. Provider backends are functions over this tree, defined as sets of hooks keyed by attachment point.

```
deploy: prod
├── type: service:api
│   └── link: access → database:db-main
└── type: database:db-main
```

Three attachment points, matched exactly by name:

- **Root** — fires once per deploy. Context: full resolved tree. Emits shared infrastructure.
- **Type `T`** — fires for each instance whose type is `T`. Context: instance + root. Emits per-instance resources.
- **Link `L` `S→T`** — fires for each directed edge whose link name is `L`, source type `S`, target type `T`. Context: source instance, target instance, resolved path segments. Emits per-edge resources (access rules, env injection).

Hooks are additive: multiple hooks on the same node merge their output fragments. A backend is a set of `(pattern, template)` pairs; the engine walks the tree, fires matching hooks, and merges all fragments via a backend-defined merge strategy (`JsonMerge` for Terraform — deep-merge JSON objects, concatenate arrays).

This makes a backend a pure function `Spec → Output` with no shared mutable state between hooks. Cross-hook references (e.g. a link hook referencing a resource emitted by a type hook) are resolved by deterministic naming: resource identifiers are derived solely from instance names, so no explicit coordination is needed.

See [`devspec/0004-rfc-semantic-core.md`](devspec/0004-rfc-semantic-core.md) for the full design.
