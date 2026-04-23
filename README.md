# The Ground

**Infrastructure as Derivation.**

Ground helps you define your system and generate infrustructure code from that definition. You focus on the architecture and let ground **derive** low level details: networking, roles, clusterss and other boilerplate.

**Heuristic for std layer:** a concept belongs in `std` if it is (1) consistent across all vendors and (2) architecturally intentional — the architect consciously names and places it, rather than it being auto-generated per-service. IAM roles and security groups fail criterion 2 (derived). CloudWatch log groups fail both. `database`, `bucket`, `secret`, `domain` pass both.

In general we want to keep templates layer dumb: simple foreach, ifs, no new concepts are created in it. The vendor layer must mirror the **complete** Terraform resource structure — every resource type and every attribute, including those that are fully derived (security groups, IAM roles, route tables, CloudWatch log groups, etc.). Templates receive fully-resolved vendor entities and only render them; they never invent structure.

---
## CLI

```
ground init [--git-ignore]   create .ground/ + settings.json; patch .gitignore
ground fmt                  format all .grd files in the nearest Ground project
ground status               print the nearest Ground project root
ground gen terra             write .ground/terra/<deploy>/main.tf.json
ground gen types             write .ground/types/*.gen.d.ts
ground lsp start             start the Ground language server
ground lsp stop              stop the Ground language server
ground plan                  plan changes per Ground entity (no apply)
ground apply                 apply changes
```
---
## Testing

Golden fixtures live in `src/ground_test/fixtures/`.

Each fixture directory contains:

- `test.grd`
- `test.ts` (optional)
- `manifest.json.tera`
- helper `*.tera` files
- `test.golden`
- `expected/` rendered files

Regenerate expected output after render changes:

```
UPDATE_GOLDENS=1 cargo test -p ground_test
```

## Todo

- bare enum variants in value position are currently accepted through expected-type resolution, e.g. `domain: vpc`
  when the field type is a nested enum like `def domain = vpc | standard`
  this works, but is not very readable; consider requiring an explicit qualifier such as `domain:vpc`

## Future Ideas

- Introduce pack versioning
- Introduce github imports with vendoring (golang style)
- Someday maybe find a way to let Ground reason about generated IDs or Terraform outputs without collapsing everything back into low-level Terraform thinking.
- Split vendor derivation into explicit node-level and edge-level transforms.
  A node transform would derive resources intrinsic to one entity (`service`, `database`, `bucket`, ...). An edge transform would derive resources produced by relationships between entities, especially `access` links.
- Make cross-entity effects first-class, especially for access links.
  For example, `service -> database` access should be modeled explicitly as something that produces database-side ingress rules, rather than as a hidden side effect inside one large deploy mapper.
- Keep transformation inputs small and explicit instead of relying on one large deploy mapper.
  Each transform should take only the context it actually needs, so the computation is easier to reason about, test, and evolve.
- Keep templates dumb: rendering only, no new concepts or structural derivation.
  Templates should receive fully derived vendor-facing entities and only serialize them into the target format.
- Support layered architecture across teams, repos, and operational views.
  Different teams should be able to model the same system with different notions of what is architecturally visible and important at their layer. Application teams may think in terms of services, databases, and queues; platform teams in terms of clusters, networks, routing, and state boundaries. Each layer should compose over the previous one rather than redefining it, while allowing some details to remain hidden until they become relevant to the next layer.
  Example:
  ```ground
  # app team repo: defines the service/data architecture
  payments = space {
    services: [ api worker ]
  }

  # platform team repo: imports that space and binds it to ops reality
  use github.com/my-org/payments

  plan payments:space {
    cluster: prod-ecs
    network: prod-vpc
  }
  ```
