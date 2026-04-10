# ground

**Infrastructure as Derivation.**

Ground helps you define your system and generate infrustructure code from that definition. You focus on the architecture and let ground **derive** low level details: networking, roles, clusterss and other boilerplate.

```ground
database db-main {
  engine:  postgres
}

service users {
  image: users:latest
  access: [ database:main ]
}

service payments {
  image: payments:latest
  access: [ database:main ]
}

service api {
  image:  api:latest
  access: [
    service:users:grpc
    service:payments:grpc
  ]
}

stack shop {
  database:main
  service:users
  service:payments
  service:api
}

deploy shop to aws as shop-eu-central {
  region: eu-central
}
```

**Heuristic for std layer:** a concept belongs in `std` if it is (1) consistent across all vendors and (2) architecturally intentional — the architect consciously names and places it, rather than it being auto-generated per-service. IAM roles and security groups fail criterion 2 (derived). CloudWatch log groups fail both. `database`, `bucket`, `secret`, `domain` pass both.

In general we want to keep templates layer dumb: simple foreach, ifs, no new concepts are created in it. The vendor layer must mirror the **complete** Terraform resource structure — every resource type and every attribute, including those that are fully derived (security groups, IAM roles, route tables, CloudWatch log groups, etc.). Templates receive fully-resolved vendor entities and only render them; they never invent structure.

---
## CLI

```
ground init [--git-ignore]   create .ground/ + settings.json; patch .gitignore
ground gen terra             write .ground/terra/<deploy>/main.tf.json
ground plan                  plan changes per Ground entity (no apply)
ground apply                 apply changes
```
---
## Testing

Golden fixtures live in `ground_test/fixtures/` — `.md` files with a *ground* input block and a *json* expected output block.

Regenerate expected output after generator changes:

```
UPDATE_FIXTURES=1 cargo test -- files
```
---

