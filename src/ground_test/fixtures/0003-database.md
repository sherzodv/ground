# database

```ground
database db-main {
  engine:  postgres
  size:    medium
  storage: 20
}

service svc-api {
  image:  svc-api:prod
  access: db-main
}

deploy prod to aws as prod {
  region: [ us-east:1  us-east:2 ]
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
        "name": "prod-svc-api-scl",
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
        "max_capacity": 1,
        "min_capacity": 1,
        "resource_id": "service/${aws_ecs_cluster.ground_prod.name}/prod-svc-api-svc",
        "scalable_dimension": "ecs:service:DesiredCount",
        "service_namespace": "ecs"
      }
    },
    "aws_cloudwatch_log_group": {
      "_ground_svc_api": {
        "name": "/prod/svc-api-log",
        "retention_in_days": 7,
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_db_instance": {
      "db_main": {
        "allocated_storage": 20,
        "db_subnet_group_name": "${aws_db_subnet_group.db_main.name}",
        "engine": "postgres",
        "engine_version": "15",
        "identifier": "prod-db-main-db",
        "instance_class": "db.t3.medium",
        "multi_az": true,
        "password": "${random_password.db_main.result}",
        "skip_final_snapshot": true,
        "tags": {
          "ground-managed": "true"
        },
        "username": "admin",
        "vpc_security_group_ids": [
          "${aws_security_group.db_main_db.id}"
        ]
      }
    },
    "aws_db_subnet_group": {
      "db_main": {
        "name": "prod-db-main-ng",
        "subnet_ids": [
          "${aws_subnet.prod_priv_1.id}",
          "${aws_subnet.prod_priv_2.id}"
        ],
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_ecs_cluster": {
      "ground_prod": {
        "name": "prod-ecs",
        "tags": {
          "ground-managed": "true"
        }
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
        "name": "prod-svc-api-svc",
        "network_configuration": {
          "security_groups": [
            "${aws_security_group.svc_api.id}"
          ],
          "subnets": [
            "${aws_subnet.prod_priv_1.id}",
            "${aws_subnet.prod_priv_2.id}"
          ]
        },
        "tags": {
          "ground-managed": "true"
        },
        "task_definition": "${aws_ecs_task_definition.svc_api.arn}"
      }
    },
    "aws_ecs_task_definition": {
      "svc_api": {
        "container_definitions": "[{\"name\":\"svc-api\",\"image\":\"svc-api:prod\",\"logConfiguration\":{\"logDriver\":\"awslogs\",\"options\":{\"awslogs-group\":\"/prod/svc-api-log\",\"awslogs-region\":\"us-east-1\",\"awslogs-stream-prefix\":\"ecs\"}}}]",
        "cpu": "256",
        "execution_role_arn": "${aws_iam_role.svc_api_exec.arn}",
        "family": "prod-svc-api-td",
        "memory": "512",
        "network_mode": "awsvpc",
        "requires_compatibilities": [
          "FARGATE"
        ],
        "tags": {
          "ground-managed": "true"
        },
        "task_role_arn": "${aws_iam_role.svc_api_task.arn}"
      }
    },
    "aws_eip": {
      "ground_prod_eip": {
        "domain": "vpc",
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_iam_role": {
      "svc_api_exec": {
        "assume_role_policy": "{\"Statement\":[{\"Action\":\"sts:AssumeRole\",\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"ecs-tasks.amazonaws.com\"}}],\"Version\":\"2012-10-17\"}",
        "name": "prod-svc-api-x",
        "tags": {
          "ground-managed": "true"
        }
      },
      "svc_api_task": {
        "assume_role_policy": "{\"Statement\":[{\"Action\":\"sts:AssumeRole\",\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"ecs-tasks.amazonaws.com\"}}],\"Version\":\"2012-10-17\"}",
        "name": "prod-svc-api-t",
        "tags": {
          "ground-managed": "true"
        }
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
          "Name": "prod-gw",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      }
    },
    "aws_nat_gateway": {
      "ground_prod": {
        "allocation_id": "${aws_eip.ground_prod_eip.id}",
        "subnet_id": "${aws_subnet.prod_pub_1.id}",
        "tags": {
          "Name": "prod-nat",
          "ground-managed": "true"
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
          "Name": "prod-rprv-1",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "rt_prod_priv_2": {
        "tags": {
          "Name": "prod-rprv-2",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "rt_prod_pub_1": {
        "tags": {
          "Name": "prod-rpub-1",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "rt_prod_pub_2": {
        "tags": {
          "Name": "prod-rpub-2",
          "ground-managed": "true"
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
        "name": "prod-db-main-sgd",
        "tags": {
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "svc_api": {
        "name": "prod-svc-api-sgs",
        "tags": {
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      }
    },
    "aws_subnet": {
      "prod_priv_1": {
        "availability_zone": "us-east-1a",
        "cidr_block": "10.0.1.0/24",
        "map_public_ip_on_launch": false,
        "tags": {
          "Name": "prod-nprv-1",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "prod_priv_2": {
        "availability_zone": "us-east-1b",
        "cidr_block": "10.0.3.0/24",
        "map_public_ip_on_launch": false,
        "tags": {
          "Name": "prod-nprv-2",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "prod_pub_1": {
        "availability_zone": "us-east-1a",
        "cidr_block": "10.0.0.0/24",
        "map_public_ip_on_launch": true,
        "tags": {
          "Name": "prod-npub-1",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.ground_prod.id}"
      },
      "prod_pub_2": {
        "availability_zone": "us-east-1b",
        "cidr_block": "10.0.2.0/24",
        "map_public_ip_on_launch": true,
        "tags": {
          "Name": "prod-npub-2",
          "ground-managed": "true"
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
          "Name": "prod-vpc",
          "ground-managed": "true"
        }
      }
    },
    "aws_vpc_security_group_egress_rule": {
      "db_main_db_all": {
        "cidr_ipv4": "0.0.0.0/0",
        "ip_protocol": "-1",
        "security_group_id": "${aws_security_group.db_main_db.id}",
        "tags": {
          "ground-managed": "true"
        }
      },
      "svc_api_all": {
        "cidr_ipv4": "0.0.0.0/0",
        "ip_protocol": "-1",
        "security_group_id": "${aws_security_group.svc_api.id}",
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_vpc_security_group_ingress_rule": {
      "svc_api_to_db_main_db": {
        "from_port": 5432,
        "ip_protocol": "tcp",
        "referenced_security_group_id": "${aws_security_group.svc_api.id}",
        "security_group_id": "${aws_security_group.db_main_db.id}",
        "tags": {
          "ground-managed": "true"
        },
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