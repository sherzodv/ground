# minimal service

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

```json
{
  "provider": {
    "aws": {
      "region": "us-east-1"
    }
  },
  "resource": {
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

A single `service` declaration generates six AWS resources that together run one
container reliably in the cloud.

**IAM roles — identity and permissions**

AWS requires every piece of infrastructure to have an explicit identity (a
"role") before it is allowed to do anything.

- `svc-api-exec` — used by AWS itself, not your code. When ECS starts your
  container it needs permission to pull the Docker image and write logs. The
  attached policy `AmazonECSTaskExecutionRolePolicy` is an AWS-managed set of
  permissions built exactly for this. Your code never assumes this role.
- `svc-api-task` — the identity your container runs as at runtime. Nothing is
  attached to it yet; permissions get added as the service gains access to
  databases, buckets, etc.

**Security group — virtual firewall**

`aws_security_group` is attached to the container's network interface. No
inbound rules means nothing can open a connection to this container from
outside. The single egress rule (`protocol: -1`, `cidr: 0.0.0.0/0`) lets the
container reach any address on any port — needed for outbound HTTP calls,
package pulls, etc. It lives inside `var.vpc_id`, the private AWS network you
provide to Ground.

**CloudWatch log group — log storage**

`aws_cloudwatch_log_group` named `/ground/svc-api` is where container stdout
and stderr land. ECS streams logs there automatically via the `awslogs` driver
declared inside the task definition. Retention is set to 7 days; after that AWS
discards old entries automatically.

**ECS task definition — the container blueprint**

`aws_ecs_task_definition` is a versioned template describing how to run the
container:

- image: `svc-api:prod`
- resources: 256 CPU units (¼ vCPU) and 512 MB RAM
- networking: `awsvpc` — each container gets its own network interface,
  required for Fargate
- roles: the exec role (for startup) and the task role (for runtime)
- logging: ship to the log group above via `awslogs`

The task definition is immutable; changing it creates a new version.

**ECS service — the runtime scheduler**

`aws_ecs_service` is what actually keeps the container running. It reads the
task definition and maintains `desired_count: 1` copy alive at all times — if
the container crashes or the host fails, ECS replaces it automatically. It
places the container in `var.private_subnet_ids` (internal subnets, not
internet-reachable) and attaches the security group. It runs on the ECS cluster
identified by `var.ecs_cluster_id`, a logical grouping of services you provide.

**Variables expected from outside**

The generated Terraform is not standalone — it references four variables that
must be provided by a root Terraform module wrapping it:

- `vpc_id` — the private AWS network your services live in. A VPC has IP
  ranges, routing tables, and gateway config. It is typically shared across many
  projects and created once separately; Ground does not create it.
- `aws_region` — e.g. `us-east-1`. Ground generates region-agnostic resources;
  the actual deployment target is injected here at apply time.
- `ecs_cluster_id` — an ECS cluster is a logical namespace that groups ECS
  services. Usually one per environment, shared, created outside Ground.
- `private_subnet_ids` — subnets are subdivisions of the VPC. Private means no
  direct inbound internet access. Ground places containers in them but does not
  create them.

Ground owns the service layer. The network and cluster layer is infrastructure
you bring.
