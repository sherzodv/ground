/// Golden tests for the resolve pass (`ground_parse::resolve2`).
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
mod golden_ir_helpers;
use golden_ir_helpers::{norm, show};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[test]
fn single_enum_type() {
    assert_eq!(show("type zone = 1 | 2 | 3"), "Type#0[zone, Enum[1|2|3]]",);
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
            Type#0[zone, Enum[1|2|3]]
            Type#1[region, Enum[eu-central|eu-west]]
        "#
        ),
    );
}

#[test]
fn struct_type_primitive_link() {
    assert_eq!(
        show("type counter = { link count = integer }"),
        "Type#0[counter, Struct[Link#0[count, Prim(integer)]]]",
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
            Type#0[zone, Enum[1|2|3]]
            Type#1[host, Struct[Link#0[zone, IrRef[Enum(Type#0)]]]]
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
            Type#0[db, Struct[Link#0[engine, Prim(string)]]]
            Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]]
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
            Type#0[database, Struct[Link#0[manage, IrRef[Enum(Type#1)]]]]
            Type#1[_, Enum[self|provider|cloud]]
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
            Type#0[svc, Struct[Link#2[scaling, IrRef[Struct(Type#1)]]]]
            Type#1[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]]
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
            Type#0[service, Struct[Link#0[image, Prim(reference)]]]
            Type#1[port, Enum[grpc|http]]
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
            Type#0[service, Struct[Link#0[image, Prim(reference)]]]
            Type#1[database, Struct[Link#1[engine, Prim(string)]]]
            Type#2[stack, Struct[Link#2[_, List[IrRef[Struct(Type#0)] | IrRef[Struct(Type#1)]]]]]
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
            Type#0[service, Struct[Link#0[image, Prim(reference)]]]
            Type#1[database, Struct[Link#1[engine, Prim(string)]]]
            Type#2[stack, Struct[Link#2[_, List[IrRef[Struct(Type#0)] | IrRef[Struct(Type#1)]]]]]
            Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]]
            Inst#1[Type#1, db-a, Field[Link#1, Str("pg")]]
            Inst#2[Type#2, my-stack, Field[Link#2, List[Inst(Inst#0), Inst(Inst#1)]]]
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
// Top-level links
// ---------------------------------------------------------------------------

#[test]
fn top_level_link_primitive() {
    assert_eq!(
        show("link image = reference"),
        "Link#0[image, Prim(reference)]",
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
            Type#0[zone, Enum[1|2|3]]
            Link#0[zone, IrRef[Enum(Type#0)]]
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
            Type#0[zone, Enum[1|2|3]]
            Type#1[region, Enum[eu-central|eu-west]]
            Link#0[location, IrRef[Enum(Type#1):Enum(Type#0)]]
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
            Type#0[service, Struct[Link#1[image, Prim(reference)]]]
            Type#1[database, Struct[Link#2[engine, Prim(string)]]]
            Link#0[access, List[IrRef[Struct(Type#0):(port)] | IrRef[Struct(Type#1)]]]
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
            Type#0[counter, Struct[Link#0[count, Prim(integer)]]]
            Inst#0[Type#0, c, Field[Link#0, Int(42)]]
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
            Type#0[svc, Struct[Link#0[label, Prim(string)]]]
            Inst#0[Type#0, my-svc, Field[Link#0, Str("hello")]]
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
            Type#0[zone, Enum[1|2|3]]
            Type#1[host, Struct[Link#0[zone, IrRef[Enum(Type#0)]]]]
            Inst#0[Type#1, my-host, Field[Link#0, Variant(Type#0, "2")]]
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
            Type#0[db, Struct[Link#0[engine, Prim(string)]]]
            Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]]
            Inst#0[Type#0, my-db, Field[Link#0, Str("pg")]]
            Inst#1[Type#1, my-svc, Field[Link#1, Inst(Inst#0)]]
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
            Type#0[db, Struct[Link#0[engine, Prim(string)]]]
            Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]]
            Inst#0[Type#1, my-svc, Field[Link#1, Inst(Inst#1)]]
            Inst#1[Type#0, my-db, Field[Link#0, Str("pg")]]
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
            Type#0[zone, Enum[1|2|3]]
            Type#1[region, Enum[eu-central|eu-west]]
            Type#2[svc, Struct[Link#0[location, IrRef[Enum(Type#1):Enum(Type#0)]]]]
            Inst#0[Type#2, s, Field[Link#0, Variant(Type#1, "eu-central"):Variant(Type#0, "2")]]
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
            Type#0[port, Enum[grpc|http]]
            Type#1[service, Struct[Link#0[port, IrRef[Enum(Type#0)]]]]
            Type#2[database, Struct[Link#1[engine, Prim(string)]]]
            Type#3[stack, Struct[Link#2[access, List[IrRef[Struct(Type#1):(Enum(Type#0))] | IrRef[Struct(Type#2)]]]]]
            Inst#0[Type#1, svc-a, Field[Link#0, Variant(Type#0, "grpc")]]
            Inst#1[Type#2, db-a, Field[Link#1, Str("pg")]]
            Inst#2[Type#3, my-stack, Field[Link#2, List[Inst(Inst#0):Variant(Type#0, "grpc"), Inst(Inst#1)]]]
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
        "Deploy[stack, aws, prod]",
    );
}

#[test]
fn deploy_multi_segment_ref() {
    assert_eq!(
        show("deploy stack to aws:eu-central as prd {}"),
        "Deploy[stack, aws:eu-central, prd]",
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
            Type#0[zone, Enum[1|2|3]]
            Type#1[region, Enum[eu-central|eu-west]]
            Type#2[svc, Struct[Link#0[location, IrRef[Enum(Type#1):Enum(Type#0)]]]]
            Inst#0[Type#2, s, Field[Link#0, Variant(Type#1, "eu-central")]]
            ERR: typed path has 1 segment(s), expected 2
        "#
        ),
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
            Type#0[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]]
            Type#1[svc, Struct[Link#2[scaling, IrRef[Struct(Type#0)]]]]
            Inst#0[Type#0, my-scaling, Field[Link#0, Int(1)], Field[Link#1, Int(10)]]
            Inst#1[Type#1, my-svc, Field[Link#2, Inst(Inst#0)]]
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
