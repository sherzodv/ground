# Ground — Project Overview

Dense technical reference for LLM sessions. Start here before reading any code.

---

## What it is

Ground is a declarative infra-as-code language that compiles to Terraform JSON.
A `.grd` file describes services, databases, and their access relationships.
The compiler expands these via **type functions** (generative templates) into AWS resources.
The terraform backend renders the expansion tree via per-vendor-type Tera templates.

---

## Repo layout

```
/home/sherzod/getground/
  ground/                     ← this repo (Rust workspace)
    src/
      ground/                 ← CLI binary (main.rs: init / gen terra / plan / apply)
      ground_compile/         ← parser + resolver + IR + ASM lowering
      ground_be_terra/        ← Tera template renderer → Terraform JSON
      ground_gen/             ← merge_json() helper, shared by backends
      ground_run/             ← spawn terraform subprocess, stream events
      ground_test/            ← golden fixture tests (fixtures/*.md)
    devspec/                  ← historical RFCs and plans (NOT source of truth)
  ground-test/                ← live integration env (real AWS account)
    infra.grd                 ← current multi-service example
    out/                      ← generated terraform
```

---

## Language syntax

```ground
# Type definitions (zero-param = plain type)
type zone = 1 | 2 | 3                           # enum
type region = eu-central | eu-west | us-east    # enum
type service = {                                 # struct
    type port   = grpc | http                   # nested enum
    link image  = reference                     # link = typed slot
    link access = [ service:(port) | database ] # list union type
    link scaling = type scaling = {             # inline nested struct type
        link min = integer
        link max = integer
    }
}

# Instance (application of zero-param type)
service svc-api { image: svc-api:prod  access: [ db-main ] }

# Import
use std:*                          # wildcard: all names from std available unqualified
use pack:std:type:service          # specific import

# Linked region field (top-level, not in a struct)
link region = [ type:region:type:zone ]

# Deploy
deploy my-stack to aws as prod { region: [ us-east:1  us-east:2 ] }

# Type function (named, 1-param)
type svc_gen(s: service) = {
    sg: aws_sg { name: {s.name}-sg }
}

# Type function (anonymous, 1-param — auto-fires for every service during walk)
type (svc: service) = {
    x:   aws_iam_role  { name: {svc.name}-x }
    sg:  aws_security_group { name: {svc.name}-sgs }
}

# Type function (anonymous, 2-param — fires for every (service → database) access pair)
type (from: service, to: database) = {
    rule: aws_vpc_security_group_ingress_rule { from_sg: {from.name}-sgs  to_sg: {to.name}-sgd  from_port: 5432  to_port: 5432 }
}
```

**Key syntax rules (current implementation):**
- `{param:field}` in type fn bodies = param substitution at fire-time
- `{param:name}` = special intrinsic (instance name, NOT a declared link)
- `{alias:field}` within a type fn body = sibling output reference (kept as opaque `Ref` at ASM level, NOT yet substituted)
- Multiple `{...}` groups in one value (e.g. `{from:name}-to-{to:name}`) — **parser support unclear, avoid**
- Service-to-service access requires explicit port: `access: [ other-svc:http ]` (bare ref fails)

**Planned changes (RFC 0017 — not yet implemented):**
- `{param:field}` → `{param.field}` — dot notation inside `{}`; `:` stays as ref accessor outside `{}`
- `{param.nested.field}` — nested field access walking dot-path through `AsmValue::Inst`
- Default values on links: `link min = 1` — applied at resolve time; missing field with no default = compile error

---

## Compilation pipeline

```
.grd source
    ↓ parse::parse()            → ParseRes  (AST: AstScope, AstTypeDef, AstInst, AstDeploy)
    ↓ resolve::resolve()        → IrRes     (IR: TypeId, LinkId, InstId, TypeFnId, IrRef)
    ↓ asm::lower()              → AsmRes    (ASM: strings only, no IDs; expansion tree)
    ↓ ground_be_terra::generate() → String (Terraform JSON)
```

### AST key types (`ast.rs`)
- `AstRef` — colon-separated segments; segment = plain atom OR `{inner:ref}` group
- `AstTypeDef { name, params: Option<Vec<AstTypeParam>>, body: AstTypeDefBody }`
  - `AstTypeDefBody::Enum | Struct | TypeFn(Vec<AstTypeFnEntry>)`
- `AstTypeFnEntry { alias, value: AstValue::Struct { type_hint, fields } }`
- `AstInst { type_name, inst_name, fields }`
- `AstDeploy { what, target, name, fields }`

### IR key types (`ir.rs`)
- `IrRef { segments: Vec<IrRefSeg> }` — resolved; seg value = Pack/Type/Link/Inst/Plain
- `IrTypeFnDef { name: Option<String>, params: Vec<IrTypeFnParam>, body: Vec<IrTypeFnEntry>, scope: ScopeId }`
- `IrTypeFnParam { name: String, ty: TypeId }`
- `IrTypeFnEntry { alias, vendor_type: TypeId, fields: Vec<IrFnBodyField> }`
- `IrFnBodyField { name, value: IrValue }` — group refs stored as `IrValue::Ref("{param:field}-suffix")`
- `IrDeployDef { what, target, name, fields, to_type_fn: Option<TypeFnId> }`
- `IrScope { types, links, insts, type_fns: HashMap<String, TypeFnId>, anon_type_fns: HashMap<TypeId, TypeFnId>, anon_pair_fns: HashMap<(TypeId, TypeId), TypeFnId> }`

### ASM key types (`asm.rs`)
All IDs → strings. Self-contained; no IrRes needed after this pass.

- `AsmRes { deploys, symbol: AsmSymbol, type_fns: Vec<AsmTypeFnDef> }`
- `AsmDeploy { target: Vec<String>, name, inst, fields, type_fn: Option<String>, expansion: Option<AsmExpansion>, overrides }`
- `AsmExpansion { inst, outputs: Vec<AsmOutput>, link_outs: Vec<AsmLinkOutput>, children: Vec<AsmExpansion> }`
- `AsmOutput { alias, vendor_type: String, fields: Vec<AsmField>, scope: Vec<String> }`
- `AsmLinkOutput { from: AsmInstRef, to: AsmInstRef, outputs: Vec<AsmOutput> }`
- `AsmTypeFnDef { name, params: Vec<AsmTypeFnParam>, scope, body: Vec<AsmTypeFnEntry> }`
- `AsmInst { type_name, name, type_hint, fields: Vec<AsmField> }`
- `AsmValue::Str | Int | Ref(String) | Variant(AsmVariant) | InstRef(AsmInstRef) | Inst(Box<AsmInst>) | Path(Vec<AsmValue>) | List(Vec<AsmValue>)`

### Expansion walk (`asm.rs expand_with_explicit_fn`)
1. Fire named (explicit) OR anonymous 1-param fn for `inst.type_name` → `outputs`
2. Collect all `InstRef` values from `inst.fields` (recursive)
3. Recurse into each ref whose type has any matching fn → `children` (cycle-guarded by name)
4. For each `(inst → ref)` pair: fire anonymous 2-param fn → `link_outs`

### Param substitution (`asm.rs substitute_value`)
- Exact match `{param:field}` → returns full `AsmValue` (preserves Inst/List/etc.)
- `{param:name}` → `AsmValue::Str(inst.name)` **(intrinsic, added recently)**
- String interpolation: replaces all `{param:field}` occurrences in a Ref string → `AsmValue::Str`
- Sibling alias refs like `{role:arn}` are NOT substituted (no inst for `role` in bindings) — stay as `Ref`

---

## stdlib (`ground_compile/src/stdlib.grd`)

Always loaded as the first compile unit (before user units). Defines:

**Core types:**
```
type zone = 1 | 2 | ... | 20
type region = eu-central | eu-west | us-east | us-west | ap-southeast
type database = { link engine = postgresql|mongodb|postgres|mysql  link version = string  link size = small|medium|large  link storage = integer }
type service = { type port = grpc|http  link image = reference  link access = [ service:(port) | database ]  link scaling = type scaling = { link min = integer  link max = integer } }  # RFC 0017: scaling defaults (min=1, max=1) planned
type stack = { link = [ type:service | type:database ] }
link region = [ type:region:type:zone ]
```

**AWS vendor type declarations** (schemas for template lookup + type checking):
```
type aws_iam_role                        = { link name = string }
type aws_iam_role_policy_attachment      = { link name = string }
type aws_security_group                  = { link name = string }
type aws_vpc_security_group_egress_rule  = { link name = string }
type aws_vpc_security_group_ingress_rule = { link from_sg = string  link to_sg = string  link from_port = integer  link to_port = integer }
type aws_cloudwatch_log_group            = { link name = string }
type aws_ecs_task_definition             = { link family = string  link container = string  link image = reference  link x_role = string  link t_role = string  link log = string }
type aws_ecs_service                     = { link name = string  link td = string  link sg = string }
type aws_appautoscaling_target           = { link name = string  link min = integer  link max = integer }
type aws_appautoscaling_policy           = { link name = string  link target = string }
type random_password                     = { link name = string }
type aws_db_subnet_group                 = { link name = string }
type aws_db_instance                     = { link identifier = string  link engine = string  link size = string  link storage = integer  link sg = string  link ng = string }
```

**Anonymous 1-param type functions:**
- `type (svc: service) = { t: aws_iam_role  x: aws_iam_role  xa: aws_iam_role_policy_attachment  sgs: aws_security_group  sgse: aws_vpc_security_group_egress_rule  log: aws_cloudwatch_log_group  td: aws_ecs_task_definition  svc: aws_ecs_service }` — `at`/`ap` entries planned (RFC 0017)
- `type (db: database) = { sgd: aws_security_group  sgde: aws_vpc_security_group_egress_rule  pw: random_password  ng: aws_db_subnet_group  db: aws_db_instance }`

**Anonymous 2-param type functions:**
- `type (from: service, to: database) = { rule: aws_vpc_security_group_ingress_rule { from_sg: {from:name}-sgs  to_sg: {to:name}-sgd  from_port: 5432  to_port: 5432 } }`
- `type (from: service, to: service) = { rule: aws_vpc_security_group_ingress_rule { from_sg: {from:name}-sgs  to_sg: {to:name}-sgs  from_port: 0  to_port: 65535 } }`

---

## Terra backend (`ground_be_terra/src/`)

### generate() flow
1. For each deploy: render `root.json.tera` (VPC, subnets, NAT, routes, ECS cluster)
2. Walk `AsmExpansion` tree via `walk_expansion()`
3. For each `AsmOutput`: `load_template(scope, vendor_type)` → render with `{ deploy, output }` context
4. For each `AsmLinkOutput` output: render with `{ deploy, from, to, output }` context
5. `merge_json(frags)` → single Terraform JSON

### Template dispatch (`load_template`)
Maps vendor_type string → `&'static str` template. Currently:
`aws_iam_role`, `aws_iam_role_policy_attachment`, `aws_security_group`, `aws_vpc_security_group_egress_rule`, `aws_vpc_security_group_ingress_rule`, `aws_cloudwatch_log_group`, `aws_ecs_task_definition`, `aws_ecs_service`, `random_password`, `aws_db_subnet_group`, `aws_db_instance`, `aws_appautoscaling_target`, `aws_appautoscaling_policy`

### Template context
- `deploy.*`: alias, region (list of [region_str, zone_str] pairs), prefix (optional)
- `output.*`: alias, vendor_type, plus all declared fields of the vendor type (fully substituted)
- `from.*` / `to.*` (link outputs only): type_name, name

### Naming convention (from `ground-terraform.md`)
Terraform resource ID = `{pfx_u}{alias_u}_{name_u}` where `_u` = replace `-` with `_`.
The `name` field from type fn body (e.g. `{svc:name}-x` = `svc-api-x`) determines `name_u`.

### Templates location
`ground_be_terra/src/templates/`
- `root.json.tera` — VPC, subnets, IGW, NAT, routes, ECS cluster
- `aws_iam_role.json.tera` — IAM role with ECS assume-role policy
- `aws_iam_role_policy_attachment.json.tera` — ECS execution policy attachment
- `aws_security_group.json.tera` — SG in VPC
- `aws_vpc_security_group_egress_rule.json.tera` — allow-all egress
- `aws_vpc_security_group_ingress_rule.json.tera` — SG ingress rule (used for both 1-param and 2-param outputs)
- `aws_cloudwatch_log_group.json.tera` — CloudWatch log group
- `aws_ecs_task_definition.json.tera` — Fargate task def with container definitions
- `aws_ecs_service.json.tera` — ECS service on Fargate
- `random_password.json.tera` — random_password provider resource
- `aws_db_subnet_group.json.tera` — DB subnet group (subnets from deploy.region)
- `aws_db_instance.json.tera` — RDS instance (size → instance_class mapping in template)
- `aws_appautoscaling_target.json.tera` — registers ECS service as scalable target; uses `output.min`, `output.max`, `output.name`
- `aws_appautoscaling_policy.json.tera` — target tracking policy; uses `output.name`, `output.target`

---

## CLI (`ground/src/main.rs`)

Reads all `*.grd` files in CWD. Always loads stdlib as unit 0.

```
ground init [--git-ignore]         # creates .ground/, writes settings.json
ground gen terra                   # compile → .ground/terra/{deploy_name}/main.tf.json
ground plan [--verbose]            # gen + tf init + tf plan (grouped by ground entity)
ground apply [--verbose]           # gen + tf init + tf apply
```

Output of `plan` groups Terraform resource changes by ground entity name (service/database) using a lookup table built from deploy members + naming prefix.

---

## Test infrastructure

### Golden fixture tests (`ground_test/src/lib.rs`)
- Files: `src/ground_test/fixtures/0001-minimal-service.md`, `0002-scaling.md`, `0003-database.md`
- Format: markdown with ` ```ground ` block (input) and ` ```json ` block (expected output)
- Runs `compile()` (stdlib included) + `generate()`, compares JSON
- Regenerate: `UPDATE_FIXTURES=1 cargo test -p ground_test`

### Golden compiler tests (`ground_compile/tests/`)
- `golden_parse_test.rs` — parser output
- `golden_ir_test.rs` — resolver/IR output
- `golden_ir_error_test.rs` — resolver error cases
- `golden_asm_test.rs` — ASM lowering + expansion output
- Helpers in `tests/helpers/golden_*_helpers.rs`
- `show(input)` — single unit, NO stdlib
- `show_multi(vec![(name, path, src), ...])` — multi-unit, NO stdlib
- Regenerate: `UPDATE_GOLDEN=1 cargo test -p ground_compile`

### Golden test principles (from `ground_compile/README.md`)
- Grouped by syntax, ordered simple → advanced
- Minimal and focused — only the minimum code to verify the tested functionality
- Comprehensive but not redundant
- Error tests in separate file (`golden_ir_error_test.rs`)
- **Any changed or new functionality must be covered by golden tests**

### ground-test (`/home/sherzod/getground/ground-test/`)
- Real AWS account, real IAM user
- Current `infra.grd`: `svc-api → svc-payments → db-main` (access chain)
- Deploy name: `ground-test-us-east`, region `us-east:1`
- Run: `cd ground-test && ground plan`

---

## devspec process

`devspec/` is historical. RFCs do NOT reflect current code.

When user requests:
1. Write `devspec/000x-rfc-feature.md` — reqs, approach, architecture (concise, technical)
2. Iterate with user
3. After user confirms: write `devspec/000x-pln-feature.md` — implementation plan
4. After user confirms plan: implement

Current devspec numbering: up to 0017.

---

## Known problems / open design issues

### 1. Type function lookup ignores scope (IMPORTANT)
`find_anon_1param_fn()` and `find_anon_2param_fn()` in `asm.rs` match by type name string only — no scope filtering. Any user-defined type named `"service"` or `"database"` will trigger stdlib type functions. Tests use `parse`/`resolve`/`lower` directly (no stdlib) to avoid this. Real fix: scope-qualified lookup.

### 2. Service-to-service access requires explicit port
`access: [ other-svc:http ]` required (not `[ other-svc ]`). The `service:(port)` union type doesn't accept bare instance refs. Options: add bare `service` to access union, or make port optional with `(opt)` marker.

### 3. Scaling not yet implemented — RFC 0017 in progress
Blocked on two prerequisite language features (see `devspec/0017-rfc-scaling.md`):
- **Dot notation inside `{}`** — `{svc.field}` replaces `{svc:field}`; parser group segment delimiter change + stdlib/fixture migration
- **Nested field access** — `{svc.scaling.min}` walks dot-path through `AsmValue::Inst`; requires `substitute_value` extension
- **Default values on links** — `link min = 1`; `IrLinkDef` gains `default: Option<IrValue>`; missing field with no default = compile error
Once those land: add `at`/`ap` entries to the service type fn in stdlib; `scaling` type defaults to `min:1 max:1`.

### 4. AWS type functions in stdlib (architectural smell)
Type functions for AWS resources live in `stdlib.grd`. Architecturally they belong in an `aws` or `aws:ecs` pack. Proper fix requires pack-from-file loading OR additional hardcoded compile units in `compile()`.

### 5. Sibling alias refs not substituted at fire time
`{role:arn}` in a multi-entry type fn body stays as `Ref("{role:arn}")` in the output. The terra backend templates don't use sibling refs currently. Needed for cross-resource Terraform references expressed in Ground.

### 6. `deploy X to aws as Y` target `aws` doesn't resolve to a type fn
Currently the type functions fire because of the anonymous 1-param walk (triggered by the stack's children), not because `aws` resolves to a named type fn. The deploy target is just a string. This is fine for now but limits override/customization.

---

## Key file quick-reference

| Task | File |
|------|------|
| Language grammar | `ground_compile/src/parse.rs` |
| Type + instance resolution | `ground_compile/src/resolve.rs` |
| ASM lowering + expansion walk | `ground_compile/src/asm.rs` |
| Stdlib types + AWS type functions | `ground_compile/src/stdlib.grd` |
| Terra backend entry | `ground_be_terra/src/lib.rs` |
| Tera templates | `ground_be_terra/src/templates/` |
| CLI main | `ground/src/main.rs` |
| Fixture tests | `ground_test/fixtures/` |
| Live infra | `../ground-test/infra.grd` |
| Naming convention | `ground_be_terra/ground-terraform.md` |
| ASM test helpers | `ground_compile/tests/helpers/golden_asm_helpers.rs` |

---

## Important constraints (from CLAUDE.md)

- **No git write operations** — commits/pushes are user's responsibility
- **No infra write operations** — never run `ground apply` or `terraform apply`
- **Consult Zeratul** (Architecture Zealot) before: new files/modules, patterns, interface changes
- **Consult Abathur** (Product Zealot) before: unclear requirements, feature edges, UX trade-offs
- **All AWS resources**: must follow terra naming pattern AND carry `ground-managed = "true"` tag
