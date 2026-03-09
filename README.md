# ground

A cloud-agnostic infrastructure tool with a declarative DSL. Define services, access rules, and placement once — generate provider-specific infrastructure.

```
service svc-api {
  image:   svc-api:prod
  scaling: { min: 2, max: 10 }
}
```

See [`devspec/0001-rfc-ground.md`](devspec/0001-rfc-ground.md) for the full DSL spec, primitives, and examples.

## Ownership modes

**Ground owns everything** — describe your services and stacks, Ground generates
the full infrastructure: network, cluster, subnets, and services. No external
inputs required. Best for new projects and small teams that want zero infra
overhead.

**Ground references existing infra** — large teams often share a VPC, cluster,
or subnet set across many projects managed by a platform team. Ground can
reference those existing entities instead of creating its own. The surrounding
infra is yours; Ground owns only the service layer.


## For Coding Agents

Never ever do infra, git or other changes other than current project file changes.
Do not assume, try to discuss if there is no sufficient info before taking any actions.
For any big changes: first show what are you going to do and only do after user confirmation.
RFC process can be requested by user:
  - Be concise and techinical, no story telling
  - Feature is designed in a corresponding devspec/000x-rfc-feature.md: reqs, approach, archtecture, tech reqs, libs etc.
  - Discuss and iterate with user on the rfc
  - After rfc is confirmed as finished by the user, create a correspnding devspec/000x-pln-feature.md with implementation plan
  - Iterate with user on the implementation plan
  - After user confirms the plan proceed with the implementation

## Architecture

```
.grd files
    │
    ▼
ground_parse     — pest grammar + semantic validation → Spec
    │
    ▼
ground_core
  high::Spec     — ground abstractions (Service, Scaling, …)
  compile        — high → low transformation
  low::Plan      — provider-agnostic primitives (Workload, Identity, Scaler, …)
    │
    ▼
ground_be_terra
  terra_gen      — Plan → Terraform JSON
  terra_ops      — Terraform binary automation (plan, apply, …)
```

## Crates

| Crate | Role |
|-------|------|
| `ground_core` | Pure data types and transformations. No external deps. |
| `ground_parse` | `.grd` parser. See [`src/ground_parse/README.md`](src/ground_parse/README.md). |
| `ground_be_terra` | Terraform JSON generation and operations. |
| `ground_test` | Integration tests and `.md` golden fixtures. |
| `ground` | CLI binary. |

## Testing

Unit tests live in `ground_test`. Golden fixtures are `.md` files in `ground_test/fixtures/` containing a `ground` input block and a `json` expected output block.

Regenerate expected output after generator changes:

```
UPDATE_FIXTURES=1 cargo test -- files
```
