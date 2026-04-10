# Transform Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the std→std:aws transformation layer: add `deploy` as a first-class std type, define `aws_deploy`, write all node/edge transformation rules in Ground, and write TypeScript function skeletons with fully-typed I/O interfaces.

**Architecture:** Two-phase resolution via types — `(d: aws_deploy)` fires first and fills infra links on the deploy entity; phase-2 anonymous type functions take `(entity, d: aws_deploy)` and produce AWS vendor entities. Edge rules take `(from, to, d: aws_deploy)` and produce connector resources (SG ingress rules, IAM policies). Phases are ordered by the arg graph; no explicit orchestration needed.

**Tech Stack:** Ground language files (`.grd`), TypeScript (`.ts`) for typed function signatures.

**Spec:** `docs/superpowers/specs/2026-04-10-transform-layer-design.md`

---

## File Map

| Action   | Path                                  | Responsibility                                              |
|----------|---------------------------------------|-------------------------------------------------------------|
| Modify   | `mvp/std.grd`                         | Add size, scaling, service_config, database_config, database, deploy types |
| Modify   | `mvp/std/aws/pack.grd`                | Add aws_deploy (extends deploy via application)             |
| Create   | `mvp/std/aws/transform.grd`           | 8 node rules + 4 edge rules (anonymous type functions)      |
| Create   | `mvp/std/aws/transform.ts`            | 12 pure TS functions with MakeXxxI/MakeXxxO interfaces      |
| Modify   | `mvp/marstech/env/prd.grd`            | Update deploy syntax to aws_deploy first-class type         |
| Modify   | `mvp/marstech/env/stg.grd`            | Update deploy syntax to aws_deploy first-class type         |
| Modify   | `GROUND-BOOK.md`                      | Add best practices section: phases via types                |

---

### Task 1: Add `deploy` and supporting types to `std.grd`

**Files:**
- Modify: `mvp/std.grd`

Current `std.grd` is missing `type database` (referenced by `stack` and `app.grd` but never defined). Add it alongside the new deploy vocabulary.

- [ ] **Step 1: Add `database` type**

Open `mvp/std.grd`. After `type boolean = true | false`, add:

```ground
type database = {
    link engine = postgres | mysql
}
```

- [ ] **Step 2: Add size and scaling types**

After the `database` type, add:

```ground
type size = small | medium | large | xlarge

type scaling = {
    link min = int
    link max = int
}
```

- [ ] **Step 3: Add service_config and database_config types**

After `scaling`, add:

```ground
type service_config = {
    link service = service
    link size    = size
    link scaling = scaling
}

type database_config = {
    link database = database
    link size     = size
    link storage  = int
}
```

- [ ] **Step 4: Add deploy type**

After `database_config`, add:

```ground
type deploy = {
    link stack     = stack
    link region    = [ string ]
    link services  = [ service_config ]
    link databases = [ database_config ]
}
```

- [ ] **Step 5: Verify final std.grd**

Read `mvp/std.grd`. Confirm it contains, in order:
- `type boolean`
- `type database` (with `engine = postgres | mysql`)
- `type size`, `type scaling`
- `type service_config`, `type database_config`
- `type deploy`
- `type bucket`, `type domain`, `type space`, `type edge`, `type service`, `type secret`, `type stack`

Also confirm `type stack` still references `type:database` in its anonymous link.

---

### Task 2: Add `aws_deploy` to `std/aws/pack.grd`

**Files:**
- Modify: `mvp/std/aws/pack.grd`

`aws_deploy` extends `deploy` via application chaining. It adds AWS infra links that the phase-1 expansion function fills.

- [ ] **Step 1: Add aws_deploy at the bottom of pack.grd**

Open `mvp/std/aws/pack.grd`. After the last `type aws_lb_listener_rule` block, add:

```ground
# ── Deploy ────────────────────────────────────────────────────────────────────

deploy aws_deploy = {
    link vpc             = aws_vpc
    link public_subnets  = [ aws_subnet ]
    link private_subnets = [ aws_subnet ]
    link internet_gw     = aws_internet_gateway
    link nat_gw          = aws_nat_gateway
    link lb              = aws_lb
    link cluster         = aws_ecs_cluster
    link namespace       = aws_service_discovery_private_dns_namespace
}
```

- [ ] **Step 2: Verify aws_deploy link types all exist in pack.grd**

Read `mvp/std/aws/pack.grd`. Confirm each of the following types is defined above `aws_deploy`:
- `aws_vpc` ✓
- `aws_subnet` ✓
- `aws_internet_gateway` ✓
- `aws_nat_gateway` ✓
- `aws_lb` ✓
- `aws_ecs_cluster` ✓
- `aws_service_discovery_private_dns_namespace` ✓

---

### Task 3: Create `mvp/std/aws/transform.grd`

**Files:**
- Create: `mvp/std/aws/transform.grd`

All 8 node rules + 4 edge rules in one file.

- [ ] **Step 1: Create the file with phase-1 deploy rule**

Create `mvp/std/aws/transform.grd`:

```ground
use std
use std:aws

# ── Phase 1 — deploy infra (fires first; fills infra links on aws_deploy) ─────

type (d: aws_deploy) = make_aws_deploy {
    link vpc             = aws_vpc
    link public_subnets  = [ aws_subnet ]
    link private_subnets = [ aws_subnet ]
    link internet_gw     = aws_internet_gateway
    link nat_gw          = aws_nat_gateway
    link lb              = aws_lb
    link cluster         = aws_ecs_cluster
    link namespace       = aws_service_discovery_private_dns_namespace
}

# ── Phase 2 — entities (d carries fully expanded infra + sizing) ──────────────

type (svc: service, d: aws_deploy) = make_service {
    link sg        = aws_security_group
    link ecs       = aws_ecs_service
    link task_def  = aws_ecs_task_definition
    link exec_role = aws_iam_role
    link task_role = aws_iam_role
    link log       = aws_cloudwatch_log_group
}

type (db: database, d: aws_deploy) = make_database {
    link sg           = aws_security_group
    link rds          = aws_db_instance
    link subnet_group = aws_db_subnet_group
}

type (sec: secret, d: aws_deploy) = make_secret {
    link sm = aws_secretsmanager_secret
}

type (bkt: bucket, d: aws_deploy) = make_bucket {
    link s3 = aws_s3_bucket
}

type (dom: domain, d: aws_deploy) = make_domain {
    link cert       = aws_acm_certificate
    link validation = aws_acm_certificate_validation
    link zone       = aws_route53_zone
}

type (edg: edge, d: aws_deploy) = make_edge {
    link tg     = aws_lb_target_group
    link rule   = aws_lb_listener_rule
    link record = aws_route53_record
}

type (sp: space, d: aws_deploy) = make_space {
    link cluster   = aws_ecs_cluster
    link namespace = aws_service_discovery_private_dns_namespace
}

# ── Edge rules — access links ─────────────────────────────────────────────────

type (from: service, to: database, d: aws_deploy) = make_service_database_access {
    link ingress = aws_vpc_security_group_ingress_rule
}

type (from: service, to: service, d: aws_deploy) = make_service_service_access {
    link ingress = aws_vpc_security_group_ingress_rule
}

type (from: service, to: secret, d: aws_deploy) = make_service_secret_access {
    link policy = aws_iam_role_policy
}

type (from: service, to: bucket, d: aws_deploy) = make_service_bucket_access {
    link policy = aws_iam_role_policy
}
```

- [ ] **Step 2: Verify all TS hook names are unique**

Read `mvp/std/aws/transform.grd`. Confirm these 12 hook names appear exactly once each:
`make_aws_deploy`, `make_service`, `make_database`, `make_secret`, `make_bucket`,
`make_domain`, `make_edge`, `make_space`,
`make_service_database_access`, `make_service_service_access`,
`make_service_secret_access`, `make_service_bucket_access`

- [ ] **Step 3: Verify all output link types exist in std/aws/pack.grd**

For each node rule output, confirm the type is defined in `mvp/std/aws/pack.grd`:
- `make_service` outputs: `aws_security_group`, `aws_ecs_service`, `aws_ecs_task_definition`, `aws_iam_role`, `aws_cloudwatch_log_group`
- `make_database` outputs: `aws_security_group`, `aws_db_instance`, `aws_db_subnet_group`
- `make_secret` outputs: `aws_secretsmanager_secret`
- `make_bucket` outputs: `aws_s3_bucket`
- `make_domain` outputs: `aws_acm_certificate`, `aws_acm_certificate_validation`, `aws_route53_zone`
- `make_edge` outputs: `aws_lb_target_group`, `aws_lb_listener_rule`, `aws_route53_record`
- `make_space` outputs: `aws_ecs_cluster`, `aws_service_discovery_private_dns_namespace`
- Edge outputs: `aws_vpc_security_group_ingress_rule`, `aws_iam_role_policy`

---

### Task 4: Create `mvp/std/aws/transform.ts`

**Files:**
- Create: `mvp/std/aws/transform.ts`

Fully-typed I/O interfaces for all 12 functions. Bodies are stubs — computation implementation follows once templates are defined.

- [ ] **Step 1: Create the file**

Create `mvp/std/aws/transform.ts`:

```typescript
// Ground-generated interfaces for transform.grd hooks.
// Each function is pure: typed input → typed output, no side effects.
// Stub bodies marked with TODO — implement using real infra patterns from marstech.tf.

// ── Shared types ─────────────────────────────────────────────────────────────

interface ServiceConfig  { service: string; size: 'small'|'medium'|'large'|'xlarge'; scaling: { min: number; max: number } }
interface DatabaseConfig { database: string; size: 'small'|'medium'|'large'|'xlarge'; storage: number }

// ── AWS vendor types (mirrors std/aws/pack.grd) ───────────────────────────────

interface AwsVpc                                   { cidr_block: string; enable_dns_support: boolean; enable_dns_hostnames: boolean }
interface AwsSubnet                                { vpc: AwsVpc; cidr_block: string; availability_zone: string; map_public_ip_on_launch: boolean }
interface AwsInternetGateway                       { vpc: AwsVpc }
interface AwsEip                                   { domain: 'vpc' }
interface AwsNatGateway                            { subnet: AwsSubnet; allocation: AwsEip }
interface AwsSecurityGroup                         { vpc: AwsVpc; name: string; description: string }
interface AwsVpcSecurityGroupIngressRule           { security_group: AwsSecurityGroup; from_port: number; to_port: number; ip_protocol: string; referenced_security_group: AwsSecurityGroup }
interface AwsIamRole                               { name: string; assume_role_policy: string }
interface AwsIamRolePolicy                         { name: string; role: AwsIamRole; policy: string }
interface AwsIamRolePolicyAttachment               { role: AwsIamRole; policy_arn: string }
interface AwsCloudwatchLogGroup                    { name: string; retention_in_days: number }
interface AwsEcsCluster                            { name: string }
interface AwsServiceDiscoveryPrivateDnsNamespace   { name: string; vpc: AwsVpc }
interface AwsEcsTaskDefinition                     { family: string; network_mode: 'awsvpc'|'bridge'|'host'|'none'; requires_compatibilities: ('FARGATE'|'EC2')[]; cpu: number; memory: number; execution_role: AwsIamRole; task_role: AwsIamRole; log_group: AwsCloudwatchLogGroup }
interface AwsEcsService                            { name: string; cluster: AwsEcsCluster; task_definition: AwsEcsTaskDefinition; desired_count: number; launch_type: 'FARGATE'|'EC2'; subnets: AwsSubnet[]; security_groups: AwsSecurityGroup[] }
interface AwsDbSubnetGroup                         { name: string; subnets: AwsSubnet[] }
interface AwsDbInstance                            { identifier: string; engine: 'postgres'|'mysql'; engine_version: string; instance_class: string; allocated_storage: number; db_name: string; username: string; subnet_group: AwsDbSubnetGroup; security_groups: AwsSecurityGroup[]; skip_final_snapshot: boolean }
interface AwsSecretsmanagerSecret                  { name: string; recovery_window_in_days: number }
interface AwsS3Bucket                              { bucket: string }
interface AwsRoute53Zone                           { name: string }
interface AwsAcmCertificate                        { domain_name: string; subject_alternative_names: string[]; zone: AwsRoute53Zone; validation_method: 'DNS'|'EMAIL' }
interface AwsRoute53Record                         { zone: AwsRoute53Zone; name: string; record_type: 'A'|'CNAME'|'MX'|'TXT'; ttl: number; records: string[] }
interface AwsAcmCertificateValidation              { certificate: AwsAcmCertificate; validation_records: AwsRoute53Record[] }
interface AwsLb                                    { name: string; load_balancer_type: 'application'|'network'; scheme: 'internal'|'internet-facing'; vpc: AwsVpc; security_groups: AwsSecurityGroup[] }
interface AwsLbTargetGroup                         { name: string; port: number; protocol: 'HTTP'|'HTTPS'; target_type: 'ip'|'instance'|'lambda'; vpc: AwsVpc; health_check_path: string; health_check_matcher: string }
interface AwsLbListenerRule                        { listener: AwsLbListener; target_group: AwsLbTargetGroup; host_header: string }
interface AwsLbListener                            { load_balancer: AwsLb; port: number; protocol: 'HTTP'|'HTTPS'; ssl_policy: string; certificate: AwsAcmCertificateValidation; default_action: object }

// ── Phase 1 — deploy infra ────────────────────────────────────────────────────

interface MakeAwsDeployI {
    d: {
        stack: { name: string }
        region: string[]
        services:  ServiceConfig[]
        databases: DatabaseConfig[]
    }
}
interface MakeAwsDeployO {
    vpc:             AwsVpc
    public_subnets:  AwsSubnet[]
    private_subnets: AwsSubnet[]
    internet_gw:     AwsInternetGateway
    nat_gw:          AwsNatGateway
    lb:              AwsLb
    cluster:         AwsEcsCluster
    namespace:       AwsServiceDiscoveryPrivateDnsNamespace
}
// TODO: CIDR allocation (10.0.0.0/16 → public/private per AZ), NAT gateway in first AZ,
//       internet-facing ALB with HTTP→HTTPS redirect, ECS cluster, service discovery namespace
export function make_aws_deploy(i: MakeAwsDeployI): MakeAwsDeployO {
    throw new Error('not implemented')
}

// ── Phase 2 — node rules ──────────────────────────────────────────────────────

interface MakeServiceI {
    svc: { name: string; port: 'grpc'|'http'; observe: { tracing: boolean; datadog: boolean } }
    d: {
        vpc:             AwsVpc
        private_subnets: AwsSubnet[]
        cluster:         AwsEcsCluster
        services:        ServiceConfig[]
    }
}
interface MakeServiceO {
    sg:        AwsSecurityGroup
    ecs:       AwsEcsService
    task_def:  AwsEcsTaskDefinition
    exec_role: AwsIamRole
    task_role: AwsIamRole
    log:       AwsCloudwatchLogGroup
}
// TODO: sg name = `${svc.name}-sg`; exec/task role names = `${svc.name}-exec-role`/`${svc.name}-task-role`;
//       cpu/memory from sizing map (small=256/512, medium=512/1024, large=1024/2048, xlarge=2048/4096);
//       desired_count = scaling.min; log group = `/ecs/${svc.name}`
export function make_service(i: MakeServiceI): MakeServiceO {
    throw new Error('not implemented')
}

interface MakeDatabaseI {
    db: { name: string; engine: 'postgres'|'mysql' }
    d: {
        vpc:             AwsVpc
        private_subnets: AwsSubnet[]
        databases:       DatabaseConfig[]
    }
}
interface MakeDatabaseO {
    sg:           AwsSecurityGroup
    rds:          AwsDbInstance
    subnet_group: AwsDbSubnetGroup
}
// TODO: sg name = `${db.name}-sg`; instance_class from size (small=db.t3.small, medium=db.t3.medium,
//       large=db.t3.large, xlarge=db.r5.large); engine_version from engine (postgres→15, mysql→8.0);
//       db_name = db.name; username = "master"; skip_final_snapshot = false
export function make_database(i: MakeDatabaseI): MakeDatabaseO {
    throw new Error('not implemented')
}

interface MakeSecretI  { sec: { name: string }; d: { stack: { name: string } } }
interface MakeSecretO  { sm: AwsSecretsmanagerSecret }
// TODO: name = `${d.stack.name}/${sec.name}`; recovery_window_in_days = 30
export function make_secret(i: MakeSecretI): MakeSecretO {
    throw new Error('not implemented')
}

interface MakeBucketI  { bkt: { name: string; access: 'read'|'write' }; d: { stack: { name: string } } }
interface MakeBucketO  { s3: AwsS3Bucket }
// TODO: bucket = `${d.stack.name}-${bkt.name}`
export function make_bucket(i: MakeBucketI): MakeBucketO {
    throw new Error('not implemented')
}

interface MakeDomainI  { dom: { host: string }; d: { lb: AwsLb } }
interface MakeDomainO  {
    cert:       AwsAcmCertificate
    validation: AwsAcmCertificateValidation
    zone:       AwsRoute53Zone
}
// TODO: zone.name = dom.host; cert.domain_name = dom.host; cert.subject_alternative_names = [`*.${dom.host}`];
//       validation_method = DNS; validation record from cert DNS validation CNAME
export function make_domain(i: MakeDomainI): MakeDomainO {
    throw new Error('not implemented')
}

interface MakeEdgeI {
    edg: { domain: { host: string }; sub: string; backend: { name: string } }
    d: { lb: AwsLb }
}
interface MakeEdgeO {
    tg:     AwsLbTargetGroup
    rule:   AwsLbListenerRule
    record: AwsRoute53Record
}
// TODO: tg.name = `${edg.backend.name}-tg`; host_header = `${edg.sub}.${edg.domain.host}`;
//       record = A alias pointing to lb dns name; health_check_path = /health
export function make_edge(i: MakeEdgeI): MakeEdgeO {
    throw new Error('not implemented')
}

interface MakeSpaceI   { sp: { host: string; services: { name: string }[] }; d: { vpc: AwsVpc; stack: { name: string } } }
interface MakeSpaceO   { cluster: AwsEcsCluster; namespace: AwsServiceDiscoveryPrivateDnsNamespace }
// TODO: cluster.name = `${d.stack.name}-${sp.host}`; namespace.name = sp.host
export function make_space(i: MakeSpaceI): MakeSpaceO {
    throw new Error('not implemented')
}

// ── Edge rules ────────────────────────────────────────────────────────────────

interface MakeServiceDatabaseAccessI { from: { name: string }; to: { name: string }; d: { vpc: AwsVpc } }
interface MakeServiceDatabaseAccessO { ingress: AwsVpcSecurityGroupIngressRule }
// TODO: sg = to's sg (`${to.name}-sg`); referenced_sg = from's sg (`${from.name}-sg`);
//       from_port = to_port = 5432 (postgres) or 3306 (mysql); ip_protocol = tcp
export function make_service_database_access(i: MakeServiceDatabaseAccessI): MakeServiceDatabaseAccessO {
    throw new Error('not implemented')
}

interface MakeServiceServiceAccessI  { from: { name: string }; to: { name: string; port: 'grpc'|'http' }; d: { vpc: AwsVpc } }
interface MakeServiceServiceAccessO  { ingress: AwsVpcSecurityGroupIngressRule }
// TODO: sg = to's sg (`${to.name}-sg`); referenced_sg = from's sg (`${from.name}-sg`);
//       port = grpc→50051, http→8080; ip_protocol = tcp
export function make_service_service_access(i: MakeServiceServiceAccessI): MakeServiceServiceAccessO {
    throw new Error('not implemented')
}

interface MakeServiceSecretAccessI   { from: { name: string }; to: { name: string }; d: { stack: { name: string } } }
interface MakeServiceSecretAccessO   { policy: AwsIamRolePolicy }
// TODO: role = from's task role (`${from.name}-task-role`);
//       policy = JSON Allow secretsmanager:GetSecretValue on `${d.stack.name}/${to.name}` ARN
export function make_service_secret_access(i: MakeServiceSecretAccessI): MakeServiceSecretAccessO {
    throw new Error('not implemented')
}

interface MakeServiceBucketAccessI   { from: { name: string }; to: { name: string; access: 'read'|'write' }; d: { stack: { name: string } } }
interface MakeServiceBucketAccessO   { policy: AwsIamRolePolicy }
// TODO: role = from's task role (`${from.name}-task-role`);
//       read → Allow s3:GetObject; write → Allow s3:PutObject,s3:DeleteObject
//       resource = `arn:aws:s3:::${d.stack.name}-${to.name}/*`
export function make_service_bucket_access(i: MakeServiceBucketAccessI): MakeServiceBucketAccessO {
    throw new Error('not implemented')
}
```

- [ ] **Step 2: Verify all 12 hook names in transform.ts match transform.grd**

Read both files. Confirm these function names appear in both:
`make_aws_deploy`, `make_service`, `make_database`, `make_secret`, `make_bucket`,
`make_domain`, `make_edge`, `make_space`,
`make_service_database_access`, `make_service_service_access`,
`make_service_secret_access`, `make_service_bucket_access`

- [ ] **Step 3: Verify I/O interface field counts match transform.grd output links**

Check each pair:
- `MakeAwsDeployO` has 8 fields — matches the 8 output links in `type (d: aws_deploy)` rule
- `MakeServiceO` has 6 fields — matches 6 output links in `type (svc: service, d: aws_deploy)` rule
- `MakeDatabaseO` has 3 fields — matches 3 output links
- `MakeSecretO` has 1 field — matches 1 output link
- `MakeBucketO` has 1 field — matches 1 output link
- `MakeDomainO` has 3 fields — matches 3 output links
- `MakeEdgeO` has 3 fields — matches 3 output links
- `MakeSpaceO` has 2 fields — matches 2 output links
- All 4 edge `O` interfaces have 1 field each — matches 1 output link each

---

### Task 5: Update `marstech/env/prd.grd`

**Files:**
- Modify: `mvp/marstech/env/prd.grd`

Replace the two `deploy ... to aws as ...` blocks with `aws_deploy` first-class type instances.

- [ ] **Step 1: Replace prd-eu deploy block**

In `mvp/marstech/env/prd.grd`, replace:

```ground
deploy prd:marstech to aws as prd-eu {
    region: [
        eu-central:1
        eu-central:2
        eu-central:3
    ]

    main:       { size: medium  storage: 20 }

    api-gen:    { size: large   scaling: { min: 1  max: 4 } }
    hub:        { size: medium  scaling: { min: 1  max: 2 } }
    product-wf: { size: medium  scaling: { min: 1  max: 2 } }
    metrics:    { size: small   scaling: { min: 1  max: 1 } }
    media:      { size: medium  scaling: { min: 1  max: 2 } }
    pay:        { size: small   scaling: { min: 1  max: 2 } }
    core:       { size: medium  scaling: { min: 1  max: 2 } }
    hubspot:    { size: medium  scaling: { min: 1  max: 2 } }
    notify:     { size: medium  scaling: { min: 1  max: 2 } }
}
```

with:

```ground
aws_deploy prd-eu {
    stack:  marstech
    region: [ eu-central:1  eu-central:2  eu-central:3 ]

    databases: [
        { database: main  size: medium  storage: 20 }
    ]

    services: [
        { service: api-gen     size: large   scaling: { min: 1  max: 4 } }
        { service: hub         size: medium  scaling: { min: 1  max: 2 } }
        { service: product-wf  size: medium  scaling: { min: 1  max: 2 } }
        { service: metrics     size: small   scaling: { min: 1  max: 1 } }
        { service: media       size: medium  scaling: { min: 1  max: 2 } }
        { service: pay         size: small   scaling: { min: 1  max: 2 } }
        { service: core        size: medium  scaling: { min: 1  max: 2 } }
        { service: hubspot     size: medium  scaling: { min: 1  max: 2 } }
        { service: notify      size: medium  scaling: { min: 1  max: 2 } }
    ]
}
```

- [ ] **Step 2: Replace prd-me deploy block**

Replace:

```ground
deploy prd:marstech to aws as prd-me {
    region: [ me-central:1 ]

    main:       { size: small  storage: 20 }

    api-gen:    { size: medium  scaling: { min: 1  max: 2 } }
    hub:        { size: small   scaling: { min: 1  max: 1 } }
    product-wf: { size: small   scaling: { min: 1  max: 1 } }
    metrics:    { size: small   scaling: { min: 1  max: 1 } }
    media:      { size: small   scaling: { min: 1  max: 1 } }
    pay:        { size: small   scaling: { min: 1  max: 1 } }
    core:       { size: small   scaling: { min: 1  max: 1 } }
    hubspot:    { size: small   scaling: { min: 1  max: 1 } }
    notify:     { size: small   scaling: { min: 1  max: 1 } }
}
```

with:

```ground
aws_deploy prd-me {
    stack:  marstech
    region: [ me-central:1 ]

    databases: [
        { database: main  size: small  storage: 20 }
    ]

    services: [
        { service: api-gen     size: medium  scaling: { min: 1  max: 2 } }
        { service: hub         size: small   scaling: { min: 1  max: 1 } }
        { service: product-wf  size: small   scaling: { min: 1  max: 1 } }
        { service: metrics     size: small   scaling: { min: 1  max: 1 } }
        { service: media       size: small   scaling: { min: 1  max: 1 } }
        { service: pay         size: small   scaling: { min: 1  max: 1 } }
        { service: core        size: small   scaling: { min: 1  max: 1 } }
        { service: hubspot     size: small   scaling: { min: 1  max: 1 } }
        { service: notify      size: small   scaling: { min: 1  max: 1 } }
    ]
}
```

- [ ] **Step 3: Verify prd.grd no longer contains old deploy syntax**

Read `mvp/marstech/env/prd.grd`. Confirm:
- No `deploy ... to aws as ...` syntax remains
- Two `aws_deploy` blocks exist: `prd-eu` and `prd-me`
- Each has `stack: marstech`
- prd-eu lists 9 services + 1 database; prd-me lists 9 services + 1 database

---

### Task 6: Update `marstech/env/stg.grd`

**Files:**
- Modify: `mvp/marstech/env/stg.grd`

- [ ] **Step 1: Replace the stg-eu deploy block**

In `mvp/marstech/env/stg.grd`, replace:

```ground
deploy stg:marstech to aws as stg-eu {
    region: [ eu-central:1 ]

    main:    { size: small  storage: 20 }
    api-gen: { size: small  scaling: { min: 1  max: 1 } }
}
```

with:

```ground
aws_deploy stg-eu {
    stack:  marstech
    region: [ eu-central:1 ]

    databases: [
        { database: main  size: small  storage: 20 }
    ]

    services: [
        { service: api-gen  size: small  scaling: { min: 1  max: 1 } }
    ]
}
```

- [ ] **Step 2: Verify stg.grd**

Read `mvp/marstech/env/stg.grd`. Confirm:
- No `deploy ... to aws as ...` syntax remains
- One `aws_deploy stg-eu` block with `stack: marstech`, 1 database entry, 1 service entry

---

### Task 7: Add best practices section to `GROUND-BOOK.md`

**Files:**
- Modify: `GROUND-BOOK.md`

- [ ] **Step 1: Append the best practices section**

Open `GROUND-BOOK.md`. At the very end of the file, append:

```markdown
## Best practices

### Expressing phases through types

When resolution must happen in stages, declare the phase boundary as a type.

A common pattern: a deploy entity must produce networking infrastructure before
individual services can be expanded. Rather than declaring phases explicitly, make
the phase-1 output a named type with the produced artifacts as links. Phase-2
functions take that type as an arg — Ground derives ordering from the arg graph.

```ground
# Phase 1 — deploy produces infra
type (d: aws_deploy) = make_aws_deploy {
    link vpc             = aws_vpc
    link private_subnets = [ aws_subnet ]
    link cluster         = aws_ecs_cluster
}

# Phase 2 — service takes deploy (with infra already filled)
type (svc: service, d: aws_deploy) = make_service {
    link sg  = aws_security_group
    link ecs = aws_ecs_service
}
```

`make_service` takes `d: aws_deploy` as an arg. Ground knows `aws_deploy` must resolve
first. The dependency graph is the execution plan — no explicit `before`/`after` needed.

This is the same principle Terraform uses: attribute references are implicit dependencies.
In Ground, args are implicit dependencies.

The same pattern applies to edge rules after node rules: edge functions reference node
outputs (security group names, IAM role ARNs) by naming convention, so they implicitly
depend on node expansion having run.
```

- [ ] **Step 2: Verify GROUND-BOOK.md**

Read `GROUND-BOOK.md`. Confirm the new `## Best practices` section appears at the end
and contains the two Ground code examples.

---

## Self-Review

**Spec coverage:**
- ✓ std.grd additions (Task 1)
- ✓ aws_deploy in pack.grd (Task 2)
- ✓ transform.grd node + edge rules (Task 3)
- ✓ transform.ts I/O interfaces + stubs (Task 4)
- ✓ prd.grd updated (Task 5)
- ✓ stg.grd updated (Task 6)
- ✓ GROUND-BOOK best practices (Task 7)

**Naming consistency across tasks:**
- Hook names in transform.grd (Task 3) match function names in transform.ts (Task 4): ✓ all 12
- Output link names in transform.grd match field names in transform.ts `O` interfaces: ✓
- `aws_deploy` type defined in pack.grd (Task 2) and referenced in transform.grd (Task 3): ✓
- `database` type added in std.grd (Task 1) and referenced in transform.grd `(db: database, ...)`: ✓
- Sizing vocabulary (`small|medium|large|xlarge`) consistent across std.grd, transform.ts `ServiceConfig`/`DatabaseConfig`, prd.grd/stg.grd: ✓
