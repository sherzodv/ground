# minimal service

```ground
service svc-api {
  image: svc-api:prod
}

stack prod {
  service:svc-api
}

deploy prod to aws as prod {
  region: [ us-east:1 ]
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
      "prod_svc_api_log": {
        "name": "/prod/svc-api-log",
        "retention_in_days": 7,
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
            "${aws_subnet.prod_nprv_1.id}"
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
      "prod_rpub_1_default": {
        "destination_cidr_block": "0.0.0.0/0",
        "gateway_id": "${aws_internet_gateway.prod_gw.id}",
        "route_table_id": "${aws_route_table.prod_rpub_1.id}"
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
      "prod_rpub_1": {
        "tags": {
          "Name": "prod-rpub-1",
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
      "prod_rpub_1": {
        "route_table_id": "${aws_route_table.prod_rpub_1.id}",
        "subnet_id": "${aws_subnet.prod_npub_1.id}"
      }
    },
    "aws_security_group": {
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
      "prod_npub_1": {
        "availability_zone": "us-east-1a",
        "cidr_block": "10.0.0.0/24",
        "map_public_ip_on_launch": true,
        "tags": {
          "Name": "prod-npub-1",
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
      "prod_svc_api_sgs_all": {
        "cidr_ipv4": "0.0.0.0/0",
        "ip_protocol": "-1",
        "security_group_id": "${aws_security_group.prod_svc_api_sgs.id}",
        "tags": {
          "ground-managed": "true"
        }
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