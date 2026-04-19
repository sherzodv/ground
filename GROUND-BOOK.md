The Ground Book

The Ground is a declarative systems design language facilitating composable abstractions to define layered architecture.

The core syntactic construct of the Ground language is a mapping between structures:
```ground
def service {
  name = string
} = aws_ecs_service {
  image = reference
  port  = http | grpc
}

```

We defined a mapping named **service** which, given the input structure, maps it to the output using the mapper **aws_ecs_service**. While the **service** estabslishes *what maps to what* defining a typed relation, the **aws_ecs_service** refers to the implementation, defining the *how this mapping happens*. **aws_ecs_service** is a reference  to a typescript function which implements the actual transformation.

Mappings are compoasable. We can define another mapping, where now the **service** defines the *how*, i.e. becomes the implementation. Note how we can pass inputs to the **service** mapping using a `field: value` syntax:
```ground
def payments {} = service {
  name: "marstech-api"
}
```

Note that the **payments** service inherits the output structure from the **service**. We can read this as: create a new mapping called **payments** with no input parameters, whose output structure is inherited from the **service** mapping given the provided input parameters.

But both of these definitions have no effect in Ground until we **plan** them. A **plan** construct in Ground is a terminal statement that starts the actual *resolution* process.
```ground
plan payments
```

Now when we have all the pieces, let's connect the dots by defining how the *resolution* works. Ground starts with a **plan** statement. It reads the **plan**'s target and triess to resolve it, in this case it's the **payments** mapping. Ground then tries to find a symbol named *service* in the scope and it finds the **service** mapping. The resolution process repeats for the **service** mapping with the inherited inputs provided in the **payments** mapping. There is no mapping called **aaws_ecs_service** but there is a typescript function with same name in the scope. Ground evaluates it with the passed through input.

On the way back from recursive resolution, Ground now checks if is there a typescript function called *service* in the scope.

But in this case there is no typescript function called **service** in the scope.

Ground allows omitting empty input args and other syntax details:
```ground
api service {
  name: "marstech-api"
}
```

Although this looks like we are creating a named instance of the **service**, from the Ground perspective we're are creating another mapping.
```ground
api service {
  name: "marstech-api"
}
```

---

### Identity def — no hook, output = input

```ground
def service {
  port = grpc | http
}
```

No hook, no output schema — the def is an identity transform. Its output shape is its
input shape. `service` accepts `port` and passes it through.

### Transformation def — with hook

```ground
def service {
  port = grpc | http
} = mk_service {
  ecs_arn = string
}
```

`mk_service` is a TypeScript function the user provides. Ground generates the interfaces:

```typescript
interface MkServiceI { port: 'grpc' | 'http' }
interface MkServiceO { ecs_arn: string }
```

### Calling a def

Call a def by providing its input values using `:` syntax:

```ground
api = service { port: http }
```

`api` is the result of calling `service` with `port: http`. If `service` has a hook,
the hook runs. `api`'s output shape is `service`'s output shape.

---

## Composition

Defs compose. This is the core of Ground.

```ground
def boo { i1 = string } = mk_boo { o1 = string }

foo = boo { i1: "hello" }
```

`foo` calls `boo`, which calls `mk_boo`. The call graph is topo-sorted and evaluated
bottom-up: `mk_boo` runs first, its output flows up to `foo`.

`foo` can forward its own inputs into another def:

```ground
foo { i2 = string } = boo { i1: {i2} }
// TS: foo(i2) = boo({ i1: i2 })
```

Or declare its own hook on top:

```ground
foo { i2 = string } = mk_foo { o2 = string }
```

The tchain can continue arbitrarily:

```ground
def service
api = service
deployment = api
```

Each link in the chain is a function call. The resolved graph is the composed result.

---

## Shorthands

### Enum and list defs

```ground
port = grpc | http
ports = [ port ]
```

These are defs whose output is an enum or list type — no input, no hook.

### Struct shorthand

When `def` and `=` are both omitted, the block is the input schema with identity transform:

```ground
service {
  image = reference
  port  = grpc | http
}
```

Equivalent to:

```ground
def service {
  image = reference
  port  = grpc | http
} = {}
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

### Primitives

Built-in terminal defs — cannot be decomposed further:

```ground
boolean
integer
string
reference
unit
```

---

## Refs within defs

Ground can express transformations without external hooks using ref expressions:

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

The hook can be omitted when all output fields are covered by ref expressions.
Ref expressions are computed before references are resolved.

Transformations can lock specific values against hook overrides using `final`:

```ground
def service {
  name = reference
} = mk_service {
  image = reference
  label = reference

  image: final images/{name}:latest
}
```

`mk_service` cannot override `image` — `final` wins unconditionally.

---

## Plan

All defs are pure, deferred descriptions. Nothing resolves until a `plan` declaration
names a symbol as a resolution root. Ground produces output only for what is planned.

```ground
prd-eu = aws_deploy {
    stack: marstech
}

plan prd-eu
```

`plan` followed by a name is the entire statement. Ground top-sorts the graph
reachable from `prd-eu` and resolves it bottom-up.

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

---

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

---

## Resolution

Ground produces output by resolving the final graph. The graph is topo-sorted and
resolution starts from the bottom. There is no need for explicit dependency declarations.

Ground collects values from all applicable sources, then applies priority:

```
final ref  >  hook  >  ref  >  decomposition
```

1. A `final` **ref expression** — wins unconditionally; no hook can override
2. A **TypeScript hook** — overrides any non-final ref expression for the same field
3. A **ref expression** — used when no hook covers the field
4. **Decomposition** — the field's type is itself a def that resolves recursively

If none apply to a terminal field (`string`, `integer`, `boolean`) — compile error.

### Hook propagation

When a hook provides a value for a field, the entire subtree rooted at that field
is considered resolved. Ground does not recurse into hook-provided values to resolve
sub-fields independently.

```ground
def firewall = {
  name       = string
  allow_port = integer
}

def node = make_node {
  fw = firewall
}
```

`make_node` returns a complete `fw` object. Ground takes it as-is — it does not
separately resolve `fw.name` or `fw.allow_port`. The hook owns the subtree.

If the hook returns a partial object (e.g. `fw` with `name` but no `allow_port`),
that is a compile error — enforced by the generated TypeScript interface which
types the return as the complete `Firewall` shape.

### Input args — pre-resolution

Fields declared in the **input def** (before `=`) are resolved by their own hooks
before the enclosing hook fires. The resolved values are injected into the enclosing
hook as inputs.

```ground
def firewall = make_fw {
  name       = string
  allow_port = integer
}

def node {
  fw = firewall       # input position — make_fw fires first
} = make_node {       # make_node receives already-resolved fw
  region = string
}
```

Generated interfaces reflect the ordering:

```typescript
// make_fw fires first
interface MakeFwI { /* firewall inputs */ }
interface MakeFwO { name: string; allow_port: number }

// make_node receives resolved fw as input
interface MakeNodeI { fw: MakeFwO }
interface MakeNodeO { region: string }
```

Compare with the same field in the **output body** — here `make_node` owns `fw`
and must return it in full:

```ground
def node = make_node {
  fw     = firewall   # output position — make_node must provide fw completely
  region = string
}
```

```typescript
interface MakeNodeI { }
interface MakeNodeO { fw: Firewall; region: string }
```

### `via` — explicit nested hook delegation

Within an output body, `via` explicitly delegates a field to its type's own hook,
while keeping the field in the enclosing def's output:

```ground
def node = make_node {
  fw:     via firewall   # make_fw fires, result owned by make_node's output
  region = string
}
```

This is the inverse of the input position: `fw` stays in `make_node`'s output
shape, but its value comes from `firewall`'s hook rather than `make_node` itself.
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

### Case 1 — Pure ref expressions, no hook

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

All terminal fields are provided by ref expressions. No hook, no decomposition.

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

### Case 2 — Coarse hook: hook owns the complete subtree

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
not recurse into `ep.host`, `ep.port`, `fw.name`, etc. The hook owns those subtrees.

```
api = node { name: "api" }

node.ep  → make_node → { host: "api.internal", port: 8080, proto: http }  ✓ (sealed)
node.fw  → make_node → { name: "api-fw", ... }                            ✓ (sealed)
node.ep.host   → NOT resolved independently (hook owns subtree)
node.fw.name   → NOT resolved independently (hook owns subtree)
```

---

### Case 3 — Fine-grained hooks with input pre-resolution

Each def carries its own hook. `endpoint` and `firewall` take some fields as inputs
(pre-resolved before the hook fires) and produce the rest via the hook.

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

`node` has no hook — Ground decomposes it. Each component def then resolves via
its own hook, with input fields supplied at the call site:

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

Hook wins — priority is `hook > ref`:

```
fw.allow_port:  ref → 9090
                hook → 8080
                result → 8080
```

---

### Case 5 — `final` blocks hook override

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
                hook → 8080
                result → 9090
```

`final` is the only way to guarantee a ref expression survives a hook.

---

### Case 6 — `via`: explicit nested hook delegation

`via` in the output body delegates a field to its def's own hook, while the field
remains in the enclosing def's output shape:

```ground
def node {
    name = string
} = make_node {
    ep: via endpoint
    fw = firewall
}
```

`endpoint`'s hook fires and produces `ep`. `make_node` receives the resolved `ep`
as an input and may use it when computing other fields. `fw` is owned entirely
by `make_node` (no `via` — sealed-subtree rule applies).

```typescript
interface MakeNodeI { name: string; ep: Endpoint }  // ep pre-resolved by endpoint's hook
interface MakeNodeO { ep: Endpoint; fw: Firewall }
```

---

### Case 7 — Partial hook output: compile error

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
decomposition for the missing sub-fields — hook claimed ownership of the subtree.

```
fw.allow_port   → hook claimed ownership → no fallback → ERROR
fw.description  → hook claimed ownership → no fallback → ERROR
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

`node` has no hook. Ground decomposes to `endpoint` and `firewall`, then tries to
resolve their terminal fields. None are provided:

```
ep.host        → no ref, no hook, terminal → ERROR
ep.port        → ERROR
ep.proto       → no ref, no hook, sum type with no value → ERROR
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

2. Does the enclosing hook provide a value for F?
   YES → use it. F's subtree is sealed. Done.

3. Is there a ref expression for F?
   YES → use it (enclosing hook may still override).

4. Is T a non-terminal def?
   YES → decompose: recurse into T's definition.

5. Is T terminal (string, integer, boolean, sum type with no value)?
   YES → COMPILE ERROR.
```

### The sealed-subtree invariant

> If a hook provides a value V for field F, then every field reachable within V
> is resolved by V. Ground does not independently resolve any sub-field of F.

This makes hooks composable and reasoning local: a hook author knows exactly what
they own (their output shape), and Ground knows exactly what it owns (everything
not covered by a hook or ref).
