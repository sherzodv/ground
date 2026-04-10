# marstech.tf — derived from marstech.grd
# All resources tagged: "ground-managed" = "true"

terraform {
  required_providers {
    aws    = { source = "hashicorp/aws", version = "~> 5.0" }
    random = { source = "hashicorp/random", version = "~> 3.0" }
  }
}

provider "aws" { alias = "eu_central_1"; region = "eu-central-1" }
provider "aws" { alias = "me_central_1"; region = "me-central-1" }

variable "ecr_registry" {
  description = "ECR registry base URL (images are not defined in marstech.grd)"
  type        = string
}

# ==============================================================================
# LOCALS — config encoded from marstech.grd
# ==============================================================================

locals {
  # marstech:services — access and observe per service
  services = {
    "api-gen" = {
      access_db       = ["main"]
      access_services = [{ name = "media" }, { name = "pay" }, { name = "core" }]
      access_secrets  = []
      access_bucket   = null
      observe_tracing = false
      observe_datadog = false
    }
    "hub" = {
      access_db = []; access_services = []; access_secrets = []
      access_bucket = null; observe_tracing = false; observe_datadog = false
    }
    "product-wf" = {
      access_db = []; access_services = []; access_secrets = []
      access_bucket = null; observe_tracing = false; observe_datadog = false
    }
    "metrics" = {
      access_db = []; access_services = []; access_secrets = []
      access_bucket = null; observe_tracing = false; observe_datadog = false
    }
    "media" = {
      access_db       = ["main"]
      access_services = []
      access_secrets  = ["media"]
      access_bucket   = "media-content"
      observe_tracing = false; observe_datadog = false
    }
    "pay" = {
      access_db       = ["main"]
      access_services = []
      access_secrets  = ["pay"]
      access_bucket   = null; observe_tracing = false; observe_datadog = false
    }
    "core" = {
      access_db = []; access_services = []; access_secrets = ["core"]
      access_bucket = null; observe_tracing = true; observe_datadog = false
    }
    "hubspot" = {
      access_db = []; access_services = []; access_secrets = ["hubspot", "datadog"]
      access_bucket = null; observe_tracing = true; observe_datadog = true
    }
    "notify" = {
      access_db = []; access_services = []; access_secrets = ["notify", "datadog"]
      access_bucket = null; observe_tracing = true; observe_datadog = true
    }
  }

  # marstech:routing — domains and edges
  domains = {
    "marstech-co"   = { host = "marstech.co" }
    "influencer-ae" = { host = "moonadmin.influencer.ae" }
  }
  edges = {
    "edge-api"       = { domain = "influencer-ae", sub = "api",       backend = "api-gen" }
    "edge-hub"       = { domain = "marstech-co",   sub = "hub",       backend = "hub" }
    "edge-workflows" = { domain = "marstech-co",   sub = "workflows", backend = "product-wf" }
    "edge-metrics"   = { domain = "marstech-co",   sub = "metrics",   backend = "metrics" }
  }

  # size → ECS cpu/memory and db instance class
  size_cpu    = { small = 256,         medium = 512,          large = 1024 }
  size_memory = { small = 512,         medium = 1024,         large = 2048 }
  size_db     = { small = "db.t3.micro", medium = "db.t3.medium", large = "db.r6g.large" }

  # deploy configs — from deploy blocks in marstech.grd
  prd_eu = {
    alias  = "prd-eu"
    region = "eu-central-1"
    azs    = ["eu-central-1a", "eu-central-1b", "eu-central-1c"]
    db     = { size = "medium", storage = 20 }
    spaces = {
      "app" = { host = "app.prd.internal", services = ["media", "pay", "core", "hubspot", "notify"] }
      "poc" = { host = "poc.prd.internal", services = ["product-wf"] }
    }
    stack_services = ["api-gen", "hub", "product-wf", "metrics", "media", "pay", "core", "hubspot", "notify"]
    stack_secrets  = ["datadog", "media", "pay", "core", "hubspot", "notify"]
    stack_bucket   = true
    stack_domains  = ["marstech-co", "influencer-ae"]
    stack_edges    = ["edge-api", "edge-hub", "edge-workflows", "edge-metrics"]
    sizing = {
      "api-gen"    = { size = "large",  min = 1, max = 4 }
      "hub"        = { size = "medium", min = 1, max = 2 }
      "product-wf" = { size = "medium", min = 1, max = 2 }
      "metrics"    = { size = "small",  min = 1, max = 1 }
      "media"      = { size = "medium", min = 1, max = 2 }
      "pay"        = { size = "small",  min = 1, max = 2 }
      "core"       = { size = "medium", min = 1, max = 2 }
      "hubspot"    = { size = "medium", min = 1, max = 2 }
      "notify"     = { size = "medium", min = 1, max = 2 }
    }
  }

  prd_me = {
    alias  = "prd-me"
    region = "me-central-1"
    azs    = ["me-central-1a"]
    db     = { size = "small", storage = 20 }
    spaces = {
      "app" = { host = "app.prd.internal", services = ["media", "pay", "core", "hubspot", "notify"] }
      "poc" = { host = "poc.prd.internal", services = ["product-wf"] }
    }
    stack_services = ["api-gen", "hub", "product-wf", "metrics", "media", "pay", "core", "hubspot", "notify"]
    stack_secrets  = ["datadog", "media", "pay", "core", "hubspot", "notify"]
    stack_bucket   = true
    stack_domains  = ["marstech-co", "influencer-ae"]
    stack_edges    = ["edge-api", "edge-hub", "edge-workflows", "edge-metrics"]
    sizing = {
      "api-gen"    = { size = "medium", min = 1, max = 2 }
      "hub"        = { size = "small",  min = 1, max = 1 }
      "product-wf" = { size = "small",  min = 1, max = 1 }
      "metrics"    = { size = "small",  min = 1, max = 1 }
      "media"      = { size = "small",  min = 1, max = 1 }
      "pay"        = { size = "small",  min = 1, max = 1 }
      "core"       = { size = "small",  min = 1, max = 1 }
      "hubspot"    = { size = "small",  min = 1, max = 1 }
      "notify"     = { size = "small",  min = 1, max = 1 }
    }
  }

  stg_eu = {
    alias  = "stg-eu"
    region = "eu-central-1"
    azs    = ["eu-central-1a"]
    db     = { size = "small", storage = 20 }
    spaces = {
      "app" = { host = "app.stg.internal", services = ["api-gen"] }
    }
    stack_services = ["api-gen"]
    stack_secrets  = ["core", "pay", "media"]
    stack_bucket   = false
    stack_domains  = ["influencer-ae"]
    stack_edges    = ["edge-api"]
    sizing = {
      "api-gen" = { size = "small", min = 1, max = 1 }
    }
  }
}

# ==============================================================================
# prd-eu — deploy prd:marstech to aws as prd-eu
# ==============================================================================

# --- networking ---

resource "aws_vpc" "ground_prd_eu" {
  provider             = aws.eu_central_1
  cidr_block           = "10.0.0.0/16"
  enable_dns_support   = true
  enable_dns_hostnames = true
  tags = { "ground-managed" = "true", Name = "ground-prd-eu-vpc" }
}

resource "aws_internet_gateway" "ground_prd_eu" {
  provider = aws.eu_central_1
  vpc_id   = aws_vpc.ground_prd_eu.id
  tags     = { "ground-managed" = "true", Name = "ground-prd-eu-igw" }
}

resource "aws_subnet" "ground_prd_eu_pub" {
  provider                = aws.eu_central_1
  for_each                = { for i, az in local.prd_eu.azs : az => i }
  vpc_id                  = aws_vpc.ground_prd_eu.id
  availability_zone       = each.key
  cidr_block              = cidrsubnet("10.0.0.0/16", 8, each.value)
  map_public_ip_on_launch = true
  tags = { "ground-managed" = "true", Name = "ground-prd-eu-pub-${each.key}" }
}

resource "aws_subnet" "ground_prd_eu_prv" {
  provider          = aws.eu_central_1
  for_each          = { for i, az in local.prd_eu.azs : az => i }
  vpc_id            = aws_vpc.ground_prd_eu.id
  availability_zone = each.key
  cidr_block        = cidrsubnet("10.0.0.0/16", 8, each.value + 10)
  tags = { "ground-managed" = "true", Name = "ground-prd-eu-prv-${each.key}" }
}

resource "aws_eip" "ground_prd_eu_nat" {
  provider = aws.eu_central_1
  domain   = "vpc"
  tags     = { "ground-managed" = "true", Name = "ground-prd-eu-nat-eip" }
}

resource "aws_nat_gateway" "ground_prd_eu" {
  provider      = aws.eu_central_1
  allocation_id = aws_eip.ground_prd_eu_nat.id
  subnet_id     = aws_subnet.ground_prd_eu_pub["eu-central-1a"].id
  tags          = { "ground-managed" = "true", Name = "ground-prd-eu-nat" }
  depends_on    = [aws_internet_gateway.ground_prd_eu]
}

resource "aws_route_table" "ground_prd_eu_pub" {
  provider = aws.eu_central_1
  vpc_id   = aws_vpc.ground_prd_eu.id
  route { cidr_block = "0.0.0.0/0"; gateway_id = aws_internet_gateway.ground_prd_eu.id }
  tags = { "ground-managed" = "true", Name = "ground-prd-eu-rpub" }
}

resource "aws_route_table_association" "ground_prd_eu_pub" {
  provider       = aws.eu_central_1
  for_each       = aws_subnet.ground_prd_eu_pub
  subnet_id      = each.value.id
  route_table_id = aws_route_table.ground_prd_eu_pub.id
}

resource "aws_route_table" "ground_prd_eu_prv" {
  provider = aws.eu_central_1
  vpc_id   = aws_vpc.ground_prd_eu.id
  route { cidr_block = "0.0.0.0/0"; nat_gateway_id = aws_nat_gateway.ground_prd_eu.id }
  tags = { "ground-managed" = "true", Name = "ground-prd-eu-rprv" }
}

resource "aws_route_table_association" "ground_prd_eu_prv" {
  provider       = aws.eu_central_1
  for_each       = aws_subnet.ground_prd_eu_prv
  subnet_id      = each.value.id
  route_table_id = aws_route_table.ground_prd_eu_prv.id
}

# --- clusters (space app, space poc) ---

resource "aws_ecs_cluster" "ground_prd_eu_app" {
  provider = aws.eu_central_1
  name     = "ground-prd-eu-app-cluster"
  tags     = { "ground-managed" = "true" }
}

resource "aws_service_discovery_private_dns_namespace" "ground_prd_eu_app" {
  provider = aws.eu_central_1
  name     = "app.prd.internal"
  vpc      = aws_vpc.ground_prd_eu.id
  tags     = { "ground-managed" = "true" }
}

resource "aws_ecs_cluster" "ground_prd_eu_poc" {
  provider = aws.eu_central_1
  name     = "ground-prd-eu-poc-cluster"
  tags     = { "ground-managed" = "true" }
}

resource "aws_service_discovery_private_dns_namespace" "ground_prd_eu_poc" {
  provider = aws.eu_central_1
  name     = "poc.prd.internal"
  vpc      = aws_vpc.ground_prd_eu.id
  tags     = { "ground-managed" = "true" }
}

# --- secrets ---

resource "aws_secretsmanager_secret" "ground_prd_eu" {
  provider                = aws.eu_central_1
  for_each                = toset(local.prd_eu.stack_secrets)
  name                    = "ground-prd-eu/${each.key}"
  recovery_window_in_days = 7
  tags                    = { "ground-managed" = "true" }
}

# --- bucket (bucket media, name: media-content) ---

resource "aws_s3_bucket" "ground_prd_eu_media" {
  provider = aws.eu_central_1
  bucket   = "ground-prd-eu-media-content"
  tags     = { "ground-managed" = "true" }
}

# --- database (database main, postgresql) ---

resource "random_password" "ground_prd_eu_main" {
  length  = 32
  special = false
}

resource "aws_db_subnet_group" "ground_prd_eu_main" {
  provider   = aws.eu_central_1
  name       = "ground-prd-eu-main-db-sng"
  subnet_ids = [for s in aws_subnet.ground_prd_eu_prv : s.id]
  tags       = { "ground-managed" = "true" }
}

resource "aws_security_group" "ground_prd_eu_main_db" {
  provider    = aws.eu_central_1
  name        = "ground-prd-eu-main-db-sg"
  description = "ground-prd-eu main db"
  vpc_id      = aws_vpc.ground_prd_eu.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_prd_eu_main_db_all" {
  provider          = aws.eu_central_1
  security_group_id = aws_security_group.ground_prd_eu_main_db.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_db_instance" "ground_prd_eu_main" {
  provider               = aws.eu_central_1
  identifier             = "ground-prd-eu-main-db"
  engine                 = "postgres"
  engine_version         = "15"
  instance_class         = local.size_db[local.prd_eu.db.size]
  allocated_storage      = local.prd_eu.db.storage
  db_name                = "main"
  username               = "ground"
  password               = random_password.ground_prd_eu_main.result
  db_subnet_group_name   = aws_db_subnet_group.ground_prd_eu_main.name
  vpc_security_group_ids = [aws_security_group.ground_prd_eu_main_db.id]
  skip_final_snapshot    = true
  tags                   = { "ground-managed" = "true" }
}

# --- services ---

resource "aws_security_group" "ground_prd_eu_svc" {
  provider    = aws.eu_central_1
  for_each    = toset(local.prd_eu.stack_services)
  name        = "ground-prd-eu-${each.key}-sg"
  description = "ground-prd-eu ${each.key}"
  vpc_id      = aws_vpc.ground_prd_eu.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_prd_eu_svc_all" {
  provider          = aws.eu_central_1
  for_each          = toset(local.prd_eu.stack_services)
  security_group_id = aws_security_group.ground_prd_eu_svc[each.key].id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_iam_role" "ground_prd_eu_exec" {
  for_each = toset(local.prd_eu.stack_services)
  name     = "ground-prd-eu-${each.key}-exec-role"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = { Service = "ecs-tasks.amazonaws.com" }, Action = "sts:AssumeRole" }]
  })
  tags = { "ground-managed" = "true" }
}

resource "aws_iam_role_policy_attachment" "ground_prd_eu_exec" {
  for_each   = toset(local.prd_eu.stack_services)
  role       = aws_iam_role.ground_prd_eu_exec[each.key].name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role" "ground_prd_eu_task" {
  for_each = toset(local.prd_eu.stack_services)
  name     = "ground-prd-eu-${each.key}-task-role"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = { Service = "ecs-tasks.amazonaws.com" }, Action = "sts:AssumeRole" }]
  })
  tags = { "ground-managed" = "true" }
}

# task role: secret access (service → secret links)
resource "aws_iam_role_policy" "ground_prd_eu_task_secrets" {
  for_each = { for svc in local.prd_eu.stack_services : svc => svc if length(local.services[svc].access_secrets) > 0 }
  name     = "ground-prd-eu-${each.key}-secrets"
  role     = aws_iam_role.ground_prd_eu_task[each.key].id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["secretsmanager:GetSecretValue"]
      Resource = [for s in local.services[each.key].access_secrets : aws_secretsmanager_secret.ground_prd_eu[s].arn]
    }]
  })
}

# task role: bucket write access (service → bucket:write link)
resource "aws_iam_role_policy" "ground_prd_eu_task_bucket" {
  for_each = { for svc in local.prd_eu.stack_services : svc => svc if local.services[svc].access_bucket != null }
  name     = "ground-prd-eu-${each.key}-bucket"
  role     = aws_iam_role.ground_prd_eu_task[each.key].id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = ["s3:PutObject"], Resource = "${aws_s3_bucket.ground_prd_eu_media.arn}/*" }]
  })
}

resource "aws_cloudwatch_log_group" "ground_prd_eu_svc" {
  provider          = aws.eu_central_1
  for_each          = toset(local.prd_eu.stack_services)
  name              = "/ground/prd-eu/${each.key}"
  retention_in_days = 30
  tags              = { "ground-managed" = "true" }
}

resource "aws_ecs_task_definition" "ground_prd_eu_svc" {
  provider                 = aws.eu_central_1
  for_each                 = toset(local.prd_eu.stack_services)
  family                   = "ground-prd-eu-${each.key}-task"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = local.size_cpu[local.prd_eu.sizing[each.key].size]
  memory                   = local.size_memory[local.prd_eu.sizing[each.key].size]
  execution_role_arn       = aws_iam_role.ground_prd_eu_exec[each.key].arn
  task_role_arn            = aws_iam_role.ground_prd_eu_task[each.key].arn
  container_definitions = jsonencode([{
    name      = each.key
    image     = "${var.ecr_registry}/${each.key}:latest"
    essential = true
    portMappings = [{ containerPort = 8080, protocol = "tcp" }]
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        "awslogs-group"         = aws_cloudwatch_log_group.ground_prd_eu_svc[each.key].name
        "awslogs-region"        = local.prd_eu.region
        "awslogs-stream-prefix" = each.key
      }
    }
  }])
  tags = { "ground-managed" = "true" }
}

resource "aws_ecs_service" "ground_prd_eu_svc" {
  provider        = aws.eu_central_1
  for_each        = toset(local.prd_eu.stack_services)
  name            = "ground-prd-eu-${each.key}-svc"
  cluster         = each.key == "product-wf" ? aws_ecs_cluster.ground_prd_eu_poc.id : aws_ecs_cluster.ground_prd_eu_app.id
  task_definition = aws_ecs_task_definition.ground_prd_eu_svc[each.key].arn
  desired_count   = local.prd_eu.sizing[each.key].min
  launch_type     = "FARGATE"
  network_configuration {
    subnets         = [for s in aws_subnet.ground_prd_eu_prv : s.id]
    security_groups = [aws_security_group.ground_prd_eu_svc[each.key].id]
  }
  tags = { "ground-managed" = "true" }
}

# --- access links: service → database (ingress on db sg) ---

locals {
  prd_eu_svc_to_db = {
    for pair in flatten([
      for svc in local.prd_eu.stack_services : [
        for db in local.services[svc].access_db : { key = "${svc}__${db}", svc = svc, db = db }
      ]
    ]) : pair.key => pair
  }
  prd_eu_svc_to_svc = {
    for pair in flatten([
      for svc in local.prd_eu.stack_services : [
        for acc in local.services[svc].access_services : { key = "${svc}__${acc.name}", caller = svc, target = acc.name }
      ]
    ]) : pair.key => pair
  }
}

resource "aws_vpc_security_group_ingress_rule" "ground_prd_eu_svc_to_db" {
  provider                     = aws.eu_central_1
  for_each                     = local.prd_eu_svc_to_db
  security_group_id            = aws_security_group.ground_prd_eu_main_db.id
  referenced_security_group_id = aws_security_group.ground_prd_eu_svc[each.value.svc].id
  from_port                    = 5432
  to_port                      = 5432
  ip_protocol                  = "tcp"
  tags                         = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_ingress_rule" "ground_prd_eu_svc_to_svc" {
  provider                     = aws.eu_central_1
  for_each                     = local.prd_eu_svc_to_svc
  security_group_id            = aws_security_group.ground_prd_eu_svc[each.value.target].id
  referenced_security_group_id = aws_security_group.ground_prd_eu_svc[each.value.caller].id
  from_port                    = 0
  to_port                      = 65535
  ip_protocol                  = "tcp"
  tags                         = { "ground-managed" = "true" }
}

# --- ALB + routing (edges) ---

resource "aws_security_group" "ground_prd_eu_alb" {
  provider    = aws.eu_central_1
  name        = "ground-prd-eu-alb-sg"
  description = "ground-prd-eu ALB"
  vpc_id      = aws_vpc.ground_prd_eu.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_ingress_rule" "ground_prd_eu_alb_http" {
  provider          = aws.eu_central_1
  security_group_id = aws_security_group.ground_prd_eu_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 80; to_port = 80; ip_protocol = "tcp"
  tags              = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_ingress_rule" "ground_prd_eu_alb_https" {
  provider          = aws.eu_central_1
  security_group_id = aws_security_group.ground_prd_eu_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 443; to_port = 443; ip_protocol = "tcp"
  tags              = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_prd_eu_alb_all" {
  provider          = aws.eu_central_1
  security_group_id = aws_security_group.ground_prd_eu_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_lb" "ground_prd_eu" {
  provider           = aws.eu_central_1
  name               = "ground-prd-eu-alb"
  load_balancer_type = "application"
  internal           = false
  subnets            = [for s in aws_subnet.ground_prd_eu_pub : s.id]
  security_groups    = [aws_security_group.ground_prd_eu_alb.id]
  tags               = { "ground-managed" = "true" }
}

# domains: ACM + Route53
data "aws_route53_zone" "ground_prd_eu_marstech_co" {
  provider = aws.eu_central_1
  name     = "marstech.co"
}

data "aws_route53_zone" "ground_prd_eu_influencer_ae" {
  provider = aws.eu_central_1
  name     = "moonadmin.influencer.ae"
}

resource "aws_acm_certificate" "ground_prd_eu_marstech_co" {
  provider                  = aws.eu_central_1
  domain_name               = "marstech.co"
  subject_alternative_names = ["*.marstech.co"]
  validation_method         = "DNS"
  tags                      = { "ground-managed" = "true" }
  lifecycle { create_before_destroy = true }
}

resource "aws_route53_record" "ground_prd_eu_marstech_co_cert" {
  provider = aws.eu_central_1
  for_each = {
    for dvo in aws_acm_certificate.ground_prd_eu_marstech_co.domain_validation_options : dvo.domain_name => dvo
  }
  zone_id = data.aws_route53_zone.ground_prd_eu_marstech_co.zone_id
  name    = each.value.resource_record_name
  type    = each.value.resource_record_type
  records = [each.value.resource_record_value]
  ttl     = 60
}

resource "aws_acm_certificate_validation" "ground_prd_eu_marstech_co" {
  provider                = aws.eu_central_1
  certificate_arn         = aws_acm_certificate.ground_prd_eu_marstech_co.arn
  validation_record_fqdns = [for r in aws_route53_record.ground_prd_eu_marstech_co_cert : r.fqdn]
}

resource "aws_acm_certificate" "ground_prd_eu_influencer_ae" {
  provider                  = aws.eu_central_1
  domain_name               = "moonadmin.influencer.ae"
  subject_alternative_names = ["*.moonadmin.influencer.ae"]
  validation_method         = "DNS"
  tags                      = { "ground-managed" = "true" }
  lifecycle { create_before_destroy = true }
}

resource "aws_route53_record" "ground_prd_eu_influencer_ae_cert" {
  provider = aws.eu_central_1
  for_each = {
    for dvo in aws_acm_certificate.ground_prd_eu_influencer_ae.domain_validation_options : dvo.domain_name => dvo
  }
  zone_id = data.aws_route53_zone.ground_prd_eu_influencer_ae.zone_id
  name    = each.value.resource_record_name
  type    = each.value.resource_record_type
  records = [each.value.resource_record_value]
  ttl     = 60
}

resource "aws_acm_certificate_validation" "ground_prd_eu_influencer_ae" {
  provider                = aws.eu_central_1
  certificate_arn         = aws_acm_certificate.ground_prd_eu_influencer_ae.arn
  validation_record_fqdns = [for r in aws_route53_record.ground_prd_eu_influencer_ae_cert : r.fqdn]
}

resource "aws_lb_listener" "ground_prd_eu_http" {
  provider          = aws.eu_central_1
  load_balancer_arn = aws_lb.ground_prd_eu.arn
  port              = 80
  protocol          = "HTTP"
  default_action {
    type = "redirect"
    redirect { port = "443"; protocol = "HTTPS"; status_code = "HTTP_301" }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_listener" "ground_prd_eu_https" {
  provider          = aws.eu_central_1
  load_balancer_arn = aws_lb.ground_prd_eu.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = aws_acm_certificate_validation.ground_prd_eu_marstech_co.certificate_arn
  default_action {
    type = "fixed-response"
    fixed_response { content_type = "text/plain"; message_body = "not found"; status_code = "404" }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_listener_certificate" "ground_prd_eu_influencer_ae" {
  provider        = aws.eu_central_1
  listener_arn    = aws_lb_listener.ground_prd_eu_https.arn
  certificate_arn = aws_acm_certificate_validation.ground_prd_eu_influencer_ae.certificate_arn
}

resource "aws_lb_target_group" "ground_prd_eu" {
  provider    = aws.eu_central_1
  for_each    = toset(local.prd_eu.stack_edges)
  name        = "ground-prd-eu-${each.key}-tg"
  port        = 8080
  protocol    = "HTTP"
  target_type = "ip"
  vpc_id      = aws_vpc.ground_prd_eu.id
  health_check { path = "/health"; matcher = "200" }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_listener_rule" "ground_prd_eu" {
  provider     = aws.eu_central_1
  for_each     = toset(local.prd_eu.stack_edges)
  listener_arn = aws_lb_listener.ground_prd_eu_https.arn
  action { type = "forward"; target_group_arn = aws_lb_target_group.ground_prd_eu[each.key].arn }
  condition {
    host_header { values = ["${local.edges[each.key].sub}.${local.domains[local.edges[each.key].domain].host}"] }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_route53_record" "ground_prd_eu_edge" {
  provider = aws.eu_central_1
  for_each = toset(local.prd_eu.stack_edges)
  zone_id = (local.edges[each.key].domain == "marstech-co"
    ? data.aws_route53_zone.ground_prd_eu_marstech_co.zone_id
    : data.aws_route53_zone.ground_prd_eu_influencer_ae.zone_id)
  name    = "${local.edges[each.key].sub}.${local.domains[local.edges[each.key].domain].host}"
  type    = "CNAME"
  ttl     = 300
  records = [aws_lb.ground_prd_eu.dns_name]
}

# ==============================================================================
# prd-me — deploy prd:marstech to aws as prd-me
# ==============================================================================

# --- networking ---

resource "aws_vpc" "ground_prd_me" {
  provider             = aws.me_central_1
  cidr_block           = "10.0.0.0/16"
  enable_dns_support   = true
  enable_dns_hostnames = true
  tags = { "ground-managed" = "true", Name = "ground-prd-me-vpc" }
}

resource "aws_internet_gateway" "ground_prd_me" {
  provider = aws.me_central_1
  vpc_id   = aws_vpc.ground_prd_me.id
  tags     = { "ground-managed" = "true", Name = "ground-prd-me-igw" }
}

resource "aws_subnet" "ground_prd_me_pub" {
  provider                = aws.me_central_1
  for_each                = { for i, az in local.prd_me.azs : az => i }
  vpc_id                  = aws_vpc.ground_prd_me.id
  availability_zone       = each.key
  cidr_block              = cidrsubnet("10.0.0.0/16", 8, each.value)
  map_public_ip_on_launch = true
  tags = { "ground-managed" = "true", Name = "ground-prd-me-pub-${each.key}" }
}

resource "aws_subnet" "ground_prd_me_prv" {
  provider          = aws.me_central_1
  for_each          = { for i, az in local.prd_me.azs : az => i }
  vpc_id            = aws_vpc.ground_prd_me.id
  availability_zone = each.key
  cidr_block        = cidrsubnet("10.0.0.0/16", 8, each.value + 10)
  tags = { "ground-managed" = "true", Name = "ground-prd-me-prv-${each.key}" }
}

resource "aws_eip" "ground_prd_me_nat" {
  provider = aws.me_central_1
  domain   = "vpc"
  tags     = { "ground-managed" = "true", Name = "ground-prd-me-nat-eip" }
}

resource "aws_nat_gateway" "ground_prd_me" {
  provider      = aws.me_central_1
  allocation_id = aws_eip.ground_prd_me_nat.id
  subnet_id     = aws_subnet.ground_prd_me_pub["me-central-1a"].id
  tags          = { "ground-managed" = "true", Name = "ground-prd-me-nat" }
  depends_on    = [aws_internet_gateway.ground_prd_me]
}

resource "aws_route_table" "ground_prd_me_pub" {
  provider = aws.me_central_1
  vpc_id   = aws_vpc.ground_prd_me.id
  route { cidr_block = "0.0.0.0/0"; gateway_id = aws_internet_gateway.ground_prd_me.id }
  tags = { "ground-managed" = "true", Name = "ground-prd-me-rpub" }
}

resource "aws_route_table_association" "ground_prd_me_pub" {
  provider       = aws.me_central_1
  for_each       = aws_subnet.ground_prd_me_pub
  subnet_id      = each.value.id
  route_table_id = aws_route_table.ground_prd_me_pub.id
}

resource "aws_route_table" "ground_prd_me_prv" {
  provider = aws.me_central_1
  vpc_id   = aws_vpc.ground_prd_me.id
  route { cidr_block = "0.0.0.0/0"; nat_gateway_id = aws_nat_gateway.ground_prd_me.id }
  tags = { "ground-managed" = "true", Name = "ground-prd-me-rprv" }
}

resource "aws_route_table_association" "ground_prd_me_prv" {
  provider       = aws.me_central_1
  for_each       = aws_subnet.ground_prd_me_prv
  subnet_id      = each.value.id
  route_table_id = aws_route_table.ground_prd_me_prv.id
}

# --- clusters ---

resource "aws_ecs_cluster" "ground_prd_me_app" {
  provider = aws.me_central_1
  name     = "ground-prd-me-app-cluster"
  tags     = { "ground-managed" = "true" }
}

resource "aws_service_discovery_private_dns_namespace" "ground_prd_me_app" {
  provider = aws.me_central_1
  name     = "app.prd.internal"
  vpc      = aws_vpc.ground_prd_me.id
  tags     = { "ground-managed" = "true" }
}

resource "aws_ecs_cluster" "ground_prd_me_poc" {
  provider = aws.me_central_1
  name     = "ground-prd-me-poc-cluster"
  tags     = { "ground-managed" = "true" }
}

resource "aws_service_discovery_private_dns_namespace" "ground_prd_me_poc" {
  provider = aws.me_central_1
  name     = "poc.prd.internal"
  vpc      = aws_vpc.ground_prd_me.id
  tags     = { "ground-managed" = "true" }
}

# --- secrets ---

resource "aws_secretsmanager_secret" "ground_prd_me" {
  provider                = aws.me_central_1
  for_each                = toset(local.prd_me.stack_secrets)
  name                    = "ground-prd-me/${each.key}"
  recovery_window_in_days = 7
  tags                    = { "ground-managed" = "true" }
}

# --- bucket ---

resource "aws_s3_bucket" "ground_prd_me_media" {
  provider = aws.me_central_1
  bucket   = "ground-prd-me-media-content"
  tags     = { "ground-managed" = "true" }
}

# --- database ---

resource "random_password" "ground_prd_me_main" {
  length  = 32
  special = false
}

resource "aws_db_subnet_group" "ground_prd_me_main" {
  provider   = aws.me_central_1
  name       = "ground-prd-me-main-db-sng"
  subnet_ids = [for s in aws_subnet.ground_prd_me_prv : s.id]
  tags       = { "ground-managed" = "true" }
}

resource "aws_security_group" "ground_prd_me_main_db" {
  provider    = aws.me_central_1
  name        = "ground-prd-me-main-db-sg"
  description = "ground-prd-me main db"
  vpc_id      = aws_vpc.ground_prd_me.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_prd_me_main_db_all" {
  provider          = aws.me_central_1
  security_group_id = aws_security_group.ground_prd_me_main_db.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_db_instance" "ground_prd_me_main" {
  provider               = aws.me_central_1
  identifier             = "ground-prd-me-main-db"
  engine                 = "postgres"
  engine_version         = "15"
  instance_class         = local.size_db[local.prd_me.db.size]
  allocated_storage      = local.prd_me.db.storage
  db_name                = "main"
  username               = "ground"
  password               = random_password.ground_prd_me_main.result
  db_subnet_group_name   = aws_db_subnet_group.ground_prd_me_main.name
  vpc_security_group_ids = [aws_security_group.ground_prd_me_main_db.id]
  skip_final_snapshot    = true
  tags                   = { "ground-managed" = "true" }
}

# --- services ---

resource "aws_security_group" "ground_prd_me_svc" {
  provider    = aws.me_central_1
  for_each    = toset(local.prd_me.stack_services)
  name        = "ground-prd-me-${each.key}-sg"
  description = "ground-prd-me ${each.key}"
  vpc_id      = aws_vpc.ground_prd_me.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_prd_me_svc_all" {
  provider          = aws.me_central_1
  for_each          = toset(local.prd_me.stack_services)
  security_group_id = aws_security_group.ground_prd_me_svc[each.key].id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_iam_role" "ground_prd_me_exec" {
  for_each = toset(local.prd_me.stack_services)
  name     = "ground-prd-me-${each.key}-exec-role"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = { Service = "ecs-tasks.amazonaws.com" }, Action = "sts:AssumeRole" }]
  })
  tags = { "ground-managed" = "true" }
}

resource "aws_iam_role_policy_attachment" "ground_prd_me_exec" {
  for_each   = toset(local.prd_me.stack_services)
  role       = aws_iam_role.ground_prd_me_exec[each.key].name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role" "ground_prd_me_task" {
  for_each = toset(local.prd_me.stack_services)
  name     = "ground-prd-me-${each.key}-task-role"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = { Service = "ecs-tasks.amazonaws.com" }, Action = "sts:AssumeRole" }]
  })
  tags = { "ground-managed" = "true" }
}

resource "aws_iam_role_policy" "ground_prd_me_task_secrets" {
  for_each = { for svc in local.prd_me.stack_services : svc => svc if length(local.services[svc].access_secrets) > 0 }
  name     = "ground-prd-me-${each.key}-secrets"
  role     = aws_iam_role.ground_prd_me_task[each.key].id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["secretsmanager:GetSecretValue"]
      Resource = [for s in local.services[each.key].access_secrets : aws_secretsmanager_secret.ground_prd_me[s].arn]
    }]
  })
}

resource "aws_iam_role_policy" "ground_prd_me_task_bucket" {
  for_each = { for svc in local.prd_me.stack_services : svc => svc if local.services[svc].access_bucket != null }
  name     = "ground-prd-me-${each.key}-bucket"
  role     = aws_iam_role.ground_prd_me_task[each.key].id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = ["s3:PutObject"], Resource = "${aws_s3_bucket.ground_prd_me_media.arn}/*" }]
  })
}

resource "aws_cloudwatch_log_group" "ground_prd_me_svc" {
  provider          = aws.me_central_1
  for_each          = toset(local.prd_me.stack_services)
  name              = "/ground/prd-me/${each.key}"
  retention_in_days = 30
  tags              = { "ground-managed" = "true" }
}

resource "aws_ecs_task_definition" "ground_prd_me_svc" {
  provider                 = aws.me_central_1
  for_each                 = toset(local.prd_me.stack_services)
  family                   = "ground-prd-me-${each.key}-task"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = local.size_cpu[local.prd_me.sizing[each.key].size]
  memory                   = local.size_memory[local.prd_me.sizing[each.key].size]
  execution_role_arn       = aws_iam_role.ground_prd_me_exec[each.key].arn
  task_role_arn            = aws_iam_role.ground_prd_me_task[each.key].arn
  container_definitions = jsonencode([{
    name      = each.key
    image     = "${var.ecr_registry}/${each.key}:latest"
    essential = true
    portMappings = [{ containerPort = 8080, protocol = "tcp" }]
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        "awslogs-group"         = aws_cloudwatch_log_group.ground_prd_me_svc[each.key].name
        "awslogs-region"        = local.prd_me.region
        "awslogs-stream-prefix" = each.key
      }
    }
  }])
  tags = { "ground-managed" = "true" }
}

resource "aws_ecs_service" "ground_prd_me_svc" {
  provider        = aws.me_central_1
  for_each        = toset(local.prd_me.stack_services)
  name            = "ground-prd-me-${each.key}-svc"
  cluster         = each.key == "product-wf" ? aws_ecs_cluster.ground_prd_me_poc.id : aws_ecs_cluster.ground_prd_me_app.id
  task_definition = aws_ecs_task_definition.ground_prd_me_svc[each.key].arn
  desired_count   = local.prd_me.sizing[each.key].min
  launch_type     = "FARGATE"
  network_configuration {
    subnets         = [for s in aws_subnet.ground_prd_me_prv : s.id]
    security_groups = [aws_security_group.ground_prd_me_svc[each.key].id]
  }
  tags = { "ground-managed" = "true" }
}

# --- access links ---

locals {
  prd_me_svc_to_db = {
    for pair in flatten([
      for svc in local.prd_me.stack_services : [
        for db in local.services[svc].access_db : { key = "${svc}__${db}", svc = svc, db = db }
      ]
    ]) : pair.key => pair
  }
  prd_me_svc_to_svc = {
    for pair in flatten([
      for svc in local.prd_me.stack_services : [
        for acc in local.services[svc].access_services : { key = "${svc}__${acc.name}", caller = svc, target = acc.name }
      ]
    ]) : pair.key => pair
  }
}

resource "aws_vpc_security_group_ingress_rule" "ground_prd_me_svc_to_db" {
  provider                     = aws.me_central_1
  for_each                     = local.prd_me_svc_to_db
  security_group_id            = aws_security_group.ground_prd_me_main_db.id
  referenced_security_group_id = aws_security_group.ground_prd_me_svc[each.value.svc].id
  from_port                    = 5432; to_port = 5432; ip_protocol = "tcp"
  tags                         = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_ingress_rule" "ground_prd_me_svc_to_svc" {
  provider                     = aws.me_central_1
  for_each                     = local.prd_me_svc_to_svc
  security_group_id            = aws_security_group.ground_prd_me_svc[each.value.target].id
  referenced_security_group_id = aws_security_group.ground_prd_me_svc[each.value.caller].id
  from_port                    = 0; to_port = 65535; ip_protocol = "tcp"
  tags                         = { "ground-managed" = "true" }
}

# --- ALB + routing ---

resource "aws_security_group" "ground_prd_me_alb" {
  provider    = aws.me_central_1
  name        = "ground-prd-me-alb-sg"
  description = "ground-prd-me ALB"
  vpc_id      = aws_vpc.ground_prd_me.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_ingress_rule" "ground_prd_me_alb_http" {
  provider          = aws.me_central_1
  security_group_id = aws_security_group.ground_prd_me_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 80; to_port = 80; ip_protocol = "tcp"
  tags              = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_ingress_rule" "ground_prd_me_alb_https" {
  provider          = aws.me_central_1
  security_group_id = aws_security_group.ground_prd_me_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 443; to_port = 443; ip_protocol = "tcp"
  tags              = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_prd_me_alb_all" {
  provider          = aws.me_central_1
  security_group_id = aws_security_group.ground_prd_me_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_lb" "ground_prd_me" {
  provider           = aws.me_central_1
  name               = "ground-prd-me-alb"
  load_balancer_type = "application"
  internal           = false
  subnets            = [for s in aws_subnet.ground_prd_me_pub : s.id]
  security_groups    = [aws_security_group.ground_prd_me_alb.id]
  tags               = { "ground-managed" = "true" }
}

data "aws_route53_zone" "ground_prd_me_marstech_co" {
  provider = aws.me_central_1
  name     = "marstech.co"
}

data "aws_route53_zone" "ground_prd_me_influencer_ae" {
  provider = aws.me_central_1
  name     = "moonadmin.influencer.ae"
}

resource "aws_acm_certificate" "ground_prd_me_marstech_co" {
  provider                  = aws.me_central_1
  domain_name               = "marstech.co"
  subject_alternative_names = ["*.marstech.co"]
  validation_method         = "DNS"
  tags                      = { "ground-managed" = "true" }
  lifecycle { create_before_destroy = true }
}

resource "aws_route53_record" "ground_prd_me_marstech_co_cert" {
  provider = aws.me_central_1
  for_each = {
    for dvo in aws_acm_certificate.ground_prd_me_marstech_co.domain_validation_options : dvo.domain_name => dvo
  }
  zone_id = data.aws_route53_zone.ground_prd_me_marstech_co.zone_id
  name    = each.value.resource_record_name
  type    = each.value.resource_record_type
  records = [each.value.resource_record_value]
  ttl     = 60
}

resource "aws_acm_certificate_validation" "ground_prd_me_marstech_co" {
  provider                = aws.me_central_1
  certificate_arn         = aws_acm_certificate.ground_prd_me_marstech_co.arn
  validation_record_fqdns = [for r in aws_route53_record.ground_prd_me_marstech_co_cert : r.fqdn]
}

resource "aws_acm_certificate" "ground_prd_me_influencer_ae" {
  provider                  = aws.me_central_1
  domain_name               = "moonadmin.influencer.ae"
  subject_alternative_names = ["*.moonadmin.influencer.ae"]
  validation_method         = "DNS"
  tags                      = { "ground-managed" = "true" }
  lifecycle { create_before_destroy = true }
}

resource "aws_route53_record" "ground_prd_me_influencer_ae_cert" {
  provider = aws.me_central_1
  for_each = {
    for dvo in aws_acm_certificate.ground_prd_me_influencer_ae.domain_validation_options : dvo.domain_name => dvo
  }
  zone_id = data.aws_route53_zone.ground_prd_me_influencer_ae.zone_id
  name    = each.value.resource_record_name
  type    = each.value.resource_record_type
  records = [each.value.resource_record_value]
  ttl     = 60
}

resource "aws_acm_certificate_validation" "ground_prd_me_influencer_ae" {
  provider                = aws.me_central_1
  certificate_arn         = aws_acm_certificate.ground_prd_me_influencer_ae.arn
  validation_record_fqdns = [for r in aws_route53_record.ground_prd_me_influencer_ae_cert : r.fqdn]
}

resource "aws_lb_listener" "ground_prd_me_http" {
  provider          = aws.me_central_1
  load_balancer_arn = aws_lb.ground_prd_me.arn
  port              = 80; protocol = "HTTP"
  default_action {
    type = "redirect"
    redirect { port = "443"; protocol = "HTTPS"; status_code = "HTTP_301" }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_listener" "ground_prd_me_https" {
  provider          = aws.me_central_1
  load_balancer_arn = aws_lb.ground_prd_me.arn
  port              = 443; protocol = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = aws_acm_certificate_validation.ground_prd_me_marstech_co.certificate_arn
  default_action {
    type = "fixed-response"
    fixed_response { content_type = "text/plain"; message_body = "not found"; status_code = "404" }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_listener_certificate" "ground_prd_me_influencer_ae" {
  provider        = aws.me_central_1
  listener_arn    = aws_lb_listener.ground_prd_me_https.arn
  certificate_arn = aws_acm_certificate_validation.ground_prd_me_influencer_ae.certificate_arn
}

resource "aws_lb_target_group" "ground_prd_me" {
  provider    = aws.me_central_1
  for_each    = toset(local.prd_me.stack_edges)
  name        = "ground-prd-me-${each.key}-tg"
  port        = 8080; protocol = "HTTP"; target_type = "ip"
  vpc_id      = aws_vpc.ground_prd_me.id
  health_check { path = "/health"; matcher = "200" }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_listener_rule" "ground_prd_me" {
  provider     = aws.me_central_1
  for_each     = toset(local.prd_me.stack_edges)
  listener_arn = aws_lb_listener.ground_prd_me_https.arn
  action { type = "forward"; target_group_arn = aws_lb_target_group.ground_prd_me[each.key].arn }
  condition {
    host_header { values = ["${local.edges[each.key].sub}.${local.domains[local.edges[each.key].domain].host}"] }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_route53_record" "ground_prd_me_edge" {
  provider = aws.me_central_1
  for_each = toset(local.prd_me.stack_edges)
  zone_id = (local.edges[each.key].domain == "marstech-co"
    ? data.aws_route53_zone.ground_prd_me_marstech_co.zone_id
    : data.aws_route53_zone.ground_prd_me_influencer_ae.zone_id)
  name    = "${local.edges[each.key].sub}.${local.domains[local.edges[each.key].domain].host}"
  type    = "CNAME"; ttl = 300
  records = [aws_lb.ground_prd_me.dns_name]
}

# ==============================================================================
# stg-eu — deploy stg:marstech to aws as stg-eu
# ==============================================================================

# --- networking ---

resource "aws_vpc" "ground_stg_eu" {
  provider             = aws.eu_central_1
  cidr_block           = "10.1.0.0/16"
  enable_dns_support   = true
  enable_dns_hostnames = true
  tags = { "ground-managed" = "true", Name = "ground-stg-eu-vpc" }
}

resource "aws_internet_gateway" "ground_stg_eu" {
  provider = aws.eu_central_1
  vpc_id   = aws_vpc.ground_stg_eu.id
  tags     = { "ground-managed" = "true", Name = "ground-stg-eu-igw" }
}

resource "aws_subnet" "ground_stg_eu_pub" {
  provider                = aws.eu_central_1
  for_each                = { for i, az in local.stg_eu.azs : az => i }
  vpc_id                  = aws_vpc.ground_stg_eu.id
  availability_zone       = each.key
  cidr_block              = cidrsubnet("10.1.0.0/16", 8, each.value)
  map_public_ip_on_launch = true
  tags = { "ground-managed" = "true", Name = "ground-stg-eu-pub-${each.key}" }
}

resource "aws_subnet" "ground_stg_eu_prv" {
  provider          = aws.eu_central_1
  for_each          = { for i, az in local.stg_eu.azs : az => i }
  vpc_id            = aws_vpc.ground_stg_eu.id
  availability_zone = each.key
  cidr_block        = cidrsubnet("10.1.0.0/16", 8, each.value + 10)
  tags = { "ground-managed" = "true", Name = "ground-stg-eu-prv-${each.key}" }
}

resource "aws_eip" "ground_stg_eu_nat" {
  provider = aws.eu_central_1
  domain   = "vpc"
  tags     = { "ground-managed" = "true", Name = "ground-stg-eu-nat-eip" }
}

resource "aws_nat_gateway" "ground_stg_eu" {
  provider      = aws.eu_central_1
  allocation_id = aws_eip.ground_stg_eu_nat.id
  subnet_id     = aws_subnet.ground_stg_eu_pub["eu-central-1a"].id
  tags          = { "ground-managed" = "true", Name = "ground-stg-eu-nat" }
  depends_on    = [aws_internet_gateway.ground_stg_eu]
}

resource "aws_route_table" "ground_stg_eu_pub" {
  provider = aws.eu_central_1
  vpc_id   = aws_vpc.ground_stg_eu.id
  route { cidr_block = "0.0.0.0/0"; gateway_id = aws_internet_gateway.ground_stg_eu.id }
  tags = { "ground-managed" = "true", Name = "ground-stg-eu-rpub" }
}

resource "aws_route_table_association" "ground_stg_eu_pub" {
  provider       = aws.eu_central_1
  for_each       = aws_subnet.ground_stg_eu_pub
  subnet_id      = each.value.id
  route_table_id = aws_route_table.ground_stg_eu_pub.id
}

resource "aws_route_table" "ground_stg_eu_prv" {
  provider = aws.eu_central_1
  vpc_id   = aws_vpc.ground_stg_eu.id
  route { cidr_block = "0.0.0.0/0"; nat_gateway_id = aws_nat_gateway.ground_stg_eu.id }
  tags = { "ground-managed" = "true", Name = "ground-stg-eu-rprv" }
}

resource "aws_route_table_association" "ground_stg_eu_prv" {
  provider       = aws.eu_central_1
  for_each       = aws_subnet.ground_stg_eu_prv
  subnet_id      = each.value.id
  route_table_id = aws_route_table.ground_stg_eu_prv.id
}

# --- cluster (space app) ---

resource "aws_ecs_cluster" "ground_stg_eu_app" {
  provider = aws.eu_central_1
  name     = "ground-stg-eu-app-cluster"
  tags     = { "ground-managed" = "true" }
}

resource "aws_service_discovery_private_dns_namespace" "ground_stg_eu_app" {
  provider = aws.eu_central_1
  name     = "app.stg.internal"
  vpc      = aws_vpc.ground_stg_eu.id
  tags     = { "ground-managed" = "true" }
}

# --- secrets ---

resource "aws_secretsmanager_secret" "ground_stg_eu" {
  provider                = aws.eu_central_1
  for_each                = toset(local.stg_eu.stack_secrets)
  name                    = "ground-stg-eu/${each.key}"
  recovery_window_in_days = 7
  tags                    = { "ground-managed" = "true" }
}

# --- database ---

resource "random_password" "ground_stg_eu_main" {
  length  = 32
  special = false
}

resource "aws_db_subnet_group" "ground_stg_eu_main" {
  provider   = aws.eu_central_1
  name       = "ground-stg-eu-main-db-sng"
  subnet_ids = [for s in aws_subnet.ground_stg_eu_prv : s.id]
  tags       = { "ground-managed" = "true" }
}

resource "aws_security_group" "ground_stg_eu_main_db" {
  provider    = aws.eu_central_1
  name        = "ground-stg-eu-main-db-sg"
  description = "ground-stg-eu main db"
  vpc_id      = aws_vpc.ground_stg_eu.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_stg_eu_main_db_all" {
  provider          = aws.eu_central_1
  security_group_id = aws_security_group.ground_stg_eu_main_db.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_db_instance" "ground_stg_eu_main" {
  provider               = aws.eu_central_1
  identifier             = "ground-stg-eu-main-db"
  engine                 = "postgres"
  engine_version         = "15"
  instance_class         = local.size_db[local.stg_eu.db.size]
  allocated_storage      = local.stg_eu.db.storage
  db_name                = "main"
  username               = "ground"
  password               = random_password.ground_stg_eu_main.result
  db_subnet_group_name   = aws_db_subnet_group.ground_stg_eu_main.name
  vpc_security_group_ids = [aws_security_group.ground_stg_eu_main_db.id]
  skip_final_snapshot    = true
  tags                   = { "ground-managed" = "true" }
}

# --- service: api-gen ---

resource "aws_security_group" "ground_stg_eu_svc" {
  provider    = aws.eu_central_1
  for_each    = toset(local.stg_eu.stack_services)
  name        = "ground-stg-eu-${each.key}-sg"
  description = "ground-stg-eu ${each.key}"
  vpc_id      = aws_vpc.ground_stg_eu.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_stg_eu_svc_all" {
  provider          = aws.eu_central_1
  for_each          = toset(local.stg_eu.stack_services)
  security_group_id = aws_security_group.ground_stg_eu_svc[each.key].id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_iam_role" "ground_stg_eu_exec" {
  for_each = toset(local.stg_eu.stack_services)
  name     = "ground-stg-eu-${each.key}-exec-role"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = { Service = "ecs-tasks.amazonaws.com" }, Action = "sts:AssumeRole" }]
  })
  tags = { "ground-managed" = "true" }
}

resource "aws_iam_role_policy_attachment" "ground_stg_eu_exec" {
  for_each   = toset(local.stg_eu.stack_services)
  role       = aws_iam_role.ground_stg_eu_exec[each.key].name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role" "ground_stg_eu_task" {
  for_each = toset(local.stg_eu.stack_services)
  name     = "ground-stg-eu-${each.key}-task-role"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = { Service = "ecs-tasks.amazonaws.com" }, Action = "sts:AssumeRole" }]
  })
  tags = { "ground-managed" = "true" }
}

# api-gen in stg accesses database:main (access_db = ["main"]) and services media/pay/core
# but stg stack only has api-gen — no secrets, no bucket
resource "aws_vpc_security_group_ingress_rule" "ground_stg_eu_api_gen_to_db" {
  provider                     = aws.eu_central_1
  security_group_id            = aws_security_group.ground_stg_eu_main_db.id
  referenced_security_group_id = aws_security_group.ground_stg_eu_svc["api-gen"].id
  from_port                    = 5432; to_port = 5432; ip_protocol = "tcp"
  tags                         = { "ground-managed" = "true" }
}

resource "aws_cloudwatch_log_group" "ground_stg_eu_svc" {
  provider          = aws.eu_central_1
  for_each          = toset(local.stg_eu.stack_services)
  name              = "/ground/stg-eu/${each.key}"
  retention_in_days = 30
  tags              = { "ground-managed" = "true" }
}

resource "aws_ecs_task_definition" "ground_stg_eu_svc" {
  provider                 = aws.eu_central_1
  for_each                 = toset(local.stg_eu.stack_services)
  family                   = "ground-stg-eu-${each.key}-task"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = local.size_cpu[local.stg_eu.sizing[each.key].size]
  memory                   = local.size_memory[local.stg_eu.sizing[each.key].size]
  execution_role_arn       = aws_iam_role.ground_stg_eu_exec[each.key].arn
  task_role_arn            = aws_iam_role.ground_stg_eu_task[each.key].arn
  container_definitions = jsonencode([{
    name      = each.key
    image     = "${var.ecr_registry}/${each.key}:latest"
    essential = true
    portMappings = [{ containerPort = 8080, protocol = "tcp" }]
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        "awslogs-group"         = aws_cloudwatch_log_group.ground_stg_eu_svc[each.key].name
        "awslogs-region"        = local.stg_eu.region
        "awslogs-stream-prefix" = each.key
      }
    }
  }])
  tags = { "ground-managed" = "true" }
}

resource "aws_ecs_service" "ground_stg_eu_svc" {
  provider        = aws.eu_central_1
  for_each        = toset(local.stg_eu.stack_services)
  name            = "ground-stg-eu-${each.key}-svc"
  cluster         = aws_ecs_cluster.ground_stg_eu_app.id
  task_definition = aws_ecs_task_definition.ground_stg_eu_svc[each.key].arn
  desired_count   = local.stg_eu.sizing[each.key].min
  launch_type     = "FARGATE"
  network_configuration {
    subnets         = [for s in aws_subnet.ground_stg_eu_prv : s.id]
    security_groups = [aws_security_group.ground_stg_eu_svc[each.key].id]
  }
  tags = { "ground-managed" = "true" }
}

# --- ALB + routing (edge-api → api-gen, domain influencer-ae) ---

resource "aws_security_group" "ground_stg_eu_alb" {
  provider    = aws.eu_central_1
  name        = "ground-stg-eu-alb-sg"
  description = "ground-stg-eu ALB"
  vpc_id      = aws_vpc.ground_stg_eu.id
  tags        = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_ingress_rule" "ground_stg_eu_alb_http" {
  provider          = aws.eu_central_1
  security_group_id = aws_security_group.ground_stg_eu_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 80; to_port = 80; ip_protocol = "tcp"
  tags              = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_ingress_rule" "ground_stg_eu_alb_https" {
  provider          = aws.eu_central_1
  security_group_id = aws_security_group.ground_stg_eu_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 443; to_port = 443; ip_protocol = "tcp"
  tags              = { "ground-managed" = "true" }
}

resource "aws_vpc_security_group_egress_rule" "ground_stg_eu_alb_all" {
  provider          = aws.eu_central_1
  security_group_id = aws_security_group.ground_stg_eu_alb.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  tags              = { "ground-managed" = "true" }
}

resource "aws_lb" "ground_stg_eu" {
  provider           = aws.eu_central_1
  name               = "ground-stg-eu-alb"
  load_balancer_type = "application"
  internal           = false
  subnets            = [for s in aws_subnet.ground_stg_eu_pub : s.id]
  security_groups    = [aws_security_group.ground_stg_eu_alb.id]
  tags               = { "ground-managed" = "true" }
}

data "aws_route53_zone" "ground_stg_eu_influencer_ae" {
  provider = aws.eu_central_1
  name     = "moonadmin.influencer.ae"
}

resource "aws_acm_certificate" "ground_stg_eu_influencer_ae" {
  provider                  = aws.eu_central_1
  domain_name               = "moonadmin.influencer.ae"
  subject_alternative_names = ["*.moonadmin.influencer.ae"]
  validation_method         = "DNS"
  tags                      = { "ground-managed" = "true" }
  lifecycle { create_before_destroy = true }
}

resource "aws_route53_record" "ground_stg_eu_influencer_ae_cert" {
  provider = aws.eu_central_1
  for_each = {
    for dvo in aws_acm_certificate.ground_stg_eu_influencer_ae.domain_validation_options : dvo.domain_name => dvo
  }
  zone_id = data.aws_route53_zone.ground_stg_eu_influencer_ae.zone_id
  name    = each.value.resource_record_name
  type    = each.value.resource_record_type
  records = [each.value.resource_record_value]
  ttl     = 60
}

resource "aws_acm_certificate_validation" "ground_stg_eu_influencer_ae" {
  provider                = aws.eu_central_1
  certificate_arn         = aws_acm_certificate.ground_stg_eu_influencer_ae.arn
  validation_record_fqdns = [for r in aws_route53_record.ground_stg_eu_influencer_ae_cert : r.fqdn]
}

resource "aws_lb_listener" "ground_stg_eu_http" {
  provider          = aws.eu_central_1
  load_balancer_arn = aws_lb.ground_stg_eu.arn
  port              = 80; protocol = "HTTP"
  default_action {
    type = "redirect"
    redirect { port = "443"; protocol = "HTTPS"; status_code = "HTTP_301" }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_listener" "ground_stg_eu_https" {
  provider          = aws.eu_central_1
  load_balancer_arn = aws_lb.ground_stg_eu.arn
  port              = 443; protocol = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = aws_acm_certificate_validation.ground_stg_eu_influencer_ae.certificate_arn
  default_action {
    type = "fixed-response"
    fixed_response { content_type = "text/plain"; message_body = "not found"; status_code = "404" }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_target_group" "ground_stg_eu_edge_api" {
  provider    = aws.eu_central_1
  name        = "ground-stg-eu-edge-api-tg"
  port        = 8080; protocol = "HTTP"; target_type = "ip"
  vpc_id      = aws_vpc.ground_stg_eu.id
  health_check { path = "/health"; matcher = "200" }
  tags = { "ground-managed" = "true" }
}

resource "aws_lb_listener_rule" "ground_stg_eu_edge_api" {
  provider     = aws.eu_central_1
  listener_arn = aws_lb_listener.ground_stg_eu_https.arn
  action { type = "forward"; target_group_arn = aws_lb_target_group.ground_stg_eu_edge_api.arn }
  condition {
    host_header { values = ["api.moonadmin.influencer.ae"] }
  }
  tags = { "ground-managed" = "true" }
}

resource "aws_route53_record" "ground_stg_eu_edge_api" {
  provider = aws.eu_central_1
  zone_id  = data.aws_route53_zone.ground_stg_eu_influencer_ae.zone_id
  name     = "api.moonadmin.influencer.ae"
  type     = "CNAME"; ttl = 300
  records  = [aws_lb.ground_stg_eu.dns_name]
}
