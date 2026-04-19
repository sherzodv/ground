/// Golden tests for the parser (`ground_compile::parse`).
///
/// Each test calls `show(input)` which parses the source as a single unit
/// named "test" and returns a compact, position-free string of the scope tree.
/// Scopes are visible: output is always wrapped in `Scope[pack:test, ...]`.
/// Errors are surfaced as `ERR: <message>` lines at the end.

#[path = "helpers/golden_parse_helpers.rs"]
mod golden_parse_helpers;
use golden_parse_helpers::{norm, show};

// ---------------------------------------------------------------------------
// Basics
// ---------------------------------------------------------------------------

#[test]
fn basic_001() {
    assert_eq!(show(""), "Scope[pack:test]");
}

#[test]
fn basic_002() {
    assert_eq!(
        show(
            r##"
            # this is a comment
            x = a | b
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[x, x, unit, Enum[Ref(a) | Ref(b)]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[test]
fn enum_001() {
    assert_eq!(
        show("zone = 1 | 2 | 3 | 4 | 5"),
        norm(
            r##"
            Scope[pack:test,
                Def[zone, zone, unit, Enum[Ref(1) | Ref(2) | Ref(3) | Ref(4) | Ref(5)]],
            ]
        "##
        ),
    );
}

#[test]
fn enum_002() {
    assert_eq!(
        show("region = eu-central | eu-west | us-east | us-west | ap-southeast"),
        norm(
            r##"
            Scope[pack:test,
                Def[region, region, unit, Enum[Ref(eu-central) | Ref(eu-west) | Ref(us-east) | Ref(us-west) | Ref(ap-southeast)]],
            ]
        "##
        ),
    );
}

#[test]
fn enum_003() {
    assert_eq!(
        show("boo = type:foo | type:goo"),
        norm(
            r##"
            Scope[pack:test,
                Def[boo, boo, unit, Enum[Ref(type:foo) | Ref(type:goo)]],
            ]
        "##
        ),
    );
}

#[test]
fn enum_004() {
    assert_eq!(
        show("boo = plain | type:foo"),
        norm(
            r##"
            Scope[pack:test,
                Def[boo, boo, unit, Enum[Ref(plain) | Ref(type:foo)]],
            ]
        "##
        ),
    );
}

#[test]
fn def_001() {
    assert_eq!(
        show("def database"),
        norm(
            r##"
            Scope[pack:test,
                Def[database, database, unit, unit],
            ]
        "##
        ),
    );
}

#[test]
fn def_002() {
    assert_eq!(
        show("def database = {}"),
        norm(
            r##"
            Scope[pack:test,
                Def[database, database, unit, unit],
            ]
        "##
        ),
    );
}

#[test]
fn def_003() {
    assert_eq!(
        show("def database = unit"),
        norm(
            r##"
            Scope[pack:test,
                Def[database, database, unit, unit],
            ]
        "##
        ),
    );
}

#[test]
fn def_004() {
    // `def database = unit` — type alias via def keyword
    assert_eq!(
        show("def database = unit"),
        norm(
            r##"
            Scope[pack:test,
                Def[database, database, unit, unit],
            ]
        "##
        ),
    );
}

#[test]
fn struct_001() {
    assert_eq!(
        show("database = { engine = string }"),
        norm(
            r##"
            Scope[pack:test,
                Def[database, database, unit, Struct[FieldDef[engine, Type[_, Primitive(string)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn struct_002() {
    assert_eq!(
        show("database = { manage = self | provider | cloud }"),
        norm(
            r##"
            Scope[pack:test,
                Def[database, database, unit, Struct[FieldDef[manage, Type[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn struct_003() {
    assert_eq!(
        show("stack = { = service | database }"),
        norm(
            r##"
            Scope[pack:test,
                Def[stack, stack, unit, Struct[FieldDef[_, Type[_, Enum[Ref(service) | Ref(database)]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn struct_004() {
    assert_eq!(
        show("service = { def port = grpc | http }"),
        norm(
            r##"
            Scope[pack:test,
                Def[service, service, unit, Struct[Def[port, port, unit, Enum[Ref(grpc) | Ref(http)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn struct_005() {
    assert_eq!(
        show(
            r##"
            service = {
                image = reference
                scaling = {
                    min = integer
                }
            }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[service, service, unit, Struct[FieldDef[image, Type[_, Primitive(reference)]], FieldDef[scaling, Type[_, Struct[FieldDef[min, Type[_, Primitive(integer)]]]]]]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------

#[test]
fn field_001() {
    assert_eq!(
        show("image = reference"),
        norm(
            r##"
            Scope[pack:test,
                Def[image, image, unit, Primitive(reference)],
            ]
        "##
        ),
    );
    assert_eq!(
        show("count = integer"),
        norm(
            r##"
            Scope[pack:test,
                Def[count, count, unit, Primitive(integer)],
            ]
        "##
        ),
    );
    assert_eq!(
        show("label = string"),
        norm(
            r##"
            Scope[pack:test,
                Def[label, label, unit, Primitive(string)],
            ]
        "##
        ),
    );
}

#[test]
fn field_002() {
    assert_eq!(
        show("engine = postgresql"),
        norm(
            r##"
            Scope[pack:test,
                Def[engine, engine, unit, Ref(postgresql)],
            ]
        "##
        ),
    );
}

#[test]
fn field_003() {
    assert_eq!(
        show("manage = self | provider | cloud"),
        norm(
            r##"
            Scope[pack:test,
                Def[manage, manage, unit, Enum[Ref(self) | Ref(provider) | Ref(cloud)]],
            ]
        "##
        ),
    );
}

#[test]
fn field_004() {
    assert_eq!(
        show("access = [ service ]"),
        norm(
            r##"
            Scope[pack:test,
                Def[access, access, unit, List[Type[_, Ref(service)]]],
            ]
        "##
        ),
    );
}

#[test]
fn field_005() {
    assert_eq!(
        show("access = [ service | database ]"),
        norm(
            r##"
            Scope[pack:test,
                Def[access, access, unit, List[Type[_, Enum[Ref(service) | Ref(database)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn field_006() {
    assert_eq!(
        show("access = [ service:(port) | database ]"),
        norm(
            r##"
            Scope[pack:test,
                Def[access, access, unit, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn field_007() {
    assert_eq!(
        show("region = type:region:type:zone"),
        norm(
            r##"
            Scope[pack:test,
                Def[region, region, unit, Ref(type:region:type:zone)],
            ]
        "##
        ),
    );
}

#[test]
fn field_008() {
    assert_eq!(
        show("scaling = { min = integer  max = integer }"),
        norm(
            r##"
            Scope[pack:test,
                Def[scaling, scaling, unit, Struct[FieldDef[min, Type[_, Primitive(integer)]], FieldDef[max, Type[_, Primitive(integer)]]]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

#[test]
fn ref_001() {
    assert_eq!(
        show("foo = svc:(grpc)"),
        norm(
            r##"
            Scope[pack:test,
                Def[foo, foo, unit, Ref(svc:grpc?)],
            ]
        "##
        ),
    );
}

#[test]
fn ref_002() {
    assert_eq!(
        show("svc = service { image: my-org:my-svc:v2 }"),
        norm(
            r##"
            Scope[pack:test,
                Def[svc, service, unit, Struct[FieldSet[image, Ref(my-org:my-svc:v2)]]],
            ]
        "##
        ),
    );
}

#[test]
fn ref_003() {
    // {role:arn} → Group segment preserving inner structure
    assert_eq!(
        show("my-svc = svc { arn: {role:arn} }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[arn, Ref({role:arn})]]],
            ]
        "##
        ),
    );
}

#[test]
fn ref_004() {
    // {this:name}-sg → Group segment + plain trailing atom
    assert_eq!(
        show("my-svc = svc { name: {this:name}-sg }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[name, Ref({this:name}-sg)]]],
            ]
        "##
        ),
    );
}

#[test]
fn ref_005() {
    // {role:arn}:suffix → Group segment + colon-separated plain segment
    assert_eq!(
        show("my-svc = svc { name: {role:arn}:suffix }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[name, Ref({role:arn}:suffix)]]],
            ]
        "##
        ),
    );
}

#[test]
fn ref_006() {
    // {a:b}{c:d} → two adjacent Group segments, no colon separator needed
    assert_eq!(
        show("my-svc = svc { name: {a:b}{c:d} }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[name, Ref({a:b}:{c:d})]]],
            ]
        "##
        ),
    );
}

#[test]
fn ref_007() {
    // [{sg:id}] → list element is a Group ref
    assert_eq!(
        show("my-svc = svc { ids: [{sg:id}] }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[ids, List[Ref({sg:id})]]]],
            ]
        "##
        ),
    );
}

#[test]
fn ref_008() {
    // Plain { field: value } still parses as struct body, not brace group
    assert_eq!(
        show("my-svc = svc { cfg: { x: 5 } }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[cfg, Struct[Field[x, Ref(5)]]]]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Explicit Hook Defs
// ---------------------------------------------------------------------------

#[test]
fn hook_003() {
    assert_eq!(
        show("api service { image: api:prod }"),
        norm(
            r##"
            Scope[pack:test,
                Def[api, service, unit, Struct[FieldSet[image, Ref(api:prod)]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_004() {
    assert_eq!(
        show("api service"),
        norm(
            r##"
            Scope[pack:test,
                Def[api, service, unit, unit],
            ]
        "##
        ),
    );
}

#[test]
fn hook_005() {
    assert_eq!(
        show("svc-api = service {}"),
        norm(
            r##"
            Scope[pack:test,
                Def[svc-api, service, unit, unit],
            ]
        "##
        ),
    );
}

#[test]
fn hook_006() {
    assert_eq!(
        show("svc-api = service"),
        norm(
            r##"
            Scope[pack:test,
                Def[svc-api, svc-api, unit, Ref(service)],
            ]
        "##
        ),
    );
}

#[test]
fn hook_006a() {
    assert_eq!(
        show("def svc-api = service"),
        norm(
            r##"
            Scope[pack:test,
                Def[svc-api, svc-api, unit, Ref(service)],
            ]
        "##
        ),
    );
}

#[test]
fn hook_007() {
    assert_eq!(
        show("svc-api = service { image: svc-api:prod }"),
        norm(
            r##"
            Scope[pack:test,
                Def[svc-api, service, unit, Struct[FieldSet[image, Ref(svc-api:prod)]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_008() {
    assert_eq!(
        show("svc-api = service { image: svc-api:prod  port: grpc }"),
        norm(
            r##"
            Scope[pack:test,
                Def[svc-api, service, unit, Struct[FieldSet[image, Ref(svc-api:prod)], FieldSet[port, Ref(grpc)]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_009() {
    assert_eq!(
        show(r##"svc = service { label: "hello world" }"##),
        norm(
            r##"
            Scope[pack:test,
                Def[svc, service, unit, Struct[FieldSet[label, Str("hello world")]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_010() {
    assert_eq!(
        show("svc = service { zone: 5 }"),
        norm(
            r##"
            Scope[pack:test,
                Def[svc, service, unit, Struct[FieldSet[zone, Ref(5)]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_011() {
    assert_eq!(
        show("svc = service { access: [ svc-b svc-c ] }"),
        norm(
            r##"
            Scope[pack:test,
                Def[svc, service, unit, Struct[FieldSet[access, List[Ref(svc-b), Ref(svc-c)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_012() {
    assert_eq!(
        show(
            r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            my-svc = svc { scaling: { min: 1  max: 10 } }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[scaling, scaling, unit, Struct[FieldDef[min, Type[_, Primitive(integer)]], FieldDef[max, Type[_, Primitive(integer)]]]],
                Def[svc, svc, unit, Struct[FieldDef[scaling, Type[_, Ref(scaling)]]]],
                Def[my-svc, svc, unit, Struct[FieldSet[scaling, Struct[Field[min, Ref(1)], Field[max, Ref(10)]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_013() {
    assert_eq!(
        show(
            r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            my-scaling = scaling { min: 1  max: 10 }
            my-svc     = svc    { scaling: my-scaling }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[scaling, scaling, unit, Struct[FieldDef[min, Type[_, Primitive(integer)]], FieldDef[max, Type[_, Primitive(integer)]]]],
                Def[svc, svc, unit, Struct[FieldDef[scaling, Type[_, Ref(scaling)]]]],
                Def[my-scaling, scaling, unit, Struct[FieldSet[min, Ref(1)], FieldSet[max, Ref(10)]]],
                Def[my-svc, svc, unit, Struct[FieldSet[scaling, Ref(my-scaling)]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_014() {
    // Space-separated items → two separate Anon fields, each a plain Ref.
    assert_eq!(
        show("my-stack = stack { svc-a  svc-b }"),
        norm(
            r##"
            Scope[pack:test,
              Def[my-stack, stack, unit, Struct[Anon[Ref(svc-a)], Anon[Ref(svc-b)]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_015() {
    // Bracket-wrapped items → single Anon field containing a List value.
    assert_eq!(
        show("my-stack = stack { [ svc-a  svc-b ] }"),
        norm(
            r##"
            Scope[pack:test,
              Def[my-stack, stack, unit, Struct[Anon[List[Ref(svc-a), Ref(svc-b)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_016() {
    // Parser does not deduplicate — both Field entries are preserved as-is.
    assert_eq!(
        show(r##"my-db = db { engine: "pg"  engine: "mysql" }"##),
        norm(
            r##"
            Scope[pack:test,
              Def[my-db, db, unit, Struct[FieldSet[engine, Str("pg")], FieldSet[engine, Str("mysql")]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_017() {
    // Space-separated bare refs each become a separate Anon — no deduplication.
    assert_eq!(
        show(r##"my-stack = stack { svc-a  svc-a }"##),
        norm(
            r##"
            Scope[pack:test,
              Def[my-stack, stack, unit, Struct[Anon[Ref(svc-a)], Anon[Ref(svc-a)]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_018() {
    assert_eq!(
        show(
            r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            my-svc = svc { scaling: type:scaling { min: 2  max: 10 } }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[scaling, scaling, unit, Struct[FieldDef[min, Type[_, Primitive(integer)]], FieldDef[max, Type[_, Primitive(integer)]]]],
                Def[svc, svc, unit, Struct[FieldDef[scaling, Type[_, Ref(scaling)]]]],
                Def[my-svc, svc, unit, Struct[FieldSet[scaling, Struct[Hint(type:scaling), Field[min, Ref(2)], Field[max, Ref(10)]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_019() {
    // Hint without type: prefix is also valid
    assert_eq!(
        show(
            r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            my-svc = svc { scaling: scaling { min: 2  max: 10 } }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[scaling, scaling, unit, Struct[FieldDef[min, Type[_, Primitive(integer)]], FieldDef[max, Type[_, Primitive(integer)]]]],
                Def[svc, svc, unit, Struct[FieldDef[scaling, Type[_, Ref(scaling)]]]],
                Def[my-svc, svc, unit, Struct[FieldSet[scaling, Struct[Hint(scaling), Field[min, Ref(2)], Field[max, Ref(10)]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_020() {
    // Brace-group refs inside a list inside a nested struct body
    assert_eq!(
        show("my-svc = svc { cfg: { ids: [{sg:id}] } }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[cfg, Struct[Field[ids, List[Ref({sg:id})]]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_021() {
    // List element that is a brace-group ref with a trailing plain atom
    assert_eq!(
        show("my-svc = svc { names: [{this:name}-sg] }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[names, List[Ref({this:name}-sg)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_022() {
    // Type hint on a struct that is itself a field value inside an outer hinted struct
    assert_eq!(
        show("my-svc = svc { outer: outer_type { inner: inner_type { x: 1 } } }"),
        norm(
            r##"
            Scope[pack:test,
                Def[my-svc, svc, unit, Struct[FieldSet[outer, Struct[Hint(outer_type), Field[inner, Struct[Hint(inner_type), Field[x, Ref(1)]]]]]]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Use statements
// ---------------------------------------------------------------------------

#[test]
fn use_001() {
    assert_eq!(
        show("use std"),
        norm(
            r##"
            Scope[pack:test,
                Use[std],
            ]
        "##
        ),
    );
}

#[test]
fn use_002() {
    assert_eq!(
        show("use pack:std"),
        norm(
            r##"
            Scope[pack:test,
                Use[pack:std],
            ]
        "##
        ),
    );
}

#[test]
fn use_003() {
    assert_eq!(
        show("use pack:std:type:service"),
        norm(
            r##"
            Scope[pack:test,
                Use[pack:std:type:service],
            ]
        "##
        ),
    );
}

#[test]
fn use_004() {
    assert_eq!(
        show("use pack:std:*"),
        norm(
            r##"
            Scope[pack:test,
                Use[pack:std:*],
            ]
        "##
        ),
    );
}

#[test]
fn use_005() {
    assert_eq!(
        show("use pack:std:type:*"),
        norm(
            r##"
            Scope[pack:test,
                Use[pack:std:type:*],
            ]
        "##
        ),
    );
}

#[test]
fn use_006() {
    assert_eq!(
        show(
            r##"
            use pack:std:type:service
            stack = { name = string }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Use[pack:std:type:service],
                Def[stack, stack, unit, Struct[FieldDef[name, Type[_, Primitive(string)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn use_007() {
    assert_eq!(
        show(
            r##"
            use std
            api = std:service {}
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Use[std],
                Def[api, std:service, unit, unit],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Multi-unit scope
// ---------------------------------------------------------------------------

#[test]
fn scope_001() {
    use golden_parse_helpers::show_scope;
    use ground_compile::ast::{AstScopeId, ParseReq, ParseUnit};
    use ground_compile::parse::parse;

    let req = ParseReq {
        units: vec![
            ParseUnit {
                name: "web".into(),
                path: vec!["infra".into()],
                src: "image = reference".into(),
                ts_src: None,
            },
            ParseUnit {
                name: "db".into(),
                path: vec!["infra".into()],
                src: "engine = string".into(),
                ts_src: None,
            },
        ],
    };
    let res = parse(req);
    // scopes[0]=root, scopes[1]=infra, scopes[2]=web, scopes[3]=db
    assert_eq!(
        norm(&show_scope(&res.scopes, AstScopeId(1))),
        norm(
            r##"
            Scope[pack:infra,
                Scope[pack:web,
                    Def[image, image, unit, Primitive(reference)],
                ],
                Scope[pack:db,
                    Def[engine, engine, unit, Primitive(string)],
                ],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Regression / integration
// ---------------------------------------------------------------------------

#[test]
fn integration_001() {
    let src = r##"
        zone   = 1 | 2 | 3 | 4 | 5
        region = eu-central | eu-west | us-east | us-west | ap-southeast
        access = [ service:(port) | database ]
        database = {
          manage  = self | provider | cloud
          engine  = postgresql | mongodb
          version = string
        }
        service = {
          def port   = grpc | http
          image  = reference
          access = [ service:(port) | database ]
          scaling = {
            min = integer
            max = integer
          }
        }
        region_path = type:region:type:zone
    "##;
    assert_eq!(
        show(src),
        norm(
            r##"
            Scope[pack:test,
                Def[zone, zone, unit, Enum[Ref(1) | Ref(2) | Ref(3) | Ref(4) | Ref(5)]],
                Def[region, region, unit, Enum[Ref(eu-central) | Ref(eu-west) | Ref(us-east) | Ref(us-west) | Ref(ap-southeast)]],
                Def[access, access, unit, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]],
                Def[database, database, unit, Struct[FieldDef[manage, Type[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]], FieldDef[engine, Type[_, Enum[Ref(postgresql) | Ref(mongodb)]]], FieldDef[version, Type[_, Primitive(string)]]]],
                Def[service, service, unit, Struct[Def[port, port, unit, Enum[Ref(grpc) | Ref(http)]], FieldDef[image, Type[_, Primitive(reference)]], FieldDef[access, Type[_, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]]], FieldDef[scaling, Type[_, Struct[FieldDef[min, Type[_, Primitive(integer)]], FieldDef[max, Type[_, Primitive(integer)]]]]]]],
                Def[region_path, region_path, unit, Ref(type:region:type:zone)],
            ]
        "##
        ),
    );
}

#[test]
fn integration_002() {
    assert_eq!(
        show(
            r##"
            service = {
                def port   = grpc | http
                sidecar = {
                    service = type:service:(port)
                }
            }
            upstream = service {}
            my-svc   = service {
                sidecar: {
                    service: upstream:grpc
                }
            }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[service, service, unit, Struct[Def[port, port, unit, Enum[Ref(grpc) | Ref(http)]], FieldDef[sidecar, Type[_, Struct[FieldDef[service, Type[_, Ref(type:service:port?)]]]]]]],
                Def[upstream, service, unit, unit],
                Def[my-svc, service, unit, Struct[FieldSet[sidecar, Struct[Field[service, Ref(upstream:grpc)]]]]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Hook defs
// ---------------------------------------------------------------------------

#[test]
fn hook_001() {
    assert_eq!(
        show(
            r##"
            def make_service { svc = service } = make_service {
                sg     = aws_security_group
                egress = aws_vpc_security_group_egress_rule
            }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[make_service, make_service, Input[Field[svc, Type[_, Ref(service)]]], Struct[FieldDef[sg, Type[_, Ref(aws_security_group)]], FieldDef[egress, Type[_, Ref(aws_vpc_security_group_egress_rule)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn hook_002() {
    use golden_parse_helpers::show_multi;
    assert_eq!(
        show_multi(vec![(
            "ecs",
            vec!["aws"],
            r##"
                def make_service { svc = service } = make_service {
                    sg = aws_security_group
                }
            "##
        ),]),
        norm(
            r##"
            Scope[pack:aws,
                Scope[pack:ecs,
                    Def[make_service, make_service, Input[Field[svc, Type[_, Ref(service)]]], Struct[FieldDef[sg, Type[_, Ref(aws_security_group)]]]],
                ],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// New syntax — keyword-free defs
// ---------------------------------------------------------------------------

#[test]
fn def_011() {
    assert_eq!(
        show("port = http | grpc"),
        norm(
            r##"
            Scope[pack:test,
                Def[port, port, unit, Enum[Ref(http) | Ref(grpc)]],
            ]
        "##
        ),
    );
}

#[test]
fn def_012() {
    assert_eq!(
        show("x = service"),
        norm(
            r##"
            Scope[pack:test,
                Def[x, x, unit, Ref(service)],
            ]
        "##
        ),
    );
}

#[test]
fn def_013() {
    assert_eq!(
        show("ports = [ port ]"),
        norm(
            r##"
            Scope[pack:test,
                Def[ports, ports, unit, List[Type[_, Ref(port)]]],
            ]
        "##
        ),
    );
}

#[test]
fn def_014() {
    assert_eq!(
        show("service = { port = grpc | http }"),
        norm(
            r##"
            Scope[pack:test,
                Def[service, service, unit, Struct[FieldDef[port, Type[_, Enum[Ref(grpc) | Ref(http)]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn def_015() {
    assert_eq!(
        show("service = { port = grpc | http  image = reference }"),
        norm(
            r##"
            Scope[pack:test,
                Def[service, service, unit, Struct[FieldDef[port, Type[_, Enum[Ref(grpc) | Ref(http)]]], FieldDef[image, Type[_, Primitive(reference)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn def_016() {
    assert_eq!(
        show("stack = { = [ service | database ] }"),
        norm(
            r##"
            Scope[pack:test,
                Def[stack, stack, unit, Struct[FieldDef[_, Type[_, List[Type[_, Enum[Ref(service) | Ref(database)]]]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn def_017() {
    assert_eq!(
        show("service = { access = [ service:(port) | database ] }"),
        norm(
            r##"
            Scope[pack:test,
                Def[service, service, unit, Struct[FieldDef[access, Type[_, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]]]]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// New syntax — def keyword
// ---------------------------------------------------------------------------

#[test]
fn def_005() {
    assert_eq!(
        show("def secret"),
        norm(
            r##"
            Scope[pack:test,
                Def[secret, secret, unit, unit],
            ]
        "##
        ),
    );
}

#[test]
fn def_006() {
    assert_eq!(
        show("def port = http | grpc"),
        norm(
            r##"
            Scope[pack:test,
                Def[port, port, unit, Enum[Ref(http) | Ref(grpc)]],
            ]
        "##
        ),
    );
}

#[test]
fn def_007() {
    assert_eq!(
        show("def service = { port = grpc | http }"),
        norm(
            r##"
            Scope[pack:test,
                Def[service, service, unit, Struct[FieldDef[port, Type[_, Enum[Ref(grpc) | Ref(http)]]]]],
            ]
        "##
        ),
    );
}

#[test]
fn def_008() {
    assert_eq!(
        show("def node { name = string } = { ep = endpoint }"),
        norm(
            r##"
            Scope[pack:test,
                Def[node, node, Input[Field[name, Type[_, Primitive(string)]]], Struct[FieldDef[ep, Type[_, Ref(endpoint)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn def_009() {
    assert_eq!(
        show("def node { name = string } = make_node { ep = endpoint }"),
        norm(
            r##"
            Scope[pack:test,
                Def[node, make_node, Input[Field[name, Type[_, Primitive(string)]]], Struct[FieldDef[ep, Type[_, Ref(endpoint)]]]],
            ]
        "##
        ),
    );
}

#[test]
fn def_010() {
    assert_eq!(
        show("def make_service { svc = service  d = deploy } = make_service { sg = aws_security_group }"),
        norm(r##"
            Scope[pack:test,
                Def[make_service, make_service, Input[Field[svc, Type[_, Ref(service)]], Field[d, Type[_, Ref(deploy)]]], Struct[FieldDef[sg, Type[_, Ref(aws_security_group)]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// New syntax — def in struct body (nested named def)
// ---------------------------------------------------------------------------

#[test]
fn struct_006() {
    assert_eq!(
        show("s = { def scaling = { min = integer  max = integer } }"),
        norm(
            r##"
            Scope[pack:test,
                Def[s, s, unit, Struct[Def[scaling, scaling, unit, Struct[FieldDef[min, Type[_, Primitive(integer)]], FieldDef[max, Type[_, Primitive(integer)]]]]]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// New syntax — def: qualifier in refs
// ---------------------------------------------------------------------------

#[test]
fn qualifier_001() {
    assert_eq!(
        show("stack = { = [ def:service | def:database ] }"),
        norm(
            r##"
            Scope[pack:test,
                Def[stack, stack, unit, Struct[FieldDef[_, Type[_, List[Type[_, Enum[Ref(def:service) | Ref(def:database)]]]]]]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// New syntax — pack declarations
// ---------------------------------------------------------------------------

#[test]
fn pack_001() {
    assert_eq!(
        show("pack std:aws"),
        norm(
            r##"
            Scope[pack:test,
                Pack[std:aws],
            ]
        "##
        ),
    );
}

#[test]
fn pack_002() {
    assert_eq!(
        show("pack std"),
        norm(
            r##"
            Scope[pack:test,
                Pack[std],
            ]
        "##
        ),
    );
}

#[test]
fn pack_003() {
    assert_eq!(
        show("pack std:aws { port = http | grpc }"),
        norm(
            r##"
            Scope[pack:test,
                Pack[std:aws,
                    Def[port, port, unit, Enum[Ref(http) | Ref(grpc)]],
                ],
            ]
        "##
        ),
    );
}

#[test]
fn pack_004() {
    assert_eq!(
        show("pack std { pack aws { port = http | grpc } }"),
        norm(
            r##"
            Scope[pack:test,
                Pack[std,
                    Pack[aws,
                        Def[port, port, unit, Enum[Ref(http) | Ref(grpc)]],
                    ],
                ],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// New syntax — plan declarations
// ---------------------------------------------------------------------------

#[test]
fn plan_001() {
    assert_eq!(
        show("plan prd-eu"),
        norm(
            r##"
            Scope[pack:test,
                Plan[prd-eu],
            ]
        "##
        ),
    );
}

#[test]
fn plan_002() {
    assert_eq!(
        show("plan prd-eu { region: eu-central }"),
        norm(
            r##"
            Scope[pack:test,
                Plan[prd-eu, Field[region, Ref(eu-central)]],
            ]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// New syntax — explicit hook defs and use still compose cleanly
// ---------------------------------------------------------------------------

#[test]
fn hook_023() {
    assert_eq!(
        show(
            r##"
            service = { port = grpc | http }
            api = service { port: grpc }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Def[service, service, unit, Struct[FieldDef[port, Type[_, Enum[Ref(grpc) | Ref(http)]]]]],
                Def[api, service, unit, Struct[FieldSet[port, Ref(grpc)]]],
            ]
        "##
        ),
    );
}

#[test]
fn use_008() {
    assert_eq!(
        show(
            r##"
            use std
            service = { port = grpc | http }
        "##
        ),
        norm(
            r##"
            Scope[pack:test,
                Use[std],
                Def[service, service, unit, Struct[FieldDef[port, Type[_, Enum[Ref(grpc) | Ref(http)]]]]],
            ]
        "##
        ),
    );
}
