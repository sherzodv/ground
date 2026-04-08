# RFC 0017 — Scaling resources via nested field access and default values

## Problem

`aws_appautoscaling_target` and `aws_appautoscaling_policy` are declared in stdlib but never emitted. Three blockers:

1. **No nested field access in `{}`** — type fn bodies need `{svc.scaling.min}` but the substitution engine only handles one level: `{param:field}`.
2. **Wrong notation** — current code uses `:` inside `{}` (`{param:field}`); ground-book establishes `.` as the field accessor inside expressions.
3. **No default values on links** — if a service omits `scaling`, substitution has nothing to resolve. Without defaults the only option is a compile error, which is acceptable but requires the guarantee to be enforced.

## Assumptions

- Either a default value is present on the link, or the user provides a value — no third option. Missing required field with no default = compile error at resolve time.
- Dot notation `.` inside `{}` is the canonical accessor going forward. The old `:` accessor inside `{}` is replaced.

---

## Change 1 — Dot notation inside `{}`

**Before:** `{svc:name}`, `{svc:image}`
**After:** `{svc.name}`, `{svc.image}`

Applies everywhere `{}` expressions appear: type fn bodies, ref expressions, inline struct fields.

The parser's group segment (`AstRef` segment containing `{inner}`) currently stores `inner` as a colon-separated string. Change the inner accessor delimiter to `.`.

Migration: update all existing stdlib.grd and golden test fixtures. No semantic change — purely syntactic.

---

## Change 2 — Nested field access in `{}`

Extend the `{}` expression resolver to walk dot-separated paths of arbitrary depth.

**Example:**
```ground
type (svc: service) = {
    at: aws_appautoscaling_target { name: {svc.name}-at  min: {svc.scaling.min}  max: {svc.scaling.max} }
}
```

### Resolver semantics (`substitute_value`)

Current: `{param.field}` → look up `field` in `param`'s inst fields → return `AsmValue`.

Extended: `{param.a.b.c}` → resolve `a` in param → if result is `AsmValue::Inst`, resolve `b` in that inst → repeat until leaf. Error if any intermediate value is not an `Inst` or the field is absent.

Intrinsic `{param.name}` → `AsmValue::Str(inst.name)` — unchanged, just `.` instead of `:`.

---

## Change 3 — Default values on links

Allow a link declaration to carry a default value:

```ground
type scaling = {
    link min = integer
    link max = integer
    min: 1
    max: 5
}
```

Or equivalently (type inferred from literal):

```ground
type scaling = {
    link min = 1
    link max = 5
}
```

### Semantics

- Default is applied at resolve time when the instance does not provide the field.
- If a field has no default and is not provided at the instance site → compile error.
- Defaults are resolved in the context of the type, not the instance (no `this` refs in defaults for now).

### IR changes

`IrLinkDef` gains `default: Option<IrValue>`. Resolve pass fills it from the type body field assignments. Instance resolution merges provided fields over defaults.

### AST changes

`AstTypeDef` struct body allows field assignments alongside `link` declarations (already partially supported as `AstInst` fields — reuse that path).

---

## Change 4 — Scaling resources in stdlib

Once changes 1–3 land, extend the service type function:

```ground
type scaling = {
    link min = 1
    link max = 1
}

type (svc: service) = {
    t:    aws_iam_role                        { name: {svc.name}-t }
    x:    aws_iam_role                        { name: {svc.name}-x }
    xa:   aws_iam_role_policy_attachment      { name: {svc.name}-x }
    sgs:  aws_security_group                  { name: {svc.name}-sgs }
    sgse: aws_vpc_security_group_egress_rule  { name: {svc.name}-sgs }
    log:  aws_cloudwatch_log_group            { name: {svc.name}-log }
    td:   aws_ecs_task_definition             { family: {svc.name}-td  container: {svc.name}  image: {svc.image}  x_role: {svc.name}-x  t_role: {svc.name}-t  log: {svc.name}-log }
    svc:  aws_ecs_service                     { name: {svc.name}-svc  td: {svc.name}-td  sg: {svc.name}-sgs }
    at:   aws_appautoscaling_target           { name: {svc.name}-at  min: {svc.scaling.min}  max: {svc.scaling.max} }
    ap:   aws_appautoscaling_policy           { name: {svc.name}-ap  target: {svc.name}-at }
}
```

The `scaling` link on `service` uses `scaling` type with defaults `min: 1  max: 1`, so services without an explicit `scaling` block still compile and emit autoscaling with sentinel values.

---

## Tera templates

Two new templates required in `ground_be_terra/src/templates/`:

- `aws_appautoscaling_target.json.tera` — registers the ECS service as a scalable target; uses `output.min`, `output.max`, `output.name`.
- `aws_appautoscaling_policy.json.tera` — target tracking policy (CPU or request-based); uses `output.name`, `output.target`.

Both must include `"ground-managed": "true"` tag and follow the `{pfx_u}{alias_u}_{name_u}` resource ID convention.

---

## Affected components

| Component | Change |
|---|---|
| `parse.rs` | Group segment inner delimiter `:` → `.` |
| `ast.rs` | No structural change (delimiter is in string content) |
| `resolve.rs` | `IrLinkDef` default field; default resolution at instance site; compile error on missing required field |
| `ir.rs` | `IrLinkDef { default: Option<IrValue> }` |
| `asm.rs` | `substitute_value`: dot-path traversal; `{param.name}` intrinsic with `.` |
| `stdlib.grd` | Dot notation migration; `scaling` defaults; `at`/`ap` entries in service type fn |
| `ground_be_terra/src/lib.rs` | `load_template` dispatch for `aws_appautoscaling_target`, `aws_appautoscaling_policy` |
| `ground_be_terra/src/templates/` | Two new Tera templates |
| Golden test fixtures | Dot notation update; `0002-scaling.md` expected JSON gains `at`/`ap` resources |

---

## Non-goals

- Conditional emission (emit scaling only if `scaling` is explicitly set) — deferred; defaults cover the practical case.
- Multi-`{}` string interpolation (e.g. `{from.name}-to-{to.name}`) — separate RFC.
- `this` keyword in defaults — deferred.
