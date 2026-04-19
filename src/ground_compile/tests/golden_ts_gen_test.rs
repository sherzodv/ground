/// Tests for TypeScript interface generation from mapper defs.
///
/// Ground generates `interface {Mapper}Input` / `interface {Mapper}Output` for
/// every root def that carries a mapper, prepending them to the TS blob before
/// execution. These tests verify the full compile+execute path with mappers.

use ground_compile::{compile, CompileReq, Unit};

/// Mapper def with primitive input/output: the mapper receives the input field and returns the output.
#[test]
fn ts_gen_primitive_fields() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name:   "test".into(),
            path:   vec![],
            src:    r#"
                def label { key = string } = make_label { value = string }
                plan env = label { key: "environment" }
            "#.into(),
            ts_src: Some(r#"
                function make_label(i) { return { value: i.key + "=prod" }; }
            "#.into()),
        }],
    });

    assert!(res.errors.is_empty(),
        "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

    let def   = res.defs.iter().find(|d| d.name == "env").expect("env def missing");
    let field = def.fields.iter().find(|f| f.name == "value").expect("value field missing");
    assert_eq!(
        format!("{:?}", field.value),
        r#"Str("environment=prod")"#,
        "mapper should produce value=environment=prod"
    );
}

/// Mapper def with integer output field.
#[test]
fn ts_gen_integer_output() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name:   "test".into(),
            path:   vec![],
            src:    r#"
                def port = make_port { number = integer }
                plan api = port {}
            "#.into(),
            ts_src: Some(r#"
                function make_port(_i) { return { number: 8080 }; }
            "#.into()),
        }],
    });

    assert!(res.errors.is_empty(),
        "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

    let def   = res.defs.iter().find(|d| d.name == "api").expect("api def missing");
    let field = def.fields.iter().find(|f| f.name == "number").expect("number field missing");
    assert_eq!(format!("{:?}", field.value), "Int(8080)", "mapper should produce number=8080");
}

/// Mapper def with boolean output field.
#[test]
fn ts_gen_boolean_output() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name:   "test".into(),
            path:   vec![],
            src:    r#"
                def feature = make_feature { enabled = boolean }
                plan beta = feature {}
            "#.into(),
            ts_src: Some(r#"
                function make_feature(_i) { return { enabled: true }; }
            "#.into()),
        }],
    });

    assert!(res.errors.is_empty(),
        "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

    let def   = res.defs.iter().find(|d| d.name == "beta").expect("beta def missing");
    let field = def.fields.iter().find(|f| f.name == "enabled").expect("enabled field missing");
    assert_eq!(format!("{:?}", field.value), "Bool(true)", "mapper should produce enabled=true");
}
