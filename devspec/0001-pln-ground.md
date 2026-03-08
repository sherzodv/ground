# Implementation Plan — Ground RFC 0001

Target: `ground gen terra` produces self-contained Terraform that can be applied
against a fresh AWS account and the service appears as running in the AWS console.
`apply` (calling the Terraform CLI) is out of scope.

Work proceeds top-down through the pipeline. Each step has a clear input and
output so layers can be tested independently.

---

## Step 1 — `ground_core::high` — new types

**File:** `src/ground_core/src/high/mod.rs`

Extend `Spec` and add five new structs:

```rust
pub struct Spec {
    pub services: Vec<Service>,
    pub groups:   Vec<Group>,
    pub regions:  Vec<Region>,
    pub envs:     Vec<Env>,
    pub stacks:   Vec<Stack>,
    pub deploys:  Vec<Deploy>,
}

pub struct Group {
    pub name:     String,
    pub services: Vec<String>,      // service names (resolved later in compile)
}

pub struct Region {
    pub name:  String,
    pub aws:   String,              // e.g. "us-east-1"
    pub zones: Vec<Zone>,
}

pub struct Zone {
    pub id:  u32,                   // logical id used in .grd (1, 2, ...)
    pub aws: String,                // e.g. "us-east-1a"
}

pub struct Env {
    pub name: String,
    pub vars: Vec<(String, String)>,
}

pub struct Stack {
    pub name:   String,
    pub env:    String,             // ref → Env
    pub region: String,             // ref → Region
    pub zones:  Vec<u32>,           // ref → Zone ids within region
    pub group:  String,             // ref → Group
}

pub struct Deploy {
    pub provider: Provider,
    pub stacks:   Vec<String>,      // ref → Stack names
}

pub enum Provider { Aws }
```

---

## Step 2 — `ground_parse` — grammar + parse functions

### 2a — `ground.pest`

**File:** `src/ground_parse/src/ground.pest`

Add rules for the five new entities. Key additions:

```pest
// shared
kv_pair  = { ident ~ ":" ~ value }
kv_entry = { kv_pair ~ sep? }

// env
env_def = { "env" ~ ident ~ "{" ~ kv_entry* ~ "}" }

// group
group_def = { "group" ~ ident ~ "{" ~ ident* ~ "}" }

// region / zone
zone_def    = { "zone" ~ integer ~ "{" ~ "aws" ~ ":" ~ value ~ "}" }
region_def  = { "region" ~ ident ~ "{" ~ "aws" ~ ":" ~ value ~ sep? ~ zone_def* ~ "}" }

// stack
zone_list       = { "[" ~ (integer ~ sep?)+ ~ "]" }
stack_env_field    = { "env"    ~ ":" ~ ident }
stack_region_field = { "region" ~ ":" ~ ident }
stack_zone_field   = { "zone"   ~ ":" ~ zone_list }
stack_group_field  = { "group"  ~ ":" ~ ident }
stack_field        = { stack_env_field | stack_region_field | stack_zone_field | stack_group_field }
stack_def          = { "stack" ~ ident ~ "{" ~ stack_field* ~ "}" }

// deploy
stack_list  = { "[" ~ (ident ~ sep?)+ ~ "]" }
deploy_def  = { "deploy" ~ "to" ~ ident ~ "{" ~ "stacks" ~ ":" ~ stack_list ~ "}" }

// top-level file
file = { SOI ~ (service_def | group_def | region_def | env_def | stack_def | deploy_def)* ~ EOI }
```

### 2b — parse functions

**File:** `src/ground_parse/src/lib.rs`

- Add `parse_group`, `parse_region`, `parse_env`, `parse_stack`, `parse_deploy`
- `parse_file` dispatches on rule type and collects all entity types into `Spec`
- `parse` accumulates into the full `Spec`

Validation at parse time (errors collected, not panics):
- `stack`: all four fields (`env`, `region`, `zone`, `group`) required
- `deploy`: `stacks` list must be non-empty
- `region`: `aws` field required

Cross-reference validation (ref resolution — does the named group/env/region
exist) is done in the compiler, not the parser.

---

## Step 3 — `ground_core::low` — network + cluster primitives

**File:** `src/ground_core/src/low/mod.rs`

Add to `Plan` and add new structs:

```rust
pub struct Plan {
    // existing
    pub workloads:      Vec<Workload>,
    pub identities:     Vec<Identity>,
    pub network_groups: Vec<NetworkGroup>,
    pub log_streams:    Vec<LogStream>,
    pub scalers:        Vec<Scaler>,
    // new
    pub provider:         Option<Provider>,
    pub cluster:          Option<Cluster>,
    pub vpc:              Option<Vpc>,
    pub subnets:          Vec<Subnet>,
    pub internet_gateway: Option<InternetGateway>,
    pub nat_gateway:      Option<NatGateway>,
    pub route_tables:     Vec<RouteTable>,
}

pub struct Provider {
    pub region: String,             // resolved provider region, e.g. "us-east-1"
}

pub struct Cluster {
    pub name: String,               // e.g. "ground-prod"
}

pub struct Vpc {
    pub name: String,
    pub cidr: String,               // "10.0.0.0/16"
}

pub struct Subnet {
    pub name:   String,
    pub cidr:   String,
    pub zone:   String,             // resolved zone identifier, e.g. "us-east-1a"
                                    // named `zone` not `az` — Ground's abstraction, not AWS's
    pub public: bool,
}

pub struct InternetGateway {
    pub name: String,
}

pub struct NatGateway {
    pub name:          String,
    pub public_subnet: String,      // ref → Subnet name
}

pub struct RouteTable {
    pub name:   String,
    pub subnet: String,             // ref → Subnet name
    pub public: bool,               // true → route to IGW, false → route to NAT
}
```

Add `env` field to `Workload`:
```rust
pub struct Workload {
    // existing fields
    pub name:     String,
    pub image:    String,
    pub identity: String,
    pub network:  String,
    pub log:      String,
    // new
    pub env: Vec<(String, String)>,
}
```

Remove `ExecRole` from `IdentityKind`:
```rust
// low::IdentityKind has only one variant now:
pub enum IdentityKind {
    TaskRole,   // the identity the workload runtime assumes
}
```

`ExecRole` was an ECS-specific artifact (the identity ECS uses to pull images
and write logs). No equivalent exists in GCP or Azure — those providers handle
it internally. The AWS terra gen creates the exec role and its policy attachment
as an internal implementation detail, without it being driven by the plan.

---

## Step 4 — `ground_core::compile` — per-stack compilation

**File:** `src/ground_core/src/compile/mod.rs`

Change the public signature:

```rust
pub fn compile(spec: &Spec) -> Result<Vec<(String, Plan)>, Vec<String>>
```

Returns one named `Plan` per stack listed across all `deploys`. Errors are reference
failures (unknown group, region, env, stack name).

### Compilation steps per stack

```
1. resolve stack.group   → Vec<&Service>
2. resolve stack.region  → &Region
3. resolve stack.zones   → Vec<&Zone>  (filter region.zones by id)
4. resolve stack.env     → &Env

5. emit Provider  { region: region.aws }
6. emit Cluster   { name: "ground-{stack.name}" }
7. emit Vpc       { name: "ground-{stack.name}", cidr: "10.0.0.0/16" }
8. per zone → emit two Subnets:
     public  name: "{stack}-pub-{zone.id}"   cidr: 10.0.<idx*2>.0/24
     private name: "{stack}-priv-{zone.id}"  cidr: 10.0.<idx*2+1>.0/24
9. emit InternetGateway { name: "ground-{stack.name}" }
10. emit NatGateway     { name: "ground-{stack.name}", public_subnet: first public subnet }
11. emit RouteTable per subnet (public → IGW, private → NAT)
12. per service → compile_service (existing logic, but emit only TaskRole identity;
    exec role is an AWS backend concern, not a plan concern)
    + inject env.vars into Workload
```

CIDR allocation for up to 8 zones is sufficient for scope. Simple index-based:
zone index 0 → public `10.0.0.0/24`, private `10.0.1.0/24`; index 1 → `10.0.2.0/24` / `10.0.3.0/24`, etc.

---

## Step 5 — `ground_be_terra::terra_gen::aws` — full generation

**File:** `src/ground_be_terra/src/terra_gen/aws.rs`

### 5a — top-level output structure

Change `generate` to emit the full Terraform JSON format:

```json
{
  "terraform": {
    "required_providers": {
      "aws": { "source": "hashicorp/aws", "version": "~> 5.0" }
    }
  },
  "provider": {
    "aws": { "region": "..." }
  },
  "resource": { ... }
}
```

### 5b — new generators

- `gen_provider(plan)` → `terraform` block + `provider "aws"` block
- `gen_cluster(res, cluster)` → `aws_ecs_cluster`
- `gen_vpc(res, vpc)` → `aws_vpc`
- `gen_subnet(res, subnet)` → `aws_subnet`
- `gen_internet_gateway(res, igw)` → `aws_internet_gateway` + `aws_internet_gateway_attachment`
- `gen_nat_gateway(res, nat)` → `aws_eip` + `aws_nat_gateway`
- `gen_route_table(res, rt)` → `aws_route_table` + `aws_route_table_association`

### 5c — fix existing generators (remove `var.*`)

| current reference        | replace with |
|--------------------------|--------------|
| `var.vpc_id`             | `aws_vpc.<name>.id` |
| `var.ecs_cluster_id`     | `aws_ecs_cluster.<name>.id` |
| `var.ecs_cluster_name`   | `aws_ecs_cluster.<name>.name` |
| `var.private_subnet_ids` | `[aws_subnet.<priv-0>.id, ...]` (all private subnets) |
| `var.aws_region`         | literal string from `plan.provider.region` |

### 5d — exec role generated internally in `gen_identity`

`low::IdentityKind` no longer has `ExecRole`. The AWS backend generates the exec
role and its `AmazonECSTaskExecutionRolePolicy` attachment internally inside
`gen_identity` whenever it sees a `TaskRole` — because ECS always requires one.
No plan-level primitive drives this; it is an AWS/ECS implementation detail.

### 5e — env injection in `gen_workload`

Add `"environment"` array to container definitions from `workload.env`.

---

## Step 6 — CLI — per-stack output

**File:** `src/ground/src/main.rs`

### `ground gen terra`

- Call `compile(spec)` → `Vec<(stack_name, Plan)>`
- For each `(name, plan)`:
  - `fs::create_dir_all(".ground/terra/{name}")`
  - Write `aws::generate(&plan)` to `.ground/terra/{name}/main.tf.json`
  - Print `wrote .ground/terra/{name}/main.tf.json`

---

## Step 7 — Tests

- Extend `ground_test` fixtures with the full minimal RFC example
- Add a fixture that asserts the generated JSON contains no `var.*` references
- Add unit tests for reference resolution errors (unknown group, region, etc.)
- Existing `codegen` and `parse` tests must continue to pass unchanged

---

## Order of execution

```
Step 1  high types          (no deps)
Step 2  parser              (depends on Step 1)
Step 3  low types           (no deps)
Step 4  compiler            (depends on Steps 1 + 3)
Step 5  terra gen           (depends on Step 3)
Step 6  CLI                 (depends on Steps 4 + 5)
Step 7  tests               (depends on all)
```

Steps 1 and 3 can be done in parallel. Steps 2 and 4 can be done in parallel
after their respective deps.
