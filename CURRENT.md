# Ground — Current State

> Last updated: 2026-04-15

## What Ground is

Architecture definition language. You write `.ground` files; infrastructure is derived. The main pipeline: `.grd` source → compiler (parse → resolve → ASM) → Terraform JSON → `terraform plan/apply`.

---

## Language (Ground Book / mvp)

**Implemented constructs:**
- `def` — enumerations, lists, structs, type aliases
- References — colon-separated segments, optional `(...)`, expression `{...}` interpolation
- Resolution strategy — priority: final ref > hook > ref > decomposition; sealed-subtree invariant
- `plan` — typed resolution root, triggers top-down resolution
- `use` — imports / wildcard (`use pack:std:*`)
- TypeScript hooks — attached at Root / Type / Link points

**Stdlib (embedded in compiler):**
- `std.grd` — core types: `service`, `database`, `deploy`, `bucket`, `domain`, `space`, `edge`, `secret`, `stack`
- `std::aws::pack.grd` — ~100 AWS vendor types (vpc, subnet, nat, iam, ecs, rds, s3, route53, alb, …)
- `std::aws::transform.ts` — 9 hooks: `make_aws_deploy`, `make_lb_listener`, `make_service`, `make_database`, `make_secret`, `make_bucket`, `make_domain`, `make_edge`, `make_space`

**MVP (marstech real infra):**
- 9 services, 1 postgres database, 2 buckets, 7 secrets, 2 domains, 4 edges, 3 spaces, 2 deploys (prd-eu, prd-me)

**Gaps / not yet spec'd:**
- `secret`, `bucket`, `domain`, `edge`, `space` — used in MVP, not documented in GROUND-BOOK.md
- `observe` field (Datadog tracing) — used but unspecified
- `size` field (small/medium/large/xlarge → CPU/memory) — no stdlib mapping
- Port protocol mapping (grpc→50051, http→8080) — TODO
- Multi-region reuse — current workaround: two `plan` statements, same stack (HACK comment in prd.grd)

---

## Compiler (`ground_compile`)

**Fully implemented end-to-end.** Pipeline: `compile(CompileReq)` → parse → resolve (7 passes) → lower to ASM → `CompileRes`.

**Parser** (`parse.rs`, `ast.rs`):
- Recursive-descent, all nodes wrapped in `AstNode<T>` with byte-offset location
- Nodes: `AstTopDef`, `AstValue`, `AstTypeDef`, `AstInst`, `AstPack`, `AstPlan`, `AstUse`

**IR / Resolve** (`ir.rs`, `resolve.rs`):
- Typed index arenas: `TypeId`, `LinkId`, `InstId`, `ScopeId`, `TypeFnId`, `HookId`
- Validates field values against link patterns; ambiguous-name detection; type functions (1- and 2-param)
- `IrHookDef` — input/output links + TS function name

**Codegen / ASM** (`asm.rs`):
- Lowers IR → `AsmRes` (symbol table + plans)
- Graph walk: reachable instances, topo-sort (leaves-first), bottom-up resolution with caching
- Hook execution: calls `ground_ts::exec::call_hook()`, serializes input to JSON, merges output back

**Weak spots:** Error diagnostics are minimal (unit/byte offset only, no pretty spans).

---

## TypeScript Engine (`ground_ts`)

Executes hook functions in-process via `deno_core` + V8.

**Flow:** strip `export` → transpile TS→JS (deno_ast/swc) → load as classic script in fresh `JsRuntime` → call named function with JSON → return JSON.

**Works:** transpilation, JSON I/O, nested objects/arrays, discriminated unions, string interpolation, conditionals.

**By design / not supported:** `import` statements, module resolution, state across calls (fresh runtime per hook).

---

## Terraform Backend (`ground_be_terra`)

Entry point: `generate()` / `generate_each()` → `plan_to_ctx()` → Tera template rendering → JSON merge.

**Implemented resources:** VPC, subnets (public/private per zone), IGW, NAT, EIP, route tables; ECS cluster/task/service (Fargate); RDS instance + subnet group; IAM roles + policy attachments; CloudWatch log groups; random password generation; security group ingress/egress rules.

**Naming:** `{prefix}{alias}-{resource}`, underscores for dashes. All resources tagged `ground-managed = "true"`.

**terra_ops:** plan/apply/init via terraform CLI; event parsing; resource change tracking (Create/Update/Replace/Delete); attribute diffs.

**Gaps:**
- No ALB / load balancer generation
- No S3, SNS, SQS, or other services
- `link_access` templates (service→service, service→database) exist but not wired into root rendering
- Container port/env var templating not implemented
- Database connection outputs not generated

---

## CLI (`ground`)

**Commands:** `init [--git-ignore]`, `gen terra`, `plan [--verbose]`, `apply [--verbose]`.

**Pipeline:**
1. `do_compile()` — reads all `.grd` recursively → `ground_compile::compile()`
2. `ground_be_terra::generate_each()` — ASM → Terraform JSON files per plan
3. `terra_ops::init/plan/apply` — runs terraform CLI
4. `TerraEnricher` (`ops_display.rs`) — maps `OpsEvent`s to Ground entities, renders colored output

**`ground_run`:** generic subprocess spawner; 3-thread model (stdout/stderr → channel → parser); emits typed `RunEvent<E>`.

**`ground_gen`:** Tera template engine wrapper + deep JSON merge utility.

**Gaps:**
- `plan` / `apply` hardcode `res.plans[0]` — only single-plan supported
- AWS provider hardcoded; settings.json provider config not wired

---

## What's working end-to-end

`.grd` source → compile (parse + resolve + hooks via TS engine) → Terraform JSON → `terraform plan/apply` with enriched output. The marstech MVP can be compiled and generates Terraform for VPC + ECS + RDS stacks.

## What's not wired yet

- ALB / load balancer (stdlib has the type, no template)
- S3 buckets, secrets (SSM/Secrets Manager), domains (Route53), edges (CloudFront)
- `link_access` security group rules between services
- Multi-plan CLI support
- Rich compiler error diagnostics
