# RFC 0008 — Use / Pack Imports

## Context

Cross-unit references currently require fully-qualified paths (`pack:std:type:service`).
There is no way to bring foreign names into local scope. This RFC introduces `use` statements
to control name visibility across pack boundaries.

Pack scopes are already represented in the IR (`ScopeKind::Pack`, `IrScope.packs`).
`use` is purely a resolver concern — no codegen impact.

## Scope

- New `use` statement in source syntax
- Resolver import resolution pass
- Ambiguity detection and reporting
- Out of scope: codegen, module loading, filesystem resolution

---

## Syntax

`use` takes a single ref argument using the existing ref grammar.
`pack`, `type`, `link` are keyword segments. `*` is a new wildcard terminal (final segment only).

```
use std                       # short for use pack:std
use pack:std                  # import pack std by name
use pack:std:service          # import name 'service' from std (all kinds)
use pack:std:type:service     # import type 'service' from std
use pack:std:link:access      # import link 'access' from std
use pack:std:*                # import all names from std
use pack:std:type:*           # import all types from std
```

Multiple `use` statements are allowed per scope, in any order relative to definitions.

---

## Semantics

### What each form brings into scope

| Form | Names imported |
|---|---|
| `use pack:std` | the pack `std` itself — enables `std:X` qualified refs |
| `use pack:std:service` | everything named `service` in std (type, inst, link — all kinds) |
| `use pack:std:type:service` | only the type named `service` from std |
| `use pack:std:link:access` | only the link named `access` from std |
| `use pack:std:*` | all names from std (all kinds) |
| `use pack:std:type:*` | all types from std |

### Unqualified access

Imported names are available unqualified if unambiguous. The general rule:

> Any ref that resolves to exactly one binding is valid unqualified.
> If two or more bindings match the same name in scope, an **ambiguous ref** error is emitted
> at the use site. The user must add a prefix to disambiguate.

Ambiguity can arise:
- Between local definitions and imports
- Between two imports from different packs
- Within a single `use pack:std:service` if std has both a type and an instance named `service`

### Disambiguation prefixes

Standard kind/pack prefixes resolve ambiguity:

```
type:service     # unambiguously the type
pack:std:service # unambiguously from std
std:service      # unambiguously from std (requires pack std to be in scope)
```

### Pack-level import (`use pack:std`)

Does not flatten std's contents into local scope. Only registers `std` as a resolvable
pack name, enabling `std:X` as a shorter alternative to `pack:std:X`.

---

## Resolver changes

### New AST node

```rust
pub struct AstUse {
    pub path: AstRef,   // reuses existing AstRef — same grammar as value refs
}
```

Added to `AstDef::Use(AstNode<AstUse>)`.

### New resolver pass — import resolution (between pass 1 and pass 2)

For each scope, collect all `AstDef::Use` entries and resolve them:

1. Resolve the ref path to a target (pack scope, type, link, inst)
2. Collect the set of names being imported (single name or wildcard expansion)
3. Insert into scope's name maps (`types`, `links`, `insts`, `packs`)
4. If a name already exists in the map (from a prior import or local def), record it as ambiguous

### Ambiguity tracking

`IrScope` gains:

```rust
pub ambiguous: HashSet<String>
```

When a name collision is detected — whether import vs import, or local def vs import —
the name is added to `ambiguous` and removed from (or not entered into) the specific name map.
At use site, a lookup that hits `ambiguous` emits:

```
ambiguous ref 'service': defined in multiple sources, use a prefix to disambiguate
```

There is no shadowing. Local definitions have no precedence over imports.

### Lookup order (updated)

1. Current scope local defs
2. Current scope imports (errors if ambiguous)
3. Parent scope (recurse)

---

## Error cases

| Condition | Error |
|---|---|
| `use` ref target does not exist | `unresolved use target 'pack:std'` |
| Wildcard on non-pack target | `wildcard requires a pack target` |
| Name used unqualified is ambiguous | `ambiguous ref 'X': use a prefix to disambiguate` |
| `use pack:std:service` — std has no name `service` | `'service' not found in pack 'std'` |

---

## Examples

```
# std pack defines: type service, type database, link region
# local scope has no 'service' or 'database'

use pack:std:type:service
use pack:std:type:database

service my-svc { ... }    # ok — service unambiguous
database my-db { ... }    # ok
```

```
# local scope defines type service; std also exports type service

type service = { link image = reference }
use pack:std:type:service

service my-svc { ... }    # ERROR: ambiguous ref 'service'
type:service my-svc { ... }     # still ambiguous — both are types
pack:std:type:service my-svc { ... }  # ok — fully qualified
```

```
use pack:std:*
use pack:other:*          # other also exports 'service'

service my-svc { ... }    # ERROR: ambiguous ref 'service'
type:service my-svc { ... } # ok — disambiguated
```

```
use pack:std              # only registers 'std' as a name

std:service my-svc { ... }  # ok
service my-svc { ... }      # ERROR: 'service' not in scope
```
