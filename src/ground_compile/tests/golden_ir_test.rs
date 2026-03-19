/// Golden tests for the resolve pass (`ground_compile::resolve`).
///
/// Each test calls `show(input)` which parses + resolves the source and returns
/// a compact, position-free string of the resulting IR.
///
/// Conventions:
///   Type#N[name, body]                        — type arena entry at index N
///   Link#N[name, type]                        — link arena entry at index N (in struct body or top-level)
///   Inst#N[Type#N, inst-name, fields…]        — instance arena entry at index N
///   Deploy[what, target, name, fields…]       — deploy statement
///
///   IrRef[segs…]             — resolved reference (link type or typed path)
///   Enum(Type#N)             — ref segment resolved to an enum type
///   Struct(Type#N)           — ref segment resolved to a struct type
///   Variant(Type#N, "name")  — instance field value: resolved enum variant
///   Inst(Inst#N)             — instance field value: resolved instance ref
///   Field[Link#N, value]     — instance field resolved to a specific link
#[path = "helpers/golden_ir_helpers.rs"] mod golden_ir_helpers;
use golden_ir_helpers::{norm, show, show_multi};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[test]
fn single_enum_type() {
    assert_eq!(
        show("type zone = 1 | 2 | 3"),
        norm(r#"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
            ]
        "#),
    );
}

#[test]
fn multiple_enum_types() {
    assert_eq!(
        show(
            r#"
            type zone   = 1 | 2 | 3
            type region = eu-central | eu-west
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[region, Enum[eu-central|eu-west]],
            ]
        "#
        ),
    );
}

#[test]
fn struct_type_primitive_link() {
    assert_eq!(
        show("type counter = { link count = integer }"),
        norm(r#"
            Scope[pack:test,
                Type#0[counter, Struct[Link#0[count, Prim(integer)]]],
                Scope[type:counter],
            ]
        "#),
    );
}

#[test]
fn struct_type_enum_ref_link() {
    assert_eq!(
        show(
            r#"
            type zone = 1 | 2 | 3
            type host = { link zone = zone }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[host, Struct[Link#0[zone, IrRef[Enum(Type#0)]]]],
                Scope[type:host],
            ]
        "#
        ),
    );
}

#[test]
fn struct_type_struct_ref_link() {
    assert_eq!(
        show(
            r#"
            type db  = { link engine = string }
            type svc = { link db = db }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[db, Struct[Link#0[engine, Prim(string)]]],
                Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]],
                Scope[type:db],
                Scope[type:svc],
            ]
        "#
        ),
    );
}

#[test]
fn inline_anonymous_enum_hoisted() {
    assert_eq!(
        show("type database = { link manage = self | provider | cloud }"),
        norm(
            r#"
            Scope[pack:test,
                Type#0[database, Struct[Link#0[manage, IrRef[Enum(Type#1)]]]],
                Scope[type:database,
                    Type#1[_, Enum[self|provider|cloud]],
                ],
            ]
        "#
        ),
    );
}

#[test]
fn inline_named_struct_hoisted() {
    assert_eq!(
        show(
            r#"
            type svc = {
                link scaling = type scaling = {
                    link min = integer
                    link max = integer
                }
            }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[svc, Struct[Link#2[scaling, IrRef[Struct(Type#1)]]]],
                Scope[type:svc,
                    Scope[type:scaling,
                        Type#1[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]],
                    ],
                ],
            ]
        "#
        ),
    );
}

#[test]
fn struct_with_inline_type_def() {
    assert_eq!(
        show(
            r#"
            type service = {
                type port  = grpc | http
                link image = reference
            }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Scope[type:service,
                    Type#1[port, Enum[grpc|http]],
                ],
            ]
        "#
        ),
    );
}

// ---------------------------------------------------------------------------
// Anonymous links
// ---------------------------------------------------------------------------

#[test]
fn struct_anonymous_link_list() {
    assert_eq!(
        show(
            r#"
            type service  = { link image  = reference }
            type database = { link engine = string    }
            type stack    = { link = [ type:service | type:database ] }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[database, Struct[Link#1[engine, Prim(string)]]],
                Type#2[stack, Struct[Link#2[_, List[IrRef[Struct(Type#0)] | IrRef[Struct(Type#1)]]]]],
                Scope[type:service],
                Scope[type:database],
                Scope[type:stack],
            ]
        "#
        ),
    );
}

#[test]
fn struct_anonymous_link_list_inst() {
    assert_eq!(
        show(
            r#"
            type service  = { link image  = reference }
            type database = { link engine = string    }
            type stack    = { link = [ type:service | type:database ] }
            service  svc-a    { image: "nginx" }
            database db-a     { engine: "pg"   }
            stack    my-stack { svc-a  db-a }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[database, Struct[Link#1[engine, Prim(string)]]],
                Type#2[stack, Struct[Link#2[_, List[IrRef[Struct(Type#0)] | IrRef[Struct(Type#1)]]]]],
                Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]],
                Inst#1[Type#1, db-a, Field[Link#1, Str("pg")]],
                Inst#2[Type#2, my-stack, Field[Link#2, List[Inst(Inst#0), Inst(Inst#1)]]],
                Scope[type:service],
                Scope[type:database],
                Scope[type:stack],
            ]
        "#
        ),
    );
}

#[test]
fn struct_anonymous_link_list_unresolved_type() {
    let out = show(
        r#"
        type service  = { link image  = reference }
        type stack    = { link = [ type:service | type:databasee ] }
    "#,
    );
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(
        out.contains("databasee"),
        "error should name the unresolved type: {}",
        out
    );
}

// ---------------------------------------------------------------------------
// Anon / named list fields — multiple occurrences gathered
// ---------------------------------------------------------------------------

#[test]
fn anon_list_field_multiple_values_gathered() {
    assert_eq!(
        show(
            r#"
            type service = { link image = reference }
            type stack   = { link = [ type:service ] }
            service svc-a { image: "nginx"  }
            service svc-b { image: "apache" }
            stack my-stack { svc-a svc-b }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[stack, Struct[Link#1[_, List[IrRef[Struct(Type#0)]]]]],
                Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]],
                Inst#1[Type#0, svc-b, Field[Link#0, Ref("apache")]],
                Inst#2[Type#1, my-stack, Field[Link#1, List[Inst(Inst#0), Inst(Inst#1)]]],
                Scope[type:service],
                Scope[type:stack],
            ]
        "#
        ),
    );
}

#[test]
fn named_list_field_multiple_values_gathered() {
    assert_eq!(
        show(
            r#"
            type service = { link image = reference }
            type stack   = { link services = [ type:service ] }
            service svc-a { image: "nginx"  }
            service svc-b { image: "apache" }
            stack my-stack { services: svc-a services: svc-b }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[stack, Struct[Link#1[services, List[IrRef[Struct(Type#0)]]]]],
                Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]],
                Inst#1[Type#0, svc-b, Field[Link#0, Ref("apache")]],
                Inst#2[Type#1, my-stack, Field[Link#1, List[Inst(Inst#0), Inst(Inst#1)]]],
                Scope[type:service],
                Scope[type:stack],
            ]
        "#
        ),
    );
}

// ---------------------------------------------------------------------------
// Top-level links
// ---------------------------------------------------------------------------

#[test]
fn top_level_link_primitive() {
    assert_eq!(
        show("link image = reference"),
        norm(r#"
            Scope[pack:test,
                Link#0[image, Prim(reference)],
            ]
        "#),
    );
}

#[test]
fn top_level_link_enum_ref() {
    assert_eq!(
        show(
            r#"
            type zone = 1 | 2 | 3
            link zone = zone
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Link#0[zone, IrRef[Enum(Type#0)]],
            ]
        "#
        ),
    );
}

#[test]
fn top_level_link_typed_path() {
    assert_eq!(
        show(
            r#"
            type zone   = 1 | 2 | 3
            type region = eu-central | eu-west
            link location = type:region:type:zone
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[region, Enum[eu-central|eu-west]],
                Link#0[location, IrRef[Enum(Type#1):Enum(Type#0)]],
            ]
        "#
        ),
    );
}

#[test]
fn top_level_link_list() {
    assert_eq!(
        show(
            r#"
            type service  = { link image  = reference }
            type database = { link engine = string    }
            link access   = [ service:(port) | database ]
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[service, Struct[Link#1[image, Prim(reference)]]],
                Type#1[database, Struct[Link#2[engine, Prim(string)]]],
                Link#0[access, List[IrRef[Struct(Type#0):(port)] | IrRef[Struct(Type#1)]]],
                Scope[type:service],
                Scope[type:database],
            ]
        "#
        ),
    );
}

// ---------------------------------------------------------------------------
// Instance field resolution
// ---------------------------------------------------------------------------

#[test]
fn inst_integer_field() {
    assert_eq!(
        show(
            r#"
            type counter = { link count = integer }
            counter c { count: 42 }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[counter, Struct[Link#0[count, Prim(integer)]]],
                Inst#0[Type#0, c, Field[Link#0, Int(42)]],
                Scope[type:counter],
            ]
        "#
        ),
    );
}

#[test]
fn inst_string_field() {
    assert_eq!(
        show(
            r#"
            type svc = { link label = string }
            svc my-svc { label: "hello" }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[svc, Struct[Link#0[label, Prim(string)]]],
                Inst#0[Type#0, my-svc, Field[Link#0, Str("hello")]],
                Scope[type:svc],
            ]
        "#
        ),
    );
}

#[test]
fn inst_enum_field() {
    assert_eq!(
        show(
            r#"
            type zone = 1 | 2 | 3
            type host = { link zone = zone }
            host my-host { zone: 2 }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[host, Struct[Link#0[zone, IrRef[Enum(Type#0)]]]],
                Inst#0[Type#1, my-host, Field[Link#0, Variant(Type#0, "2")]],
                Scope[type:host],
            ]
        "#
        ),
    );
}

#[test]
fn inst_struct_ref_field() {
    assert_eq!(
        show(
            r#"
            type db  = { link engine = string }
            type svc = { link db = db }
            db  my-db  { engine: "pg" }
            svc my-svc { db: my-db }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[db, Struct[Link#0[engine, Prim(string)]]],
                Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#0, my-db, Field[Link#0, Str("pg")]],
                Inst#1[Type#1, my-svc, Field[Link#1, Inst(Inst#0)]],
                Scope[type:db],
                Scope[type:svc],
            ]
        "#
        ),
    );
}

#[test]
fn inst_forward_reference() {
    assert_eq!(
        show(
            r#"
            type db  = { link engine = string }
            type svc = { link db = db }
            svc my-svc { db: my-db }
            db  my-db  { engine: "pg" }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[db, Struct[Link#0[engine, Prim(string)]]],
                Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#1, my-svc, Field[Link#1, Inst(Inst#1)]],
                Inst#1[Type#0, my-db, Field[Link#0, Str("pg")]],
                Scope[type:db],
                Scope[type:svc],
            ]
        "#
        ),
    );
}

#[test]
fn inst_typed_path_field() {
    assert_eq!(
        show(
            r#"
            type zone   = 1 | 2 | 3
            type region = eu-central | eu-west
            type svc    = { link location = type:region:type:zone }
            svc s { location: eu-central:2 }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[region, Enum[eu-central|eu-west]],
                Type#2[svc, Struct[Link#0[location, IrRef[Enum(Type#1):Enum(Type#0)]]]],
                Inst#0[Type#2, s, Field[Link#0, Variant(Type#1, "eu-central"):Variant(Type#0, "2")]],
                Scope[type:svc],
            ]
        "#
        ),
    );
}

#[test]
fn inst_list_field() {
    assert_eq!(
        show(
            r#"
            type port     = grpc | http
            type service  = { link port   = port   }
            type database = { link engine = string }
            type stack    = { link access = [ service:(port) | database ] }
            service  svc-a { port: grpc   }
            database db-a  { engine: "pg" }
            stack    my-stack { access: [ svc-a:grpc  db-a ] }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[port, Enum[grpc|http]],
                Type#1[service, Struct[Link#0[port, IrRef[Enum(Type#0)]]]],
                Type#2[database, Struct[Link#1[engine, Prim(string)]]],
                Type#3[stack, Struct[Link#2[access, List[IrRef[Struct(Type#1):(Enum(Type#0))] | IrRef[Struct(Type#2)]]]]],
                Inst#0[Type#1, svc-a, Field[Link#0, Variant(Type#0, "grpc")]],
                Inst#1[Type#2, db-a, Field[Link#1, Str("pg")]],
                Inst#2[Type#3, my-stack, Field[Link#2, List[Inst(Inst#0):Variant(Type#0, "grpc"), Inst(Inst#1)]]],
                Scope[type:service],
                Scope[type:database],
                Scope[type:stack],
            ]
        "#
        ),
    );
}

// ---------------------------------------------------------------------------
// Deploys
// ---------------------------------------------------------------------------

#[test]
fn deploy_refs_resolved() {
    assert_eq!(
        show("deploy stack to aws as prod {}"),
        norm(r#"
            Scope[pack:test]
            Deploy[stack, aws, prod]
        "#),
    );
}

#[test]
fn deploy_multi_segment_ref() {
    assert_eq!(
        show("deploy stack to aws:eu-central as prd {}"),
        norm(r#"
            Scope[pack:test]
            Deploy[stack, aws:eu-central, prd]
        "#),
    );
}

// ---------------------------------------------------------------------------
// Known problems
// ---------------------------------------------------------------------------

#[test]
fn typed_path_value_segment_count_mismatch_errors() {
    assert_eq!(
        show(
            r#"
            type zone   = 1 | 2 | 3
            type region = eu-central | eu-west
            type svc    = { link location = type:region:type:zone }
            svc s { location: eu-central }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[region, Enum[eu-central|eu-west]],
                Type#2[svc, Struct[Link#0[location, IrRef[Enum(Type#1):Enum(Type#0)]]]],
                Inst#0[Type#2, s, Field[Link#0, Variant(Type#1, "eu-central")]],
                Scope[type:svc],
            ]
            ERR: typed path has 1 segment(s), expected 2
        "#
        ),
    );
}

#[test]
fn inline_named_type_with_typed_path_ref() {
    assert_eq!(
        show(r#"
            type service = {
                type port   = grpc | http
                link sidecar = type sidecar = {
                    link service = type:service:(port)
                }
            }
            service upstream {}
            service my-svc {
                sidecar: {
                    service: upstream:grpc
                }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                Type#0[service, Struct[Link#1[sidecar, IrRef[Struct(Type#2)]]]],
                Inst#0[Type#0, upstream],
                Inst#1[Type#0, my-svc, Field[Link#1, Inst(Inst#2)]],
                Inst#2[Type#2, _, Field[Link#0, Inst(Inst#0):Variant(Type#1, "grpc")]],
                Scope[type:service,
                    Type#1[port, Enum[grpc|http]],
                    Scope[type:sidecar,
                        Type#2[sidecar, Struct[Link#0[service, IrRef[Struct(Type#0):(Enum(Type#1))]]]],
                    ],
                ],
            ]
        "#),
    );
}

#[test]
fn inst_inline_struct_value() {
    assert_eq!(
        show(r#"
            type scaling = {
                link min = integer
                link max = integer
            }
            type svc = { link scaling = scaling }
            svc my-svc {
                scaling: {
                    min: 1
                    max: 10
                }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                Type#0[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]],
                Type#1[svc, Struct[Link#2[scaling, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#1, my-svc, Field[Link#2, Inst(Inst#1)]],
                Inst#1[Type#0, _, Field[Link#0, Int(1)], Field[Link#1, Int(10)]],
                Scope[type:scaling],
                Scope[type:svc],
            ]
        "#),
    );
}

#[test]
fn inst_struct_as_field_value() {
    assert_eq!(
        show(
            r#"
            type scaling = {
                link min = integer
                link max = integer
            }
            type svc = { link scaling = scaling }
            scaling my-scaling { min: 1  max: 10 }
            svc     my-svc     { scaling: my-scaling }
        "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type#0[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]],
                Type#1[svc, Struct[Link#2[scaling, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#0, my-scaling, Field[Link#0, Int(1)], Field[Link#1, Int(10)]],
                Inst#1[Type#1, my-svc, Field[Link#2, Inst(Inst#0)]],
                Scope[type:scaling],
                Scope[type:svc],
            ]
        "#
        ),
    );
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[test]
fn error_field_from_different_type_rejected() {
    // Link#0 = A.a (string), Link#1 = B.b (integer).
    // Instantiating B with field 'a' must error — field lookup is scoped to
    // the instance's own type, not the global link arena.
    let out = show(
        r#"
        type A = { link a = string  }
        type B = { link b = integer }
        B my-b { a: "boo" }
    "#,
    );
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(
        out.contains("'a'"),
        "error should name the unknown field: {}",
        out
    );
}

#[test]
fn error_unknown_type_in_link() {
    let out = show("type svc = { link engine = nonexistent }");
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
}

#[test]
fn error_invalid_enum_variant() {
    let out = show(
        r#"
        type zone = 1 | 2 | 3
        type host = { link zone = zone }
        host my-host { zone: 99 }
    "#,
    );
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(
        out.contains("99"),
        "error should mention the bad variant: {}",
        out
    );
}

#[test]
fn error_unknown_instance_ref() {
    let out = show(
        r#"
        type db  = { link engine = string }
        type svc = { link db = db }
        svc my-svc { db: ghost }
    "#,
    );
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(
        out.contains("ghost"),
        "error should mention the unknown instance: {}",
        out
    );
}

#[test]
fn error_unknown_inst_type() {
    let out = show("ghost my-inst {}");
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(
        out.contains("ghost"),
        "error should name the unknown type: {}",
        out
    );
}

#[test]
fn error_named_non_list_field_multiple_values() {
    let out = show(
        r#"
        type service = { link image = reference }
        type stack   = { link service = type:service }
        service svc-a { image: "nginx"  }
        service svc-b { image: "apache" }
        stack my-stack { service: svc-a service: svc-b }
    "#,
    );
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(
        out.contains("multiple values defined for a non-List field 'service'"),
        "error should mention the field name: {}",
        out
    );
}

#[test]
fn error_anon_non_list_field_multiple_values() {
    let out = show(
        r#"
        type service = { link image = reference }
        type stack   = { link = type:service }
        service svc-a { image: "nginx"  }
        service svc-b { image: "apache" }
        stack my-stack { svc-a svc-b }
    "#,
    );
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(
        out.contains("multiple values defined for a non-List field '_'"),
        "error should mention the anonymous field: {}",
        out
    );
}

// ---------------------------------------------------------------------------
// Use / pack imports — IR layer
// ---------------------------------------------------------------------------

#[test]
fn use_pack_name_no_error() {
    // `use pack:std` registers std in the local packs map — no error expected.
    let out = show_multi(vec![
        ("std",  vec![],  "type service = { link image = reference }"),
        ("app",  vec![],  "use pack:std"),
    ]);
    assert!(!out.contains("ERR:"), "expected no error, got: {}", out);
}

#[test]
fn use_type_specific_import() {
    // `use pack:std:type:service` brings `service` into scope unqualified.
    assert_eq!(
        show_multi(vec![
            ("std", vec![], "type service = { link image = reference }"),
            ("app", vec![], r#"
                use pack:std:type:service
                service my-svc { image: "nginx" }
            "#),
        ]),
        norm(r#"
            Scope[pack:std,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Scope[type:service],
            ]
            Scope[pack:app,
                Inst#0[Type#0, my-svc, Field[Link#0, Ref("nginx")]],
            ]
        "#),
    );
}

#[test]
fn use_wildcard_type_import() {
    // `use pack:std:type:*` brings all types from std into scope.
    assert_eq!(
        show_multi(vec![
            ("std", vec![], r#"
                type service = { link image = reference }
                type database = { link engine = string }
            "#),
            ("app", vec![], r#"
                use pack:std:type:*
                service my-svc { image: "nginx" }
                database my-db { engine: "pg" }
            "#),
        ]),
        norm(r#"
            Scope[pack:std,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[database, Struct[Link#1[engine, Prim(string)]]],
                Scope[type:service],
                Scope[type:database],
            ]
            Scope[pack:app,
                Inst#0[Type#0, my-svc, Field[Link#0, Ref("nginx")]],
                Inst#1[Type#1, my-db, Field[Link#1, Str("pg")]],
            ]
        "#),
    );
}

#[test]
fn use_wildcard_all_import() {
    // `use pack:std:*` brings types AND instances from std into scope.
    assert_eq!(
        show_multi(vec![
            ("std", vec![], r#"
                type service = { link image = reference }
                service svc-a { image: "nginx" }
            "#),
            ("app", vec![], r#"
                use pack:std:*
                type stack = { link svc = type:service }
                stack my-stack { svc: svc-a }
            "#),
        ]),
        norm(r#"
            Scope[pack:std,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]],
                Scope[type:service],
            ]
            Scope[pack:app,
                Type#1[stack, Struct[Link#1[svc, IrRef[Struct(Type#0)]]]],
                Inst#1[Type#1, my-stack, Field[Link#1, Inst(Inst#0)]],
                Scope[type:stack],
            ]
        "#),
    );
}

#[test]
fn type_and_link_same_name_no_ambiguity() {
    // A type and a link with the same name can be imported together without
    // conflict — they are resolved through separate lookup functions.
    assert_eq!(
        show_multi(vec![
            ("pkg", vec![], r#"
                type region = north | south
                link region = string
            "#),
            ("app", vec![], r#"
                use pack:pkg:*
                type place = { link region = string }
                place home { region: "x" }
            "#),
        ]),
        norm(r#"
            Scope[pack:pkg,
                Type#0[region, Enum[north|south]],
                Link#0[region, Prim(string)],
            ]
            Scope[pack:app,
                Type#1[place, Struct[Link#1[region, Prim(string)]]],
                Inst#0[Type#1, home, Field[Link#1, Str("x")]],
                Scope[type:place],
            ]
        "#),
    );
}

#[test]
fn error_use_pack_not_found() {
    let out = show("use pack:nonexistent");
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(out.contains("nonexistent"), "error should name the missing pack: {}", out);
}

#[test]
fn error_use_ambiguous_local_vs_import() {
    // Local type `service` + imported type `service` from std — must disambiguate.
    let out = show_multi(vec![
        ("std", vec![], "type service = { link image = reference }"),
        ("app", vec![], r#"
            use pack:std:type:service
            type service = { link name = string }
            service my-svc { name: "x" }
        "#),
    ]);
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(out.contains("ambiguous"), "error should mention ambiguity: {}", out);
}

#[test]
fn error_use_ambiguous_two_imports() {
    // Two packs both export `service` — collision when both are imported.
    let out = show_multi(vec![
        ("a",   vec![], "type service = { link x = string }"),
        ("b",   vec![], "type service = { link y = string }"),
        ("app", vec![], r#"
            use pack:a:type:service
            use pack:b:type:service
        "#),
    ]);
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
    assert!(out.contains("ambiguous"), "error should mention ambiguity: {}", out);
}

#[test]
fn use_qualified_import_bypasses_shadowed_name() {
    // `outer` defines `type service`; `inner` (nested under outer) defines its own
    // `type service` which shadows the outer one via parent-chain lookup.
    // `app` (nested under inner) explicitly imports `pack:outer:type:service`,
    // bypassing the shadowing — the instance resolves to outer's type, not inner's.
    assert_eq!(
        show_multi(vec![
            ("outer", vec![],                 "type service = { link image = reference }"),
            ("inner", vec!["outer"],          "type service = { link name = string }"),
            ("app",   vec!["outer", "inner"], r#"
                use pack:outer:type:service
                service my-svc { image: "nginx" }
            "#),
        ]),
        norm(r#"
            Scope[pack:outer,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Scope[type:service],
                Scope[pack:inner,
                    Type#1[service, Struct[Link#1[name, Prim(string)]]],
                    Scope[type:service],
                    Scope[pack:app,
                        Inst#0[Type#0, my-svc, Field[Link#0, Ref("nginx")]],
                    ],
                ],
            ]
        "#),
    );
}
