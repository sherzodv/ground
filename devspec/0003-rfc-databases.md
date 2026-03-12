# RFC 0003 — Databases

## Problem

Ground's system model is incomplete. A service that connects to a database is
a core architectural relation — but today Ground has no way to express it. The
database is invisible to the system description, so the connection, the access
rules, and the credentials are all left to the user to wire manually.

---

## Goals

- `database` as a first-class system element alongside `service`.
- `access` block as the relation between a service and a database (same block used for service-to-service).
- All derived details — networking, access rules, credentials, HA — follow
  from the system description. The user declares none of them.

---

## New primitive: `database`

```ground
database db-main {
  engine:  postgres
  version: 15
  size:    small
}
```

Fields:

| Field     | Required | Values                                        |
|-----------|----------|-----------------------------------------------|
| `engine`  | yes      | `postgres` \| `mysql`                         |
| `version` | no       | integer, e.g. `15`                            |
| `size`    | no       | `small` \| `medium` \| `large` \| `xlarge`   |
| `storage` | no       | integer (GB), default 20                      |
| `compute` | no       | ref to a `compute` block; `aws` field used as RDS instance class |

`size` is a Ground-native wrapper — provider mapping:

| Ground   | AWS             |
|----------|-----------------|
| `small`  | `db.t3.micro`   |
| `medium` | `db.t3.medium`  |
| `large`  | `db.r6g.large`  |
| `xlarge` | `db.r6g.xlarge` |

Default `size` is `small`. If a `compute` ref is provided its `aws` field overrides `size`.

---

## Relation: `access`

A service declares access to a database using the same `access` block used for
service-to-service connections. Ground resolves the target name to its type at
compile time. Database entries have no port annotations — the port is derived
from the engine.

```ground
service svc-api {
  image:  svc-api:prod
  access {
    main
    svc-internal: http, grpc
  }
}
```

Entries without `: ports` that resolve to a database get DB access rules.
Entries without `: ports` must appear before entries with ports in the same
block (grammar constraint: the port list is greedy).

Ground derives from a service→database access entry:
- `aws_vpc_security_group_ingress_rule` from the service SG to the database SG on the engine port
- DB connection details injected as environment variables into the service's ECS task definition:

```
<DB_NAME>_HOST
<DB_NAME>_PORT
<DB_NAME>_NAME
<DB_NAME>_USER
<DB_NAME>_PASSWORD
```

Variable names are derived from the database name (uppercased, hyphens →
underscores). Username is `ground`. Password is a `random_password` resource.

---

## Group membership

Databases are listed in `group` alongside services.

```ground
group backend {
  svc-api
  db-main
}
```

Ground resolves each name to its type. No new `group` syntax required.

---

## High availability

Derived from the stack's zone count:

- 1 zone → `multi_az = false`
- ≥ 2 zones → `multi_az = true`

---

## Example

```ground
database main {
  engine:  postgres
  version: 15
  size:    small
}

service svc-api {
  image:  svc-api:prod
  access {
    main
    svc-internal: http, grpc
  }
}

service svc-internal {
  image: svc-internal:prod
  ports: { http: 8080, grpc: 9090 }
}

group backend {
  svc-api
  svc-internal
  main
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

Two zones → `multi_az = true`. `svc-api` gets DB connection env vars injected,
network access rule generated.

---

## Derived AWS resources

| Derived from              | AWS resources                                                                 |
|---------------------------|-------------------------------------------------------------------------------|
| `database` in group       | `random_password`, `aws_db_subnet_group`, `aws_security_group` (+ egress rule), `aws_db_instance` |
| service→database in access | `aws_vpc_security_group_ingress_rule` (service SG → db SG on engine port)   |

---

## Layer map

| Layer | Crate | Change |
|-------|-------|--------|
| `high::Rdb`, `high::Service.access` | `ground_core` | `Rdb` struct + `AccessEntry` resolves to rdb |
| `low::ManagedRdb`, `low::DbAccessRule`, `low::Workload.rdb_access` | `ground_core` | new structs + field |
| `database_def` | `ground_parse` | grammar + parse |
| `compile_rdb`, resolve access entries to rdb | `ground_core::compile` | new logic |
| `gen_rdb`, `gen_db_access_rule`, `db_env_vars` | `ground_be_terra::terra_gen::aws` | new generators |
