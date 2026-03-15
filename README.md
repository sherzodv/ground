# ground

**Infrastructure as Derivation.**

Ground helps you define your system and generate infrustructure code from that definition. You focus on the architecture and let ground **derive** low level details: networking, roles, clusterss and other boilerplate.

```ground
database db-main {
  engine:  postgres
}

service users {
  image: users:latest
  access: [ database:main ]
}

service payments {
  image: payments:latest
  access: [ database:main ]
}

service api {
  image:  api:latest
  access: [
    service:users:grpc
    service:payments:grpc
  ]
}

stack shop {
  database:main
  service:users
  service:payments
  service:api
}

deploy shop to aws as shop-eu-central {
  region: eu-central
}
```

---
## CLI

```
ground init [--git-ignore]   create .ground/ + settings.json; patch .gitignore
ground gen terra             write .ground/terra/<deploy>/main.tf.json
ground plan                  plan changes per Ground entity (no apply)
ground apply                 apply changes
```
---
## Testing

Golden fixtures live in `ground_test/fixtures/` — `.md` files with a *ground* input block and a *json* expected output block.

Regenerate expected output after generator changes:

```
UPDATE_FIXTURES=1 cargo test -- files
```
---

## Semantic Core

Ground has a minimal language that is used to define basic architectural concepts. It is based on two core constructs: a *type* and a *link*. Additionally the core language has some primitives: *string*, *integer* and a special "structured" primitive a *reference*.

A *reference* is a generalization of an URI:

```
registry/payments:latest
https://getground.com
service:api:http
```

The *type* construct can define enumerations:

```ground
type port = http | grpc
type region = us-east | us-west
```

and architectural concepts:

```ground
type service = {}
type database = {}
```

The *link* construct defines relations & internal structure of concepts. It can define a primitive field of a concept:

```ground
link image = string

type service = {
  image
}

service payments {
  image: docker-registry/payments:latest
}
```

or more advanced structural relations between types:

```ground
type port = http | grpc
type engine = postgres | mysql
link engine = type:engine

link image = string
link access = database | service:port

type service = {
  access
}

type database = {
  engine
}

service main {
  image: docker-registry/main:latest
}

database main {
  engine: postgres
}

service payments {
  image: docker-registry/payments:latest
  access: database:main
}```

Note how access defines a certain structure for a reference. It enforces the values of an *access* field to refer to a service name followed by a port or a database name. Concept names can be used to disambiguate references.

Links can define array fields `[]` as well as optional parts `()` of references:

```ground
link access = [database | service:(port)]

...

service payments {
  image: docker-registry/payments:latest
  access: [
    database:main
    service:main:grpc
  ]
}
```

Definitions can be inlined:

```ground
link access  = [ service:(port) | database ]

type database = {
  link manage = type manage = self | provider | cloud
  link engine = type engine = postgresql | mongodb
  link version = string
}

type service = {
  type port    = grpc | http
  link image   = reference
  link access  = [ service:(port) | database ]
  link scaling = type scaling = {
    link min = integer
    link max = integer
  }
}
```

Links & types can be anounimous:

```ground
database users
database main
service users
service payments
service api

type database = {
  link manage = type = self | provider | cloud
  link engine = type = postgresql | mongodb
}

type stack = {
  link = [ type:service | type:database ]
}

stack marketing {
  main
  service:users
  payments
  api
}
```

Ground has special *terminal* construct **deploy** that will trigger a generation of all the needed infrastructure configuration based on a core and user level templates. Without *deploy* clause there will be no configuration generated, other concepts are purely descriptive.

`deploy <reference> to <reference> as <reference> { links }`

```ground
stack marketing {
  database:main
  service:users
  service:payments
  service:api
}

type region = eu-central | eu-west | us-east | us-west | ap-southeast
type zone   = 1 | 2 | 3 | 4 | 5
link region = type:region:type:zone

deploy stack:marketing to aws as marketing-eu-central {
  region: eu-central
}
```

The `reference` structure:

Ground treats *references* as a colon separated list of segments. Resolving references has local semantics and happens segment by segment with respect to precendence rules:
- *type* and *link* keywords have highest priority, e.g. *type:service* references a *type* named *service*
- local names come after keywords: *service:main* references a instance *main* of a type named *service*

Fully qualified references increase readiblity but are not required if there is no ambiguity in resolving.

To disambiguate references from field declarations at least one space is required after a colon: `service api { image: api:latest }`

## Standard Library

TBD

