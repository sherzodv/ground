# RFC 0015 — Type Functions

## Problem

RFC 0014 introduced `gen` blocks as a special construct for output structure expansion. The construct solves a real problem but at the wrong layer — it is ad-hoc syntax bolted on top of a type system that already contains the necessary primitives. The result is a parallel value system (`IrRefExpr`, `IrGenValue`), parallel IR/ASM structures (`IrGenDef`, `AsmGenDef`), implicit magic keywords (`{this:...}`), and implicit type-based lookup at deploy time.

## Core insight

Everything is a type function. A type definition is a function with zero parameters. A structure transformer is a function with one or more parameters. Link pair expansion is a 2-param type function — no separate `link` definition form exists. Definition syntax is uniform across all arities. Arity, auto-fire, and pair enumeration are application-layer concerns, not definition-layer constraints.

## Type function definition

A type function has an optional name, an optional parameter list, and a body. The `link` keyword inside type struct bodies is unchanged. What is removed is `link` as a separate *function definition* form.

**Named, zero params — plain type:**
```
type service = {
    link port   = integer
    link access = [ service ]
}
```

**Named, N params:**
```
type service-gen(this: service) = {
    aws_sg   { name: {this:name}-sg }
    aws_task { port: {this:port}    }
}
```

**Anonymous, 1 param — fires for every instance of matching type during walk:**
```
type (this: service) = {
    aws_sg   { name: {this:name}-sg }
    aws_task { port: {this:port}    }
}
```

**Anonymous, 2 params — fires for every (from, to) pair of matching types found across links during walk:**
```
type (from: service, to: service) = {
    aws_sg_rule { from: {from:name}, to: {to:name} }
}
```

**Named, 2 params — not fired automatically; selected via override at the application site:**
```
type connect(from: service, to: service) = {
    aws_sg_rule { from: {from:name}-sg, to: {to:name}-sg }
}
```

The definition makes no distinction between arities beyond the presence or absence of parameters.

## Application binding rules

Arity determines how a type function is bound during a walk. These are application-level rules:

| Arity | Named | Behavior |
|---|---|---|
| 0 | yes | Instantiation — `type name { fields }` |
| N | yes | Explicit application — `fn-ref(arg1, ...)` |
| 1 | no | Auto-fires for every instance of the matching param type |
| 2 | no | Auto-fires for every `(from, to)` pair of matching param types found across link fields |

**Ambiguity rule:** if an anonymous N-param type function matches multiple link slots (same type signature, multiple links), it is an error. Resolve via named override at the application site.

Multiple anonymous type functions with the same param type signature in the same pack is an error.

## Application

```
aws:ecs:gen(boo) {
    service:access: connect
}
```

The body is an optional list of `ref: ref` overrides. Two forms, resolved unambiguously:

- **Field path** — left side resolves as a field on the applied type: `scaling:min: 2`
- **Link slot** — left side resolves to a link slot, selecting a named type function for pair expansion: `service:access: connect`

Resolution is unambiguous — the ref is walked until it resolves to either a field value or a link slot. Error if it resolves to neither or is ambiguous.

## Recursive walk semantics

Application of a type function with N≥1 params triggers a recursive structural walk:

1. Fire the named or anonymous 1-param type function for the argument's type
2. For each link field in the instance, walk its values
3. If a value's type has a matching anonymous 1-param type function in scope — fire it, recurse
4. For each link field, enumerate all `(from, to)` pairs of matching types; fire the anonymous 2-param type function (or named override) for each pair
5. Stop on cycles

Overrides are threaded through the entire walk. An override specified at the application site applies at every depth.

## Grammar

```
# type function (named, zero params)
type <name> = <body>

# type function (named, N params)
type <name>(<param>: <type>, ...) = <body>

# type function (anonymous, N params)
type (<param>: <type>, ...) = <body>

# instance — application of zero-param type function
<type> <name> { <field>: <value> ... }

# application
<type-fn-ref>(<arg>, ...) { <ref>: <ref> ... }
```

The `link` keyword inside type struct bodies (`link name = type-expr`) is unchanged — it declares a typed slot on a type. What is removed is `link` as a *function definition* keyword (`link type:slot name(from, to) = { ... }`). Link expansion is now expressed as a 2-param type function.

## ASM — expansion semantics

The ASM layer expands all type-function applications to maximum depth, stopping only on cycles. By the time ASM hands off to ground_gen:

- All parameter references (`{param:field}`) are substituted with concrete values
- All 2-param pair expansions are enumerated and expanded
- Each deploy produces one rooted `AsmInst` tree — the deploy root is the top node, expansion outputs are child nodes
- No template residue

## ground_gen

ground_gen takes the `AsmInst` tree per deploy and renders it into output (e.g. Terraform JSON). It has no knowledge of the language core — it only sees concrete nodes and edges.

For each deploy:
1. Walk the `AsmInst` tree
2. For each node: `type_name` → Tera template → render with node fields
3. Merge rendered outputs into one document per deploy

No expression evaluation. All values are already resolved by the compiler.

## Terra backend

The terra backend is ground_gen with predefined Tera templates. One Terraform JSON per deploy. No logic beyond template rendering and schema validation.

## Evolution from RFC 0014

RFC 0014 introduced ad-hoc `gen` constructs as a temporary solution. This RFC supersedes them with a principled functional model — the same expressive power, no special cases.

| RFC 0014 | RFC 0015 |
|---|---|
| `gen` keyword | anonymous / named type function |
| `AstGenDef` | `AstTypeDef` with parameter list |
| `IrGenDef` / `AsmGenDef` | unified `IrTypeFnDef` — `params: Vec<(name, TypeId)>`, `name: Option<String>`; arity 0 = plain type, no separate `IrTypeDef` |
| `IrRefExpr` / `IrGenValue` | ordinary `IrValue` field traversal |
| `{this:...}` implicit binding | named parameter, explicit field ref |
| implicit type-based gen lookup | anonymous type function + recursive walk |
| separate link function machinery | 2-param type function + pair enumeration in walk |
| deferred expression evaluation in backend | full expansion at ASM layer |

Vendor type declarations (e.g. `type aws_sg = { link name = string }`) are preserved — they are zero-param type functions serving as schemas for vendor output structure, template lookup, and full type checking of type function bodies.
