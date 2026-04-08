# RFC 0014 — Gen Blocks

## Context

The current terra backend (`ground_be_terra`) knows too much about the Ground type tree:
it hardcodes type names, field names, and template dispatch rules in Rust. Template
context is built manually by traversing ASM internals. Any change to Ground types or the
expansion model requires Rust changes.

The goal: the backend is a dumb executor. It knows how to match gen defs to instances
and render templates. It knows nothing about what types exist, what fields mean, or how
types expand to vendor resources. All expansion rules live in Ground source.

---

## Scope

- `gen` keyword, namespace, and block syntax
- Gen ref resolution — `gen:` prefix, deploy `to` ref semantics
- Vendor type declarations (Ground-typed, field-validated, output link formulas)
- Pack hierarchy for runtime strategies
- `this` semantics in type gen vs link gen
- Context derivation from the fully resolved gen ASM tree
- ASM pre-flattening of link pairs
- Backend as gen executor: gen-driven dispatch replaces all Rust type/link hooks
- Out of scope: turing-complete field expressions, cross-gen sibling refs, deploy execution, pack loading changes

---

## Core Idea

A `gen` block lives in a pack file and declares how a Ground type expands into vendor
entities. Each field names one vendor entity (alias = field name, type hint = vendor
resource type) and defines its field values using Ground ref expressions and inline struct
syntax. The Ground resolver resolves all ref expressions — the backend receives a fully
prepared ASM gen tree and walks it to render templates.

---

## Pack Structure

Vendor type declarations live in the top-level provider pack (`pack aws`). Runtime
strategies are sub-packs (`pack ecs`, `pack eks`) and define gen blocks for Ground types.
This allows multiple runtime strategies within the same provider pack, sharing vendor
type definitions.

```
# pack aws — vendor type declarations + sub-packs

type aws_vpc = {
  link cidr_block = string
  link id         = {aws_vpc:{this:name}:id}
}
type aws_ecs_cluster = {
  link name = string
  link id   = {aws_ecs_cluster:{this:name}:id}
}
type aws_security_group = {
  link name   = string
  link vpc_id = reference
  link id     = {aws_security_group:{this:name}:id}
}
type aws_iam_role = {
  link name               = string
  link assume_role_policy = string
  link arn                = {aws_iam_role:{this:name}:arn}
}
type aws_ecs_task_definition = {
  link family             = string
  link execution_role_arn = reference
  link arn                = {aws_ecs_task_definition:{this:name}:arn}
}
type aws_ecs_service = {
  link name            = string
  link cluster         = reference
  link task_definition = reference
}
type aws_db_instance = {
  link identifier           = string
  link engine               = string
  link db_subnet_group_name = string
}
type aws_db_subnet_group = {
  link name = string
}
type aws_vpc_security_group_ingress_rule = {
  link from_port                = integer
  link to_port                  = integer
  link source_security_group_id = reference
  link security_group_id        = reference
}

pack ecs

  gen stack {
    vpc:     aws_vpc         { cidr_block: "10.0.0.0/16" }
    cluster: aws_ecs_cluster { name: {this:deploy:alias} }
    # ... nat, subnets, routes
  }

  gen service {
    sg:   aws_security_group      { name: {this:name}-sg }
    role: aws_iam_role            { name: {this:name}-role }
    task: aws_ecs_task_definition {
      family:             {this:name}-task
      execution_role_arn: {role:arn}
    }
    svc:  aws_ecs_service {
      name:            {this:name}-svc
      cluster:         # see cross-gen refs open question
      task_definition: {task:arn}
    }
  }

  gen database {
    sg:     aws_security_group  { name: {this:name}-db-sg }
    subnet: aws_db_subnet_group { name: {this:name}-subnet-group }
    db:     aws_db_instance {
      identifier:           {this:name}-db
      engine:               {this:engine}
      db_subnet_group_name: {subnet:name}
    }
  }

  gen service:access {
    rule: aws_vpc_security_group_ingress_rule {
      from_port:                {this:to:port}
      to_port:                  {this:to:port}
      source_security_group_id: {this:from:sg:id}
      security_group_id:        {this:to:sg:id}
    }
  }

pack eks
  gen stack   { ... }
  gen service { ... }
```

Deploy names the gen block explicitly — runtime choice is at deploy time:

```
deploy my-stack to aws:ecs:stack as my-stack-ecs {
  region: eu-central:1
}

deploy my-stack to aws:eks:stack as my-stack-eks {
  region: eu-central:1
}
```

`aws:ecs:stack` — `aws` is a pack, `ecs` is a sub-pack, `stack` is the gen block
(unambiguous, no `gen:` prefix needed). Switching runtimes = one word change in the
deploy statement.

---

## Gen Namespace & Ref Resolution

Gen blocks occupy a separate namespace per scope (`gen:`), alongside `type:`, `link:`,
`inst:`, and `pack:`. Standard ref disambiguation rules apply.

The deploy `to` ref expects a gen block at the tail. If the final segment is unambiguous,
no `gen:` prefix is needed. It is an error if the tail does not resolve to a gen block.

Gen block target is a ref to an existing type or link:
- `gen stack` — resolves `stack` as a type → type gen
- `gen service:access` — resolves `access` scoped inside `service` type → link gen
- Ambiguity between type and link requires `type:` or `link:` prefix
- Link defined only inside a type is an error if referenced without scope: `gen access` where
  `access` is not global → error, must use `gen service:access`

---

## `this` in Gen Bodies

The meaning of `this` depends on the gen block kind:

**Type gen** (`gen stack`, `gen service`, `gen database`) — `this` is the source instance:
- `{this:name}` — instance name
- `{this:field}` — any instance field, validated against the source Ground type's links
- `{this:deploy:field}` — deploy context field

**Link gen** (`gen service:access`) — `this` is the link relationship, not an instance.
Its intrinsic properties are `from` (source instance) and `to` (target instance):
- `{this:from:field}` — field on the source instance
- `{this:to:field}` — field on the target instance
- `{this:from:alias:attr}` — sibling vendor entity on the source side
- `{this:to:alias:attr}` — sibling vendor entity on the target side
- `{this:deploy:field}` — deploy context (valid in both gen kinds)

`from` and `to` are not reserved keywords — they are only meaningful as the first segment
after `this` in a link gen body, where `this` is known to represent a relationship. In a
type gen body, `{this:from}` resolves to an instance field named `from` as usual.

**Sibling refs** — in both gen kinds, `{alias:attr}` references a sibling vendor entity
defined in the same gen block. The resolver expands these using the output link formula
declared on the vendor type — no backend knowledge required. Example: `{role:arn}` where
`role` is `aws_iam_role` → resolver reads `aws_iam_role.arn` formula
`{aws_iam_role:{this:name}:arn}`, substitutes `role`'s resolved name → fully resolved
reference value passed to the template.

---

## Vendor Types

Vendor types are declared in the top-level provider pack and available to all sub-packs.
They are fully typed Ground types — the resolver validates gen body field names and
sibling ref attributes against their declared links.

Two kinds of links:
- **Input links** — set in gen body structs (`name`, `engine`, `cluster`, ...)
- **Output links** — carry a ref expression formula using `{this:name}` to construct a
  fully qualified resource reference. When a sibling ref like `{sg:id}` is encountered,
  the resolver finds `sg`'s vendor type (`aws_security_group`), reads the `id` output
  link formula (`{aws_security_group:{this:name}:id}`), substitutes `{this:name}` with
  `sg`'s resolved name → produces `{aws_security_group:api-sg:id}`. The backend receives
  this as a pre-resolved reference value.

Using an undeclared vendor field in a gen body is a compile error. Output link formulas
are resolved entirely by Ground — the backend knows nothing about vendor naming or
resource reference patterns.

---

## Templates

Each gen field alias maps to a template by name: field `sg` in `gen service` inside
`pack ecs` → `templates/ecs/service/sg.tera`. The template receives the fully resolved
gen ASM subtree for that field as its context. All values are pre-resolved — templates
are structural shells with no variable setup, no type dispatch, no conditionals beyond
optional fields.

```json
// templates/ecs/service/sg.tera
{
  "name": "{{ name }}",
  "vpc_id": "{{ vpc_id }}",
  "tags": { "ground-managed": "true" }
}
```

---

## Backend Execution

`ground_be_terra` becomes a gen executor with no knowledge of type names, field names,
or vendor resource patterns:

```
load pack tree (compiled + lowered Ground source)
for each deploy:
  resolve deploy.to ref → entry gen block (e.g. aws:ecs:stack)
  1. render entry gen fields:
       for each gen field → templates/{sub-pack}/{entry-type}/{alias}.tera
  2. for each inst in deploy.members (pre-computed by ASM):
       find gen block by inst.type_name in sub-pack gen namespace
       for each gen field → render templates/{sub-pack}/{type}/{alias}.tera
  3. for each (source, target) in deploy.links (pre-flattened by ASM):
       find link gen block whose target link matches the relationship
       for each gen field → render templates/{sub-pack}/{link-scope}/{alias}.tera
  4. merge all fragments → final JSON
```

---

## Compiler Changes

### Parse

- New `AstDef::Gen(AstGenDef)`
- `AstGenDef { target: AstRef, fields: Vec<AstGenField> }` — target resolved against type/link namespaces
- `AstGenField { alias: String, type_hint: AstRef, value: AstValue }`
- `AstValue::Interp(AstRefExpr)` — `{this:field}`, `{alias:attr}` expressions
- `AstRefExpr` — colon-separated ident segments; first = `this` or sibling alias

### IR

- `IrGenDef { target: IrGenTarget, fields: Vec<IrGenField> }`
- `IrGenTarget::Type(TypeId)` | `IrGenTarget::Link(LinkId)` — determines `this` semantics;
  link target → link gen (`this` = relationship), type target → type gen (`this` = instance)
- `IrGenField { alias: String, vendor_type_id: TypeId, value: IrGenValue }`
- `IrGenValue::Interp(IrRefExpr)` | `IrGenValue::Struct(Vec<IrGenField>)` | `IrGenValue::List(...)`
- `IrRefExpr` segments: `This | Deploy | From | To | Alias(String) | Field(String)`
  — `From` and `To` only valid after `This` in a link gen body
- `IrRes` gains `gen_defs: Vec<IrGenDef>`

### Resolver

- New pass: resolve gen `target` ref → `TypeId` or `LinkId`; ambiguity requires `type:` or `link:` prefix
- New pass: resolve `type_hint` → `TypeId` (vendor type, must be declared in scope)
- Validate gen body field names against vendor type's declared input links
- Validate `{this:field}` segments against source Ground type's links (type gen only)
- Validate `{this:from:*}` / `{this:to:*}` only in link gen bodies (`target: Link`)
- Validate `{alias:attr}` — alias defined in same gen block; attr declared on vendor type
- Resolve output link formulas: substitute `{this:name}` with sibling's resolved name
- Deploy `to` ref resolution: tail segment resolved against gen namespace

### ASM

- `AsmGenDef` mirrors `IrGenDef` with all IDs replaced by strings
- Interp expressions carry fully resolved ref chains
- `AsmDeploy` gains `links: Vec<(AsmInstRef, AsmInstRef)>` — pre-flattened `(source, target)`
  pairs, collected by walking all instance fields including variant payloads (sum types).
  Circular tail references (A→B→A) are represented as `InstRef` pairs; backend resolves
  them by name lookup in `AsmSymbol`
- `AsmDeploy.members` already pre-computed; backend iterates it directly, no tree traversal

---

## Open Questions

1. **Cross-gen sibling refs** — a type gen (`gen service`) needs to reference vendor
   entities produced by the entry gen (`gen stack`), e.g. the ECS cluster id. Currently
   there is no syntax for referencing a sibling from a different gen block in the same
   sub-pack. This is out of scope for this RFC and will be addressed during implementation.
