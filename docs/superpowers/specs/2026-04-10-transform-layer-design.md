# Transform Layer Design

**Date:** 2026-04-10  
**Status:** Approved

---

## Context

Ground's pipeline:

```
std.grd              types & links          human-readable architecture
std/aws/pack.grd     vendor types & links   mirrors Terraform exactly
std/aws/transform.grd  type functions       declares input/output shapes
std/aws/transform.ts   pure TS functions    computation only
templates            foreach / if           dumb rendering
```

This doc specifies steps 2–3 of the plan: adding `deploy` as a first-class std type and
defining the transformation layer from `std` entities to `std:aws` entities.

---

## 1. `std.grd` additions

`deploy` becomes a first-class type alongside `service`, `database`, etc.
Separate typed maps for service and database sizing config.

```ground
type size = small | medium | large | xlarge

type scaling = {
    link min = int
    link max = int
}

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

type deploy = {
    link stack     = stack
    link region    = [ string ]
    link services  = [ service_config ]
    link databases = [ database_config ]
}
```

Updated deploy instantiation syntax in env files:

```ground
aws_deploy prd-eu {
    stack:   marstech
    region:  [ eu-central:1  eu-central:2  eu-central:3 ]
    services: [
        { service: api-gen  size: large   scaling: { min: 1  max: 4 } }
        { service: hub      size: medium  scaling: { min: 1  max: 2 } }
        ...
    ]
    databases: [
        { database: main  size: medium  storage: 20 }
    ]
}
```

---

## 2. `aws_deploy` in `std/aws/pack.grd`

`aws_deploy` extends `deploy` via application chaining — Ground's existing extension
mechanism. It inherits all std deploy links and adds AWS infra links filled by the
phase-1 expansion function.

```ground
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

---

## 3. `mvp/std/aws/transform.grd` — node and edge rules

All rules in one file. Node rules fire per entity; edge rules fire per access link pair.
Ordering is derived from the arg graph — no explicit phase declaration needed.

```ground
use std
use std:aws

# ── Phase 1 — deploy (fires first; produces infra links on aws_deploy) ────────

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

type (svc: service,  d: aws_deploy) = make_service {
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

**Ordering**: `(d: aws_deploy)` fires first because phase-2 functions take `d: aws_deploy`
as arg — Ground derives the dependency order from the arg graph. Edge rules fire after
node rules because they depend on node expansion outputs (sg names, role arns).

---

## 4. `mvp/std/aws/transform.ts` — TypeScript function signatures

Ground generates `I`/`O` interfaces; users implement the function bodies.
All functions are pure: typed input → typed output, no side effects.

```typescript
// Phase 1
interface MakeAwsDeployI { d: { stack: Stack; region: string[]; services: ServiceConfig[]; databases: DatabaseConfig[] } }
interface MakeAwsDeployO { vpc: AwsVpc; public_subnets: AwsSubnet[]; private_subnets: AwsSubnet[]; internet_gw: AwsInternetGateway; nat_gw: AwsNatGateway; lb: AwsLb; cluster: AwsEcsCluster; namespace: AwsServiceDiscoveryPrivateDnsNamespace }
export function make_aws_deploy(i: MakeAwsDeployI): MakeAwsDeployO { ... }
// computation: CIDR allocation, subnet-per-AZ arithmetic, NAT gateway placement

// Phase 2 — node rules
interface MakeServiceI { svc: { name: string; port: 'grpc'|'http'; access: AccessLink[]; observe: Observe }; d: { vpc: AwsVpc; private_subnets: AwsSubnet[]; cluster: AwsEcsCluster; services: ServiceConfig[] } }
interface MakeServiceO { sg: AwsSecurityGroup; ecs: AwsEcsService; task_def: AwsEcsTaskDefinition; exec_role: AwsIamRole; task_role: AwsIamRole; log: AwsCloudwatchLogGroup }
export function make_service(i: MakeServiceI): MakeServiceO { ... }

interface MakeDatabaseI { db: { name: string; engine: 'postgres'|'mysql' }; d: { vpc: AwsVpc; private_subnets: AwsSubnet[]; databases: DatabaseConfig[] } }
interface MakeDatabaseO { sg: AwsSecurityGroup; rds: AwsDbInstance; subnet_group: AwsDbSubnetGroup }
export function make_database(i: MakeDatabaseI): MakeDatabaseO { ... }

interface MakeSecretI  { sec: { name: string }; d: { vpc: AwsVpc } }
interface MakeSecretO  { sm: AwsSecretsmanagerSecret }
export function make_secret(i: MakeSecretI): MakeSecretO { ... }

interface MakeBucketI  { bkt: { name: string; access: 'read'|'write' }; d: { vpc: AwsVpc } }
interface MakeBucketO  { s3: AwsS3Bucket }
export function make_bucket(i: MakeBucketI): MakeBucketO { ... }

interface MakeDomainI  { dom: { host: string }; d: { lb: AwsLb } }
interface MakeDomainO  { cert: AwsAcmCertificate; validation: AwsAcmCertificateValidation; zone: AwsRoute53Zone }
export function make_domain(i: MakeDomainI): MakeDomainO { ... }

interface MakeEdgeI    { edg: { domain: Domain; sub: string; backend: Service }; d: { lb: AwsLb } }
interface MakeEdgeO    { tg: AwsLbTargetGroup; rule: AwsLbListenerRule; record: AwsRoute53Record }
export function make_edge(i: MakeEdgeI): MakeEdgeO { ... }

interface MakeSpaceI   { sp: { host: string; services: Service[] }; d: { vpc: AwsVpc } }
interface MakeSpaceO   { cluster: AwsEcsCluster; namespace: AwsServiceDiscoveryPrivateDnsNamespace }
export function make_space(i: MakeSpaceI): MakeSpaceO { ... }

// Edge rules
interface MakeServiceDatabaseAccessI { from: { name: string }; to: { name: string }; d: { vpc: AwsVpc } }
interface MakeServiceDatabaseAccessO { ingress: AwsVpcSecurityGroupIngressRule }
export function make_service_database_access(i: MakeServiceDatabaseAccessI): MakeServiceDatabaseAccessO { ... }

interface MakeServiceServiceAccessI  { from: { name: string }; to: { name: string; port: 'grpc'|'http' }; d: { vpc: AwsVpc } }
interface MakeServiceServiceAccessO  { ingress: AwsVpcSecurityGroupIngressRule }
export function make_service_service_access(i: MakeServiceServiceAccessI): MakeServiceServiceAccessO { ... }

// from.name used to derive task role by convention: `${from.name}-task-role`
interface MakeServiceSecretAccessI   { from: { name: string }; to: { name: string }; d: { vpc: AwsVpc } }
interface MakeServiceSecretAccessO   { policy: AwsIamRolePolicy }
export function make_service_secret_access(i: MakeServiceSecretAccessI): MakeServiceSecretAccessO { ... }

// to.access used to derive S3 policy actions (read → GetObject, write → PutObject)
interface MakeServiceBucketAccessI   { from: { name: string }; to: { name: string; access: 'read'|'write' }; d: { vpc: AwsVpc } }
interface MakeServiceBucketAccessO   { policy: AwsIamRolePolicy }
export function make_service_bucket_access(i: MakeServiceBucketAccessI): MakeServiceBucketAccessO { ... }
```

---

## 5. File layout

```
mvp/
  std.grd                       ← add: size, scaling, service_config, database_config, deploy
  std/aws/
    pack.grd                    ← add: aws_deploy (extends deploy via application)
    transform.grd               ← new: 8 node rules + 4 edge rules
    transform.ts                ← new: 12 pure TS functions (I/O interfaces)
  marstech/
    env/prd.grd                 ← update: deploy syntax → aws_deploy first-class type
    env/stg.grd                 ← update: same
GROUND-BOOK.md                  ← add: best practices — phases via types
```

---

## 6. GROUND-BOOK best practices: phases via types

> **Expressing phases through types**
>
> When resolution must happen in stages — deploy infra before entity expansion, node
> expansion before edge expansion — declare the phase boundary as a type.
>
> The phase-1 output becomes a named type (e.g. `aws_deploy` with its infra links
> filled). Phase-2 functions take that type as an arg, and Ground derives the ordering
> from the arg graph. No explicit phase declaration. No separate orchestration engine.
> The dependency graph is the execution plan.
>
> This is the same principle Terraform uses: attribute references are implicit
> dependencies. In Ground, args are implicit dependencies.

---

## Key properties

- No rule sees the whole tree — each function has a fixed, small, explicit input
- Cross-entity side effects are first-class: `(from: service, to: database)` makes explicit
  that *this pair* produces *this resource*
- TypeScript functions are small, independently testable, pure — no framework to understand
- Phases fall out of the type system; no explicit ordering needed
