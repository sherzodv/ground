# RFC 0009 — Gen Construct

## Context

Ground currently transforms source concepts into target infrastructure via Tera templates
in `ground_be_terra`. The expansion rules (one concept → multiple target entities) and
naming patterns (cross-entity references) are buried in Rust template-dispatch logic and
Tera template bodies. This RFC introduces `gen` as a first-class language construct that
brings expansion and naming into Ground syntax, making backends declarative and readable.

Templates remain as an escape hatch for turing-complete field logic.

---

## Scope

- New `gen` keyword and syntax
- Type hints on struct values (vendor entity types)
- Template references in field values
- `this` and sibling ref expressions in gen bodies
- Parser, IR, resolver, ASM changes
- Out of scope: backend serialization format, pack loading, deploy execution changes

---

## Core Idea

A `gen` block defines how a ground type expands into vendor entities within the context
of an active pack. The pack declares the vendor target (`pack aws`). The `gen` block
names the type it applies to and enumerates the vendor entities produced, with named
aliases that become cross-referenceable within and across gen bodies.

```
pack aws

gen service {
  sg: aws_security_group {
    name: {this.name}-sg
  }
  role: aws_iam_role {
    name: {this.name}-role
  }
  task: aws_ecs_task_definition {
    family: {this.name}-task
    execution_role_arn: {role.arn}
    container_definitions: template(ecs_container)
  }
  svc: aws_ecs_service {
    name: {this.name}-svc
    cluster: {this.deploy.cluster}
    task_definition: {task.arn}
    network_configuration: {
      security_groups: [{sg.id}]
    }
  }
}
```

---

## Syntax

```
gen_def     = "gen" ident block
gen_body    = gen_field*
gen_field   = ident ":" type_hint? (struct_val | scalar_val)
type_hint   = ident                  # vendor entity type name
scalar_val  = template_expr | interp_expr | primitive
template_expr = "template" "(" ident ")"
interp_expr   = "{" ref_expr "}"
ref_expr      = "this" ("." ident)*
              | ident ("." ident)*   # sibling alias ref
```

Field values inside gen bodies support:
- `{this.field}` — access source instance field
- `{this.deploy.field}` — access deploy context field
- `{this.from.alias.attr}` / `{this.to.alias.attr}` — for link gen, navigate both sides
- `{alias.attr}` — reference a sibling entity's output attribute
- `template(name)` — delegate to a named template fragment (escape hatch)
- Bare string concatenation: `{this.name}-sg` — inline interpolation with literals

---

## Type Hints

A struct value in a gen field may be prefixed with a type hint:

```
sg: aws_security_group { ... }
```

The type hint is a vendor entity type name. It is opaque to the Ground compiler — passed
through to the backend as metadata. The compiler resolves it as a string, not as a Ground
type. This allows backends to use it for target schema validation or serialization format
selection.

Type hints are supported at all compiler layers: parse, IR, ASM.

---

## Semantics

### Expansion

Each field in a gen body declares one vendor entity. The field alias (`sg`, `role`, etc.)
is the local name for that entity within the gen block scope. All aliases are mutually
visible within the gen body — order is irrelevant, the compiler resolves the dependency
graph.

### Naming and cross-references

`{alias.attr}` binds to the output attribute of a sibling vendor entity. The compiler
records this as a typed edge in the expansion graph. The backend resolves the actual
attribute value (e.g. `arn`, `id`, `name`) at generation time.

For link gen blocks, `{this.from.alias.attr}` and `{this.to.alias.attr}` navigate into
the gen expansion of the source and target instances respectively.

### Template escape hatch

`template(name)` marks a field value as delegated to a named template fragment.
The template receives the fully resolved gen context (all `this.*` values, all sibling
aliases) as its input. Templates are backend-specific files, located by the backend
loader. They are narrow by design — they fill in a single field value, not a whole entity.

---

## Examples

### Terraform AWS

```
pack aws

gen service {
  sg: aws_security_group {
    name: {this.name}-sg
  }
  role: aws_iam_role {
    name: {this.name}-role
  }
  task: aws_ecs_task_definition {
    family: {this.name}-task
    execution_role_arn: {role.arn}
    container_definitions: template(ecs_container)
  }
  svc: aws_ecs_service {
    name: {this.name}-svc
    cluster: {this.deploy.cluster}
    task_definition: {task.arn}
    network_configuration: {
      security_groups: [{sg.id}]
    }
  }
}

gen database {
  sg: aws_security_group {
    name: {this.name}-db-sg
  }
  subnet_group: aws_db_subnet_group {
    name: {this.name}-subnet-group
  }
  db: aws_db_instance {
    identifier: {this.name}-db
    engine: {this.engine}
    instance_class: {this.size}
    db_subnet_group_name: {subnet_group.name}
    vpc_security_group_ids: [{sg.id}]
  }
}

gen access {
  rule: aws_security_group_rule {
    type: ingress
    from_port: {this.to.port}
    to_port: {this.to.port}
    source_security_group_id: {this.from.sg.id}
    security_group_id: {this.to.sg.id}
  }
}
```

### Kubernetes

```
pack k8s

gen service {
  dep: Deployment {
    metadata: {
      name: {this.name}
    }
    spec: {
      containers: [
        {
          image: {this.image}
          ports: [
            {
              containerPort: {this.port}
            }
          ]
        }
      ]
    }
  }
  svc: Service {
    metadata: {
      name: {this.name}
    }
    spec: {
      selector: {
        app: {dep.metadata.name}
      }
      ports: [
        {
          port: {this.port}
          targetPort: {this.port}
        }
      ]
    }
  }
}

gen access {
  policy: NetworkPolicy {
    metadata: {
      name: {this.from.name}-to-{this.to.name}
    }
    spec: {
      podSelector: {
        matchLabels: {
          app: {this.to.dep.metadata.name}
        }
      }
      ingress: [
        {
          from: [
            {
              podSelector: {
                matchLabels: {
                  app: {this.from.dep.metadata.name}
                }
              }
            }
          ]
        }
      ]
    }
  }
}
```

---

## Compiler Changes

### Parse

- New `AstGenDef { type_name: String, fields: Vec<AstGenField> }`
- New `AstGenField { alias: String, type_hint: Option<String>, value: AstValue }`
- `AstValue::Template(String)` — new variant for `template(name)`
- `AstValue::Interp(AstInterpExpr)` — interpolated expression `{...}`
- `AstInterpExpr` — chain of ident segments, first segment is `this` or a sibling alias
- Added to `AstDef::Gen(AstNode<AstGenDef>)`

### IR

- New `IrGenDef { type_id: TypeId, fields: Vec<IrGenField> }`
- New `IrGenField { alias: String, type_hint: Option<String>, value: IrGenValue }`
- `IrGenValue::Template(String)` | `IrGenValue::Interp(IrInterpExpr)` | `IrGenValue::Struct(...)` | `IrGenValue::List(...)`
- `IrInterpExpr` — resolved ref chain: `IrInterpSeg::This | Alias(String) | Field(String)`
- `IrRes` gains `gen_defs: Vec<IrGenDef>`

### Resolver

- New resolver pass: `resolve_gen_defs`
- Resolves `type_name` ident to `TypeId` in current scope
- Validates sibling alias refs (`{alias.attr}`) are defined within the same gen body
- Validates `{this.*}` segments against the resolved type's field definitions
- Validates `{this.from.*}` / `{this.to.*}` only within link gen bodies
- Does NOT validate vendor type hints or vendor attribute names — those are backend concerns

### ASM

- `AsmGenDef` mirrors `IrGenDef` with all IDs flattened to strings
- Type hints passed through as opaque strings
- Interp expressions fully resolved to typed ref chains

---

## Open Questions

1. **Circular alias refs** — if `task` refs `role` and `role` refs `task`, is this an error
   or should the compiler detect and report cycles?

2. **List interpolation** — `[{sg.id}]` is a list with one interp element. Should interp
   be allowed to expand to a list (splat), e.g. `[...{this.sgs}]`?

3. **Multiple gen per type** — can one pack define multiple `gen service` blocks (e.g.
   for different deploy tiers)? If so, how does the resolver select which applies?

4. **Gen inheritance** — should gen blocks be composable (a k8s gen extending a base gen)?
   Out of scope for now but worth noting.
