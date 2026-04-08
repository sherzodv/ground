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
