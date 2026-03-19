/// Golden tests for the gen lowering pass (`ground_compile::asm`).
///
/// Each test calls `show(input)` which parses + resolves + lowers the source
/// and returns a compact, position-free string of the resulting AsmRes.
///
/// Conventions:
///   Symbol
///     Inst[type, name, ...]         — named instances (one per line, all in program)
///
///   Deploy[target, name]            — deploy with target path and name
///     inst: Inst[type, name, ...]   — the deploy instance (with overrides applied)
///     fields: k=v, ...              — deploy-specific fields
///
///   Str("x")                      — string value
///   Int(n)                        — integer value
///   Ref("x")                      — reference primitive
///   Variant(type, "x")            — resolved enum variant
///   InstRef(type, name)           — reference to a named instance (full data in Symbol)
///   Inst[type, name, ...]         — anonymous inline instance
///   val1:val2                     — typed path
///   List[val1, val2]              — list
#[path = "helpers/golden_asm_helpers.rs"] mod golden_asm_helpers;
use golden_asm_helpers::{norm, show};

// ---------------------------------------------------------------------------
// Basic deploys
// ---------------------------------------------------------------------------

#[test]
fn deploy_no_inst() {
    // `what` doesn't resolve to a known instance — deploy emitted with empty inst.
    let out = show("deploy ghost to aws as prod {}");
    assert!(out.contains("Deploy[aws, prod]"), "got: {}", out);
}

#[test]
fn deploy_simple_inst() {
    assert_eq!(
        show(r#"
            type stack = { link name = string }
            stack my-stack { name: "prod" }
            deploy my-stack to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[stack, my-stack, name=Str("prod")]
            Deploy[aws, prod]
              inst: Inst[stack, my-stack, name=Str("prod")]
        "#),
    );
}

// ---------------------------------------------------------------------------
// Field value types
// ---------------------------------------------------------------------------

#[test]
fn deploy_integer_field() {
    assert_eq!(
        show(r#"
            type counter = { link count = integer }
            counter c { count: 42 }
            deploy c to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[counter, c, count=Int(42)]
            Deploy[aws, prod]
              inst: Inst[counter, c, count=Int(42)]
        "#),
    );
}

#[test]
fn deploy_reference_field() {
    assert_eq!(
        show(r#"
            type svc = { link image = reference }
            svc s { image: "nginx:latest" }
            deploy s to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[svc, s, image=Ref("nginx:latest")]
            Deploy[aws, prod]
              inst: Inst[svc, s, image=Ref("nginx:latest")]
        "#),
    );
}

#[test]
fn deploy_enum_variant_field() {
    assert_eq!(
        show(r#"
            type zone = eu-central | eu-west
            type host = { link zone = zone }
            host h { zone: eu-west }
            deploy h to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[host, h, zone=Variant(zone, "eu-west")]
            Deploy[aws, prod]
              inst: Inst[host, h, zone=Variant(zone, "eu-west")]
        "#),
    );
}

#[test]
fn deploy_typed_path_field() {
    assert_eq!(
        show(r#"
            type zone   = 1 | 2 | 3
            type region = eu-central | eu-west
            type svc    = { link location = type:region:type:zone }
            svc s { location: eu-central:2 }
            deploy s to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[svc, s, location=Variant(region, "eu-central"):Variant(zone, "2")]
            Deploy[aws, prod]
              inst: Inst[svc, s, location=Variant(region, "eu-central"):Variant(zone, "2")]
        "#),
    );
}

// ---------------------------------------------------------------------------
// Symbol — named instances
// ---------------------------------------------------------------------------

#[test]
fn deploy_collects_named_inst_as_member() {
    assert_eq!(
        show(r#"
            type db  = { link engine = string }
            type svc = { link db = db }
            db  my-db  { engine: "pg" }
            svc my-svc { db: my-db }
            deploy my-svc to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[db, my-db, engine=Str("pg")]
              Inst[svc, my-svc, db=InstRef(db, my-db)]
            Deploy[aws, prod]
              inst: Inst[svc, my-svc, db=InstRef(db, my-db)]
        "#),
    );
}

#[test]
fn deploy_deduplicates_members() {
    // The same instance appears twice in the list — appears once in Symbol.
    assert_eq!(
        show(r#"
            type db    = { link engine = string }
            type stack = { link = [ type:db | type:db ] }
            db    my-db    { engine: "pg" }
            stack my-stack { my-db  my-db }
            deploy my-stack to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[db, my-db, engine=Str("pg")]
              Inst[stack, my-stack, _=List[InstRef(db, my-db), InstRef(db, my-db)]]
            Deploy[aws, prod]
              inst: Inst[stack, my-stack, _=List[InstRef(db, my-db), InstRef(db, my-db)]]
        "#),
    );
}

#[test]
fn deploy_list_of_members() {
    // The main terra backend pattern: stack with service and database members.
    assert_eq!(
        show(r#"
            type service  = { link image  = reference }
            type database = { link engine = string    }
            type stack    = { link = [ type:service | type:database ] }
            service  svc-a    { image:  "nginx" }
            database db-a     { engine: "pg"    }
            stack    my-stack { svc-a  db-a }
            deploy my-stack to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[service, svc-a, image=Ref("nginx")]
              Inst[database, db-a, engine=Str("pg")]
              Inst[stack, my-stack, _=List[InstRef(service, svc-a), InstRef(database, db-a)]]
            Deploy[aws, prod]
              inst: Inst[stack, my-stack, _=List[InstRef(service, svc-a), InstRef(database, db-a)]]
        "#),
    );
}

// ---------------------------------------------------------------------------
// Anonymous inline instances
// ---------------------------------------------------------------------------

#[test]
fn deploy_anonymous_inst_inlined() {
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
            deploy my-svc to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[svc, my-svc, scaling=Inst[scaling, _, min=Int(1), max=Int(10)]]
            Deploy[aws, prod]
              inst: Inst[svc, my-svc, scaling=Inst[scaling, _, min=Int(1), max=Int(10)]]
        "#),
    );
}

#[test]
fn deploy_named_inst_not_inlined() {
    // Named scaling instance → InstRef, not Inst[...].
    assert_eq!(
        show(r#"
            type scaling = {
                link min = integer
                link max = integer
            }
            type svc = { link scaling = scaling }
            scaling my-scaling { min: 1  max: 10 }
            svc     my-svc     { scaling: my-scaling }
            deploy my-svc to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[scaling, my-scaling, min=Int(1), max=Int(10)]
              Inst[svc, my-svc, scaling=InstRef(scaling, my-scaling)]
            Deploy[aws, prod]
              inst: Inst[svc, my-svc, scaling=InstRef(scaling, my-scaling)]
        "#),
    );
}

// ---------------------------------------------------------------------------
// Deploy-specific fields
// ---------------------------------------------------------------------------

#[test]
fn deploy_fields_are_separate_from_inst() {
    // Deploy fields (top-level links) appear under `fields:`, not in `inst:`.
    assert_eq!(
        show(r#"
            link prefix = string
            type svc = { link image = reference }
            svc my-svc { image: "nginx" }
            deploy my-svc to aws as prod { prefix: "acme" }
        "#),
        norm(r#"
            Symbol
              Inst[svc, my-svc, image=Ref("nginx")]
            Deploy[aws, prod]
              inst: Inst[svc, my-svc, image=Ref("nginx")]
              fields: prefix=Str("acme")
        "#),
    );
}

// ---------------------------------------------------------------------------
// Multi-segment target
// ---------------------------------------------------------------------------

#[test]
fn deploy_multi_segment_target() {
    assert_eq!(
        show(r#"
            type stack = { link name = string }
            stack s { name: "x" }
            deploy s to aws:eu-central as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[stack, s, name=Str("x")]
            Deploy[aws:eu-central, prod]
              inst: Inst[stack, s, name=Str("x")]
        "#),
    );
}

// ---------------------------------------------------------------------------
// Cross-references (access)
// ---------------------------------------------------------------------------

#[test]
fn deploy_services_access_each_other() {
    // Two services each referencing the other via access — InstRef in both directions.
    assert_eq!(
        show(r#"
            type service = {
                link image  = reference
                link access = [ type:service ]
            }
            type stack = { link = [ type:service ] }
            service svc-a { image: "nginx"  access: [ svc-b ] }
            service svc-b { image: "redis"  access: [ svc-a ] }
            stack my-stack { svc-a  svc-b }
            deploy my-stack to aws as prod {}
        "#),
        norm(r#"
            Symbol
              Inst[service, svc-a, image=Ref("nginx"), access=List[InstRef(service, svc-b)]]
              Inst[service, svc-b, image=Ref("redis"), access=List[InstRef(service, svc-a)]]
              Inst[stack, my-stack, _=List[InstRef(service, svc-a), InstRef(service, svc-b)]]
            Deploy[aws, prod]
              inst: Inst[stack, my-stack, _=List[InstRef(service, svc-a), InstRef(service, svc-b)]]
        "#),
    );
}
