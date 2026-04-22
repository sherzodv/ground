The Ground Book

The Ground is a declarative systems design language facilitating composable abstractions to define layered architecture.

## The core idea

The Ground language only defines typed relations called mappings, and delegates the actual transformation logic to strictly typed and pure Typescript functions. During the resolution process Ground generates all the necessary types and signatures for Typescript, letting a user to define implementations.

```ground
rectangle {
  width  = integer
  height = integer
} = {
  area = integer
}

r1 rectangle {
  width:  1
  height: 2
}

plan r1
```

```typescript
export interface rectangleI {
  width: number;   // ground:integer
  height: number;  // ground:integer
}

export interface rectangleO extends rectangleI {
  area: number;    // ground:integer
}

export function rectangle(input: rectangleI): rectangleO {
  const { width, height } = input;

  if (!Number.isInteger(width) || !Number.isInteger(height)) {
    throw new Error("width and height must be integers");
  }

  return {
    width,
    height,
    area: width * height,
  };
}
```

Things to note in this example:

- **rectangle** defines a typed mapping between input and output structures
- by default the input fields are included in the output as is
- `field = type` syntax defines a field and it's type
- `field: value` syntax provides a value for a field
- Ground starts the resolution process from a **plan** statement, and all definitions that are not reachable from it are purely descriptive and have no effect

If we run the Ground on the code above it will internally resolve it to `r1 { width: 1 height: 2 area: 2 }` and render it with a user provided template to a file. This example uses a Ground's shorthand syntax to define mappings.

Now let's dive into more details and explore the **def** construct, which is the core of the Ground language. The full mapping definition looks like this:
```ground
def rectangle {
  width  = integer
  height = integer
} = mk_rect {
  area   = integer
}
```

We used the **def** keyword to define a mapping named **rectangle** which, given the input structure, maps it to the output using the mapper **mk_rect**. While the **rectangle** estabslishes *what* maps to what defining a typed relation, the **mk_rect** refers to the implementation, defining the *how* this mapping is done. **mk_rect** is a reference to a typescript function which implements the actual transformation.

If no mapper is explicitly referenced, Ground uses shorthand defaults:
- with no output block, the mapper is `identity`
- with an output block, the mapper is the def name itself

Mappings are composable. The **r1** "instance" that we declared earlier is nothing more than just another mapping based on the **rectangle** mapping and inheriting the output structure from it, where now the **rectangle** defines the *how*, i.e. not only it is the inherited mapping, but also is the Typescript function reference. Note how input params are passed using the `field: value` syntax. Using the full mapping definition it becomes:
```ground
def r1 {} = rectangle {
  width:  1
  height: 2
}
```

A composed def is still a full def. It inherits the output structure of the def it is based on, and may further refine or extend that structure in its own output block. Inside such a block, `field: value` assigns a value to a field of the current def, while `field = type` defines a new field on the current def.

```ground
def colored_rectangle {} = rectangle {
  color = string
}
```

Here `colored_rectangle` is its own def. It inherits the structure of `rectangle` and adds a new field `color = string`.

And finally the **plan** statement is there to trigger the *resolution process*:
```ground
plan r1
```

Now when we have all the pieces, let's connect the dots by defining how the *resolution* works. Ground starts with a **plan** statement. It reads the **plan**'s target and triess to resolve it, in this case it's the **r1** mapping. Ground then tries to find a symbol named *rectangle* in the scope and it finds the **rectangle** mapping. The resolution process repeats for the **rectangle** mapping with the inherited inputs provided in the **r1** mapping. There is no mapping called **mk_rect** but there is a typescript function with same name in the scope. Ground evaluates it with the passed through input. On the way back from the recursive resolution, Ground now checks if is there a typescript function called *rectangle* in the scope. If there is Ground feds the resolved output along with the input provided to **r1**.
```typescript
// =========================
// ground.generated.d.ts
// =========================

export interface rectangleI {
  width: number;
  height: number;
}

export interface rectangleO extends rectangleI {
  area: number;
}

export interface r1I {}

export interface r1O extends rectangleO {}

export function mk_rect(input: rectangleI): rectangleO;

export function rectangle(
  resolved: rectangleO,
  input: r1I
): r1O;


// =========================
// impl.ts (user code)
// =========================

import {
  rectangleI,
  rectangleO,
  r1I,
  r1O,
} from "./ground.generated";

export function mk_rect(input: rectangleI): rectangleO {
  const { width, height } = input;

  if (!Number.isInteger(width) || !Number.isInteger(height)) {
    throw new Error("width and height must be integers");
  }

  return {
    width,
    height,
    area: width * height,
  };
}

export function rectangle(
  resolved: rectangleO,
  _input: r1I
): r1O {
  return resolved;
}
```

When resolving `r1` defined as `def r1 {} = rectangle { width: 1 height: 2 }`:

- Ground sees `rectangle` in mapper position
- It checks whether `rectangle` is also a mapping → yes
- Ground **descends** and resolves the `rectangle` mapping using the provided input
- The `rectangle` mapping uses its mapper `mk_rect`, producing `{ width: 1, height: 2, area: 2 }`

After that:

- Ground **ascends** and checks for a Typescript function named `rectangle`
- Since it exists, Ground calls it as the mapper for `r1`, passing:
  - the resolved output of `rectangle`
  - the input of `r1`

The result of this call is the final value of `r1`.

> [!IMPORTANT]
> A mapper name may intentionally refer to both:
> - a Ground mapping, used during descent
> - a Typescript function with the same name, used during ascent
>
> This is not ambiguous. Ground resolves in two phases:
> 1. **Descent**: if the mapper name matches a different Ground mapping, Ground resolves that mapping first
> 2. **Ascent**: when returning to the current mapping, if a Typescript function with that same name exists, Ground applies it to the descended result and the current mapping input
>
> If the mapper name is the same as the current mapping name, Ground does not descend into that mapping again. In that case the name refers only to the Typescript mapper for the current def.
>
> This allows each composition step to redefine or refine the result on the way back up, rather than leaving all behavior owned only by the leaf mapper.

In summary:

- a mapper name may resolve to both a Ground mapping and a Typescript function of the same name
- if the mapper name matches a different Ground mapping, Ground resolves that mapping on descent, then applies the function on ascent
- if the mapper name matches the current mapping name, Ground skips descent and calls the Typescript mapper for the current def
- the mapper function receives the resolved output of the descended mapping and the input of the current mapping

As we said earlier, Ground allows more readable syntax by omitting **def**, input definition, **=** and a mapper reference.
```ground
r1 rectangle {
  width:  1
  height: 2
}

plan r1
```

> [!IMPORTANT]
> The `field: value` syntax always appears in the current def's output block.
> In composed defs, those assigned values are also passed as the input to the mapper or descended def.
> So the block is syntactically an output block, but during resolution its values may serve as input to the next mapping.

When `def` and `=` are both omitted, shorthand rules apply. There are two possible cases: when no `field: value` is met, the block is assumed to be an input block. It's only possible for the initial **def**, and not for composed definitions:
```ground
rectangle {
  width  = integer
  height = integer
}
```

is equivalent to:
```ground
def rectangle {
  width  = integer
  height = integer
} = identity {}
```

This is convenient for simple data carrier types without computed fields.

When at least one `field: value` is present, the block assumed as an output block:
```ground
r1 rectangle {
  width:  1
  height: 2
}
```

## Primitives

Built-in terminal defs — cannot be decomposed further:
```ground
boolean
integer
string
reference
unit
```

## Enum and list defs

These are defs whose output is an enum or list type — no input:
```ground
port = grpc | http
ports = [ port ]
```

## Nested defs

The `def` keyword disambiguates a nested def from a field inside a struct body:
```ground
service = {
  def scaling = {
    min = integer
    max = integer
  }
  scaling = def:scaling # references the previously defined scaling mapping
}
```

## References

A *reference* is Ground's string interpolation which does not utilize quotes:
```
registry/payments:latest
https://getground.com
service:api:http
```

References allow to express more complex relations between defs:
```ground
def service = {
  image: reference
  access = def:service
}

payments = service {
  image: local.dev/payments:latest
}

api = service {
  image: local.dev/api:latest
  access: payments
}
```

Refs can express typed connections:
```ground
def service = {
  image: reference
  port   = http | grpc
  access = service:port
}

payments = service { port: grpc }

api = service {
  image:  local.dev/api:latest
  access: payments:grpc
}
```

Lists:
```ground
def service = {
  port   = http | grpc
  access = [ service:port ]
}

users    = service { port: grpc }
payments = service { port: grpc }

api = service {
  image:  local.dev/api:latest
  access: [ users:grpc  payments:grpc ]
}
```

Sum types in access lists:
```ground
def database = {
  engine = postgres | mysql
}

def service = {
  port   = http | grpc
  access = [ service:port | database ]
}

main     = database { engine: postgres }
users    = service  { port: grpc }
payments = service  { port: grpc }

api = service {
  image:  local.dev/api:latest
  access: [ main  users:grpc  payments:grpc ]
}
```

Refs can use keywords and defined names to disambiguate resolving:
```ground
main-db  = database { engine: postgres }
main-svc = service  { port: grpc }
payments = service  { port: grpc }

api = service {
  image:  local.dev/api:latest
  access: [
    def:database:main-db
    def:service:main-svc:grpc
    payments:grpc
  ]
}
```

> [!IMPORTANT]
> `def` and `pack` qualifiers apply only to the segment immediately following them.
> For example, `std:aws:tf:def:backend_s3` means:
> - `std:aws:tf` is a pack path
> - `def:backend_s3` means `backend_s3` is resolved as a definition/type inside that pack
>
> Likewise, `def:database:main-db` qualifies only `database`; `main-db` is then resolved under that type.

Refs can have optional segments surrounded by `()`:
```ground
def service = {
  port   = http | grpc
  access = [ service:(port) ]
}

users    = service { port: grpc }
payments = service { port: grpc }

api = service {
  image:  local.dev/api:latest
  access: [ users  payments:grpc ]
}
```

Ref expressions are part of refs:
```ground
def service {
  imageBase = string
  name      = string
} = {
  image = reference
  image: {imageBase}/{name}:latest
}

api = service {
  imageBase: "local.domain"
  name:      "payments"
}
```

Ref expressions can use def fields:
```ground
def service {
  major = integer
  minor = integer
} = {
  versionInfo = reference
  versionInfo: V:{major}:{minor}
}

payments = service { major: 2  minor: 1 }
```

Ref expressions are computed (reduced) before references are resolved:
```ground
def env = dev | prd

dev-payments = service {}
prd-payments = service {}

def router {
  env = def:env
} = {
  upstream = def:service
  upstream: {env}-payments
}

api = router { env: prd }
```

Ref expressions can define names:
```ground
def tag {
  name  = reference
  value = reference
} = {
  {name}: {value}
}

def service {
  name = reference
  tags = [ def:tag ]
}

payments = service {
  name: {this.name}
  tags: [
    tag(project, ground)
    tag(ground-managed, true)
  ]
}
```

> [!IMPORTANT] Ground disambiguates referencess from the `field: value` syntax by spaces after the `:`

## Refs within defs

Ground can express simple mappings without mappers using ref expressions:
```ground
def service {
  name = reference
} = {
  image = reference
  label = reference

  image: images/{name}:latest
  label: svc-{name}
}
```

The mapper can be omitted when all the output fields are covered by ref expressions. Ref expressions are computed before references are resolved.
Mappings can lock specific values against mapper overrides using the **final** keyword:
```ground
def service {
  name = reference
} = mk_service {
  image = reference
  label = reference

  image: final images/{name}:latest
}
```

**mk_service** cannot override **image** — **final** wins unconditionally.

## `via` — explicit nested mapper delegation

Within an output body, `via` explicitly delegates a field to its type's own mapper,
while keeping the field in the enclosing def's output:

```ground
def node = make_node {
  fw:     via firewall   # make_fw fires, result owned by make_node's output
  region = string
}
```

This is the inverse of the input position: `fw` stays in `make_node`'s output
shape, but its value comes from `firewall`'s mapper rather than `make_node` itself.
`make_node` may use the resolved `fw` to compute other output fields.

```typescript
interface MakeNodeI { fw: Firewall }  // resolved by make_fw, injected back
interface MakeNodeO { fw: Firewall; region: string }
```

Without `via`, `make_node` must supply `fw` itself (sealed-subtree rule applies).
With `via`, `make_fw` supplies it — `make_node` receives it and passes it through.

## Plan

All defs are pure, deferred descriptions. Nothing resolves until a `plan` declaration names a symbol as a resolution root. Ground produces output only for what is planned.
```ground
prd-eu = deploy {
    name: marstech
}

plan prd-eu
```

When the symbol has an input def, `plan` supplies the args:
```ground
prd-eu {
    region = string
} = deploy {
    name: marstech
}

plan prd-eu {
    region: eu-central
}
```

Ground validates the args against the input def before resolving. A missing required
arg or a type mismatch is a compile error at the `plan` line.

Multiple plans can reference the same or different symbols:
```ground
plan prd-eu { region: eu-central }
plan prd-me { region: me-central }
plan stg-eu { region: eu-central }
```

Each produces independent output. Defs not reachable from any `plan` are not resolved.

## Packs

A **pack** is a Ground namespacing tool. The can be nested.

- Each folder from the source root creates a nested pack with same as folder's name
- Each file creates a pack names as file's basename
- Special pack.grd contains definitions for a pack of the same name as the containing folder

For example:

```
./std/           # pack std
./std/pack.grd   # pack definitions belonging to pack std
./std/math.grd   # pack std:math
./std/io/        # pack std:io
./std/io/net.grd # pack std:io:net
```

Typescript & ground files with the same name are in the same named pack.

Ground allows explicit packs:

```ground
# File: geometry.grd
pack shapes # creates a pack named shapes which continues to end of file

pack smooth { # creates a pack named smooth inside pack shapes, with scoped defined by {}
}
```

Packs may be brought into scope with `use`:

```ground
use std
use pack:std
use std:service
use pack:std:service
use pack:std:def:service
use std:aws:tf:def:backend_s3
use std:*
use pack:std:*
use std:def:*
use pack:std:def:*
use pack:std:aws:tf
```

`pack` and `def` are optional keywords in `use` refs.

`use std` and `use pack:std` bring the pack into scope.

`use std:service`, `use pack:std:service`, and `use pack:std:def:service`
bring one name into scope.

In `use std:aws:tf:def:backend_s3`, the `def` qualifier applies only to
`backend_s3`, not to the whole preceding path.

`use std:*`, `use pack:std:*`, `use std:def:*`, and `use pack:std:def:*`
bring all visible names from that pack into scope.

## Code style

Prefer narrow `use` statements over `*` imports.

```ground
use std:def:project
use std:tf:def:state_store
use std:aws:tf:def:deploy
```

This keeps the active vocabulary small and makes the modeling layer visible at
the call site.

Use qualifiers to express meaning when they help the reader see the boundary:

```ground
access: [ database:main service:pay ]
```

Prefer qualified refs when the type or layer matters more than the local name.

Use pack paths to show the realization layer explicitly:

```ground
tf:deploy
tf:state_store
```

Prefer the shortest form that is still clear. Omit qualifiers when the local
scope already makes the meaning obvious, but keep them when they make ownership,
layer, or type boundaries easier to read.
