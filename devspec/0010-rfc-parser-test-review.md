# RFC 0010 — Parser Golden Test Review

## Findings

### 1. Order

Current grouping is mostly reasonable but has scattered exceptions:

| Issue | Tests affected |
|---|---|
| `ref_optional_segment`, `ref_multi_segment_value` placed after error/integration tests | should precede inst tests (refs are inst field values) |
| `inst_inline_struct_value`, `inst_struct_as_field_value` placed after `multiple_defs` / `stdlib_subset` | should precede integration tests |
| `inline_named_type_with_typed_path_ref` (complex integration) mixed with unit inst tests | belongs with regression group |
| Error tests split between two locations | should be a single group |

Proposed order for `golden_parse_test.rs`:
1. Basics (empty, comment)
2. Types (enum → struct variants)
3. Links (primitive → inline named)
4. Refs (plain → optional seg → typed path → brace group)
5. Instances (no fields → fields → inline struct → anon → type hints)
6. Deploy
7. Use
8. Multi-unit scope
9. Regression / integration (`multiple_defs`, `stdlib_subset`)

Error tests move to `golden_parse_error_test.rs` (separate file, see proposed changes).

---

### 2. Coverage gaps

**Refs**
- `{a:b}:extra` — brace group followed by colon-separated plain segment: not tested
- `{a:b}{c:d}` — adjacent brace groups: not tested

**Instances**
- Struct body containing a brace-group ref in a nested list: `cfg: { ids: [{sg:id}] }` — not tested
- Inst field value that is a multi-segment brace group ref with trailing, inside a list: `[{this:name}-sg]` — not tested

**Errors**
- Only 2 error tests, both top-level parse failures. Missing:
  - Unclosed struct body: `svc my-svc { image: svc-api`
  - Unclosed list: `svc my-svc { access: [ svc-b`
  - Unclosed brace group ref: `svc my-svc { name: {this:name }`
  - Missing inst name: `service {}`

**Deploy**
- `deploy_with_fields` — only one field; multi-field deploy not tested (low priority)

**Type hints**
- Type hint on nested inline struct (hint on a struct that is itself a field value of an outer struct): not tested

---

### 3. Redundancy

| Test | Issue |
|---|---|
| `multiple_defs` | Largely a subset of `stdlib_subset`; the only unique value is asserting mixed def kinds parse in order — worth keeping but can be trimmed |
| `link_primitive` | Tests all 3 primitives in one function — fine as a grouped unit test, no action needed |
| `brace_group_ref_simple` / `brace_group_ref_with_trailing` / `brace_group_ref_in_list` | Each has a full type+inst scaffolding that is not under test; scaffolding can be reduced to the minimal type definition needed |
| `inst_duplicate_named_field_allowed_by_parser` / `inst_duplicate_anon_field_allowed_by_parser` | Both are parser-tolerance assertions — good to have, minimal already |

---

## Proposed changes

1. **Split** error tests into `golden_parse_error_test.rs` — error cases have different assertion style (`contains("ERR:")`) and will grow independently
2. **Reorder** `golden_parse_test.rs` into the 10-group sequence above (errors group removed, covered by new file)
3. **Add** 5 missing coverage tests:
   - `brace_group_ref_colon_after` — `{a:b}:extra` → `golden_parse_test.rs`
   - `brace_group_ref_adjacent` — `{a:b}{c:d}` → `golden_parse_test.rs`
   - `error_unclosed_struct` → `golden_parse_error_test.rs`
   - `error_unclosed_list` → `golden_parse_error_test.rs`
   - `error_unclosed_brace_group_ref` → `golden_parse_error_test.rs`
4. **Trim** scaffolding in brace group tests (remove unnecessary type/inst preamble)
5. **Trim** `multiple_defs` to only the mixed-kind ordering assertion (remove fields that duplicate `stdlib_subset`)
