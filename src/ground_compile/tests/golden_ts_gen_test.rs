/// Tests for TypeScript interface generation from hook defs.
///
/// Ground generates `interface {Hook}Input` / `interface {Hook}Output` for
/// every root def that carries a hook, prepending them to the TS blob before
/// execution. These tests verify the full compile+execute path with hooks.

use ground_compile::{compile, CompileReq, Unit};

/// Hook def with primitive input/output: hook receives input field, returns output.
#[test]
fn ts_gen_primitive_fields() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name:   "test".into(),
            path:   vec![],
            src:    r#"
                def label { key = string } = make_label { value = string }
                env = label { key: "environment" }
                plan env
            "#.into(),
            ts_src: Some(r#"
                function make_label(i) { return { value: i.key + "=prod" }; }
            "#.into()),
        }],
    });

    assert!(res.errors.is_empty(),
        "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

    let plan  = &res.plans[0];
    let inst  = plan.reachable.iter().find(|i| i.name == "env").expect("env instance missing");
    let field = inst.fields.iter().find(|f| f.name == "value").expect("value field missing");
    assert_eq!(
        format!("{:?}", field.value),
        r#"Str("environment=prod")"#,
        "hook should produce value=environment=prod"
    );
}

/// Hook def with integer output field.
#[test]
fn ts_gen_integer_output() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name:   "test".into(),
            path:   vec![],
            src:    r#"
                def port = make_port { number = integer }
                api = port {}
                plan api
            "#.into(),
            ts_src: Some(r#"
                function make_port(_i) { return { number: 8080 }; }
            "#.into()),
        }],
    });

    assert!(res.errors.is_empty(),
        "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

    let plan  = &res.plans[0];
    let inst  = plan.reachable.iter().find(|i| i.name == "api").expect("api instance missing");
    let field = inst.fields.iter().find(|f| f.name == "number").expect("number field missing");
    assert_eq!(format!("{:?}", field.value), "Int(8080)", "hook should produce number=8080");
}
