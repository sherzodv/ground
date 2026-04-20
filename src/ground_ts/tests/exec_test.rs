use ground_ts::exec::{call_hook, ts_to_js};

// ---------------------------------------------------------------------------
// ts_to_js — transpilation only
// ---------------------------------------------------------------------------

#[test]
fn transpile_strips_types() {
    let ts = r#"
        interface AddI { a: number; b: number }
        interface AddO { sum: number }
        function add(i: AddI): AddO { return { sum: i.a + i.b }; }
    "#;
    let js = ts_to_js(ts).unwrap();
    // Types erased, function body intact
    assert!(!js.contains("interface"), "interface should be erased");
    assert!(!js.contains(": number"),  "type annotations should be erased");
    assert!(js.contains("function add"), "function must survive");
    assert!(js.contains("i.a + i.b"),    "body must survive");
}

#[test]
fn transpile_strips_export_modifier() {
    let ts = r#"
        export interface FooI { x: number }
        export function foo(i: FooI): number { return i.x * 2; }
    "#;
    let js = ts_to_js(ts).unwrap();
    assert!(!js.contains("export"), "export modifiers should be stripped");
    assert!(js.contains("function foo"), "function must survive");
}

// ---------------------------------------------------------------------------
// call_hook — full transpile + execute
// ---------------------------------------------------------------------------

#[test]
fn call_simple_function() {
    let ts = r#"
        interface AddI { a: number; b: number }
        interface AddO { sum: number }
        function add(i: AddI): AddO { return { sum: i.a + i.b }; }
    "#;
    let out = call_hook(ts, "add", r#"{"a": 3, "b": 4}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["sum"], 7);
}

#[test]
fn call_exported_function() {
    // export modifier is stripped — function still accessible as global
    let ts = r#"
        export interface MakeTagI { name: string; value: string }
        export interface MakeTagO { tag: string }
        export function make_tag(i: MakeTagI): MakeTagO {
            return { tag: `${i.name}=${i.value}` };
        }
    "#;
    let out = call_hook(ts, "make_tag", r#"{"name":"env","value":"prd"}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["tag"], "env=prd");
}

#[test]
fn call_hook_with_nested_output() {
    let ts = r#"
        interface NodeI { name: string }
        interface Ep    { host: string; port: number }
        interface NodeO { ep: Ep }
        function make_node(i: NodeI): NodeO {
            return { ep: { host: i.name + ".internal", port: 8080 } };
        }
    "#;
    let out = call_hook(ts, "make_node", r#"{"name":"api"}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["ep"]["host"], "api.internal");
    assert_eq!(v["ep"]["port"], 8080);
}

#[test]
fn call_hook_enum_field() {
    let ts = r#"
        type Proto = 'http' | 'grpc'
        interface SvcI { proto: Proto }
        interface SvcO { port: number }
        function make_svc(i: SvcI): SvcO {
            return { port: i.proto === 'grpc' ? 50051 : 8080 };
        }
    "#;
    let out = call_hook(ts, "make_svc", r#"{"proto":"grpc"}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["port"], 50051);
}

#[test]
fn call_hook_array_output() {
    let ts = r#"
        interface TagsI { prefix: string; count: number }
        function make_tags(i: TagsI): string[] {
            return Array.from({ length: i.count }, (_, k) => `${i.prefix}-${k}`);
        }
    "#;
    let out = call_hook(ts, "make_tags", r#"{"prefix":"svc","count":3}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v, serde_json::json!(["svc-0", "svc-1", "svc-2"]));
}

#[test]
fn typecheck_simple_hook() {
    use ground_ts::typecheck::typecheck;
    // Declarations: only interfaces, no `declare function` (that would clash with the
    // non-ambient function definition in user.ts and trigger TS2384).
    let declarations = "interface MakeLabelI { key: string; }\ninterface MakeLabelO { value: string; }";
    let user_ts = r#"function make_label(i: MakeLabelI): MakeLabelO {
    return { value: i.key + "=prod" };
}"#;
    let diags = typecheck(declarations, user_ts).expect("typecheck engine error");
    let errors: Vec<_> = diags.iter().filter(|d| d.category == 1).collect();
    assert!(errors.is_empty(), "unexpected TS errors: {errors:?}");
}

#[test]
fn typecheck_catches_type_error() {
    use ground_ts::typecheck::typecheck;
    let declarations = "interface MakeLabelI { key: string; }\ninterface MakeLabelO { value: string; }";
    // Return type mismatch: returns { value: number } but MakeLabelO needs string
    let user_ts = r#"function make_label(i: MakeLabelI): MakeLabelO {
    return { value: 42 };
}"#;
    let diags = typecheck(declarations, user_ts).expect("typecheck engine error");
    let errors: Vec<_> = diags.iter().filter(|d| d.category == 1).collect();
    assert!(!errors.is_empty(), "expected a type error but got none");
    assert!(errors.iter().any(|d| d.message.contains("not assignable")),
        "expected 'not assignable' error, got: {errors:?}");
}
