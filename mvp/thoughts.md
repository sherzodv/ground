# Transformation layer design

## Context

Ground needs a transformation layer that maps `std` entities to vendor (`std:aws`) entities.
The transformation step is plan step 3: the most complex and crucial part of the pipeline.

```
std.grd (architecture)
    ↓  transform
std/aws/pack.grd (vendor entities)
    ↓  templates
Terraform HCL
```

Complex computations arise here — CIDR subnet allocation from AZ lists, inverting the
service access graph to produce security group ingress rules, building IAM policy documents
from access links, etc. These cannot be expressed as simple pattern matches.

The natural answer is to delegate computation to TypeScript. But how to integrate it?

---

## Constraints

1. **No TypeScript in Ground files.** Ground syntax stays pure. TS lives in its own files.

2. **Cannot attach computation to links.** A link is a leaf in the deploy tree, but it can
   cause changes in arbitrary, distant places of the resulting vendor tree — e.g. a
   `service.access: [database:main]` link places an ingress rule on the *database's*
   security group, not the service's. Computation at link level hides these side effects.

3. **Cannot do big-tree → big-tree.** A single transformation from the full `deploy` tree
   to the full vendor tree is too large to reason about, test, or maintain.

**The problem:** find the underlying modularity structure.

---

## Key insight: std is a graph, not a tree

The `deploy` block looks like a tree — deploy → stack → services/databases/secrets/… —
but the `access` links between entities make it a **graph**. Entities are nodes, `access`
links are edges.

The complexity concentrates on those edges. A service's `access: [database:main]` causes
a resource to be created on the *database's* security group. This is a cross-node side
effect, not a local property of the service or the database.

---

## Solution: node rules + edge rules

Decompose along two axes that mirror the graph structure:

### Node rules — one per std entity type

Each rule takes a single std entity and produces the vendor resources intrinsic to it.
No cross-entity effects.

```
service  → aws_ecs_task_definition, aws_ecs_service,
           aws_security_group, aws_cloudwatch_log_group,
           aws_iam_role (exec + task), aws_iam_role_policy_attachment

database → aws_db_instance, aws_db_subnet_group, aws_security_group
secret   → aws_secretsmanager_secret
bucket   → aws_s3_bucket
domain   → aws_acm_certificate, aws_acm_certificate_validation,
           aws_route53_zone, aws_route53_record (cert validation)
edge     → aws_lb_target_group, aws_lb_listener_rule, aws_route53_record (dns)
space    → aws_ecs_cluster, aws_service_discovery_private_dns_namespace
deploy   → aws_vpc, aws_subnet, aws_internet_gateway, aws_nat_gateway,
           aws_route_table, aws_route_table_association,
           aws_lb, aws_lb_listener
```

### Edge rules — one per access link type

Each rule takes both endpoints of an access link explicitly and produces the vendor
resources that connect them. No hidden mutation from a distance.

```
(service, database) → aws_vpc_security_group_ingress_rule  on the database's sg
(service, service)  → aws_vpc_security_group_ingress_rule  on the target's sg
(service, secret)   → aws_iam_role_policy                  on the service's task role
(service, bucket)   → aws_iam_role_policy                  on the service's task role
```

### Properties

- No rule sees the whole tree — each has a fixed, small, explicit input.
- Cross-entity side effects become first-class: `(service, database) → ingress_rule`
  makes explicit that *this pair* produces *this resource*.
- The orchestrator is mechanical: walk the graph, apply node rules to each node,
  apply edge rules to each edge, collect all output entities.
- Each rule is independently testable with mock inputs.

---

## Ground syntax

Ground declares the rule signatures. TypeScript implements the function bodies
in a co-located `.ts` file. Ground enforces the type contract on both sides.

```ground
# std/aws/transform.grd

transform service  -> [ aws_ecs_task_definition, aws_ecs_service,
                        aws_security_group, aws_cloudwatch_log_group,
                        aws_iam_role, aws_iam_role_policy_attachment ]

transform database -> [ aws_db_instance, aws_db_subnet_group, aws_security_group ]
transform secret   -> [ aws_secretsmanager_secret ]
transform bucket   -> [ aws_s3_bucket ]
transform domain   -> [ aws_acm_certificate, aws_acm_certificate_validation,
                        aws_route53_zone, aws_route53_record ]
transform edge     -> [ aws_lb_target_group, aws_lb_listener_rule, aws_route53_record ]
transform space    -> [ aws_ecs_cluster, aws_service_discovery_private_dns_namespace ]
transform deploy   -> [ aws_vpc, aws_subnet, aws_internet_gateway, aws_nat_gateway,
                        aws_route_table, aws_route_table_association,
                        aws_lb, aws_lb_listener ]

transform service:access:database -> [ aws_vpc_security_group_ingress_rule ]
transform service:access:service  -> [ aws_vpc_security_group_ingress_rule ]
transform service:access:secret   -> [ aws_iam_role_policy ]
transform service:access:bucket   -> [ aws_iam_role_policy ]
```

---

## Final syntax: expand bindings with explicit context

The `transform` keyword above was a stepping stone. The final design replaces it with
`expand` — a binding that names both the type function AND the context passed to it.
Context is explicit: you see exactly what flows from the deploy graph into the function.

### Node bindings

```ground
expand std:service with service_in_aws {
    sizing:  deploy.sizing[service.name]
    cluster: deploy.spaces[service.name].cluster
    vpc:     deploy.vpc
    subnets: deploy.subnets.private
}

expand std:database with database_in_aws {
    sizing:  deploy.db
    vpc:     deploy.vpc
    subnets: deploy.subnets.private
}

expand std:secret  with secret_in_aws  { alias: deploy.alias }
expand std:bucket  with bucket_in_aws  { alias: deploy.alias }
expand std:domain  with domain_in_aws  { alias: deploy.alias }
expand std:edge    with edge_in_aws    { lb: deploy.lb }
expand std:space   with space_in_aws   { vpc: deploy.vpc }
expand std:deploy  with deploy_in_aws  { }
```

### Edge bindings

Edge bindings can reference node expansion outputs. Ground resolves node expansions first.

```ground
expand std:service:access:database with service_database_in_aws {
    caller_sg: service_in_aws[caller].security_group
    target_sg: database_in_aws[target].security_group
}

expand std:service:access:service with service_service_in_aws {
    caller_sg: service_in_aws[caller].security_group
    target_sg: service_in_aws[target].security_group
}

expand std:service:access:secret with service_secret_in_aws {
    task_role: service_in_aws[service].task_role
    secret_sm: secret_in_aws[secret].secret
}

expand std:service:access:bucket with service_bucket_in_aws {
    task_role: service_in_aws[service].task_role
    bucket_s3: bucket_in_aws[bucket].bucket
}
```

### Why explicit context

Explicit context makes each binding self-documenting: you see exactly what deploy
information flows into each type function. It also keeps type functions themselves
context-free — they take only what they're given, making them independently testable
with mock inputs, with no implicit access to global deploy state.

---

## Lifecycle and dependency ordering

### The problem

When type functions produce values consumed by other type functions, there is an implicit
ordering: producer must resolve before consumer. Node expansions must resolve before edge
expansions that reference their outputs. Within a deploy, `aws_ecs_service` needs
`aws_ecs_task_definition` to exist first. This is the same ordering problem Terraform
faces.

### Terraform's solution — and Ground's

Terraform solves this through **attribute references as implicit dependencies**:

```hcl
resource "aws_ecs_service" "x" {
    task_definition = aws_ecs_task_definition.x.arn  # reference = implicit dep
    cluster         = aws_ecs_cluster.x.id            # reference = implicit dep
}
```

Terraform infers: create `task_definition` and `cluster` before `service`. The reference
graph is the dependency graph. Terraform topologically sorts it.

Ground uses the same principle through **args as dependencies**:

```ground
type aws_ecs_service(
    task_definition: aws_ecs_task_definition   # arg = dep
    cluster:         aws_ecs_cluster           # arg = dep
    sg:              aws_security_group        # arg = dep
) = make_ecs_svc { ... }
```

If a type function takes another type's output as an arg, Ground knows that type must
resolve first. The arg graph is the dependency graph. Ground topologically sorts it and
resolves in that order. Cycles are a compile error.

Edge bindings automatically run after node bindings because they reference node outputs
as args — the ordering is derived, not declared.

### No hidden dependencies

Terraform provides `depends_on` for cases where attribute references don't capture the
full dependency (e.g. an IAM policy that must exist before an ECS service starts, even
though the service doesn't reference the policy's ARN). In Ground, hidden dependencies
are not allowed — if a dep isn't captured by an arg, the arg list is incomplete. Make
it explicit. This is stricter than Terraform but produces a fully verifiable dep graph.

### Operational lifecycle

`create_before_destroy`, `prevent_destroy`, `ignore_changes` — these are operational
concerns, not architectural ones. They belong in vendor type definitions or templates,
not in Ground's resolution model. The vendor type for `aws_acm_certificate` knows it
needs `create_before_destroy`; Ground's type system doesn't need to model it.

---

## How the design evolved — the breakthrough

The `transform` syntax above was a stepping stone, not the final answer. The problem with
it: `transform` is ad-hoc syntax bolted onto Ground. It sits outside the type system, uses
a separate keyword, and requires a separate orchestration engine to run the rules. The
modularity is right but the integration is wrong.

The question became: **how does Ground itself initiate and orchestrate the transformation,
using its own language rather than a foreign DSL?**

### Step 1 — reject obvious approaches

Three approaches were considered and rejected:

**Embed TypeScript in Ground files.** Clean boundary collapses. Ground syntax gets
polluted with a second language. Rejected immediately.

**Attach computation to links.** A link is a leaf. But transforming a leaf (e.g.
`service.access: [database:main]`) can affect arbitrary distant places in the vendor
tree — an ingress rule on the *database's* security group. Computation at the leaf hides
cross-entity side effects. Reasoning breaks down.

**Big-tree → big-tree.** One function takes the full deploy tree and returns the full
vendor tree. Too large to reason about, test, or maintain.

### Step 2 — Ground already has everything needed

Ground's existing generation mechanism (anonymous type functions that fire on resolution)
already does exactly what's needed:

```ground
type (svc: service) = {
    sg: aws_security_group { name: {svc.name}-sg }
}
```

When `service` instances are resolved during stack application, this function fires
automatically — Ground is already the orchestrator. No separate engine needed.

The same mechanism handles edges naturally:

```ground
type (from: service, to: database) = {
    rule: aws_security_group_rule { from_sg: {from.name}-sg  to_sg: {to.name}-sg }
}
```

Ground walks the access links and fires two-parameter functions for each pair.
The node/edge modularity falls out of the type system for free.

### Step 3 — the missing piece: TypeScript for computation

Ref expressions (`{svc.name}-sg`) handle simple derivation but not real computation —
CIDR arithmetic, policy document construction, graph inversions. These need TypeScript.

The integration question: where does the TypeScript function name appear in Ground syntax,
without TypeScript appearing *in* Ground syntax?

The answer: **as an optional token between `=` and the output body of a type function.**

```ground
type (svc: service, vpc_id: string) = make_service {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}
```

- `( )` — input structure, Ground types and links
- `= make_service` — the TypeScript function name (optional hook)
- `{ }` — output structure, Ground types and links

Ground generates TypeScript interfaces for both sides. The user implements the pure
function. No TypeScript appears in the Ground file.

### Step 4 — optional hook, mandatory at terminals

The function name is optional. Without it, Ground fills output values via ref expressions
or further type decomposition. With it, TypeScript fills them.

The function name is **mandatory** only when all output links are primitive types
(`string`, `int`, `boolean`) — Ground cannot produce primitive values through
decomposition alone. These are "terminal constructs."

This gives full control over granularity:

```ground
# coarse — TypeScript owns everything below this point
type (svc: service, vpc_id: string) = make_service {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}

# fine — Ground holds the structure, TypeScript only at the leaves
type (svc: service, vpc_id: string) = {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}

type aws_security_group(svc: service, vpc_id: string) = make_sg {
    link name   = string   # terminal — mandatory hook
    link vpc_id = string   # terminal — mandatory hook
}
```

Both produce identical output. The hook placement is an implementation choice, not an
architectural one.

### Why this is the right design

The final design has no new concepts beyond one optional token. Ground's existing
mechanisms — types, links, anonymous generation functions, stack resolution — are the
orchestration engine. TypeScript is purely computational: pure functions, generated
interfaces, no knowledge of Ground's structure.

The full pipeline is stratified with clean boundaries at every layer:

```
std.grd            types & links          Ground   human-readable architecture
std/aws/pack.grd   vendor types & links   Ground   mirrors Terraform exactly
transform.grd      type functions         Ground   declares input/output shapes
transform.ts       pure functions         TypeScript  computation only
templates          foreach / if           dumb     no new concepts
```

Each layer speaks only to the adjacent one. Ground orchestrates the whole thing by
walking the graph it already holds. The TypeScript functions are small, independently
testable, and have no framework to understand — just typed inputs in, typed outputs out.
