
Ground is a cloud-agnostic infrastructure tool with a declarative DSL to define,
group, and deploy services across cloud providers.

## Scope

**Current focus: full ownership mode, AWS + Terraform.**

Ground generates complete, self-contained infrastructure — provider config,
network, cluster, and services — from a single `.grd` description. No external
inputs required.

**RFC scope: a single minimal running service.**

The target is achieved when the following `.grd` file produces Terraform that can
be applied against a fresh AWS account and the service appears as running in the
AWS console:

```ground
service svc-api { image: svc-api:prod }

group backend {
  svc-api
}

region us-east {
  aws:  us-east-1
  zone 1 { aws: us-east-1a }
}

env prod {
  LOG_LEVEL: info
}

stack prod {
  env:    prod
  region: us-east
  zone:   [1]
  group:  backend
}

deploy to aws {
  stacks: [prod]
}
```

## Primitives

### `service`

A deployable container workload. The core unit of Ground.

```ground
service svc-api {
  image:   svc-api:prod
  scaling: { min: 2, max: 10 }
}
```

Fields:
- `image` — Docker image reference. Required.
- `scaling` — autoscaling bounds. Optional. If omitted, the service runs as a
  single fixed instance.

### `group`

A named collection of services. Reusable across stacks.

```ground
group backend {
  svc-api
  svc-core
}
```

### `region` / `zone`

Cloud-agnostic placement definitions. Each maps to a provider-specific
identifier. In scope: AWS only.

```ground
region us-east {
  aws:  us-east-1
  zone 1 { aws: us-east-1a }
  zone 2 { aws: us-east-1b }
}
```

### `env`

Named set of environment variables. Applied to all services in a stack at deploy
time. Each service in the stack gets these injected as container environment
variables.

```ground
env prod {
  LOG_LEVEL: info
}

env staging {
  LOG_LEVEL: debug
}
```

### `stack`

The complete deployment context. Binds together what (group), where
(region + zone), and with what config (env). Provider-agnostic — the same stack
can be targeted by different `deploy` declarations.

```ground
stack prod {
  env:    prod
  region: us-east
  zone:   [1, 2]
  group:  backend
}
```

`env` is required — it is what distinguishes a stack from a bare placement
declaration.

### `deploy`

Binds one or more stacks to a concrete cloud provider and triggers Terraform
generation. One declaration per provider.

```ground
deploy to aws {
  stacks: [prod, staging]
}
```

Each listed stack produces an independent, self-contained Terraform config with
no external variable references.

## How stack + env maps to Terraform and AWS

Each stack generates its own Terraform config in a dedicated directory:

```
.ground/terra/
  prod/
    main.tf.json
  staging/
    main.tf.json
```

Stacks are fully isolated — own VPC, own subnets, own cluster, own Terraform
state. They never share resources.

### Region + zone → network resources

Given:

```ground
region us-east {
  aws:  us-east-1
  zone 1 { aws: us-east-1a }
}

stack prod {
  region: us-east
  zone:   [1]
  ...
}
```

Ground resolves `us-east` → `us-east-1` and `zone 1` → `us-east-1a` and
generates the full network stack:

```
aws provider          region = us-east-1
aws_vpc               10.0.0.0/16
aws_subnet (public)   10.0.1.0/24   AZ: us-east-1a
aws_subnet (private)  10.0.2.0/24   AZ: us-east-1a
aws_internet_gateway  attached to vpc
aws_nat_gateway       in public subnet
aws_route_table       private → nat, public → igw
aws_ecs_cluster       named "ground-prod"
```

The NAT gateway is required so containers in the private subnet can reach the
internet to pull Docker images.

Services reference these generated resources directly — no `var.*` placeholders.

### Env → container environment variables

```ground
env prod {
  LOG_LEVEL: info
}
```

Injected into every ECS task definition in the stack:

```json
"environment": [
  { "name": "LOG_LEVEL", "value": "info" }
]
```

The same `service` definition deployed in two stacks gets different runtime
config based solely on which `env` the stack binds. This is the core value of
`env` as a first-class entity.

## Output

`ground gen terra` reads all `.grd` files in the current directory, finds all
`deploy` declarations, and writes one self-contained `main.tf.json` per stack
under `.ground/terra/<stack-name>/`.

`ground apply terra` calls the Terraform binary against each generated directory.
