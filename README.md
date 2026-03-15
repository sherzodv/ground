# ground

**Infrastructure as Derivation.**

Ground helps you define your system and generate infrustructure code from that definition. You focus on the architecture and let ground handle low level details: networking, roles & other cluster boilerplate.

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
    service:payments:grpc
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

Ground reads `.grd` files and generates Terraform JSON. Networking, security groups, IAM roles, log groups — all derived automatically from the system description.

See [`syntax.md`](syntax.md) for the full DSL reference.

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

Golden fixtures live in `ground_test/fixtures/` — `.md` files with a `ground` input block and a `json` expected output block.

Regenerate expected output after generator changes:

```
UPDATE_FIXTURES=1 cargo test -- files
```
---

## Semantic Core

Ground has minimal core language that is used to define basic architectural concepts.
The `type` construct defines a concept and the `link` construct defines relations between concepts and their properties.

Let's define a simplified abstraction to describe services and access patterns between them:
```ground
type port = http | grpc
link image = reference
link access  = [ service:(port)? ]

type service = {
  image
  access
}```

The snipped defines a concept named `service` that may have two fields: `image` and `access`. The `image` field is a `reference` which is an extended identifier that may contain `:` and `/` symbols in it. For this example it's basically a string representing a path to the runtime image of a service in a docker registry.

The `access` link defines a property that can contain a list of references of a certain shape: a name of a service and an optional port descriptor divided by a colon.

Now we can describe our system using the defined `service` concept:
```ground
service payments {
  image: payments:latest
}

service api {
  image: api:latest
  access: [
    payments:grpc
  ]
}
```

---

## For Coding Agents

Never ever do infra, git or other changes other than current project file changes.
Do not assume, try to discuss if there is no sufficient info before taking any actions.
For any big changes: first show what are you going to do and only do after user confirmation.
RFC process can be requested by user:
  - Be concise and technical, no story telling
  - Feature is designed in a corresponding devspec/000x-rfc-feature.md: reqs, approach, architecture, tech reqs, libs etc.
  - Discuss and iterate with user on the rfc
  - After rfc is confirmed as finished by the user, create a corresponding devspec/000x-pln-feature.md with implementation plan
  - Iterate with user on the implementation plan
  - After user confirms the plan proceed with the implementation
