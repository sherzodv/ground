# RFC 0013 — Enum Ref Resolution

## Problem

Enum variant refs are parsed as single bare atoms and stored as plain strings in the IR.
Multi-segment refs such as `type:foo` fail at parse time because `type` is a keyword:

```
type boo = type:foo | type:goo
# ERR: unexpected token at top level: Some(':')
```

This blocks ADT sum types where variants name other struct types.

## Requirements

1. `type boo = type:foo | type:goo` must parse without error
2. Enum variant refs resolve through the same mechanism as other refs
3. The IR stores resolved refs per variant, not plain strings
4. Values resolve against typed variants; type hints required only when the value is ambiguous
5. Plain atom variants remain valid and unchanged (`type region = eu-central | eu-west`)

## Approach

### 1. Parser — `parse_enum_body()` in `parse.rs`

Currently calls `parse_atom_as_ref()` which produces a single-segment plain ref.

Change each variant arm to use `parse_ref()` instead. `parse_ref()` already handles
keyword prefix segments (`type`, `link`, `pack`), colon-separated paths, optional
segments `(foo)`, and brace-group segments `{this:x}`. Same function used by
`parse_type_expr()` for link type bodies.

**Targeted change:** swap `parse_atom_as_ref()` → `parse_ref()` inside `parse_enum_body()`.

### 2. IR — `ir.rs`

**`IrTypeBody::Enum`:**
```rust
// before
Enum(Vec<String>)

// after
Enum(Vec<IrRef>)
```

Plain atom variants become single-segment `IrRef { segments: [Plain("grpc")] }`.
Typed ref variants become `IrRef { segments: [Type(TypeId)] }`.
No new IR types — `IrRef` already exists.

**`IrValue::Variant`:**
```rust
// before
Variant(TypeId, u32)

// after
Variant(TypeId, u32, Option<Box<IrValue>>)
```

Plain variant: `Variant(enum_tid, idx, None)` — pure discriminant, no payload.
Typed variant: `Variant(enum_tid, idx, Some(inner))` — inner is `Inst(iid)` for
named instance refs or an anonymous inst for struct literals.

### 3. Resolver — type body in `resolve.rs`

In `resolve_type_body()`, change enum arm to call `resolve_ref()` per variant:

```rust
AstTypeDefBody::Enum(refs) => {
    let variants = refs.iter()
        .map(|r| resolve_ref(&r.inner.segments, ctx, scope))
        .collect();
    IrTypeBody::Enum(variants)
}
```

### 4. Value resolution — `resolve_single_seg_value()` in `resolve.rs`

When the link type resolves to `Enum(variants)`, iterate variants to find a match.

**Plain variant** (`IrRef { Plain("active") }`): string-match against raw value.
Produces `Variant(enum_tid, idx, None)`. Unchanged from current behavior.

**Typed variant** (`IrRef { Type(tid) }`):

| Value form | Hint needed? | Resolution |
|---|---|---|
| Named ref (`my-num`) | No | Look up instance → check instance type == `tid` → `Variant(enum_tid, idx, Some(Inst(iid)))` |
| Struct with hint (`Num { ... }`) | Provided | Resolve hint → check hint type == `tid` → resolve struct as anon inst of `tid` → `Variant(enum_tid, idx, Some(Inst(anon_iid)))` |
| Struct without hint (`{ ... }`) | Required | Error: `type hint required` |

For named refs the variant is inferred by looking up the instance's type — no hint
needed because the type is unambiguous. For struct literals the type cannot be inferred
without a hint.

**Matching order:** plain variants matched first; typed variants tried in declaration order.

**Errors:**
- No variant matched → `not a variant of {enum_type_name}`
- Struct literal, no hint, typed variants present → `type hint required for {enum_type_name}`
- Hint resolves to a type not in the enum → `{hint} is not a variant of {enum_type_name}`

## Impact

| Layer | Change | Scope |
|---|---|---|
| `parse.rs` `parse_enum_body()` | `parse_atom_as_ref` → `parse_ref` | ~1 line |
| `ir.rs` `IrTypeBody::Enum` | `Vec<String>` → `Vec<IrRef>` | type change, cascades |
| `ir.rs` `IrValue::Variant` | add `Option<Box<IrValue>>` payload | cascades into gen/asm |
| `resolve.rs` `resolve_type_body()` | extract first segment → `resolve_ref()` | ~5 lines |
| `resolve.rs` `resolve_single_seg_value()` | extend enum match logic | new typed-variant branch |

`IrValue::Variant` payload cascades into gen/asm consumers — all existing match arms
on `Variant(tid, idx)` need updating to `Variant(tid, idx, _)`. Plain variant behavior
is preserved (`None` payload).

## Tests

### Parse — `golden_parse_test.rs`, section: Types

Format: typed variants remain as refs in the parse output, no resolution yet.

| Test | Input | Expected output |
|---|---|---|
| `type_enum_typed_ref_variants` (update — currently records parse error) | `type boo = type:foo \| type:goo` | `Type[boo, Enum[Ref(type:foo) \| Ref(type:goo)]]` |
| `type_enum_mixed_plain_and_typed_ref` | `type boo = plain \| type:foo` | `Type[boo, Enum[Ref(plain) \| Ref(type:foo)]]` |

### IR type body — `golden_ir_test.rs`, section: Types

Format: typed variants render as `Struct(Type#N)` or `Enum(Type#N)` — same segment
notation as in `IrRef[...]` for link types.

| Test | Source | Expected IR |
|---|---|---|
| `enum_typed_struct_ref_variants` | `type num = { link val = integer }` `type add = { link lhs = string }` `type expr = type:num \| type:add` | `Type#2[expr, Enum[Struct(Type#0)\|Struct(Type#1)]]` |
| `enum_mixed_plain_and_typed_ref` | `type leaf = { link val = integer }` `type tree = leaf-val \| type:leaf` | `Type#1[tree, Enum[leaf-val\|Struct(Type#0)]]` |
| `enum_typed_enum_ref_variant` | `type zone = 1 \| 2 \| 3` `type loc = type:zone` | `Type#1[loc, Enum[Enum(Type#0)]]` |

### IR type body errors — `golden_ir_error_test.rs`, section: Type resolution errors

| Test | Source | Error must mention |
|---|---|---|
| `error_typed_enum_variant_unknown_type` | `type boo = type:nonexistent \| type:goo` | `nonexistent` |

### IR value resolution — `golden_ir_test.rs`, section: Instance fields

Format: plain variant stays `Variant(Type#N, "name")`. Typed variant with payload:
`Variant(Type#N, Inst(Inst#M))` for named ref, `Variant(Type#N, Inst#_)` for anonymous
struct literal.

| Test | Scenario | Expected field value |
|---|---|---|
| `inst_typed_enum_variant_named_ref` | Named instance ref against typed variant — no hint needed, variant inferred from instance type | `Variant(Type#expr, Inst(Inst#0))` |
| `inst_typed_enum_variant_struct_with_hint` | Struct literal with type hint | `Variant(Type#expr, Inst#_)` |
| `inst_typed_enum_variant_disambiguates_by_inst_type` | Two typed variants, named ref selects correct variant by instance type | `Variant(Type#expr, Inst(Inst#0))` at variant index 0 |
| `inst_plain_variant_in_mixed_enum_unchanged` | Plain string value in enum that also has typed variants | `Variant(Type#N, "plain-val")` — no payload, format unchanged |

### IR value resolution errors — `golden_ir_error_test.rs`, section: Instance field errors

| Test | Scenario | Error must mention |
|---|---|---|
| `error_typed_enum_struct_without_hint` | Struct literal against typed variant, no hint provided | `type hint required` |
| `error_typed_enum_wrong_hint` | Hint type exists but is not in the enum's variants | hint name + `not a variant of` |
| `error_typed_enum_instance_wrong_type` | Named instance whose type matches no variant | instance or type name |
