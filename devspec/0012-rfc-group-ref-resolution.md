# RFC 0012 — Group Ref Resolution

## Mechanism

Refs containing `{}` expressions are resolved in two steps:

1. **Reduction** — each `{inner_ref}` segment is resolved to its plain value; the result is an all-plain ref
2. **Resolve** — the all-plain ref goes through the existing resolve pass unchanged

Reduction happens before the resolve pass. Refs stay refs — `{}` is not a new value kind, it is a substitution mechanism for ref segments.

## `{this:xxx}` semantics

`this` is the current instance (the one the field belongs to).
`xxx` is a link name on its type.
`{this:xxx}` reduces to the plain value of that link on the current instance.

```
# image ref uses the instance's own id as its first segment
type service = {
    link id    = string
    link image = reference
}
service svc-a { id: "api"  image: {this:id}:latest }
# reduction:  {this:id} → "api"
# resolve:    image = Ref("api:latest")
```

```
# security group name derived from id with a suffix
type service = {
    link id      = string
    link sg-name = reference
}
service svc-a { id: "api"  sg-name: {this:id}-sg }
# reduction:  {this:id}-sg → "api-sg"   (trailing atom concatenated)
# resolve:    sg-name = Ref("api-sg")
```

```
# typed-path field: region segment comes from the instance's own region field
type region  = eu-central | eu-west
type zone    = 1 | 2 | 3
type service = {
    link region   = region
    link location = type:region:type:zone
}
service svc-a { region: eu-central  location: {this:region}:2 }
# reduction:  {this:region} → "eu-central"
# resolve:    location = Variant(region,"eu-central"):Variant(zone,"2")
```

## Resolution table

Columns: source value → currently resolves to → should resolve to after reduction.
All "should be" entries assume a successful reduction to the equivalent plain value shown in the adjacent plain row.

| Link type | Value | Currently | Should be |
|---|---|---|---|
| `Prim(integer)` | `42` | `Int(42)` | — |
| `Prim(integer)` | `"42"` | `Int(42)` | — |
| `Prim(integer)` | `{this:count}` | error: `'' is not integer` | same as plain equivalent |
| `Prim(integer)` | `{this:count}-1` | error: `'' is not integer` | same as plain equivalent |
| `Prim(string)` | `"hello"` | `Str("hello")` | — |
| `Prim(string)` | `grpc` | `Str("grpc")` | — |
| `Prim(string)` | `{this:label}` | `Str("")` | same as plain equivalent |
| `Prim(reference)` | `"nginx"` | `Ref("nginx")` | — |
| `Prim(reference)` | `org:repo:tag` | `Ref("org:repo:tag")` | — |
| `Prim(reference)` | `{role:arn}` | `Ref("")` | same as plain equivalent |
| `Prim(reference)` | `{this:id}-sg` | `Ref(":-sg")` | same as plain equivalent |
| `Ref(Enum(T))` | `grpc` | `Variant(T, "grpc")` | — |
| `Ref(Enum(T))` | `{this:zone}` | error: `'' is not a variant` | same as plain equivalent |
| `Ref(Struct(T))` | `my-db` | `Inst(Inst#N)` | — |
| `Ref(Struct(T))` | `{ min: 1 }` | `Inst(Inst#N)` (anonymous) | — |
| `Ref(Struct(T))` | `{this:db}` | error: `'' is not a known instance` | same as plain equivalent |
| `Ref([Enum(A):Enum(B)])` | `eu-central:2` | `Variant(A,"eu-central"):Variant(B,"2")` | — |
| `Ref([Enum(A):Enum(B)])` | `{this:region}:{this:zone}` | error: both segs `→ ""` | same as plain equivalent |
| `Ref([Enum(A):Enum(B)])` | `eu-central:{this:zone}` | error: `""` not a variant of B | same as plain equivalent |
| `List([Ref(Struct(T))])` | `svc-a` | `List[Inst(Inst#N)]` | — |
| `List([Ref(Struct(T)):Enum(P)])` | `svc-a:grpc` | `List[Inst(Inst#N):Variant(P,"grpc")]` | — |
| `List([Ref(Struct(T))])` | `{sg:id}` (item) | error: `""` fails inst lookup | same as plain equivalent |
| `List([Ref(Struct(T)):Enum(P)])` | `{sg:id}:grpc` (item) | error: `""` fails inst lookup | same as plain equivalent |

## Scope

- Add a reduction pass in `resolve.rs` that runs before ref resolution
- Reduction is recursive: the inner ref of a Group is itself resolved, then the result substituted as a plain segment
- Trailing atom after a Group (`{x:y}-sg`) is concatenated with the reduced value into a single plain segment
- No AST or IR struct changes needed
