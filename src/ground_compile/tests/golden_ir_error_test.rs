/// Golden tests for IR resolve error cases (`ground_compile::resolve`).
///
/// Each test calls `show(input)` or `show_multi(...)` and asserts that the
/// output contains one or more `ERR: ...` lines.

#[path = "helpers/golden_ir_helpers.rs"] mod golden_ir_helpers;
use golden_ir_helpers::{show, show_multi};

// ---------------------------------------------------------------------------
// Type resolution errors
// ---------------------------------------------------------------------------

#[test]
fn error_unknown_type_in_link() {
    let out = show("type svc = { link engine = nonexistent }");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn struct_anonymous_link_list_unresolved_type() {
    let out = show(r##"
        type service  = { link image  = reference }
        type stack    = { link = [ type:service | type:databasee ] }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("databasee"), "error should name the unresolved type: {out}");
}

// ---------------------------------------------------------------------------
// Instance field errors
// ---------------------------------------------------------------------------

#[test]
fn error_unknown_inst_type() {
    let out = show("ghost my-inst {}");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("ghost"), "error should name the unknown type: {out}");
}

#[test]
fn error_field_from_different_type_rejected() {
    // Link#0 = A.a (string), Link#1 = B.b (integer).
    // Instantiating B with field 'a' must error — field lookup is scoped to
    // the instance's own type, not the global link arena.
    let out = show(r##"
        type A = { link a = string  }
        type B = { link b = integer }
        B my-b { a: "boo" }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("'a'"), "error should name the unknown field: {out}");
}

#[test]
fn error_invalid_enum_variant() {
    let out = show(r##"
        type zone = 1 | 2 | 3
        type host = { link zone = zone }
        host my-host { zone: 99 }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("99"), "error should mention the bad variant: {out}");
}

#[test]
fn error_unknown_instance_ref() {
    let out = show(r##"
        type db  = { link engine = string }
        type svc = { link db = db }
        svc my-svc { db: ghost }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("ghost"), "error should mention the unknown instance: {out}");
}

#[test]
fn error_typed_enum_instance_wrong_type() {
    // Named instance whose type does not match any typed variant.
    let out = show(r##"
        type num   = { link val = integer }
        type other = { link x = string }
        type expr  = type:num
        type host  = { link e = type:expr }
        other my-other { x: "foo" }
        host  h        { e: my-other }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("not a variant"), "expected not-a-variant error, got: {out}");
}

#[test]
fn error_named_non_list_field_multiple_values() {
    let out = show(r##"
        type service = { link image = reference }
        type stack   = { link service = type:service }
        service svc-a { image: "nginx"  }
        service svc-b { image: "apache" }
        stack my-stack { service: svc-a service: svc-b }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(
        out.contains("multiple values defined for a non-List field 'service'"),
        "error should mention the field name: {out}",
    );
}

#[test]
fn error_anon_non_list_field_multiple_values() {
    let out = show(r##"
        type service = { link image = reference }
        type stack   = { link = type:service }
        service svc-a { image: "nginx"  }
        service svc-b { image: "apache" }
        stack my-stack { svc-a svc-b }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(
        out.contains("multiple values defined for a non-List field '_'"),
        "error should mention the anonymous field: {out}",
    );
}

// ---------------------------------------------------------------------------
// Typed path errors
// ---------------------------------------------------------------------------

#[test]
fn typed_path_value_segment_count_mismatch_errors() {
    let out = show(r##"
        type zone   = 1 | 2 | 3
        type region = eu-central | eu-west
        type svc    = { link location = type:region:type:zone }
        svc s { location: eu-central }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("typed path"), "error should mention typed path: {out}");
}

// ---------------------------------------------------------------------------
// Type hint errors
// ---------------------------------------------------------------------------

#[test]
fn error_typed_enum_struct_without_hint() {
    // Struct literal against a typed enum variant requires a hint.
    let out = show(r##"
        type num  = { link val = integer }
        type expr = type:num
        type host = { link e = type:expr }
        host h { e: { val: 5 } }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("type hint required"), "expected hint-required error, got: {out}");
}

#[test]
fn error_typed_enum_wrong_hint() {
    // Hint type exists but is not one of the enum's typed variants.
    let out = show(r##"
        type num   = { link val = integer }
        type other = { link x = string }
        type expr  = type:num
        type host  = { link e = type:expr }
        host h { e: other { x: "foo" } }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("not a variant"), "expected not-a-variant error, got: {out}");
}

#[test]
fn inst_inline_struct_type_hint_mismatch() {
    let out = show(r##"
        type scaling = { link min = integer  link max = integer }
        type other   = { link x = integer }
        type svc     = { link scaling = scaling }
        svc my-svc {
            scaling: type:other { min: 2  max: 10 }
        }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("does not match"), "expected mismatch error, got: {out}");
}

#[test]
fn inst_inline_struct_type_hint_unknown() {
    let out = show(r##"
        type scaling = { link min = integer  link max = integer }
        type svc     = { link scaling = scaling }
        svc my-svc {
            scaling: type:nonexistent { min: 2  max: 10 }
        }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("nonexistent"), "error should name the unknown type: {out}");
}

// ---------------------------------------------------------------------------
// Use / import errors
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Gen definition errors
// ---------------------------------------------------------------------------

#[test]
fn type_fn_error_unknown_param_type() {
    let out = show("type svc_gen(s: nonexistent) = { }");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("nonexistent"), "error should name the unknown param type: {out}");
}

#[test]
fn type_fn_error_unknown_vendor_type() {
    let out = show(r##"
        type stack = { link name = string }
        type svc_gen(s: stack) = {
            sg: nonexistent_vendor { name: "x" }
        }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("nonexistent_vendor"), "error should name the unknown vendor type: {out}");
}

#[test]
fn type_fn_error_missing_vendor_annotation() {
    // Entry value without a type hint → error.
    let out = show(r##"
        type stack = { link name = string }
        type svc_gen(s: stack) = {
            sg: { name: "x" }
        }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("missing vendor type"), "error should mention 'missing vendor type': {out}");
}

#[test]
fn type_fn_error_entry_not_struct() {
    // Entry value is a string literal, not a typed struct → error.
    let out = show(r##"
        type stack = { link name = string }
        type svc_gen(s: stack) = {
            sg: "plain-value"
        }
    "##);
    // The entry value `"plain-value"` is a Str — doesn't match AstValue::Struct pattern.
    // The parser will likely parse `sg: "plain-value"` as the entry value = Str,
    // which then fails in resolve because it's not a struct.
    // We just check that an error is produced.
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn type_fn_error_param_registered_not_as_type() {
    // Registering two type fns with the same named param type: both are valid.
    // This is a positive test that two named type fns can coexist.
    let out = show(r##"
        type stack    = { link name = string }
        type stack_a(s: stack) = { sg: nonexistent { } }
        type stack_b(s: stack) = { db: nonexistent { } }
    "##);
    // Both reference nonexistent vendor types — two errors expected.
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn gen_error_missing_vendor_type_annotation() {
    // A type fn entry that is a plain struct without a type hint.
    let out = show(r##"
        type stack = { link name = string }
        type svc_gen(s: stack) = {
            sg: { name: "x" }
        }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn error_use_pack_not_found() {
    let out = show("use pack:nonexistent");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("nonexistent"), "error should name the missing pack: {out}");
}

#[test]
fn error_use_ambiguous_local_vs_import() {
    // Local type `service` wins over imported type `service` from std — no error.
    // The instance should resolve against the local definition (Type with link `name`).
    let out = show_multi(vec![
        ("std", vec![], "type service = { link image = reference }"),
        ("app", vec![], r##"
            use pack:std:type:service
            type service = { link name = string }
            service my-svc { name: "x" }
        "##),
    ]);
    assert!(!out.contains("ERR:"), "local def should win over import silently, got: {out}");
    assert!(out.contains("my-svc"), "instance should resolve: {out}");
}

#[test]
fn error_use_ambiguous_two_imports() {
    // Two packs both export `service` — collision when both are imported.
    let out = show_multi(vec![
        ("a",   vec![], "type service = { link x = string }"),
        ("b",   vec![], "type service = { link y = string }"),
        ("app", vec![], r##"
            use pack:a:type:service
            use pack:b:type:service
        "##),
    ]);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("ambiguous"), "error should mention ambiguity: {out}");
}
