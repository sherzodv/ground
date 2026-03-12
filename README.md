# ground

**Infrastructure as Derivation.**

Describe your system — services, databases, access rules — and ground generates the infrastructure. No networking boilerplate, no manual secrets wiring, no IAM policies by hand. IaD, not IaC.

```ground
service api {
  image:  api:prod
  access: [ svc-core:http  db-main ]
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

Ground reads `.grd` files and generates Terraform JSON. Networking, security groups, IAM roles, log groups — all derived automatically from the system description.

See [`syntax.md`](syntax.md) for the full DSL reference.

---

## CLI

```
ground init [--git-ignore]   create .ground/ + settings.json; patch .gitignore
ground gen terra             write .ground/terra/<deploy>/main.tf.json
ground plan                  plan changes per Ground entity (no apply)
ground apply                 apply changes
```

---

## What ground generates

**Per service**

| Ground | AWS |
|--------|-----|
| workload | `aws_ecs_task_definition`, `aws_ecs_service` |
| identity | `aws_iam_role` ×2 (task + exec), `aws_iam_role_policy_attachment` |
| network | `aws_security_group`, `aws_vpc_security_group_egress_rule` |
| logs | `aws_cloudwatch_log_group` |
| scaling | `aws_appautoscaling_target`, `aws_appautoscaling_policy` |
| access rule | `aws_vpc_security_group_ingress_rule` per target |

**Per database**

| Ground | AWS |
|--------|-----|
| instance | `aws_db_instance`, `random_password` |
| network | `aws_security_group`, `aws_db_subnet_group` |

**Per deploy**

| Ground | AWS |
|--------|-----|
| cluster | `aws_ecs_cluster` |
| network | `aws_vpc`, `aws_subnet` ×2 per zone, `aws_internet_gateway`, `aws_eip`, `aws_nat_gateway`, `aws_route_table` ×2, `aws_route` ×2, `aws_route_table_association` ×2 |

---

## Architecture

```
.grd files
    │
    ▼
ground_parse     — pest grammar + AST → parse_to_items()
    │
    ▼
ground_compile   — symbol table + resolve → Spec  (loads stdlib)
    │
    ▼
ground_gen       — template engine + JSON merger
    │
    ▼
ground_be_terra  — hook templates (root / type / link) → Terraform JSON
    │
    ▼
terra_ops        — terraform binary automation (plan, apply)
```

## Crates

| Crate | Role |
|-------|------|
| `ground_parse` | Pest grammar, AST types, CST→AST conversion. |
| `ground_compile` | Symbol table, type resolution, stdlib. Outputs `Spec`. |
| `ground_core` | Shared data types: `Spec`, `ScalarValue`, `ParseError`. No external deps. |
| `ground_gen` | Tera template rendering and JSON fragment merging. |
| `ground_be_terra` | Terraform backend: hook templates + `terra_ops` (plan/apply). |
| `ground_test` | Integration tests and `.md` golden fixtures. |
| `ground` | CLI binary. |

---

## Testing

Golden fixtures live in `ground_test/fixtures/` — `.md` files with a `ground` input block and a `json` expected output block.

Regenerate expected output after generator changes:

```
UPDATE_FIXTURES=1 cargo test -- files
```

---

## Advanced: Semantic Core

Ground's type system is built on two primitives — `type` and `link` — that define both the structure and semantics of all constructs. The built-in `service`, `database`, and `deploy` keywords are instances of this core, not special syntax.

This makes the system extensible: custom types and links can be declared alongside built-ins using the same rules.

See [`devspec/0004-rfc-semantic-core.md`](devspec/0004-rfc-semantic-core.md) for the full design, and [`syntax.md`](syntax.md#advanced-semantic-core) for the syntax reference.

---

## For Coding Agents

Never ever do infra, git or other changes other than current project file changes.
Do not assume, try to discuss if there is no sufficient info before taking any actions.
For any big changes: first show what are you going to do and only do after user confirmation.
RFC process can be requested by user:
  - Be concise and technical, no story telling
  - Feature is designed in a corresponding devspec/000x-rfc-feature.md: reqs, approach, architecture, tech reqs, libs etc.
  - Discuss and iterate with user on the rfc
  - After rfc is confirmed as finished by the user, create a corresponding devspec/000x-pln-feature.md with implementation plan
  - Iterate with user on the implementation plan
  - After user confirms the plan proceed with the implementation
