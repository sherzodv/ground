
Ground is a cloud-agnostic infrastructure tool with a declarative DSL to define,
group, and deploy services across cloud providers.

## Scope

**Current focus: full ownership mode, AWS + Terraform.**

Ground generates complete, self-contained infrastructure — provider config,
network, cluster, and services — from a single `.grd` description. No external
inputs required.

## Primitives

### `service`

A deployable container workload. The core unit of Ground.

```ground
service svc-api {
  image:   svc-api:prod
  scaling: { min: 2, max: 10 }
  compute: c4-small
  ports:   { http: 8080, grpc: 9090 }
  access {
    svc-internal: http, grpc
  }
}
```

Fields:
- `image` — Docker image reference. Required.
- `scaling` — autoscaling bounds (`min`, `max`). Optional. If omitted, the service runs as a single fixed instance.
- `compute` — ref to a `compute` block. Optional. Defaults to 256 cpu / 512 memory / FARGATE.
- `ports` — named port declarations. Optional. Used to resolve port names in `access` entries.
- `access` — services or databases this service needs to reach. Optional. Each entry is a target name optionally followed by `: port, port, ...`. If ports are omitted for a service target, all declared ports are used. For database targets, ports are derived from the engine.

### `compute`

Named compute resource. Decouples sizing from service definitions.

```ground
compute c4-small {
  cpu:    1024
  memory: 2048
  aws:    FARGATE
}
```

Fields:
- `cpu` — ECS cpu units. Optional. Default 256.
- `memory` — MiB. Optional. Default 512.
- `aws` — capacity provider string (e.g. `FARGATE`, `FARGATE_SPOT`). Optional. Default `FARGATE`.

### `group`

A named collection of services and databases. Reusable across stacks.

```ground
group backend {
  svc-api
  svc-internal
  db-main
}
```

### `region` / `zone`

Cloud-agnostic placement definitions. Each maps to a provider-specific identifier.

```ground
region us-east {
  aws:  us-east-1
  zone 1 { aws: us-east-1a }
  zone 2 { aws: us-east-1b }
}
```

### `env`

Named set of environment variables. Injected into all services in a stack.

```ground
env prod {
  LOG_LEVEL: warn
}
```

### `stack`

The complete deployment context. Binds group, region, zones, and env.

```ground
stack prod {
  env:    prod
  region: us-east
  zone:   [1, 2]
  group:  backend
}
```

All four fields are required.

### `deploy`

Binds stacks to a provider. One declaration per provider.

```ground
deploy to aws {
  stacks: [prod, staging]
  override {
    "provider": { "aws": { "assume_role": { "role_arn": "arn:aws:iam::123:role/deploy" } } }
  }
}
```

- `stacks` — required, non-empty list.
- `override` — optional raw JSON deep-merged into the generated Terraform JSON after generation.

## How stack + env maps to Terraform and AWS

Each stack generates its own Terraform config in a dedicated directory:

```
.ground/terra/
  prod/
    main.tf.json
  staging/
    main.tf.json
```

Stacks are fully isolated — own VPC, own subnets, own cluster, own Terraform state.

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
aws provider              region = us-east-1
aws_ecs_cluster           ground-prod
aws_vpc                   ground-prod   10.0.0.0/16
aws_subnet (public)       prod-pub-1    10.0.0.0/24   AZ: us-east-1a
aws_subnet (private)      prod-priv-1   10.0.1.0/24   AZ: us-east-1a
aws_internet_gateway      ground-prod   (vpc_id inline, no separate attachment)
aws_eip                   ground-prod_eip
aws_nat_gateway           ground-prod   in public subnet
aws_route_table           rt-prod-pub-1  → IGW
aws_route_table           rt-prod-priv-1 → NAT
aws_route_table_association  (one per subnet)
aws_route                    (one per route table)
```

For two zones the CIDR allocation continues: zone 2 → `10.0.2.0/24` (public), `10.0.3.0/24` (private). Multi-AZ is derived from zone count (≥ 2 → `multi_az = true` on RDS).

### Services → ECS resources

Per service:

```
aws_iam_role                  <name>-task   (task role)
aws_iam_role                  <name>-exec   (exec role — ECS implementation detail, not in Plan)
aws_iam_role_policy_attachment <name>-exec  → AmazonECSTaskExecutionRolePolicy
aws_security_group            <name>
aws_vpc_security_group_egress_rule  <name>_all  (allow all outbound)
aws_cloudwatch_log_group      /ground/<name>   retention 7 days
aws_ecs_task_definition       <name>
aws_ecs_service               <name>
```

Autoscaling (when `scaling` is declared):

```
aws_appautoscaling_target   <name>   CPU tracking, target 70%
aws_appautoscaling_policy   <name>_scale
```

Service-to-service access (when `access` entries reference services):

```
aws_vpc_security_group_ingress_rule  <src>_to_<tgt>[_<port>]
```

### Env → container environment variables

```ground
env prod {
  LOG_LEVEL: warn
}
```

Injected into every ECS task definition in the stack:

```json
"environment": [
  { "name": "LOG_LEVEL", "value": "warn" }
]
```

## Output

`ground gen terra` reads all `.grd` files in the current directory, finds all
`deploy` declarations, and writes one self-contained `main.tf.json` per stack
under `.ground/terra/<stack-name>/`.

`ground plan` generates Terraform, runs `terraform init`, then `terraform plan`
and displays a ground-level summary grouped by service, stack, and region.
