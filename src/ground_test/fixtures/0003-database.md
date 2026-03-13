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
      "prod_svc_api_scl": {
        "name": "prod-svc-api-scl",
        "policy_type": "TargetTrackingScaling",
        "resource_id": "${aws_appautoscaling_target.prod_svc_api_svc.resource_id}",
        "scalable_dimension": "${aws_appautoscaling_target.prod_svc_api_svc.scalable_dimension}",
        "service_namespace": "${aws_appautoscaling_target.prod_svc_api_svc.service_namespace}",
        "target_tracking_scaling_policy_configuration": {
          "predefined_metric_specification": {
            "predefined_metric_type": "ECSServiceAverageCPUUtilization"
          },
          "target_value": 70.0
        }
      }
    },
    "aws_appautoscaling_target": {
      "prod_svc_api_svc": {
        "max_capacity": 1,
        "min_capacity": 1,
        "resource_id": "service/${aws_ecs_cluster.prod_ecs.name}/prod-svc-api-svc",
        "scalable_dimension": "ecs:service:DesiredCount",
        "service_namespace": "ecs"
      }
    },
    "aws_cloudwatch_log_group": {
      "prod_svc_api_log": {
        "name": "/prod/svc-api-log",
        "retention_in_days": 7,
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_db_instance": {
      "prod_db_main_db": {
        "allocated_storage": 20,
        "db_subnet_group_name": "${aws_db_subnet_group.prod_db_main_ng.name}",
        "engine": "postgres",
        "engine_version": "15",
        "identifier": "prod-db-main-db",
        "instance_class": "db.t3.medium",
        "multi_az": true,
        "password": "${random_password.prod_db_main_db.result}",
        "skip_final_snapshot": true,
        "tags": {
          "ground-managed": "true"
        },
        "username": "admin",
        "vpc_security_group_ids": [
          "${aws_security_group.prod_db_main_sgd.id}"
        ]
      }
    },
    "aws_db_subnet_group": {
      "prod_db_main_ng": {
        "name": "prod-db-main-ng",
        "subnet_ids": [
          "${aws_subnet.prod_nprv_1.id}",
          "${aws_subnet.prod_nprv_2.id}"
        ],
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_ecs_cluster": {
      "prod_ecs": {
        "name": "prod-ecs",
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_ecs_service": {
      "prod_svc_api_svc": {
        "capacity_provider_strategy": [
          {
            "capacity_provider": "FARGATE",
            "weight": 1
          }
        ],
        "cluster": "${aws_ecs_cluster.prod_ecs.id}",
        "desired_count": 1,
        "name": "prod-svc-api-svc",
        "network_configuration": {
          "security_groups": [
            "${aws_security_group.prod_svc_api_sgs.id}"
          ],
          "subnets": [
            "${aws_subnet.prod_nprv_1.id}",
            "${aws_subnet.prod_nprv_2.id}"
          ]
        },
        "tags": {
          "ground-managed": "true"
        },
        "task_definition": "${aws_ecs_task_definition.prod_svc_api_td.arn}"
      }
    },
    "aws_ecs_task_definition": {
      "prod_svc_api_td": {
        "container_definitions": "[{\"name\":\"svc-api\",\"image\":\"svc-api:prod\",\"logConfiguration\":{\"logDriver\":\"awslogs\",\"options\":{\"awslogs-group\":\"/prod/svc-api-log\",\"awslogs-region\":\"us-east-1\",\"awslogs-stream-prefix\":\"ecs\"}}}]",
        "cpu": "256",
        "execution_role_arn": "${aws_iam_role.prod_svc_api_x.arn}",
        "family": "prod-svc-api-td",
        "memory": "512",
        "network_mode": "awsvpc",
        "requires_compatibilities": [
          "FARGATE"
        ],
        "tags": {
          "ground-managed": "true"
        },
        "task_role_arn": "${aws_iam_role.prod_svc_api_t.arn}"
      }
    },
    "aws_eip": {
      "prod_nat_eip": {
        "domain": "vpc",
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_iam_role": {
      "prod_svc_api_t": {
        "assume_role_policy": "{\"Statement\":[{\"Action\":\"sts:AssumeRole\",\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"ecs-tasks.amazonaws.com\"}}],\"Version\":\"2012-10-17\"}",
        "name": "prod-svc-api-t",
        "tags": {
          "ground-managed": "true"
        }
      },
      "prod_svc_api_x": {
        "assume_role_policy": "{\"Statement\":[{\"Action\":\"sts:AssumeRole\",\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"ecs-tasks.amazonaws.com\"}}],\"Version\":\"2012-10-17\"}",
        "name": "prod-svc-api-x",
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_iam_role_policy_attachment": {
      "prod_svc_api_x": {
        "policy_arn": "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy",
        "role": "${aws_iam_role.prod_svc_api_x.name}"
      }
    },
    "aws_internet_gateway": {
      "prod_gw": {
        "tags": {
          "Name": "prod-gw",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      }
    },
    "aws_nat_gateway": {
      "prod_nat": {
        "allocation_id": "${aws_eip.prod_nat_eip.id}",
        "subnet_id": "${aws_subnet.prod_npub_1.id}",
        "tags": {
          "Name": "prod-nat",
          "ground-managed": "true"
        }
      }
    },
    "aws_route": {
      "prod_rprv_1_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "nat_gateway_id": "${aws_nat_gateway.prod_nat.id}",
        "route_table_id": "${aws_route_table.prod_rprv_1.id}"
      },
      "prod_rprv_2_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "nat_gateway_id": "${aws_nat_gateway.prod_nat.id}",
        "route_table_id": "${aws_route_table.prod_rprv_2.id}"
      },
      "prod_rpub_1_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "gateway_id": "${aws_internet_gateway.prod_gw.id}",
        "route_table_id": "${aws_route_table.prod_rpub_1.id}"
      },
      "prod_rpub_2_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "gateway_id": "${aws_internet_gateway.prod_gw.id}",
        "route_table_id": "${aws_route_table.prod_rpub_2.id}"
      }
    },
    "aws_route_table": {
      "prod_rprv_1": {
        "tags": {
          "Name": "prod-rprv-1",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      },
      "prod_rprv_2": {
        "tags": {
          "Name": "prod-rprv-2",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      },
      "prod_rpub_1": {
        "tags": {
          "Name": "prod-rpub-1",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      },
      "prod_rpub_2": {
        "tags": {
          "Name": "prod-rpub-2",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      }
    },
    "aws_route_table_association": {
      "prod_rprv_1": {
        "route_table_id": "${aws_route_table.prod_rprv_1.id}",
        "subnet_id": "${aws_subnet.prod_nprv_1.id}"
      },
      "prod_rprv_2": {
        "route_table_id": "${aws_route_table.prod_rprv_2.id}",
        "subnet_id": "${aws_subnet.prod_nprv_2.id}"
      },
      "prod_rpub_1": {
        "route_table_id": "${aws_route_table.prod_rpub_1.id}",
        "subnet_id": "${aws_subnet.prod_npub_1.id}"
      },
      "prod_rpub_2": {
        "route_table_id": "${aws_route_table.prod_rpub_2.id}",
        "subnet_id": "${aws_subnet.prod_npub_2.id}"
      }
    },
    "aws_security_group": {
      "prod_db_main_sgd": {
        "name": "prod-db-main-sgd",
        "tags": {
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      },
      "prod_svc_api_sgs": {
        "name": "prod-svc-api-sgs",
        "tags": {
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      }
    },
    "aws_subnet": {
      "prod_nprv_1": {
        "availability_zone": "us-east-1a",
        "cidr_block": "10.0.1.0/24",
        "map_public_ip_on_launch": false,
        "tags": {
          "Name": "prod-nprv-1",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      },
      "prod_nprv_2": {
        "availability_zone": "us-east-1b",
        "cidr_block": "10.0.3.0/24",
        "map_public_ip_on_launch": false,
        "tags": {
          "Name": "prod-nprv-2",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      },
      "prod_npub_1": {
        "availability_zone": "us-east-1a",
        "cidr_block": "10.0.0.0/24",
        "map_public_ip_on_launch": true,
        "tags": {
          "Name": "prod-npub-1",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      },
      "prod_npub_2": {
        "availability_zone": "us-east-1b",
        "cidr_block": "10.0.2.0/24",
        "map_public_ip_on_launch": true,
        "tags": {
          "Name": "prod-npub-2",
          "ground-managed": "true"
        },
        "vpc_id": "${aws_vpc.prod_vpc.id}"
      }
    },
    "aws_vpc": {
      "prod_vpc": {
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
      "prod_db_main_sgd_all": {
        "cidr_ipv4": "0.0.0.0/0",
        "ip_protocol": "-1",
        "security_group_id": "${aws_security_group.prod_db_main_sgd.id}",
        "tags": {
          "ground-managed": "true"
        }
      },
      "prod_svc_api_sgs_all": {
        "cidr_ipv4": "0.0.0.0/0",
        "ip_protocol": "-1",
        "security_group_id": "${aws_security_group.prod_svc_api_sgs.id}",
        "tags": {
          "ground-managed": "true"
        }
      }
    },
    "aws_vpc_security_group_ingress_rule": {
      "prod_svc_api_to_db_main_db": {
        "from_port": 5432,
        "ip_protocol": "tcp",
        "referenced_security_group_id": "${aws_security_group.prod_svc_api_sgs.id}",
        "security_group_id": "${aws_security_group.prod_db_main_sgd.id}",
        "tags": {
          "ground-managed": "true"
        },
        "to_port": 5432
      }
    },
    "random_password": {
      "prod_db_main_db": {
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