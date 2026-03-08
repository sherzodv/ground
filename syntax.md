# Ground DSL Syntax

Ground infrastructure is declared in `.grd` files. Multiple files in the same directory are merged. Whitespace and line breaks are insignificant. Comments start with `//`.

---

## service

Declares a containerised workload.

```
service <name> {
  image:   <image>:<tag>
  scaling: { min: <n>, max: <n> }
  ports:   { <name>: <port>, … }
  access {
    <service>: <port>, <port>
    <service>: <port>
  }
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `image` | yes | Container image reference |
| `scaling` | no | Autoscaling bounds (CPU-based, target 70%) |
| `ports` | no | Named ports the service listens on |
| `access` | no | Services this service is allowed to reach |

`access` entries reference another service by name and list the port names to open. Port names must be declared in the target service's `ports` block. If no ports are listed, all declared ports are opened.

```
service svc-api {
  image:   svc-api:prod
  scaling: { min: 2, max: 10 }
  access {
    svc-core: http,
    svc-pay:  http
  }
}

service svc-core {
  image: svc-core:prod
  ports: { http: 8080 }
}

service svc-pay {
  image: svc-pay:prod
  ports: { http: 8080, grpc: 9090 }
}
```

---

## group

A named set of services deployed together.

```
group <name> {
  <service>
  <service>
}
```

```
group backend {
  svc-api
  svc-core
  svc-pay
}
```

---

## region

Maps a Ground region name to a provider region and its availability zones.

```
region <name> {
  aws:  <provider-region>
  zone <id> { aws: <provider-zone> }
}
```

```
region us-east {
  aws:  us-east-1
  zone 1 { aws: us-east-1a }
  zone 2 { aws: us-east-1b }
}
```

---

## env

A named set of environment variables injected into all services in a stack.

```
env <name> {
  <KEY>: <value>
}
```

```
env prod {
  LOG_LEVEL: info
  METRICS:   enabled
}
```

---

## stack

Binds a group of services to a region, zone selection, and environment. A stack is the unit of deployment.

```
stack <name> {
  env:    <env>
  region: <region>
  zone:   [<id>, …]
  group:  <group>
}
```

```
stack prod {
  env:    prod
  region: us-east
  zone:   [1, 2]
  group:  backend
}
```

---

## deploy

Triggers generation for one or more stacks on a provider.

```
deploy to <provider> {
  stacks: [<stack>, …]
  override { <json> }
}
```

`provider` is currently `aws`. The `override` block is optional — its content is raw JSON deep-merged into the generated Terraform output, useful for provider configuration like local testing endpoints.

```
deploy to aws {
  stacks: [prod]
}
```

```
deploy to aws {
  stacks: [prod]
  override {
    "provider": {
      "aws": {
        "endpoint_url": "http://localhost:4566"
      }
    }
  }
}
```

---

## CLI

```
ground init [--git-ignore]   create .ground/ and settings; optionally patch .gitignore
ground gen terra             generate .ground/terra/<stack>/main.tf.json
ground plan                  show what would change per Ground entity
```

### ground plan output

```
stack prod → aws
region us-east
env    prod
group  backend

create service svc-api
  + aws_ecs_service                   svc_api
  + aws_ecs_task_definition           svc_api
  + aws_iam_role                      svc_api_task
  + aws_iam_role                      svc_api_exec
  + aws_iam_role_policy_attachment    svc_api_exec
  + aws_security_group                svc_api
  + aws_vpc_security_group_egress_rule  svc_api_all
  + aws_cloudwatch_log_group          _ground_svc_api

create stack prod
  + aws_ecs_cluster  ground_prod

create region us-east
  + aws_vpc                    ground_prod
  + aws_subnet                 prod_pub_1
  + aws_subnet                 prod_priv_1
  + aws_internet_gateway       ground_prod
  + aws_eip                    ground_prod_eip
  + aws_nat_gateway            ground_prod
  + aws_route_table            rt_prod_pub_1
  + aws_route_table            rt_prod_priv_1
  + aws_route                  rt_prod_pub_1_default
  + aws_route                  rt_prod_priv_1_default
  + aws_route_table_association  rt_prod_pub_1
  + aws_route_table_association  rt_prod_priv_1

create 21  modify 0  delete 0
```

---

## What Ground generates per service (AWS)

| Ground concept | AWS resource |
|----------------|-------------|
| service identity | `aws_iam_role` (task + exec) + `aws_iam_role_policy_attachment` |
| service network | `aws_security_group` + `aws_vpc_security_group_egress_rule` |
| service logs | `aws_cloudwatch_log_group` |
| service workload | `aws_ecs_task_definition` + `aws_ecs_service` |
| scaling | `aws_appautoscaling_target` + `aws_appautoscaling_policy` |
| access rule | `aws_vpc_security_group_ingress_rule` (on target service) |

Per stack:

| Ground concept | AWS resource |
|----------------|-------------|
| stack | `aws_ecs_cluster` |
| region network | `aws_vpc`, `aws_subnet` ×2/zone, `aws_internet_gateway`, `aws_eip`, `aws_nat_gateway`, `aws_route_table` ×2/zone, `aws_route` ×2/zone, `aws_route_table_association` ×2/zone |
