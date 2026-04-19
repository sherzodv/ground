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
    let out = show("def svc { engine = nonexistent }");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn struct_anonymous_link_list_unresolved_type() {
    let out = show(r##"
        def service { image = reference }
        def stack { items = [ service | databasee ] }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("databasee"), "error should name the unresolved type: {out}");
}

// ---------------------------------------------------------------------------
// Instance field errors
// ---------------------------------------------------------------------------

#[test]
fn error_unknown_inst_type() {
    let out = show("my-inst = ghost {}");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("ghost"), "error should name the unknown type: {out}");
}

#[test]
fn error_field_from_different_type_rejected() {
    // Input#0 = A.a (string), Input#1 = B.b (integer).
    // Instantiating B with field 'a' must error — field lookup is scoped to
    // the instance's own def, not the global arena.
    let out = show(r##"
        def A { a = string  }
        def B { b = integer }
        my-b = B { a: "boo" }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("'a'"), "error should name the unknown field: {out}");
}

#[test]
fn error_composed_shape_redefines_inherited_field() {
    let out = show(r##"
        service = { image = reference }
        api = service { image = string }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("image"), "error should name the duplicate inherited field: {out}");
    assert!(out.contains("already exists"), "error should explain the inherited collision: {out}");
}

#[test]
fn error_invalid_enum_variant() {
    let out = show(r##"
        zone = 1 | 2 | 3
        def host { zone = zone }
        my-host = host { zone: 99 }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("99"), "error should mention the bad variant: {out}");
}

#[test]
fn error_unknown_instance_ref() {
    let out = show(r##"
        def db  { engine = string }
        def svc { db = db }
        my-svc = svc { db: ghost }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("ghost"), "error should mention the unknown instance: {out}");
}

#[test]
fn error_planned_def_cannot_be_referenced() {
    let out = show(r##"
        service = { image = reference }
        plan api = service { image: nginx }
        stack = { service = service }
        my-stack = stack { service: api }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("api"), "error should mention the planned def name: {out}");
}

#[test]
fn error_typed_enum_instance_wrong_type() {
    // Named instance whose type does not match any typed variant.
    let out = show(r##"
        def num   { val = integer }
        def other { x = string }
        expr = num
        def host  { e = expr }
        my-other = other { x: "foo" }
        h = host { e: my-other }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("not a variant"), "expected not-a-variant error, got: {out}");
}

#[test]
fn error_named_non_list_field_multiple_values() {
    let out = show(r##"
        def service { image = reference }
        def stack   { service = service }
        svc-a = service { image: "nginx"  }
        svc-b = service { image: "apache" }
        my-stack = stack { service: svc-a service: svc-b }
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
        def service { image = reference }
        def stack   { item = service }
        svc-a = service { image: "nginx"  }
        svc-b = service { image: "apache" }
        my-stack = stack { item: svc-a item: svc-b }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(
        out.contains("multiple values defined for a non-List field 'item'"),
        "error should mention the field name: {out}",
    );
}

// ---------------------------------------------------------------------------
// Typed path errors
// ---------------------------------------------------------------------------

#[test]
fn typed_path_value_segment_count_mismatch_errors() {
    let out = show(r##"
        zone   = 1 | 2 | 3
        region = eu-central | eu-west
        def svc { location = region:zone }
        s = svc { location: eu-central }
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
        def num  { val = integer }
        expr = num
        def host { e = expr }
        h = host { e: { val: 5 } }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("shape hint required"), "expected hint-required error, got: {out}");
}

#[test]
fn error_typed_enum_wrong_hint() {
    // Hint type exists but is not one of the enum's typed variants.
    let out = show(r##"
        def num   { val = integer }
        def other { x = string }
        expr = num
        def host  { e = expr }
        h = host { e: other { x: "foo" } }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("not a variant"), "expected not-a-variant error, got: {out}");
}

#[test]
fn inst_inline_struct_type_hint_mismatch() {
    let out = show(r##"
        def scaling { min = integer  max = integer }
        def other   { x = integer }
        def svc     { scaling = scaling }
        my-svc = svc {
            scaling: other { min: 2  max: 10 }
        }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("does not match"), "expected mismatch error, got: {out}");
}

#[test]
fn inst_inline_struct_type_hint_unknown() {
    let out = show(r##"
        def scaling { min = integer  max = integer }
        def svc     { scaling = scaling }
        my-svc = svc {
            scaling: nonexistent { min: 2  max: 10 }
        }
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("nonexistent"), "error should name the unknown type: {out}");
}

// ---------------------------------------------------------------------------
// Use / import errors
// ---------------------------------------------------------------------------

#[test]
fn error_use_pack_not_found() {
    let out = show("use pack:nonexistent");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("nonexistent"), "error should name the missing pack: {out}");
}

#[test]
fn error_use_ambiguous_local_vs_import() {
    // Local def `service` shadows the imported `service` from std — no ambiguity error.
    // The instance resolves against the local definition.
    let out = show_multi(vec![
        ("std", vec![], "def service { image = reference }"),
        ("app", vec![], r##"
            use pack:std:service
            def service { name = string }
            my-svc = service { name: "x" }
        "##),
    ]);
    assert!(!out.contains("ambiguous"), "local def should shadow import without ambiguity error, got: {out}");
    assert!(out.contains("my-svc"), "instance should resolve against local def: {out}");
}

#[test]
fn error_use_ambiguous_two_imports() {
    // Two packs both export `service` — collision when both are imported.
    let out = show_multi(vec![
        ("a",   vec![], "def service { x = string }"),
        ("b",   vec![], "def service { y = string }"),
        ("app", vec![], r##"
            use pack:a:service
            use pack:b:service
        "##),
    ]);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("ambiguous"), "error should mention ambiguity: {out}");
}

// ---------------------------------------------------------------------------
// Mapper function scope errors
// ---------------------------------------------------------------------------

/// A mapper def that names a TS function with no ts_src provided → resolve error.
#[test]
fn error_mapper_fn_not_in_scope() {
    let out = show(r#"
        def label { key = string } = make_label { value = string }
        label env { key: "environment" }
    "#);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("make_label"), "error should name the missing function: {out}");
    assert!(out.contains("not in scope"), "error should explain scoping: {out}");
}

/// A mapper def that names a TS function defined in another pack but not imported.
#[test]
fn error_mapper_fn_not_imported() {
    use ground_compile::ast::{ParseReq, ParseUnit};
    use ground_compile::parse::parse;
    use ground_compile::resolve::resolve;

    let res = parse(ParseReq { units: vec![
        ParseUnit {
            name:   "mappers".into(),
            path:   vec![],
            src:    String::new(),
            ts_src: Some("function make_label(i) { return { value: i.key }; }".into()),
        },
        ParseUnit {
            name:   "main".into(),
            path:   vec![],
            src:    r#"def label { key = string } = make_label { value = string }"#.into(),
            ts_src: None,
        },
    ]});
    let ir = resolve(res);
    let errors: Vec<_> = ir.errors.iter().map(|e| e.message.as_str()).collect();
    let out = format!("ERR: {}", errors.join("\nERR: "));
    assert!(!errors.is_empty(), "expected error, got none");
    assert!(out.contains("make_label"), "error should name the missing function: {out}");
    assert!(out.contains("not in scope"), "error should explain scoping: {out}");
}
