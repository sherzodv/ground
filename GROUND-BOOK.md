A ground book: a definitive guide to ground language.

A *reference* is a generalization of an URI:

```
registry/payments:latest
https://getground.com
service:api:http
```

## Types

The *type* construct can define enumerations:

```ground
type port() = http | grpc
type region() = us-east | us-west
```

Lists:

```ground
type port() = http | grpc
type ports() = [ port ]
```

Or event simpler:

```ground
type ports() = [ http | grpc ]
```

Architectural concepts:

```ground
type service() = {}
type database() = {}
```

## Links

The *link* defines an internal structure of a concept, e.g. adds a primitive field to it:

```ground
link image() = string

type service() = {
  image
}
```

Links can be inlined and both `()` and `type()` can be omitted:

```
type service = {
  link image = string
}
```

Types can be inlined in links:

```ground
type service() = {
  link image() = string
  link port() = type() = grpc | http
}
```

Parentheses and anonymous type definition are optional, omitted it reads much simpler:

```ground
type service = {
  link image = string
  link port = grpc | http
}
```

## Function application

Types are functions and they can be applied:

```
payments = service()
```

Application can override link value:

```
payments = service() {
  image: "payments:latest"
}
```

Refs can also be used as values:

```
type service = {
  link image = reference
}

payments = service() {
  image: local.dev/payments:latest
}
```

Application parentheses can be omitted too, if there are no parameters:

```
type service = {
  link image = reference
}

payments = service {
  image: local.dev/payments:latest
}
```

Links can nest structures and applications can be nested as well:

```ground
type scaling = {
  link min = int
  link max = int
}

type service() = {
  link image = reference
  link port = grpc | http
  link scaling = scaling
}

payments = service {
  image: local.dev/payments:latest
  port: http
  scaling: scaling() {
    min: 1
    max: 5
  }
}
```

## Default values

Default values can be provided:

```ground
type scaling = {
  link min = int
  link max = int
  min: 1
  max: 5
}

type service = {
  link image = reference
  link port = grpc | http
  link scaling = scaling

  port: grpc
}

payments = service {
  image: local.dev/payments:latest
  scaling: scaling {
    min: 1
  }
}
```

Default values can be directly used instead of type reference, in this case type will be inferred:

```ground
type scaling = {
  link min = 1
  link max = 2
}
```

Type hints can be used for default values both for readability and disambiguation:

```ground
type scaling = {
  link min = int
  link max = int
}

type service {
  link scaling = type:scaling {
    min: 1
    max: 3
  }
}
```

Ref expressions can be used for more complex values:

```ground
type service {
  link version = int
  link versionInfo = V:{this.version}
}
```

Application can use refs to resolve deeply nested fields:

```ground
payments = service {
  image: local.dev/payments:latest
  scaling:max: 3
}
```

Application has a nicer syntax which is more definitive rather than instructive:

```ground
service payments {
  image: local.dev/payments:latest
  scaling:max: 3
}
```

Links can express more complex relations between concepts:

```
type service = {
  link image = reference
  link access = service
}

service payments = {
  image: local.dev/payments:latest
}

service api = {
  image: local.dev/api:latest
  access: payments
}
```

Links and refs can work together to express even more advanced connections:

```
type service() = {
  link port = http | grpc
  link access = service:port
}

service payments = {
  port: grpc
}

service api = {
  image: local.dev/api:latest
  access: payments:grpc
}
```

Add lists:

```
type service = {
  link port = http | grpc
  link access = [ service:port ]
}

service users = {
  port: grpc
}

service payments = {
  port: grpc
}

service api = {
  image: local.dev/api:latest
  access: [
    users:grpc
    payments:grpc
  ]
}
```

Enumeration (sum) types:

```
type database = {
  link engine = postgres | mysql
}

type service = {
  link port = http | grpc
  link access = [ service:port | database ]
}

database main {
  engine: postgres
}

service users = {
  port: grpc
}

service payments = {
  port: grpc
}

service api = {
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
type database = {
  link engine = postgres | mysql
}

type service = {
  link port = http | grpc
  link access = [ service:port | database ]
}

database main {
  engine: postgres
}

service main = {
  port: grpc
}

service payments = {
  port: grpc
}

service api = {
  image: local.dev/api:latest
  access: [
    type:database:main
    type:service:main:grpc
    payments:grpc
  ]
}
```

Refs can have optional segments surrounded by `()`:

```
type service = {
  link port = http | grpc
  link access = [ service:(port) ]
}

service users = {
  port: grpc
}

service payments = {
  port: grpc
}

service api = {
  image: local.dev/api:latest
  access: [ users payments:grpc ]
}
```

Links can be anonymous and define unnamed fields, essentialy plain values which can be used in the application body:

```ground
type database
type service

database main
service users
service api

type stack = {
  link = [ type:service | type:database ]
}

stack marketing {
  database:main
  service:users
  api
}
```

## Function parameters

Types are functions and can have parameters:

```
type service(v: string) = {
  link version = {v}
}

service api("v1")
```

Expressions `{}` are part of refs - ref expressions:

```ground
type service(imageBase: string, image: string) = {
  link image = {imageBase}/{image}:latest
}

service api("local.domain", "payments")
```

Ref expression can use types:

```ground
type version {
  link major = int
  link minor = int
}

type service(v: type:version) = {
  link versionInfo = reference
  versionInfo: V:{v.major}:{v.minor}
}

service payments({ major: 2, minor: 1 }) # anonymous application for argument, type is inferred
```

Ref expressions are computed (reduced) before references are resolved:

```ground
type env = dev | prd

service dev-payments {}
service prd-payments {}

type router(e: type:env) = {
  link upstream = type:service
  upstream: {e}-payments
}

router api(prod)
```

Ref expressions can define link names:

```ground
type tag(name: reference, value: reference) = {
  link {name} = {value}
}

type service = {
  link name = reference
  link tags = [ type:tag ]
}

service payments = {
  name: {this.name}
  tags: [
    tag(project, ground)
    tag(ground-managed, true)
  ]
}
```

## Application chaining

Applications can be applied as well, using explicit syntax:

```ground
type service() = {
  link name = string
}

api = service() {
  name: api
}

deployment = api() {
}

aws = deployment() {
}
```

Using declarative syntax:

```ground
type service = {
  link name = string
}

service api {
  name: api
}

api deployment {
}

deployment aws {
}
```

Each application may redefine fields or add new ones:

```ground
type service = {
  link name = string
}

service api {
  name: api
}

api deployment {
  name: api-prod
}

deployment aws {
  link tags = [ reference ]
  tags: [ ground-managed ]
}
```

## Turing completeness

List field overrides:

```ground
type dummy = {}

type service = {
  link access = [ service ]
}

type deployment(s: service) = {
  service: {s}
  network-rules: service.access.fold(other) {
  }
}
```

## Implicit applications

Anonymous links and types are picked implicitly if in the scope to morph application:

```ground
type database
type service

database main
service users
service api

type stack = {
  link = [ type:service | type:database ]
}

stack marketing {
  database:main
  service:users
  api
}

type tag = {
  link {name} = {value}
}

type aws_ecs_cluster = {
  link name = reference
}

type deployment(stack) = {
  ecs: aws_ecs_cluster {
    name: {this.name}
  }
}

marketing () {
}
```

## Generation

An anonymous type function matches on its parameter type:

```ground
type aws_security_group = {
  link name = string
}

type service = {
  link image = reference
}

type (svc: service) = {
  sg: aws_security_group { name: {svc.name}-sg }
}
```

The function has no name — it matches any `service`. When a `service` is resolved, the function fires with `svc` bound to that instance.

Application triggers resolution. Applying a stack resolves its members and fires matching functions:

```ground
service payments {
  image: local.dev/payments:latest
}

type stack = {
  link = [ type:service ]
}

stack prod {
  payments
}

prod deployment {
}
```

Applying `prod` resolves `payments`. The anonymous `(svc: service)` function fires with `svc` bound to `payments` and produces `sg: aws_security_group { name: payments-sg }`.

Each entry is an application of its type — `role` is an instance of `aws_iam_role`, `sg` of `aws_security_group`, `ecs` of `aws_ecs_service`:

```ground
type aws_iam_role = { link name = string }
type aws_security_group = { link name = string }
type aws_ecs_service = { link name = string  link sg = string }

type (svc: service) = {
  role: aws_iam_role        { name: {svc.name}-role }
  sg:   aws_security_group  { name: {svc.name}-sg }
  ecs:  aws_ecs_service     { name: {svc.name}-ecs  sg: {svc.name}-sg }
}
```

Two-parameter functions match on link types. The `access` link connects `api` (a `service`) to `main` (a `database`). A function with parameters `(from: service, to: database)` matches that pair:

```ground
type database = {
  link engine = postgres | mysql
}

type service = {
  link image = reference
  link access = [ service | database ]
}

type aws_security_group_rule = {
  link from_sg = string
  link to_sg = string
}

type (from: service, to: database) = {
  rule: aws_security_group_rule {
    from_sg: {from.name}-sg
    to_sg:   {to.name}-sg
  }
}

database main { engine: postgres }

service api {
  image: local.dev/api:latest
  access: [ main ]
}
```

Resolving `api` walks its `access` list. For each value, the matching two-parameter function fires — `from` bound to `api`, `to` bound to that value.

Functions compose. Applying a stack resolves each member and each link between members:

```ground
database main { engine: postgres }
service users  { image: local.dev/users:latest }
service api    { image: local.dev/api:latest  access: [ users  main ] }

stack prod {
  main
  users
  api
}

prod deployment {
}

deployment aws {
  region: [ us-east:1 ]
}
```

Resolving `prod` produces:
- `main`: fires `(db: database)`
- `users`: fires `(svc: service)`
- `api`: fires `(svc: service)`
- `api → main`: fires `(from: service, to: database)`
- `api → users`: fires `(from: service, to: service)`

## TypeScript functions

A type function can name a TypeScript implementation that computes its output links. The name sits between `=` and the output body:

```ground
type (svc: service, vpc_id: string) = make_service {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}
```

Ground generates TypeScript interfaces for both sides. The user implements the pure function:

```typescript
// generated
interface MakeServiceInput  { svc: { name: string }; vpc_id: string }
interface MakeServiceOutput { sg: AwsSecurityGroup; ecs: AwsEcsService }

// user implements
export function make_service(input: MakeServiceInput): MakeServiceOutput { ... }
```

The function name is optional. Without it, output values come from ref expressions or further Ground decomposition:

```ground
# ref expressions — no function needed
type (svc: service, vpc_id: string) = {
    sg:  aws_security_group { name: {svc.name}-sg  vpc_id: {vpc_id} }
    ecs: aws_ecs_service    { name: {svc.name}-svc  sg_name: {svc.name}-sg }
}
```

The function name is mandatory when all output links are primitive types — Ground has no other way to produce their values:

```ground
# string outputs cannot be produced by decomposition — function is mandatory
type aws_security_group(svc: service, vpc_id: string) = make_sg {
    link name   = string
    link vpc_id = string
}
```

The hook can be placed at any level of the output hierarchy. Coarser means TypeScript handles more; finer means Ground handles more structure, TypeScript only at the leaves:

```ground
# coarse — one function produces everything
type (svc: service, vpc_id: string) = make_service {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}

# fine — Ground holds the structure, TypeScript only at terminal types
type (svc: service, vpc_id: string) = {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}

type aws_security_group(svc: service, vpc_id: string) = make_sg {
    link name   = string
    link vpc_id = string
}

type aws_ecs_service(svc: service, sg: aws_security_group) = make_ecs {
    link name    = string
    link sg_name = string
}
```

Both produce identical output — the difference is where the computation boundary sits.

### Complete example

```ground
type service = {
    link name = string
}

type aws_security_group = {
    link name   = string
    link vpc_id = string
}

type aws_ecs_service = {
    link name    = string
    link sg_name = string
}

type (svc: service, vpc_id: string) = make_service {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}

service payments { name: payments }
service api      { name: api }

stack prod {
    payments
    api
}
```

Resolving `prod` with `vpc_id = "vpc-abc"` calls `make_service` once per service.
Ground generates the input and output interfaces; the user fills in the function body:

```typescript
export function make_service({ svc, vpc_id }: MakeServiceInput): MakeServiceOutput {
    return {
        sg:  { name: `${svc.name}-sg`,  vpc_id },
        ecs: { name: `${svc.name}-svc`, sg_name: `${svc.name}-sg` },
    }
}
```

Output for `prod`:
```
payments → sg: { name: "payments-sg", vpc_id: "vpc-abc" }
           ecs: { name: "payments-svc", sg_name: "payments-sg" }
api      → sg: { name: "api-sg",      vpc_id: "vpc-abc" }
           ecs: { name: "api-svc",     sg_name: "api-sg" }
```

## Resolution

Ground produces output by resolving the final graph. Applying a stack triggers resolution
of every node and every link. For each link value, Ground tries three things in order:

1. A **ref expression** provides the value directly — `name: {svc.name}-sg`
2. A **TypeScript hook** on the enclosing type function provides it — `= make_sg { ... }`
3. **Further decomposition** — the link's type is itself a type function that resolves recursively

If none of these apply, the link is unresolvable — a compile error.

A bare primitive link with no ref and no hook is always an error:

```ground
type aws_security_group(svc: service, vpc_id: string) = {
    link name   = string   # error: name cannot be resolved
    link vpc_id = string   # error: vpc_id cannot be resolved
}
```

`string` is a terminal type — there is no further decomposition and no ref expression
has been given. Ground cannot produce the value. A hook or a ref is required:

```ground
# resolved via ref expression — no hook needed
type aws_security_group(svc: service, vpc_id: string) = {
    name:   {svc.name}-sg
    vpc_id: {vpc_id}
}

# resolved via TypeScript hook — no ref needed
type aws_security_group(svc: service, vpc_id: string) = make_sg {
    link name   = string
    link vpc_id = string
}
```

TypeScript hooks are not a special transformation phase — they are one resolution
mechanism among others. Ground simply tries to resolve every link in the graph and
stops, with an error, wherever resolution is not possible.

## Ref expressions and TypeScript values (raw idea)

Ref expressions currently reach into Ground-resolved values using dot syntax:

```ground
{svc.name}         # name field of svc
{svc.scaling.max}  # nested field
```

An open question: can dot syntax reach into TypeScript-produced values the same way?

```ground
# hypothetical — ts function result used inline in a ref expression
type (cidr: string, azs: [string]) = {
    first_public: {allocate_subnets(cidr, azs).public.first.cidr_block}
}
```

The idea is that any ref segment could resolve through a TypeScript function call,
making TS results first-class values in Ground's ref system. A TypeScript function
becomes callable anywhere a ref expression is valid — not just as a named hook on a
type function body, but inline, as a value source.

This would allow fine-grained use of TypeScript computation without declaring a full
type function: individual field values can call TypeScript directly, while the
surrounding structure remains Ground. The exact syntax and resolution semantics are
not yet defined.

## Best practices

### Expressing phases through types

When resolution must happen in stages, declare the phase boundary as a type.

A common pattern: a deploy entity must produce networking infrastructure before
individual services can be expanded. Rather than declaring phases explicitly, make
the phase-1 output a named type with the produced artifacts as links. Phase-2
functions take that type as an arg — Ground derives ordering from the arg graph.

```ground
# Phase 1 — deploy produces infra
type (d: aws_deploy) = make_aws_deploy {
    link vpc             = aws_vpc
    link private_subnets = [ aws_subnet ]
}

# Phase 2 — service takes deploy (with infra already filled)
type (svc: service, sp: space, d: aws_deploy) = make_service {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}
```

`make_service` takes `d: aws_deploy` as an arg. Ground knows `aws_deploy` must resolve
first. The dependency graph is the execution plan — no explicit `before`/`after` needed.

This is the same principle Terraform uses: attribute references are implicit dependencies.
In Ground, args are implicit dependencies.

The same pattern applies to edge rules after node rules: edge functions reference node
outputs (security group names, IAM role ARNs) by naming convention, so they implicitly
depend on node expansion having run.
