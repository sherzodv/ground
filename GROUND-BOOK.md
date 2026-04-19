The Ground Book

The Ground is a declarative systems design language facilitating composable abstractions to define layered architecture.

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
- by default Ground searches for a Typescript function with the same name as as the mapping name to perform the resolution of the output value
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

If no mapper function is referenced, Ground searches the Typescript function with the same name as the mapping name, as it was in the first example

Mappings are compoasable. The **r1** "instance" that we declared earlier is nothing more than just another mapping based on the **rectangle** mapping and inheriting the output structure from it, where now the **rectangle** defines the *how*, i.e. not only it is the inherited mapping, but also is the Typescript function reference. Note how input params are passed using the `field: value` syntax. Using the full mapping definition it becomes:
```ground
def r1 {} = rectangle {
  width:  1
  height: 2
}
```

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

In summary:

- if a mapper name also resolves to a mapping, Ground
  - resolves that mapping first (descent)
  - then applies the Typescript function with the same name on ascent
- the mapper function receives the resolved output of the descended mapping and the input of the current mapping

As we said earlier, Ground allows more readable syntax by omitting **def**, input definition, **=** and a mapper reference.
```ground
r1 rectangle {
  width:  1
  height: 2
}

plan r1
```

> [!IMPORTANT] The `field: value` syntax can only appear in the output block

When `def` and `=` are both omitted, the definition is the input schema with the identity mapper. It's only possible for the initial **def**, and not for composed definitions:
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

### Primitives

Built-in terminal defs — cannot be decomposed further:
```ground
boolean
integer
string
reference
unit
```

### Enum and list defs

These are defs whose output is an enum or list type — no input:
```ground
port = grpc | http
ports = [ port ]
```

### Nested defs

The `def` keyword disambiguates a nested def from a field inside a struct body:
```ground
service = {
  def scaling = {
    min = integer
    max = integer
  }
  scaling = def:scaling
}
```

# References

A *reference* is Ground's string interpolation which does not utilize quotes:
```
registry/payments:latest
https://getground.com
service:api:http
```

References allow to express more complex relations between defs:
```ground
def service = {
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

## Plan

All defs are pure, deferred descriptions. Nothing resolves until a `plan` declaration names a symbol as a resolution root. Ground produces output only for what is planned.
```ground
prd-eu = aws_deploy {
    stack: marstech
}

plan prd-eu
```

When the symbol has an input def, `plan` supplies the args:
```ground
prd-eu {
    region = string
} = aws_deploy {
    stack: marstech
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

### `via` — explicit nested mapper delegation

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

---

## Resolution — Worked Examples

The following cases use a single set of defs throughout:

```ground
def proto = http | grpc

def endpoint = {
    host  = string
    port  = integer
    proto = proto
}

def firewall = {
    name        = string
    description = string
    allow_port  = integer
}

def node = {
    name = string
    ep   = endpoint
    fw   = firewall
}
```

`string` and `integer` are terminal — Ground cannot decompose them further.
`endpoint`, `firewall`, and `node` are non-terminal — Ground recurses into their fields.

Each case shows an alternative resolution strategy for `node` and its component defs.

---

### Case 1 — Pure ref expressions, no mapper

```ground
def node {
    name = string
} = {
    ep = endpoint
    fw = firewall

    ep: {
        host:  {name}.internal
        port:  8080
        proto: http
    }

    fw: {
        name:        {name}-fw
        description: firewall for {name}
        allow_port:  8080
    }
}
```

All terminal fields are provided by ref expressions. No mapper, no decomposition.

```
api = node { name: "api" }

node.ep.host        → ref "api.internal"      ✓
node.ep.port        → ref 8080                ✓
node.ep.proto       → ref http                ✓
node.fw.name        → ref "api-fw"            ✓
node.fw.description → ref "firewall for api"  ✓
node.fw.allow_port  → ref 8080                ✓
```

---

### Case 2 — Coarse mapper: mapper owns the complete subtree

```ground
def node {
    name = string
} = make_node {
    ep = endpoint
    fw = firewall
}
```

```typescript
interface MakeNodeI { name: string }
interface MakeNodeO { ep: Endpoint; fw: Firewall }

function make_node(i: MakeNodeI): MakeNodeO {
    return {
        ep: { host: `${i.name}.internal`, port: 8080, proto: 'http' },
        fw: { name: `${i.name}-fw`, description: `firewall for ${i.name}`, allow_port: 8080 },
    }
}
```

`make_node` returns complete `ep` and `fw` objects. Ground takes each as-is — it does
not recurse into `ep.host`, `ep.port`, `fw.name`, etc. The mapper owns those subtrees.

```
api = node { name: "api" }

node.ep  → make_node → { host: "api.internal", port: 8080, proto: http }  ✓ (sealed)
node.fw  → make_node → { name: "api-fw", ... }                            ✓ (sealed)
node.ep.host   → NOT resolved independently (mapper owns subtree)
node.fw.name   → NOT resolved independently (mapper owns subtree)
```

---

### Case 3 — Fine-grained mappers with input pre-resolution

Each def carries its own mapper. `endpoint` and `firewall` take some fields as inputs
(pre-resolved before the mapper fires) and produce the rest via the mapper.

```ground
def endpoint {
    host  = string    # input — provided at call site, injected into make_ep
    proto = proto     # input — provided at call site, injected into make_ep
} = make_ep {
    port = integer    # output — make_ep computes this
}

def firewall {
    name = string     # input — provided at call site, injected into make_fw
} = make_fw {
    description = string    # output — make_fw computes this
    allow_port  = integer   # output — make_fw computes this
}

def node {
    name = string
} = {
    ep = endpoint
    fw = firewall
}
```

`node` has no mapper — Ground decomposes it. Each component def then resolves via
its own mapper, with input fields supplied at the call site:

```ground
api = node {
    name: api
    ep:   { host: api.internal  proto: http }
    fw:   { name: api-fw }
}
```

```typescript
interface MakeEpI  { host: string; proto: 'http' | 'grpc' }
interface MakeEpO  { port: number }

interface MakeFwI  { name: string }
interface MakeFwO  { description: string; allow_port: number }
```

```
node.ep         → decompose endpoint
  ep.host       → input "api.internal"   ✓ (injected into make_ep)
  ep.proto      → input http             ✓ (injected into make_ep)
  ep.port       → make_ep               ✓
node.fw         → decompose firewall
  fw.name       → input "api-fw"        ✓ (injected into make_fw)
  fw.description → make_fw             ✓
  fw.allow_port  → make_fw             ✓
```

---

### Case 4 — Hook overrides ref expression

```ground
def node {
    name = string
} = make_node {
    ep = endpoint
    fw = firewall

    fw: { allow_port: 9090 }
}
```

Ground resolves `fw.allow_port` to `9090` via ref expression.
`make_node` also returns `fw.allow_port: 8080`.

Mapper wins — priority is `mapper > ref`:

```
fw.allow_port:  ref → 9090
                mapper → 8080
                result → 8080
```

---

### Case 5 — `final` blocks mapper override

```ground
def node {
    name = string
} = make_node {
    ep = endpoint
    fw = firewall

    fw: { allow_port: final 9090 }
}
```

`final` locks `fw.allow_port`. Hook cannot override it:

```
fw.allow_port:  final ref → 9090
                mapper → 8080
                result → 9090
```

`final` is the only way to guarantee a ref expression survives a mapper.

---

### Case 6 — `via`: explicit nested mapper delegation

`via` in the output body delegates a field to its def's own mapper, while the field
remains in the enclosing def's output shape:

```ground
def node {
    name = string
} = make_node {
    ep: via endpoint
    fw = firewall
}
```

`endpoint`'s mapper fires and produces `ep`. `make_node` receives the resolved `ep`
as an input and may use it when computing other fields. `fw` is owned entirely
by `make_node` (no `via` — sealed-subtree rule applies).

```typescript
interface MakeNodeI { name: string; ep: Endpoint }  // ep pre-resolved by endpoint's mapper
interface MakeNodeO { ep: Endpoint; fw: Firewall }
```

---

### Case 7 — Partial mapper output: compile error

```ground
def node {
    name = string
} = make_node {
    ep = endpoint
    fw = firewall
}
```

```typescript
function make_node(i: MakeNodeI): MakeNodeO {
    return {
        ep: { host: `${i.name}.internal`, port: 8080, proto: 'http' },
        fw: { name: `${i.name}-fw` },  // description and allow_port missing
    }
}
```

`make_node` claims `fw` but returns it incomplete. Ground does not fall back to
decomposition for the missing sub-fields — mapper claimed ownership of the subtree.

```
fw.allow_port   → mapper claimed ownership → no fallback → ERROR
fw.description  → mapper claimed ownership → no fallback → ERROR
```

Caught at compile time: `MakeNodeO.fw` is typed as the complete `Firewall` shape,
making the partial return a TypeScript type error before Ground runs.

---

### Case 8 — No resolution path: compile error

```ground
def node {
    name = string
} = {
    ep = endpoint
    fw = firewall
}
```

`node` has no mapper. Ground decomposes to `endpoint` and `firewall`, then tries to
resolve their terminal fields. None are provided:

```
ep.host        → no ref, no mapper, terminal → ERROR
ep.port        → ERROR
ep.proto       → no ref, no mapper, sum type with no value → ERROR
fw.name        → ERROR
fw.description → ERROR
fw.allow_port  → ERROR
```

---

### Resolution decision tree

```
For each field F of type T:

1. Is there a `final` ref expression for F?
   YES → use it. Done. Hook cannot override.

2. Does the enclosing mapper provide a value for F?
   YES → use it. F's subtree is sealed. Done.

3. Is there a ref expression for F?
   YES → use it (enclosing mapper may still override).

4. Is T a non-terminal def?
   YES → decompose: recurse into T's definition.

5. Is T terminal (string, integer, boolean, sum type with no value)?
   YES → COMPILE ERROR.
```

### The sealed-subtree invariant

> If a mapper provides a value V for field F, then every field reachable within V
> is resolved by V. Ground does not independently resolve any sub-field of F.

This makes mappers composable and reasoning local: a mapper author knows exactly what
they own (their output shape), and Ground knows exactly what it owns (everything
not covered by a mapper or ref).
