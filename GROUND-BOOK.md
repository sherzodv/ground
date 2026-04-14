The Ground Book

## Defs

The def construct can define enumerations:

```ground
port = http | grpc
region = us-east | us-west
```

Lists:

```ground
port = http | grpc
ports = [ port ]
```

Or even simpler:

```ground
ports = [ http | grpc ]
```

Structures:
```ground
service = {
  image = string
  port = grpc | http
}
```

There are primitive predefined defs, all of them are keywords:

```ground
boolean
integer
string
reference
unit
```

Defs declare transformations, the general syntax is:
```ground
def <name> <input def> = <transformation> <output def>

def port unit = port_to_int http | grpc

def service {
  name = string
  port = grpc | http
} = aws_ecs_service {
  image = string
  label = string
  port = integer
}
```

All of these are optional and can be omitted:

- The `def` keyword
- Input def
- Output def
- Transformation

A `def` keyword is used to disabmiguate internal def definition from a field definition:
```ground
service = {
  def scaling = {
    min = integer
    max = integer
  }
  scaling = def:scaling
}
```

The transformation is an external typescript function that is used to produce output tree given the input tree.

Ground can define simple transformations without external functions using refs:
```
service {
  name: reference
} = {
  def scaling = {
    min = integer
    max = integer
  }

  image = reference
  label = reference
  scaling = def:scaling

  image: images/{name}:latest
  label: svc-{name}
  scaling:max: 10
}
```

Transformations can override field values specified in Ground. This can be changed by using the `final` keyword.
```
service {
  name: reference
} = aws_ecs_service {
  image = reference
  label = reference

  image: final images/{name}:latest
}
```

Both transformation and input def can be omitted, yielding a simple structure syntax. In this case input is assumed `unit` and a default transformation ts function is generated — named after the def itself:
```ground
service {
  image = reference
  label = reference
}
```

This expands to:
```ground
def service unit = service {
  image = reference
  label = reference
}
```

The power of defs is revealed in a transformation chain:
```ground
def service unit = service {
  image = reference
  label = reference
}

def service unit = api {
}
```

Which in case of when unit is omitted can be simplified, resulting in smth that looks like just an instance declaration:
```ground
service {
  image = reference
  label = reference
}

service api
```

Tchain by default inherits all the previously defined fields and defs:
```ground
service {
  image = reference
  label = reference
}

service api```
```typescript
// generated

interface ServiceI {}
interface ServiceO { image: Ref; label: Ref }

interface ServiceApiI {}
interface ServiceApiO { image: Ref; label: Ref }

export function api(arg: ServiceApiI): ServiceApiO { ... }
```

Tchain arguments handling:
```ground
service {
  version = string
} = {
  image = reference
  label = reference
}

service {
  version: "0.0.1"
} api
```

To make it look more an instance declaration, arguments can be specified in the output section:
```ground
service {
  version = string
} = {
  image = reference
  label = reference
}

service api {
  version: "0.0.1"
}
```

Tchain can be continued further:
```ground
def service
service api
api deployment
```

Structure fields can be anonymous, i.e define unnamed fields, essentialy plain values which can be used in the structure body:
```ground
def database
def service

database main
service users
service api

def stack = {
  [ def:service | def:database ]
}

stack marketing {
  database:main
  service:users
  api
}
```

# References

A *reference* is Ground's string interpolation which does not utilize quotes

```
registry/payments:latest
https://getground.com
service:api:http
```

References allow to express more complex relations between defs:

```
def service = {
  access = def:service
}

service payments {
  image: local.dev/payments:latest
}

service api {
  image: local.dev/api:latest
  access: payments
}
```

Refs can express even more advanced connections:

```
def service() = {
  port = http | grpc
  access = service:port
}

service payments {
  port: grpc
}

service api {
  image: local.dev/api:latest
  access: payments:grpc
}
```

Add lists:

```
def service = {
  port = http | grpc
  access = [ service:port ]
}

service users {
  port: grpc
}

service payments {
  port: grpc
}

service api {
  image: local.dev/api:latest
  access: [
    users:grpc
    payments:grpc
  ]
}
```

Enumeration (sum) defs:
```
def database = {
  engine = postgres | mysql
}

def service = {
  port = http | grpc
  access = [ service:port | database ]
}

database main {
  engine: postgres
}

service users {
  port: grpc
}

service payments {
  port: grpc
}

service api {
  image: local.dev/api:latest
  access: [
    main
    users:grpc
    payments:grpc
  ]
}
```

Refs can use keywords and defined names to disambiguate resolving:
```
def database = {
  engine = postgres | mysql
}

def service = {
  port = http | grpc
  access = [ service:port | database ]
}

database main {
  engine: postgres
}

service main {
  port: grpc
}

service payments {
  port: grpc
}

service api {
  image: local.dev/api:latest
  access: [
    def:database:main
    def:service:main:grpc
    payments:grpc
  ]
}
```

Refs can have optional segments surrounded by `()`:
```
def service = {
  port = http | grpc
  access = [ service:(port) ]
}

service users {
  port: grpc
}

service payments {
  port: grpc
}

service api {
  image: local.dev/api:latest
  access: [ users payments:grpc ]
}
```

Expressions `{}` are part of refs - ref expressions:
```ground
def service {
  imageBase = string
  image = string
} = {
  image: {imageBase}/{image}:latest
}

service api {
  imageBase: "local.domain"
  image: "payments"
}
```

Ref expression can use defs:
```ground
def service {
  major = int
  minor = int
} = {
  versionInfo = reference
  versionInfo: V:{v.major}:{v.minor}
}

service payments {
  major: 2
  minor: 1
}
```

Ref expressions are computed (reduced) before references are resolved:
```ground
def env = dev | prd

service dev-payments {}
service prd-payments {}

def router {
  env = def:env
} = {
  upstream = def:service
  upstream: {e}-payments
}

router api {
  env: prod
}
```

Ref expressions can define names:
```ground
def tag {
  name: reference
  value: reference
} = {
  {name}: {value}
}

def service {
  name = reference
  tags = [ def:tag ]
}

service payments {
  name: {this.name}
  tags: [
    tag(project, ground)
    tag(ground-managed, true)
  ]
}
```

## Resolution

Ground produces output by resolving the final graph. Graph is top-sorted and resolution starts from the bottom.
Because of these there is no need to have explicit dependencies like in terraform.

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
  name        = string
  allow_port  = integer
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
  name        = string
  allow_port  = integer
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

## Resolution — Worked Examples

The following cases use a single set of types throughout:

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

Each case shows an alternative resolution strategy for `node` and its component types.

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
node api { name: "api" }

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
node api { name: "api" }

node.ep  → make_node → { host: "api.internal", port: 8080, proto: http }  ✓ (sealed)
node.fw  → make_node → { name: "api-fw", ... }                            ✓ (sealed)
node.ep.host   → NOT resolved independently (hook owns subtree)
node.fw.name   → NOT resolved independently (hook owns subtree)
```

---

### Case 3 — Fine-grained hooks with input pre-resolution

Each type carries its own hook. `endpoint` and `firewall` take some fields as inputs
(pre-resolved before the hook fires) and produce the rest via the hook.

```ground
def endpoint {
    host  = string       # input — provided at instantiation, injected into make_ep
    proto = proto        # input — provided at instantiation, injected into make_ep
} = make_ep {
    port = integer       # output — make_ep computes this
}

def firewall {
    name = string        # input — provided at instantiation, injected into make_fw
} = make_fw {
    description = string   # output — make_fw computes this
    allow_port  = integer  # output — make_fw computes this
}

def node {
    name = string
} = {
    ep = endpoint
    fw = firewall
}
```

`node` has no hook — Ground decomposes it. Each component type then resolves via
its own hook, with input fields supplied at instantiation:

```ground
node api {
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

`via` in the output body delegates a field to its type's own hook, while the field
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
