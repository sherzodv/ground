# Ground DSL

`.grd` files, merged per directory. Whitespace insignificant. Comments: `//`.

---

## Primitives

```
service <name> {
  image:   <image>:<tag>          // required
  scaling: { min: <n>, max: <n> } // optional; CPU-based autoscaling at 70%
  ports:   { <name>: <port>, … }  // named ports this service listens on
  access {                        // services this service may reach
    <service>: <port>, <port>     // port names from target's ports block
    <service>:                    // omit ports → all declared ports opened
  }
}

group <name> {
  <service>
  …
}

region <name> {
  aws:  <region>                  // e.g. us-east-1
  zone <id> { aws: <az> }         // e.g. zone 1 { aws: us-east-1a }
}

env <name> {
  <KEY>: <value>                  // injected into every container in the stack
}

stack <name> {
  env:    <env>
  region: <region>
  zone:   [<id>, …]
  group:  <group>
}

deploy to aws {
  stacks: [<stack>, …]
  override { <json> }             // optional; deep-merged into generated Terraform
}
```

---

## Example

```
service svc-api {
  image:   svc-api:prod
  scaling: { min: 2, max: 10 }
  access { svc-core: http }
}

service svc-core {
  image: svc-core:prod
  ports: { http: 8080 }
}

group backend { svc-api  svc-core }

region us-east {
  aws:  us-east-1
  zone 1 { aws: us-east-1a }
}

env prod { LOG_LEVEL: info }

stack prod {
  env:    prod
  region: us-east
  zone:   [1]
  group:  backend
}

deploy to aws { stacks: [prod] }
```

---

## CLI

```
ground init [--git-ignore]   create .ground/ + settings.json; patch .gitignore
ground gen terra             write .ground/terra/<stack>/main.tf.json
ground plan                  plan changes per Ground entity (no apply)
```

### ground plan output

```
running in plan mode, no changes will be made
plan to deploy prod stack to aws / us-east
running terraform plan
  terraform 1.9.0 ready
  starting state refresh
  ↻ refreshing aws_vpc.ground_prod
  ↻ refreshing aws_ecs_service.svc_api
  …
  state refresh complete
  computing plan
  running terraform show -json .tfplan

stack prod → aws
region us-east
env    prod
group  backend

create service svc-api
  + aws_ecs_service                     svc_api
  + aws_ecs_task_definition             svc_api
  + aws_iam_role                        svc_api_task
  + aws_iam_role                        svc_api_exec
  + aws_iam_role_policy_attachment      svc_api_exec
  + aws_security_group                  svc_api
  + aws_vpc_security_group_egress_rule  svc_api_all
  + aws_vpc_security_group_ingress_rule svc_api_to_svc_core_8080
  + aws_cloudwatch_log_group            _ground_svc_api

create stack prod
  + aws_ecs_cluster  ground_prod

create region us-east
  + aws_vpc                       ground_prod
  + aws_subnet                    prod_pub_1
  + aws_subnet                    prod_priv_1
  + aws_internet_gateway          ground_prod
  + aws_eip                       ground_prod_eip
  + aws_nat_gateway               ground_prod
  + aws_route_table               rt_prod_pub_1
  + aws_route_table               rt_prod_priv_1
  + aws_route                     rt_prod_pub_1_default
  + aws_route                     rt_prod_priv_1_default
  + aws_route_table_association   rt_prod_pub_1
  + aws_route_table_association   rt_prod_priv_1

create 23  modify 0  delete 0
```

---

## AWS resource mapping

**Per service**

| Ground | AWS |
|--------|-----|
| identity | `aws_iam_role` ×2 (task + exec), `aws_iam_role_policy_attachment` |
| network | `aws_security_group`, `aws_vpc_security_group_egress_rule` |
| logs | `aws_cloudwatch_log_group` |
| workload | `aws_ecs_task_definition`, `aws_ecs_service` |
| scaling | `aws_appautoscaling_target`, `aws_appautoscaling_policy` |
| access rule | `aws_vpc_security_group_ingress_rule` per port (on target service) |

**Per stack**

| Ground | AWS |
|--------|-----|
| stack | `aws_ecs_cluster` |
| region / zone | `aws_vpc`, `aws_subnet` ×2, `aws_internet_gateway`, `aws_eip`, `aws_nat_gateway`, `aws_route_table` ×2, `aws_route` ×2, `aws_route_table_association` ×2 |
