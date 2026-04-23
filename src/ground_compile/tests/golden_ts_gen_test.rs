/// Tests for TypeScript interface generation from mapper defs.
///
/// Ground generates `interface {Mapper}I` / `interface {Mapper}O` for
/// every root def that carries a mapper, prepending them to the TS blob before
/// execution. These tests verify the full compile+execute path with mappers.
use ground_compile::{compile, CompileReq, Unit};

/// Mapper def with primitive input/output: the mapper receives the input field and returns the output.
#[test]
fn ts_gen_primitive_fields() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: r#"
                pack test
                def label { key = string } = make_label { value = string }
                plan env = label { key: "environment" }
            "#
            .into(),
            ts_src: Some(
                r#"
                function make_label(i) { return { value: i.key + "=prod" }; }
            "#
                .into(),
            ),
        }],
    });

    assert!(
        res.errors.is_empty(),
        "compile errors: {:?}",
        res.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    let def = res
        .defs
        .iter()
        .find(|d| d.name == "env")
        .expect("env def missing");
    let field = def
        .fields
        .iter()
        .find(|f| f.name == "value")
        .expect("value field missing");
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
            name: "test".into(),
            path: vec![],
            src: r#"
                pack test
                def port = make_port { number = integer }
                plan api = port {}
            "#
            .into(),
            ts_src: Some(
                r#"
                function make_port(_i) { return { number: 8080 }; }
            "#
                .into(),
            ),
        }],
    });

    assert!(
        res.errors.is_empty(),
        "compile errors: {:?}",
        res.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    let def = res
        .defs
        .iter()
        .find(|d| d.name == "api")
        .expect("api def missing");
    let field = def
        .fields
        .iter()
        .find(|f| f.name == "number")
        .expect("number field missing");
    assert_eq!(
        format!("{:?}", field.value),
        "Int(8080)",
        "mapper should produce number=8080"
    );
}

/// Mapper def with boolean output field.
#[test]
fn ts_gen_boolean_output() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: r#"
                pack test
                def feature = make_feature { enabled = boolean }
                plan beta = feature {}
            "#
            .into(),
            ts_src: Some(
                r#"
                function make_feature(_i) { return { enabled: true }; }
            "#
                .into(),
            ),
        }],
    });

    assert!(
        res.errors.is_empty(),
        "compile errors: {:?}",
        res.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    let def = res
        .defs
        .iter()
        .find(|d| d.name == "beta")
        .expect("beta def missing");
    let field = def
        .fields
        .iter()
        .find(|f| f.name == "enabled")
        .expect("enabled field missing");
    assert_eq!(
        format!("{:?}", field.value),
        "Bool(true)",
        "mapper should produce enabled=true"
    );
}

#[test]
fn ts_gen_type_units_include_named_shapes() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: r#"
                pack test
                endpoint = { host = string  port = integer }
                def node { name = string } = make_node { endpoint = endpoint }
            "#.into(),
            ts_src: Some(r#"
                function make_node(i) { return { endpoint: { host: i.name + ".internal", port: 8080 } }; }
            "#.into()),
        }],
    });

    assert!(
        res.errors.is_empty(),
        "compile errors: {:?}",
        res.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    let unit = res
        .type_units
        .iter()
        .find(|u| u.file == "test.gen.d.ts")
        .expect("test.gen.d.ts missing");

    assert!(unit.content.contains("interface TestEndpoint"));
    assert!(unit.content.contains("endpoint: TestEndpoint;"));
    assert!(unit
        .content
        .contains("declare function make_node(i: MakeNodeI): MakeNodeO;"));
}

#[test]
fn ts_gen_optional_output_field() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: r#"
                pack test
                def feature = make_feature { note = (string) }
                plan beta = feature {}
            "#
            .into(),
            ts_src: Some(
                r#"
                function make_feature(_i) { return {}; }
            "#
                .into(),
            ),
        }],
    });

    assert!(
        res.errors.is_empty(),
        "compile errors: {:?}",
        res.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    let unit = res
        .type_units
        .iter()
        .find(|u| u.file == "test.gen.d.ts")
        .expect("test.gen.d.ts missing");

    assert!(unit.content.contains("note?: string;"));
    let def = res
        .defs
        .iter()
        .find(|d| d.name == "beta")
        .expect("beta def missing");
    assert!(
        def.fields.iter().all(|f| f.name != "note"),
        "optional omitted field should not be materialized"
    );
}

#[test]
fn compile_requires_pack_as_first_item() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: "def service".into(),
            ts_src: None,
        }],
    });

    assert_eq!(res.errors.len(), 1);
    assert_eq!(
        res.errors[0].message,
        "compile unit must start with `pack ...`"
    );
}

#[test]
fn compile_pack_must_refine_file_path() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "aws".into(),
            path: vec!["std".into()],
            src: "pack wrong:tf\naws_vpc = string".into(),
            ts_src: None,
        }],
    });

    assert_eq!(res.errors.len(), 1);
    assert_eq!(
        res.errors[0].message,
        "compile unit pack 'wrong:tf' must match file path prefix 'std:aws'"
    );
}

#[test]
fn composed_def_without_new_fields_reuses_inherited_mapper() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: r#"
                pack test
                def vpc { cidr_block = string } = make_vpc { rendered = string }
                plan main = vpc { cidr_block: "10.0.0.0/16" }
            "#
            .into(),
            ts_src: Some(
                r#"
                function make_vpc(i) { return { rendered: "cidr=" + i.cidr_block }; }
            "#
                .into(),
            ),
        }],
    });

    assert!(
        res.errors.is_empty(),
        "compile errors: {:?}",
        res.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    let def = res
        .defs
        .iter()
        .find(|d| d.name == "main")
        .expect("main def missing");
    let field = def
        .fields
        .iter()
        .find(|f| f.name == "rendered")
        .expect("rendered field missing");
    assert_eq!(format!("{:?}", field.value), r#"Str("cidr=10.0.0.0/16")"#);
}

#[test]
fn composed_def_with_new_fields_requires_explicit_mapper_override() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: r#"
                pack test
                def vpc { cidr_block = string } = make_vpc {}
                decorated = vpc { label = string }
            "#
            .into(),
            ts_src: Some(
                r#"
                function make_vpc(_i) { return {}; }
            "#
                .into(),
            ),
        }],
    });

    assert_eq!(res.errors.len(), 1);
    assert_eq!(
        res.errors[0].message,
        "mapper function 'decorated' not in scope; define it in a co-located .ts file or import its pack with `use pack:<name>`"
    );
}

#[test]
fn def_with_input_and_bare_mapper_rhs_uses_explicit_mapper() {
    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: r#"
                pack test
                def aws_vpc { cidr_block = string } = make_aws_vpc
                plan main = aws_vpc { cidr_block: "10.0.0.0/16" }
            "#
            .into(),
            ts_src: Some(
                r#"
                function make_aws_vpc(i) { return { cidr_block: i.cidr_block }; }
            "#
                .into(),
            ),
        }],
    });

    assert!(
        res.errors.is_empty(),
        "compile errors: {:?}",
        res.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    let def = res
        .defs
        .iter()
        .find(|d| d.name == "main")
        .expect("main def missing");
    let field = def
        .fields
        .iter()
        .find(|f| f.name == "cidr_block")
        .expect("cidr_block field missing");
    assert_eq!(format!("{:?}", field.value), r#"Str("10.0.0.0/16")"#);
}
