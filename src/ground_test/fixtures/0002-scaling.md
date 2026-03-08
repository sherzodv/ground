# service with autoscaling

```ground
service svc-api { image: svc-api:prod scaling: { min: 2, max: 10 } }

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

```json
{
  "provider": {
    "aws": {
      "region": "us-east-1"
    }
  },
  "resource": {
    "aws_appautoscaling_policy": {
      "svc_api_scale": {
        "name": "svc-api-scale",
        "policy_type": "TargetTrackingScaling",
        "resource_id": "${aws_appautoscaling_target.svc_api.resource_id}",
        "scalable_dimension": "${aws_appautoscaling_target.svc_api.scalable_dimension}",
        "service_namespace": "${aws_appautoscaling_target.svc_api.service_namespace}",
        "target_tracking_scaling_policy_configuration": {
          "predefined_metric_specification": {
            "predefined_metric_type": "ECSServiceAverageCPUUtilization"
          },
          "target_value": 70.0
        }
      }
    },
    "aws_appautoscaling_target": {
      "svc_api": {
        "depends_on": [
          "${aws_ecs_service.svc_api}"
        ],
        "max_capacity": 10,
        "min_capacity": 2,
        "resource_id": "service/${aws_ecs_cluster.ground_prod.name}/svc-api",
        "scalable_dimension": "ecs:service:DesiredCount",
        "service_namespace": "ecs"
      }
    },
    "aws_cloudwatch_log_group": {
      "_ground_svc_api": {
        "name": "/ground/svc-api",
        "retention_in_days": 7
      }
    },
    "aws_ecs_cluster": {
      "ground_prod": {
        "name": "ground-prod"
      }
    },
    "aws_ecs_service": {
      "svc_api": {
        "capacity_provider_strategy": [
          {
            "capacity_provider": "FARGATE",
            "weight": 1
          }
        ],
        "cluster": "${aws_ecs_cluster.ground_prod.id}",
        "desired_count": 1,
        "name": "svc-api",
        "network_configuration": {
          "security_groups": [
            "${aws_security_group.svc_api.id}"
          ],
          "subnets": [
            "${aws_subnet.prod_priv_1.id}"
          ]
        },
        "task_definition": "${aws_ecs_task_definition.svc_api.arn}"
      }
    },
    "aws_ecs_task_definition": {
      "svc_api": {
        "container_definitions": "[{\"environment\":[{\"name\":\"LOG_LEVEL\",\"value\":\"info\"}],\"image\":\"svc-api:prod\",\"logConfiguration\":{\"logDriver\":\"awslogs\",\"options\":{\"awslogs-group\":\"/ground/svc-api\",\"awslogs-region\":\"us-east-1\",\"awslogs-stream-prefix\":\"ecs\"}},\"name\":\"svc-api\"}]",
        "cpu": "256",
        "execution_role_arn": "${aws_iam_role.svc_api_exec.arn}",
        "family": "svc-api",
        "memory": "512",
        "network_mode": "awsvpc",
        "requires_compatibilities": [
          "FARGATE"
        ],
        "task_role_arn": "${aws_iam_role.svc_api_task.arn}"
      }
    },
    "aws_eip": {
      "ground_prod_eip": {
        "domain": "vpc"
      }
    },
    "aws_iam_role": {
      "svc_api_exec": {
        "assume_role_policy": "{\"Statement\":[{\"Action\":\"sts:AssumeRole\",\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"ecs-tasks.amazonaws.com\"}}],\"Version\":\"2012-10-17\"}",
        "name": "svc-api-exec"
      },
      "svc_api_task": {
        "assume_role_policy": "{\"Statement\":[{\"Action\":\"sts:AssumeRole\",\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"ecs-tasks.amazonaws.com\"}}],\"Version\":\"2012-10-17\"}",
        "name": "svc-api-task"
      }
    },
    "aws_iam_role_policy_attachment": {
      "svc_api_exec": {
        "policy_arn": "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy",
        "role": "${aws_iam_role.svc_api_exec.name}"
      }
    },
    "aws_internet_gateway": {
      "ground_prod": {
        "tags": {
          "Name": "ground-prod"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      }
    },
    "aws_nat_gateway": {
      "ground_prod": {
        "allocation_id": "${aws_eip.ground_prod_eip.id}",
        "subnet_id": "${aws_subnet.prod_pub_1.id}",
        "tags": {
          "Name": "ground-prod"
        }
      }
    },
    "aws_route": {
      "rt_prod_priv_1_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "nat_gateway_id": "${aws_nat_gateway.ground_prod.id}",
        "route_table_id": "${aws_route_table.rt_prod_priv_1.id}"
      },
      "rt_prod_pub_1_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "gateway_id": "${aws_internet_gateway.ground_prod.id}",
        "route_table_id": "${aws_route_table.rt_prod_pub_1.id}"
      }
    },
    "aws_route_table": {
      "rt_prod_priv_1": {
        "tags": {
          "Name": "rt-prod-priv-1"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "rt_prod_pub_1": {
        "tags": {
          "Name": "rt-prod-pub-1"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      }
    },
    "aws_route_table_association": {
      "rt_prod_priv_1": {
        "route_table_id": "${aws_route_table.rt_prod_priv_1.id}",
        "subnet_id": "${aws_subnet.prod_priv_1.id}"
      },
      "rt_prod_pub_1": {
        "route_table_id": "${aws_route_table.rt_prod_pub_1.id}",
        "subnet_id": "${aws_subnet.prod_pub_1.id}"
      }
    },
    "aws_security_group": {
      "svc_api": {
        "name": "svc-api",
        "vpc_id": "${aws_vpc.ground_prod.id}"
      }
    },
    "aws_subnet": {
      "prod_priv_1": {
        "availability_zone": "us-east-1a",
        "cidr_block": "10.0.1.0/24",
        "map_public_ip_on_launch": false,
        "tags": {
          "Name": "prod-priv-1"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "prod_pub_1": {
        "availability_zone": "us-east-1a",
        "cidr_block": "10.0.0.0/24",
        "map_public_ip_on_launch": true,
        "tags": {
          "Name": "prod-pub-1"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      }
    },
    "aws_vpc": {
      "ground_prod": {
        "cidr_block": "10.0.0.0/16",
        "enable_dns_hostnames": true,
        "enable_dns_support": true,
        "tags": {
          "Name": "ground-prod"
        }
      }
    },
    "aws_vpc_security_group_egress_rule": {
      "svc_api_all": {
        "cidr_ipv4": "0.0.0.0/0",
        "ip_protocol": "-1",
        "security_group_id": "${aws_security_group.svc_api.id}"
      }
    }
  },
  "terraform": {
    "required_providers": {
      "aws": {
        "source": "hashicorp/aws",
        "version": "~> 5.0"
      }
    }
  }
}
```

## Explain

Adding `scaling: { min: 2, max: 10 }` generates two additional resources on top
of the six from a minimal service. Everything else is identical.

**Application Auto Scaling target — registering what can scale**

`aws_appautoscaling_target` tells AWS's general-purpose autoscaling system that
this ECS service is a thing it is allowed to adjust. The key fields:

- `scalable_dimension: ecs:service:DesiredCount` — the knob being turned is the
  number of running container copies
- `min_capacity: 2` / `max_capacity: 10` — hard bounds; autoscaling will never
  go below 2 or above 10 regardless of load
- `depends_on` the ECS service — the service must exist before it can be
  registered

Note that `desired_count: 1` in the ECS service is just the initial value at
Terraform apply time. Autoscaling overrides it at runtime and takes ownership
from there. Because `min` is 2, AWS will immediately correct to 2 when
autoscaling first runs.

**Application Auto Scaling policy — the scaling rule**

`aws_appautoscaling_policy` defines when and how to move the desired count. This
uses *target tracking*: you name a metric and a target value, and AWS
continuously adjusts the count to keep the metric near that target — similar to
a thermostat.

- metric: `ECSServiceAverageCPUUtilization` — average CPU across all running
  containers for this service, measured as a percentage
- target: `70.0` — AWS tries to keep average CPU at 70 %

When CPU rises above 70 % and stays there, AWS adds containers (scale out).
When it drops well below, AWS removes containers (scale in) — with a built-in
cooldown to avoid thrashing. AWS calculates how many containers to add or remove
automatically; you only specify the target.

An additional variable is required beyond the minimal set: `var.ecs_cluster_name`
(the cluster name as a string, used to build the resource ID for the autoscaling
target).
