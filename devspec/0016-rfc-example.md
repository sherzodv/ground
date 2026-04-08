# ECS Architecture Example

Demonstrates option C — anonymous link functions as edge handlers — to hide all infrastructure
complexity in the std. User only specifies architecture; vendor resources are derived implicitly.

---

## Std

### Common

```ground
type tag(name: reference, value: reference) = {
  link {name} = {value}
}
```

### Network

```ground
type aws_vpc = {
  link cidr_block = reference
  link enable_dns_hostnames = true | false
  link enable_dns_support = true | false
  link tags = [ type:tag ]
  enable_dns_hostnames: true
  enable_dns_support: true
}

type aws_subnet = {
  link vpc = type:aws_vpc
  link cidr_block = reference
  link availability_zone = reference
  link map_public_ip_on_launch = true | false
  link tags = [ type:tag ]
  map_public_ip_on_launch: false
}

type aws_internet_gateway = {
  link vpc = type:aws_vpc
  link tags = [ type:tag ]
}

type aws_sg_rule = {
  link from_port = int
  link to_port = int
  link protocol = tcp | udp | icmp | all
  link cidr_blocks = [ reference ]
  link source = type:aws_security_group
}

type aws_security_group = {
  link vpc = type:aws_vpc
  link description = string
  link ingress = [ type:aws_sg_rule ]
  link egress = [ type:aws_sg_rule ]
  link tags = [ type:tag ]
}
```

### IAM

```ground
type aws_iam_role = {
  link name = reference
  link assume_role_policy = reference
  link tags = [ type:tag ]
}

type aws_iam_role_policy_attachment = {
  link role = type:aws_iam_role
  link policy_arn = reference
}
```

### ECS

```ground
type aws_ecs_cluster = {
  link name = reference
  link tags = [ type:tag ]
}

type aws_env_var = {
  link name = reference
  link value = reference
}

type aws_ecs_container = {
  link name = reference
  link image = reference
  link cpu = int
  link memory = int
  link port = int
  link environment = [ type:aws_env_var ]
}

type aws_ecs_task_definition = {
  link family = reference
  link cpu = int
  link memory = int
  link network_mode = awsvpc | bridge | host
  link requires_compatibilities = fargate | ec2
  link execution_role = type:aws_iam_role
  link container = type:aws_ecs_container
  link tags = [ type:tag ]
  network_mode: awsvpc
  requires_compatibilities: fargate
}

type aws_ecs_load_balancer = {
  link target_group = type:aws_lb_target_group
  link container_name = reference
  link container_port = int
}

type aws_ecs_service = {
  link cluster = type:aws_ecs_cluster
  link task_definition = type:aws_ecs_task_definition
  link desired_count = int
  link launch_type = fargate | ec2
  link subnets = [ type:aws_subnet ]
  link security_groups = [ type:aws_security_group ]
  link load_balancer = type:aws_ecs_load_balancer
  link assign_public_ip = true | false
  link tags = [ type:tag ]
  desired_count: 1
  launch_type: fargate
  assign_public_ip: false
}
```

### Load Balancer

```ground
type aws_lb = {
  link name = reference
  link internal = true | false
  link load_balancer_type = application | network
  link subnets = [ type:aws_subnet ]
  link security_groups = [ type:aws_security_group ]
  link tags = [ type:tag ]
  internal: false
  load_balancer_type: application
}

type aws_lb_target_group = {
  link name = reference
  link port = int
  link protocol = HTTP | HTTPS | TCP
  link target_type = ip | instance | lambda
  link vpc = type:aws_vpc
  link health_check_path = reference
  link tags = [ type:tag ]
  protocol: HTTP
  target_type: ip
  health_check_path: /health
}

type aws_lb_listener = {
  link load_balancer = type:aws_lb
  link port = int
  link protocol = HTTP | HTTPS | TCP
  link action = forward | redirect
  link target_group = type:aws_lb_target_group
  port: 80
  protocol: HTTP
  action: forward
}
```

### RDS

```ground
type aws_db_subnet_group = {
  link name = reference
  link subnets = [ type:aws_subnet ]
  link tags = [ type:tag ]
}

type aws_db_instance = {
  link identifier = reference
  link engine = postgres | mysql
  link engine_version = reference
  link instance_class = reference
  link allocated_storage = int
  link db_name = reference
  link subnet_group = type:aws_db_subnet_group
  link vpc_security_groups = [ type:aws_security_group ]
  link multi_az = true | false
  link skip_final_snapshot = true | false
  link tags = [ type:tag ]
  engine: postgres
  multi_az: false
  skip_final_snapshot: true
}
```

### Service Discovery

```ground
type aws_service_discovery_namespace = {
  link name = reference
  link vpc = type:aws_vpc
  link tags = [ type:tag ]
}

type aws_service_discovery_service = {
  link name = reference
  link namespace = type:aws_service_discovery_namespace
  link dns_ttl = int
  link tags = [ type:tag ]
  dns_ttl: 10
}
```

### Arch types

```ground
type port = http | grpc | https

type service = {
  link image = reference
  link port = type:port
  link cpu = 256
  link memory = 512
  link replicas = 1
  link access = [ type:service | type:database ]
}

type database = {
  link engine = postgres | mysql
  link engine_version = reference
  link instance_class = reference
  link storage = int
  engine: postgres
  instance_class: db.t3.micro
  storage: 20
}

type stack = {
  link = [ type:service | type:database ]
}
```

### aws type — shared infrastructure

When a stack is chained as `aws`, these links are added to the instance. Subnet CIDR blocks,
AZ assignments and security group rules are opinionated defaults that cover the common case.

```ground
type aws = {
  link vpc = type:aws_vpc {
    cidr_block: 10.0.0.0/16
    tags: [ tag(ground-managed, true) tag(name, {this.name}) ]
  }
  link igw = type:aws_internet_gateway {
    vpc: {this.vpc}
    tags: [ tag(ground-managed, true) ]
  }
  link public-a = type:aws_subnet {
    vpc: {this.vpc}
    cidr_block: 10.0.1.0/24
    availability_zone: us-east-1a
    map_public_ip_on_launch: true
    tags: [ tag(ground-managed, true) tag(tier, public) ]
  }
  link public-b = type:aws_subnet {
    vpc: {this.vpc}
    cidr_block: 10.0.2.0/24
    availability_zone: us-east-1b
    map_public_ip_on_launch: true
    tags: [ tag(ground-managed, true) tag(tier, public) ]
  }
  link private-a = type:aws_subnet {
    vpc: {this.vpc}
    cidr_block: 10.0.3.0/24
    availability_zone: us-east-1a
    tags: [ tag(ground-managed, true) tag(tier, private) ]
  }
  link private-b = type:aws_subnet {
    vpc: {this.vpc}
    cidr_block: 10.0.4.0/24
    availability_zone: us-east-1b
    tags: [ tag(ground-managed, true) tag(tier, private) ]
  }
  link isolated-a = type:aws_subnet {
    vpc: {this.vpc}
    cidr_block: 10.0.5.0/24
    availability_zone: us-east-1a
    tags: [ tag(ground-managed, true) tag(tier, isolated) ]
  }
  link isolated-b = type:aws_subnet {
    vpc: {this.vpc}
    cidr_block: 10.0.6.0/24
    availability_zone: us-east-1b
    tags: [ tag(ground-managed, true) tag(tier, isolated) ]
  }
  link alb-sg = type:aws_security_group {
    vpc: {this.vpc}
    description: ALB public ingress
    ingress: [
      aws_sg_rule { from_port: 80  to_port: 80  protocol: tcp cidr_blocks: [ 0.0.0.0/0 ] }
      aws_sg_rule { from_port: 443 to_port: 443 protocol: tcp cidr_blocks: [ 0.0.0.0/0 ] }
    ]
    egress: [
      aws_sg_rule { from_port: 0 to_port: 0 protocol: all cidr_blocks: [ 0.0.0.0/0 ] }
    ]
    tags: [ tag(ground-managed, true) ]
  }
  link ecs-sg = type:aws_security_group {
    vpc: {this.vpc}
    description: ECS tasks
    ingress: [
      aws_sg_rule { from_port: 0 to_port: 65535 protocol: tcp source: {this.alb-sg} }
      aws_sg_rule { from_port: 0 to_port: 65535 protocol: tcp source: {this.ecs-sg} }
    ]
    egress: [
      aws_sg_rule { from_port: 0 to_port: 0 protocol: all cidr_blocks: [ 0.0.0.0/0 ] }
    ]
    tags: [ tag(ground-managed, true) ]
  }
  link rds-sg = type:aws_security_group {
    vpc: {this.vpc}
    description: RDS isolated
    ingress: [
      aws_sg_rule { from_port: 5432 to_port: 5432 protocol: tcp source: {this.ecs-sg} }
    ]
    egress: [
      aws_sg_rule { from_port: 0 to_port: 0 protocol: all cidr_blocks: [ 0.0.0.0/0 ] }
    ]
    tags: [ tag(ground-managed, true) ]
  }
  link execution-role = type:aws_iam_role {
    name: {this.name}-ecs-execution
    assume_role_policy: ecs-tasks.amazonaws.com
    tags: [ tag(ground-managed, true) ]
  }
  link execution-policy = type:aws_iam_role_policy_attachment {
    role: {this.execution-role}
    policy_arn: arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy
  }
  link cluster = type:aws_ecs_cluster {
    name: {this.name}
    tags: [ tag(ground-managed, true) ]
  }
  link alb = type:aws_lb {
    name: {this.name}
    subnets: [ {this.public-a} {this.public-b} ]
    security_groups: [ {this.alb-sg} ]
    tags: [ tag(ground-managed, true) ]
  }
  link discovery-namespace = type:aws_service_discovery_namespace {
    name: {this.name}.local
    vpc: {this.vpc}
    tags: [ tag(ground-managed, true) ]
  }
}
```

### ecs type — per-element resources via edge handlers (option C)

`type ecs` itself is empty. All resources are produced by the anonymous link functions below,
which fire for each `(ecs owner, element)` edge in the stack's anonymous list.

```ground
type ecs = {}
```

Service edge — fires for every `service` element in the stack:

```ground
link (owner: type:ecs, s: type:service) = {
  link task-definition = type:aws_ecs_task_definition {
    family: {s.name}
    cpu: {s.cpu}
    memory: {s.memory}
    execution_role: {owner.execution-role}
    container: aws_ecs_container {
      name: {s.name}
      image: {s.image}
      cpu: {s.cpu}
      memory: {s.memory}
      port: 80
    }
    tags: [ tag(ground-managed, true) ]
  }
  link ecs-service = type:aws_ecs_service {
    cluster: {owner.cluster}
    task_definition: {this.task-definition}
    desired_count: {s.replicas}
    subnets: [ {owner.private-a} {owner.private-b} ]
    security_groups: [ {owner.ecs-sg} ]
    tags: [ tag(ground-managed, true) ]
  }
  link discovery = type:aws_service_discovery_service {
    name: {s.name}
    namespace: {owner.discovery-namespace}
    tags: [ tag(ground-managed, true) ]
  }
}
```

HTTP service edge — fires for `service` elements with `port: http`, adds ALB wiring:

```ground
link (owner: type:ecs, s: type:service) where s.port = http = {
  link target-group = type:aws_lb_target_group {
    name: {s.name}
    port: 80
    vpc: {owner.vpc}
    tags: [ tag(ground-managed, true) ]
  }
  link listener = type:aws_lb_listener {
    load_balancer: {owner.alb}
    target_group: {this.target-group}
    tags: [ tag(ground-managed, true) ]
  }
  ecs-service:load_balancer: aws_ecs_load_balancer {
    target_group: {this.target-group}
    container_name: {s.name}
    container_port: 80
  }
}
```

Database edge — fires for every `database` element in the stack:

```ground
link (owner: type:ecs, d: type:database) = {
  link subnet-group = type:aws_db_subnet_group {
    name: {d.name}
    subnets: [ {owner.isolated-a} {owner.isolated-b} ]
    tags: [ tag(ground-managed, true) ]
  }
  link db-instance = type:aws_db_instance {
    identifier: {d.name}
    engine: {d.engine}
    engine_version: {d.engine_version}
    instance_class: {d.instance_class}
    allocated_storage: {d.storage}
    db_name: {d.name}
    subnet_group: {this.subnet-group}
    vpc_security_groups: [ {owner.rds-sg} ]
    tags: [ tag(ground-managed, true) ]
  }
}
```

---

## Example

Everything the user writes. All complexity is in the std above.

```ground
database main {}

service payments {
  image: local.dev/payments:latest
}

service api {
  image: local.dev/api:latest
  port: http
  replicas: 2
  access: [ payments main ]
}

stack app {
  api
  payments
  main
}

app aws {}
aws ecs {}
```
