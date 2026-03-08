# ground_parse

Parses `.grd` source files into `ground_core::high::Spec`.

## Public API

```rust
pub fn parse(sources: &[(&str, &str)]) -> Result<Spec, Vec<ParseError>>
```

Accepts a slice of `(path, content)` pairs. Merges all services into a single `Spec`.

## Error model

`ParseError` carries exact source position (`path`, `line`, `col`) and a human-readable message. The return type is `Vec<ParseError>` — callers always receive **all** errors discovered, not just the first.

## Two-pass strategy

**Pass 1 — structural (pest grammar)**
The grammar is kept intentionally lenient: it validates syntax and structure only. A pest-level failure (input so malformed a tree cannot be built) produces one error for that file; parsing continues with the remaining files.

**Pass 2 — semantic (Rust validation)**
After a parse tree is produced, a separate validation pass checks invariants the grammar cannot express: missing required fields, duplicate fields, constraint violations (e.g. `scaling min > max`). All violations are collected before returning.

## Internal conventions

All internal parse functions follow the same contract:

```rust
type Parsed<T> = (Option<T>, Vec<ParseError>);
```

| Helper | Purpose |
|--------|---------|
| `ok(v)` | Infallible result — value, no errors |
| `fail(e)` | Structural failure — cannot continue parsing this item |
| `finish(value, errors)` | Seal a function: `Some(value)` if no errors, `None` otherwise |
| `merge(iter)` | Fan-in: fold `Iterator<Parsed<T>>` into `(Vec<T>, Vec<ParseError>)` |

Each function owns its local error accumulator and returns it alongside the value. No mutable state is passed through parameters. The caller combines results using `merge` or `finish`.
