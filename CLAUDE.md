Ground is an architecture definition language. Infrastructure is derived.

## Agents

**Tassadar** (Ground Compiler Expert) — delegate all exploration and questions about ground syntax:
- Root: `ground_compile/` crate
- Ground language syntax, grammar, parsing, IR, type system
- Compiler passes, codegen pipeline, error reporting
- Ask: "How does X work in ground_compile?" → spawn Tassadar to explore and report back

**Fenix** (Terraform Backend Expert) — delegate all exploration and questions about AWS Terraform generation:
- Root: `ground_be_terra/` crate
- Template rendering, resource naming, tagging rules, HCL generation
- Ask: "How is X generated in ground_be_terra?" → spawn Fenix to explore and report back

**Artanis** (Real Infra Reader) — read-only reference for real-world Terraform patterns:
- Root: `../../moontech/repo/infra-cloud-aws/` (never write, never modify)
- Use to understand how real AWS resources are structured, named, tagged in production
- Ask: "What does real infra look like for X?" → spawn Artanis to explore and report back

## Current focus

We are trying to explore minimal syntax / grammar for the Ground language that will allow us to define an architecture of any system abstracting over raw provider specific details, but still keeping it definitive. For that we use real infra in terraform and defining it in `./mvp/`. We need to keep in mind the whole flow:

1. Ground has predefined entities to describe arcitecture: std pack
2. Ground has predefined vendor entites in packs: std:aws, std:gcp etc. For now we only focus on aws
3. Ground has predefined transformations defined for std -> std:aws
4. Ground has predefined templates to render terraform out of std:aws

Current grammar is described in GROUND-BOOK.md. Nothing is set in stone, we're exploring possibilities, but we want to be consistent with basic ideas of the ground language: types & links. Some of the possibilities like nested structs are not described and implemented but used in marstech.

**File layout:** `mvp/` is the working example project:
- `mvp/std.grd` — std pack (no pack wrapper; file = pack)
- `mvp/marstech/pack.grd` — marstech root (shared declarations, e.g. `secret datadog`)
- `mvp/marstech/app.grd`, `poc.grd`, `ops.grd`, `routing.grd` — sub-packs
- `mvp/marstech/env/prd.grd`, `stg.grd` — environment spaces + deploy configs
- `mvp/marstech.tf` — hand-written Terraform derived from marstech, covering prd-eu, prd-me, stg-eu

**Pack syntax conventions:**
- File path = pack identity; no `pack foo { }` wrapper needed
- Empty bodies omitted: `service hub` not `service hub { }`
- `use` imports are file-level or inside a scope that needs them

Main questions we should always ask ourselves:

1. What to have on architecture definition layer
2. What to have on vendor layer
3. What to have on templates layer

**Heuristic for std layer:** a concept belongs in `std` if it is (1) consistent across all vendors and (2) architecturally intentional — the architect consciously names and places it, rather than it being auto-generated per-service. IAM roles and security groups fail criterion 2 (derived). CloudWatch log groups fail both. `database`, `bucket`, `secret`, `domain` pass both.

In general we want to keep templates layer dumb: simple foreach, ifs, no new concepts are created in it. The vendor layer must mirror the **complete** Terraform resource structure — every resource type and every attribute, including those that are fully derived (security groups, IAM roles, route tables, CloudWatch log groups, etc.). Templates receive fully-resolved vendor entities and only render them; they never invent structure.

Our current plan is:

1. **[ACTIVE]** Make marstech definitive and complete.
   - Method: explore real infra (via Artanis) and map every resource/pattern into marstech
   - Real infra is the floor, not the ceiling — also consider common architectural patterns not yet
     present in marstech (queues, caches, CDN, workers, etc.) and decide what belongs in std
   - Done when: marstech fully represents the real system AND exposes the language capabilities
     needed for a general architecture definition language
   - Do NOT touch the compiler, IR, templates, or backend crates during this step
2. Define comprehensive vendor entities for aws to completely cover marstech.
3. Define transformations from std types & links to std:aws — most complex and crucial step.
4. Define aws templates that will give us complete real terraform structure.

## Default behavior

1. **Read first.** Explore relevant code before asking anything.
2. **Consult agents early, not late.** Ask before implementing, not after.
3. **Implement autonomously.** Once direction is clear, write the code — no need to check in mid-task.
4. **One consultation round per feature.** Don't loop zealots repeatedly on the same task.

## Agent coordination rules

- **Spawn agents for exploration, not for decisions.** Agents read and report; you synthesize and decide.
- **One agent per domain per task.** Don't spawn Tassadar and Fenix for the same question — if the task spans both, ask one to explore first, then the other with context from the first.
- **Front-load the prompt.** Give the agent: (1) the specific question, (2) the suspected location, (3) what you already know. No vague asks.
- **Cap scope.** Ask agents to return findings in under 300 words. Summaries, not dumps.
- **Artanis is reference-only.** Never use Artanis findings as requirements — only as real-world pattern reference.
- **Parallel-spawn when independent.** If questions to Tassadar and Fenix don't depend on each other, spawn both simultaneously.

## When NOT to consult

- Obvious implementation details (variable names, local logic)
- Bug fixes with a clear root cause
- Tasks explicitly scoped by the user already
- When you've already read the relevant code yourself

## Restrictions

- Do not do writing git actions, only use git for reading & exploration, let the user handle commits
- Do not do any infra writing operations, ask user instead
- Do not do assumptions always clarify requirements with a user
- For any big changes: first show what are you going to do and only do after user confirmation.

## Process

The **devspec** folder is historical, do not treat it as a source of truth. RFCs in it do not reflect the current state but rather design choices of the past.

The RFC process can be requested by user:

  - Feature is designed in a corresponding devspec/000x-rfc-feature.md: reqs, approach, architecture, tech reqs, libs etc.
  - Be concise and technical, no story telling
  - Discuss and iterate with user on the rfc
  - After rfc is confirmed as finished by the user, create a corresponding devspec/000x-pln-feature.md with implementation plan
  - Iterate with user on the implementation plan
  - After user confirms the plan proceed with the implementation

