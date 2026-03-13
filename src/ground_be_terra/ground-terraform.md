# Ground → Terraform AWS Resource Naming

Variables: `{pfx}` = `deploy.prefix` (default: empty), `{alias}` = deploy alias, `{svc}` = service name, `{db}` = database name, `{src}` / `{tgt}` = source/target name (links), `{n}` = zone number from the region entry (e.g. `us-east:2` → `2`; stable across reordering, range 1–20).

AWS names use variables as-is. TF state keys use the same variables with hyphens replaced by underscores.

Rule: `{pfx}{alias}-{suffix}` / `{pfx}{alias}-{svc}-{suffix}` / `{pfx}{alias}-{db}-{suffix}`

## Named resources

| Resource | AWS name | Source | Limit |
|---|---|---|---|
| `aws_vpc` | `{pfx}{alias}-vpc` *(Name tag)* | deploy | 256 |
| `aws_internet_gateway` | `{pfx}{alias}-gw` *(Name tag)* | deploy | 256 |
| `aws_nat_gateway` | `{pfx}{alias}-nat` *(Name tag)* | deploy | 256 |
| `aws_subnet` public | `{pfx}{alias}-npub-{n}` *(Name tag)* | deploy | 256 |
| `aws_subnet` private | `{pfx}{alias}-nprv-{n}` *(Name tag)* | deploy | 256 |
| `aws_route_table` public | `{pfx}{alias}-rpub-{n}` *(Name tag)* | deploy | 256 |
| `aws_route_table` private | `{pfx}{alias}-rprv-{n}` *(Name tag)* | deploy | 256 |
| `aws_ecs_cluster` | `{pfx}{alias}-ecs` | deploy | 255 |
| `aws_ecs_task_definition` | `{pfx}{alias}-{svc}-td` *(family)* | service | 255 |
| `aws_ecs_service` | `{pfx}{alias}-{svc}-svc` | service | 255 |
| `aws_iam_role` task | `{pfx}{alias}-{svc}-t` | service | 64 |
| `aws_iam_role` exec | `{pfx}{alias}-{svc}-x` | service | 64 |
| `aws_cloudwatch_log_group` | `/{pfx}{alias}/{svc}-log` | service | 512 |
| `aws_security_group` (service) | `{pfx}{alias}-{svc}-sgs` | service | 255 |
| `aws_security_group` (database) | `{pfx}{alias}-{db}-sgd` | database | 255 |
| `aws_appautoscaling_policy` | `{pfx}{alias}-{svc}-scl` | service | 256 |
| `aws_db_subnet_group` | `{pfx}{alias}-{db}-ng` | database | 255 |
| `aws_db_instance` | `{pfx}{alias}-{db}-db` *(identifier)* | database | 63 |

## Unnamed resources

| Resource | Parent | Source |
|---|---|---|
| `aws_eip` | `aws_nat_gateway` | deploy |
| `aws_route_table_association` | `aws_route_table` | deploy |
| `aws_route` | `aws_route_table` | deploy |
| `aws_iam_role_policy_attachment` | `aws_iam_role` exec | service |
| `aws_vpc_security_group_egress_rule` | `aws_security_group` | service |
| `aws_appautoscaling_target` | `aws_ecs_service` | service |
| `aws_vpc_security_group_egress_rule` | `aws_security_group` | database |
| `aws_vpc_security_group_ingress_rule` | `aws_security_group` | access link |
| `random_password` | `aws_db_instance` | database |

## Suffixes

| Suffix | Resource | Limit |
|---|---|---|
| `vpc` | `aws_vpc` | 256 |
| `gw` | `aws_internet_gateway` | 256 |
| `nat` | `aws_nat_gateway` | 256 |
| `npub-{n}` | `aws_subnet` public | 256 |
| `nprv-{n}` | `aws_subnet` private | 256 |
| `rpub-{n}` | `aws_route_table` public | 256 |
| `rprv-{n}` | `aws_route_table` private | 256 |
| `ecs` | `aws_ecs_cluster` | 255 |
| `td` | `aws_ecs_task_definition` | 255 |
| `svc` | `aws_ecs_service` | 255 |
| `t` | `aws_iam_role` task | 64 |
| `x` | `aws_iam_role` exec | 64 |
| `log` | `aws_cloudwatch_log_group` | 512 |
| `sgs` | `aws_security_group` (service) | 255 |
| `sgd` | `aws_security_group` (database) | 255 |
| `scl` | `aws_appautoscaling_policy` | 256 |
| `ng` | `aws_db_subnet_group` | 255 |
| `db` | `aws_db_instance` | 63 |

## TF state keys

TF state key = the resource address Terraform stores in state (`resource_type.key`). Renaming a key destroys + recreates the resource.

| Resource | TF key |
|---|---|
| `aws_ecs_cluster` | `{pfx}{alias}_ecs` |
| `aws_vpc` | `{pfx}{alias}_vpc` |
| `aws_internet_gateway` | `{pfx}{alias}_gw` |
| `aws_eip` | `{pfx}{alias}_nat_eip` |
| `aws_nat_gateway` | `{pfx}{alias}_nat` |
| `aws_subnet` public | `{pfx}{alias}_npub_{n}` |
| `aws_subnet` private | `{pfx}{alias}_nprv_{n}` |
| `aws_route_table` public | `{pfx}{alias}_rpub_{n}` |
| `aws_route_table` private | `{pfx}{alias}_rprv_{n}` |
| `aws_route_table_association` public | `{pfx}{alias}_rpub_{n}` |
| `aws_route_table_association` private | `{pfx}{alias}_rprv_{n}` |
| `aws_route` public | `{pfx}{alias}_rpub_{n}_default` |
| `aws_route` private | `{pfx}{alias}_rprv_{n}_default` |
| `aws_cloudwatch_log_group` | `{pfx}{alias}_{svc}_log` |
| `aws_iam_role` task | `{pfx}{alias}_{svc}_t` |
| `aws_iam_role` exec | `{pfx}{alias}_{svc}_x` |
| `aws_iam_role_policy_attachment` exec | `{pfx}{alias}_{svc}_x` |
| `aws_security_group` (service) | `{pfx}{alias}_{svc}_sgs` |
| `aws_vpc_security_group_egress_rule` (service) | `{pfx}{alias}_{svc}_sgs_all` |
| `aws_ecs_task_definition` | `{pfx}{alias}_{svc}_td` |
| `aws_ecs_service` | `{pfx}{alias}_{svc}_svc` |
| `aws_appautoscaling_target` | `{pfx}{alias}_{svc}_svc` |
| `aws_appautoscaling_policy` | `{pfx}{alias}_{svc}_scl` |
| `random_password` | `{pfx}{alias}_{db}_db` |
| `aws_db_subnet_group` | `{pfx}{alias}_{db}_ng` |
| `aws_security_group` (database) | `{pfx}{alias}_{db}_sgd` |
| `aws_vpc_security_group_egress_rule` (database) | `{pfx}{alias}_{db}_sgd_all` |
| `aws_db_instance` | `{pfx}{alias}_{db}_db` |
| `aws_vpc_security_group_ingress_rule` (svc→svc) | `{pfx}{alias}_{src}_to_{tgt}` |
| `aws_vpc_security_group_ingress_rule` (svc→db) | `{pfx}{alias}_{src}_to_{tgt}_db` |

