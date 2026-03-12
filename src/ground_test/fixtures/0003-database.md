# 0003 — database

```ground
compute db-small {
  aws: db.t3.micro
}

database db-main {
  engine:  postgres
  version: 15
  compute: db-small
}

service svc-api {
  image:  svc-api:prod
  access { db-main }
}

group backend {
  svc-api
  db-main
}

region us-east {
  aws:  us-east-1
  zone 1 { aws: us-east-1a }
  zone 2 { aws: us-east-1b }
}

env prod {
  LOG_LEVEL: info
}

stack prod {
  env:    prod
  region: us-east
  zone:   [1, 2]
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
    "aws_db_instance": {
      "db_main": {
        "allocated_storage": 20,
        "db_name": "db_main",
        "db_subnet_group_name": "${aws_db_subnet_group.ground_prod_db_main.name}",
        "engine": "postgres",
        "engine_version": "15",
        "identifier": "db-main",
        "instance_class": "db.t3.micro",
        "multi_az": true,
        "password": "${random_password.db_main.result}",
        "skip_final_snapshot": true,
        "username": "ground",
        "vpc_security_group_ids": [
          "${aws_security_group.db_main_db.id}"
        ]
      }
    },
    "aws_db_subnet_group": {
      "ground_prod_db_main": {
        "name": "ground-prod-db-main",
        "subnet_ids": [
          "${aws_subnet.prod_priv_1.id}",
          "${aws_subnet.prod_priv_2.id}"
        ]
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
            "${aws_subnet.prod_priv_1.id}",
            "${aws_subnet.prod_priv_2.id}"
          ]
        },
        "task_definition": "${aws_ecs_task_definition.svc_api.arn}"
      }
    },
    "aws_ecs_task_definition": {
      "svc_api": {
        "container_definitions": "[{\"environment\":[{\"name\":\"LOG_LEVEL\",\"value\":\"info\"},{\"name\":\"DB_MAIN_HOST\",\"value\":\"${aws_db_instance.db_main.address}\"},{\"name\":\"DB_MAIN_PORT\",\"value\":\"5432\"},{\"name\":\"DB_MAIN_NAME\",\"value\":\"db_main\"},{\"name\":\"DB_MAIN_USER\",\"value\":\"ground\"},{\"name\":\"DB_MAIN_PASSWORD\",\"value\":\"${random_password.db_main.result}\"}],\"image\":\"svc-api:prod\",\"logConfiguration\":{\"logDriver\":\"awslogs\",\"options\":{\"awslogs-group\":\"/ground/svc-api\",\"awslogs-region\":\"us-east-1\",\"awslogs-stream-prefix\":\"ecs\"}},\"name\":\"svc-api\"}]",
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
      "rt_prod_priv_2_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "nat_gateway_id": "${aws_nat_gateway.ground_prod.id}",
        "route_table_id": "${aws_route_table.rt_prod_priv_2.id}"
      },
      "rt_prod_pub_1_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "gateway_id": "${aws_internet_gateway.ground_prod.id}",
        "route_table_id": "${aws_route_table.rt_prod_pub_1.id}"
      },
      "rt_prod_pub_2_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "gateway_id": "${aws_internet_gateway.ground_prod.id}",
        "route_table_id": "${aws_route_table.rt_prod_pub_2.id}"
      }
    },
    "aws_route_table": {
      "rt_prod_priv_1": {
        "tags": {
          "Name": "rt-prod-priv-1"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "rt_prod_priv_2": {
        "tags": {
          "Name": "rt-prod-priv-2"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "rt_prod_pub_1": {
        "tags": {
          "Name": "rt-prod-pub-1"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "rt_prod_pub_2": {
        "tags": {
          "Name": "rt-prod-pub-2"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      }
    },
    "aws_route_table_association": {
      "rt_prod_priv_1": {
        "route_table_id": "${aws_route_table.rt_prod_priv_1.id}",
        "subnet_id": "${aws_subnet.prod_priv_1.id}"
      },
      "rt_prod_priv_2": {
        "route_table_id": "${aws_route_table.rt_prod_priv_2.id}",
        "subnet_id": "${aws_subnet.prod_priv_2.id}"
      },
      "rt_prod_pub_1": {
        "route_table_id": "${aws_route_table.rt_prod_pub_1.id}",
        "subnet_id": "${aws_subnet.prod_pub_1.id}"
      },
      "rt_prod_pub_2": {
        "route_table_id": "${aws_route_table.rt_prod_pub_2.id}",
        "subnet_id": "${aws_subnet.prod_pub_2.id}"
      }
    },
    "aws_security_group": {
      "db_main_db": {
        "name": "db-main-db",
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
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
      "prod_priv_2": {
        "availability_zone": "us-east-1b",
        "cidr_block": "10.0.3.0/24",
        "map_public_ip_on_launch": false,
        "tags": {
          "Name": "prod-priv-2"
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
      },
      "prod_pub_2": {
        "availability_zone": "us-east-1b",
        "cidr_block": "10.0.2.0/24",
        "map_public_ip_on_launch": true,
        "tags": {
          "Name": "prod-pub-2"
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
      "db_main_db_all": {
        "cidr_ipv4": "0.0.0.0/0",
        "ip_protocol": "-1",
        "security_group_id": "${aws_security_group.db_main_db.id}"
      },
      "svc_api_all": {
        "cidr_ipv4": "0.0.0.0/0",
        "ip_protocol": "-1",
        "security_group_id": "${aws_security_group.svc_api.id}"
      }
    },
    "aws_vpc_security_group_ingress_rule": {
      "svc_api_to_db_main_db": {
        "from_port": 5432,
        "ip_protocol": "tcp",
        "referenced_security_group_id": "${aws_security_group.svc_api.id}",
        "security_group_id": "${aws_security_group.db_main_db.id}",
        "to_port": 5432
      }
    },
    "random_password": {
      "db_main": {
        "length": 32,
        "special": false
      }
    }
  },
  "terraform": {
    "required_providers": {
      "aws": {
        "source": "hashicorp/aws",
        "version": "~> 5.0"
      },
      "random": {
        "source": "hashicorp/random",
        "version": "~> 3.0"
      }
    }
  }
}
```
