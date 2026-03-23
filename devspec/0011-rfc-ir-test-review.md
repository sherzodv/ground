# RFC 0011 — IR Golden Test Review

## Findings

### 1. Order

Current grouping has structural problems:

| Issue | Tests affected |
|---|---|
| `"Known problems"` group is misnamed and mixed | contains instance tests, an error test, and an integration test |
| `inst_inline_struct_value`, `inst_struct_as_field_value` in "Known problems" | belong in Instance field resolution |
| `typed_path_value_segment_count_mismatch_errors` in "Known problems" | belongs in error file |
| `inline_named_type_with_typed_path_ref` in "Known problems" | belongs in Regression |
| Error tests interleaved between Deploys and Use | should move to `golden_ir_error_test.rs` |
| Type hint error sub-tests inside the happy-path hints group | should move to error file |

Proposed order for `golden_ir_test.rs`:
1. Types (enum → struct variants → inline hoisted)
2. Top-level links (primitive → ref → typed path → list)
3. Instance field resolution (integer → string → enum → struct ref → forward ref → typed path → list)
4. Inline struct values (anonymous inline → named ref → type hint)
5. Anonymous link (list type → list inst)
6. List field aggregation (anon multi → named multi)
7. Deploy
8. Use / pack imports
9. Regression (`inline_named_type_with_typed_path_ref`)

Error tests move to `golden_ir_error_test.rs` (separate file).

---

### 2. Coverage gaps

**Instance field resolution**
- Reference-typed field (`link image = reference`) with a bare string value that stays as `Ref(...)` — not yet tested at IR level

**Type hints**
- Bare hint without `type:` prefix (e.g. `scaling { min: 2 }`) not tested at IR level
- Nested type hint (hint inside a hint) not tested

**Deploy**
- No test with actual field values (field resolution path for deploy fields is untested)

**Brace group passthrough**
- `Group` segment passes through as `Plain("{repr}")` — no smoke test confirming this at IR level

---

### 3. Redundancy

| Test | Issue |
|---|---|
| `struct_anonymous_link_list` | Fully subsumed by `struct_anonymous_link_list_inst` (same type setup, adds instance); type-only variant can be removed |
| `multiple_enum_types` | Tests nothing beyond `single_enum_type` except coexistence — low value; remove or merge into a types regression |
| `use_pack_name_no_error` | Only asserts absence of error, no IR structure assertion — upgrade to a structural assertion or remove |

---

## Proposed changes

1. **Split** error tests into `golden_ir_error_test.rs` — move all `assert!(out.contains("ERR:"))` tests there:
   - Moved from main file: `struct_anonymous_link_list_unresolved_type`, `typed_path_value_segment_count_mismatch_errors`, `error_field_from_different_type_rejected`, `error_unknown_type_in_link`, `error_invalid_enum_variant`, `error_unknown_instance_ref`, `error_unknown_inst_type`, `error_named_non_list_field_multiple_values`, `error_anon_non_list_field_multiple_values`, `error_use_pack_not_found`, `error_use_ambiguous_local_vs_import`, `error_use_ambiguous_two_imports`, `inst_inline_struct_type_hint_mismatch`, `inst_inline_struct_type_hint_unknown`

2. **Reorder** `golden_ir_test.rs` into the 9-group sequence above

3. **Remove** redundant tests: `struct_anonymous_link_list`, `multiple_enum_types`

4. **Upgrade** `use_pack_name_no_error` to assert IR structure instead of absence-of-error

5. **Add** 4 missing coverage tests:
   - `inst_reference_field` — `link image = reference` with string value staying as `Ref(...)` → `golden_ir_test.rs`
   - `inst_inline_struct_bare_hint` — bare hint without `type:` prefix → `golden_ir_test.rs`
   - `deploy_with_fields` — deploy field value resolution → `golden_ir_test.rs`
   - `brace_group_ref_passthrough` — `Group` segment passes through as plain at IR level → `golden_ir_test.rs`

6. **Convert** all `r#"..."#` to `r##"..."##` throughout both files
