#[path = "helpers/golden_ir_helpers.rs"]
mod golden_ir_helpers;
use golden_ir_helpers::{norm, show, show_multi, show_multi_ts};

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

#[test]
fn scope_001() {
    assert_eq!(show(""), "Scope[pack:test]");
}

#[test]
fn scope_002() {
    assert_eq!(
        show_multi(vec![
            ("infra", vec![], ""),
            ("web", vec!["infra"], "service = { image = reference }"),
            ("db", vec!["infra"], "database = { engine = string }"),
        ]),
        norm(
            r##"
            Scope[pack:infra,
                Scope[pack:web,
                    Shape#0[service, Struct[Field#0[image, Prim(reference)]]],
                    Def#0[service, Shape#0],
                ],
                Scope[pack:db,
                    Shape#1[database, Struct[Field#0[engine, Prim(string)]]],
                    Def#1[database, Shape#1],
                ],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Def
// ---------------------------------------------------------------------------

#[test]
fn def_001() {
    assert_eq!(
        show("counter = { count = integer }"),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[counter, Struct[Field#0[count, Prim(integer)]]],
                Def#0[counter, Shape#0],
            ]
        "##
        ),
    );
}

#[test]
fn def_002() {
    assert_eq!(
        show(
            r##"
            service = {
                def port = grpc | http
                image = reference
            }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[service, Struct[Field#0[image, Prim(reference)]]],
                Def#0[service, Shape#0],
                Scope[struct:service,
                    Shape#1[port, Enum[grpc|http]],
                    Def#1[port, Shape#1],
                ],
            ]
        "##
        ),
    );
}

#[test]
fn def_003() {
    assert_eq!(
        show(
            r##"
            service = { image = reference }
            api = service { color = string }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[service, Struct[Field#0[image, Prim(reference)]]],
                Shape#1[api, Struct[Field#0[image, Prim(reference)], Field#1[color, Prim(string)]]],
                Def#0[service, Shape#0],
                Def#1[api, Shape#1, base=Def#0],
            ]
        "##
        ),
    );
}

#[test]
fn def_004() {
    assert_eq!(
        show(
            r##"
            service = { image = reference }
            api = service {
                image: nginx
                color = string
            }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[service, Struct[Field#0[image, Prim(reference)]]],
                Shape#1[api, Struct[Field#0[image, Prim(reference)], Field#1[color, Prim(string)]]],
                Def#0[service, Shape#0],
                Def#1[api, Shape#1, base=Def#0, Set[Field#0, Ref(nginx)]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[test]
fn shape_001() {
    assert_eq!(
        show("zone = 1 | 2 | 3"),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[zone, Enum[1|2|3]],
                Def#0[zone, Shape#0],
            ]
        "##
        ),
    );
}

#[test]
fn shape_002() {
    assert_eq!(
        show("counter = { count = integer  enabled = boolean }"),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[counter, Struct[Field#0[count, Prim(integer)], Field#1[enabled, Prim(boolean)]]],
                Def#0[counter, Shape#0],
            ]
        "##
        ),
    );
}

#[test]
fn shape_003() {
    assert_eq!(
        show(
            r##"
            num  = { val = integer }
            add  = { lhs = string }
            expr = def:num | def:add
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[num, Struct[Field#0[val, Prim(integer)]]],
                Shape#1[add, Struct[Field#0[lhs, Prim(string)]]],
                Shape#2[expr, Enum[Struct(Shape#0)|Struct(Shape#1)]],
                Def#0[num, Shape#0],
                Def#1[add, Shape#1],
                Def#2[expr, Shape#2],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Value
// ---------------------------------------------------------------------------

#[test]
fn value_001() {
    assert_eq!(
        show(
            r##"
            service = { image = reference  enabled = boolean }
            api = service { image: nginx  enabled: true }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[service, Struct[Field#0[image, Prim(reference)], Field#1[enabled, Prim(boolean)]]],
                Def#0[service, Shape#0],
                Def#1[api, Shape#0, base=Def#0, Set[Field#0, Ref(nginx)], Set[Field#1, Bool(true)]],
            ]
        "##
        ),
    );
}

#[test]
fn value_002() {
    assert_eq!(
        show(
            r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            my-svc = svc { scaling: def:scaling { min: 2  max: 10 } }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[scaling, Struct[Field#0[min, Prim(integer)], Field#1[max, Prim(integer)]]],
                Shape#1[svc, Struct[Field#0[scaling, IrRef[Struct(Shape#0)]]]],
                Def#0[scaling, Shape#0],
                Def#1[svc, Shape#1],
                Def#2[my-svc, Shape#1, base=Def#1, Set[Field#0, Inst(Def#3)]],
                Def#3[_, Shape#0, hint=scaling, Set[Field#0, Int(2)], Set[Field#1, Int(10)]],
            ]
        "##
        ),
    );
}

#[test]
fn value_003() {
    assert_eq!(
        show(
            r##"
            service = {
                def scaling = {
                    min = integer
                    max = integer
                }
                scaling = def:scaling
            }
            my-svc = service { scaling: def:scaling { min: 2  max: 10 } }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[service, Struct[Field#0[scaling, IrRef[Struct(Shape#1)]]]],
                Def#0[service, Shape#0],
                Def#1[my-svc, Shape#0, base=Def#0, Set[Field#0, Inst(Def#3)]],
                Def#3[_, Shape#1, hint=scaling, Set[Field#0, Int(2)], Set[Field#1, Int(10)]],
                Scope[struct:service,
                    Shape#1[scaling, Struct[Field#0[min, Prim(integer)], Field#1[max, Prim(integer)]]],
                    Def#2[scaling, Shape#1],
                ],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[test]
fn import_001() {
    assert_eq!(
        show_multi(vec![
            ("std", vec![], "service = { image = reference }"),
            ("app", vec![], "use pack:std"),
        ]),
        norm(
            r##"
            Scope[pack:std,
                Shape#0[service, Struct[Field#0[image, Prim(reference)]]],
                Def#0[service, Shape#0],
            ]
            Scope[pack:app]
        "##
        ),
    );
}

#[test]
fn import_002() {
    assert_eq!(
        show_multi(vec![
            ("std", vec![], "service = { image = reference }"),
            (
                "app",
                vec![],
                r##"
                use pack:std:def:service
                my-svc = service { image: nginx }
            "##,
            ),
        ]),
        norm(
            r##"
            Scope[pack:std,
                Shape#0[service, Struct[Field#0[image, Prim(reference)]]],
                Def#0[service, Shape#0],
            ]
            Scope[pack:app,
                Def#1[my-svc, Shape#0, Set[Field#0, Ref(nginx)]],
            ]
        "##
        ),
    );
}

#[test]
fn import_003() {
    assert_eq!(
        show_multi(vec![
            (
                "std",
                vec![],
                r##"
                service  = { image = reference }
                database = { engine = string }
            "##,
            ),
            (
                "app",
                vec![],
                r##"
                use pack:std:def:*
                my-svc = service { image: nginx }
                my-db = database { engine: "pg" }
            "##,
            ),
        ]),
        norm(
            r##"
            Scope[pack:std,
                Shape#0[service, Struct[Field#0[image, Prim(reference)]]],
                Shape#1[database, Struct[Field#0[engine, Prim(string)]]],
                Def#0[service, Shape#0],
                Def#1[database, Shape#1],
            ]
            Scope[pack:app,
                Def#2[my-svc, Shape#0, Set[Field#0, Ref(nginx)]],
                Def#3[my-db, Shape#1, Set[Field#0, Str("pg")]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Mapper
// ---------------------------------------------------------------------------

#[test]
fn mapper_001() {
    let out = show_multi_ts(vec![
        (
            "main",
            vec![],
            r#"def label { key = string } = make_label { value = string }"#,
            Some("function make_label(i) { return { value: i.key }; }"),
        ),
    ]);
    assert!(
        !out.contains("ERR:"),
        "same-pack mapper should resolve cleanly, got: {out}"
    );
}

#[test]
fn mapper_002() {
    let out = show_multi_ts(vec![
        (
            "mappers",
            vec![],
            "",
            Some("function make_label(i) { return { value: i.key }; }"),
        ),
        (
            "main",
            vec![],
            r#"use pack:mappers:make_label
            def label { key = string } = make_label { value = string }"#,
            None,
        ),
    ]);
    assert!(
        !out.contains("ERR:"),
        "pack-imported mapper should resolve cleanly, got: {out}"
    );
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

#[test]
fn plan_001() {
    assert_eq!(
        show(
            r##"
            service = { image = reference }
            plan api = service { image: nginx }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Shape#0[service, Struct[Field#0[image, Prim(reference)]]],
                Def#0[service, Shape#0],
                Def#1[api, Shape#0, planned, base=Def#0, Set[Field#0, Ref(nginx)]],
            ]
        "##
        ),
    );
}
