/// Golden tests for the RFC-0005 parser (`ground_compile::parse`).
///
/// Each test calls `show(input)` which parses the source as a single unit
/// named "test" and returns a compact, position-free string of the scope tree.
/// Scopes are visible: output is always wrapped in `Scope[pack:test, ...]`.
/// Errors are surfaced as `ERR: <message>` lines at the end.
#[path = "helpers/golden_parse_helpers.rs"] mod golden_parse_helpers;
use golden_parse_helpers::{norm, show};

#[test]
fn empty_file() {
    assert_eq!(show(""), "Scope[pack:test]");
}

#[test]
fn line_comment_ignored() {
    assert_eq!(
        show(
            r#"
            # this is a comment
            type x = a | b
            "#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type[x, Enum[Ref(a) | Ref(b)]],
            ]
            "#
        ),
    );
}

#[test]
fn integer_enum() {
    assert_eq!(
        show("type zone = 1 | 2 | 3 | 4 | 5"),
        norm(
            r#"
            Scope[pack:test,
                Type[zone, Enum[Ref(1) | Ref(2) | Ref(3) | Ref(4) | Ref(5)]],
            ]
            "#
        ),
    );
}

#[test]
fn ident_enum() {
    assert_eq!(
        show("type region = eu-central | eu-west | us-east | us-west | ap-southeast"),
        norm(
            r#"
            Scope[pack:test,
                Type[region, Enum[Ref(eu-central) | Ref(eu-west) | Ref(us-east) | Ref(us-west) | Ref(ap-southeast)]],
            ]
        "#
        ),
    );
}

#[test]
fn struct_type_primitive_link() {
    assert_eq!(
        show("type database = { link engine = string }"),
        norm(
            r#"
            Scope[pack:test,
                Type[database, Struct[Link[engine, Type[_, Primitive(string)]]]],
                Scope[type:database],
            ]
        "#
        ),
    );
}

#[test]
fn struct_type_link_union() {
    assert_eq!(
        show("type database = { link manage = self | provider | cloud }"),
        norm(
            r#"
            Scope[pack:test,
                Type[database, Struct[Link[manage, Type[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]]]],
                Scope[type:database],
            ]
        "#
        ),
    );
}

#[test]
fn struct_type_with_inline_type() {
    assert_eq!(
        show("type service = { type port = grpc | http }"),
        norm(
            r#"
            Scope[pack:test,
                Type[service, Struct[]],
                Scope[type:service,
                    Type[port, Enum[Ref(grpc) | Ref(http)]],
                ],
            ]
        "#
        ),
    );
}

#[test]
fn struct_type_with_inline_link() {
    assert_eq!(
        show(
            r#"
            type service = {
                link image = reference
                link scaling = type scaling = {
                    link min = integer
                }
            }"#
        ),
        norm(
            r#"
            Scope[pack:test,
                Type[service, Struct[Link[image, Type[_, Primitive(reference)]], Link[scaling, Type[scaling, Struct[Link[min, Type[_, Primitive(integer)]]]]]]],
                Scope[type:service,
                    Scope[type:scaling],
                ],
            ]
        "#
        ),
    );
}

#[test]
fn struct_type_bare_ref_is_error() {
    let out = show("type stack = { service database }");
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
}

#[test]
fn struct_type_anon_link() {
    assert_eq!(
        show("type stack = { link = service | database }"),
        norm(
            r#"
            Scope[pack:test,
                Type[stack, Struct[Link[_, Type[_, Enum[Ref(service) | Ref(database)]]]]],
                Scope[type:stack],
            ]
        "#
        ),
    );
}

#[test]
fn link_primitive() {
    assert_eq!(
        show("link image = reference"),
        norm(
            r#"
            Scope[pack:test,
                Link[image, Type[_, Primitive(reference)]],
            ]
        "#
        ),
    );
    assert_eq!(
        show("link count = integer"),
        norm(
            r#"
            Scope[pack:test,
                Link[count, Type[_, Primitive(integer)]],
            ]
        "#
        ),
    );
    assert_eq!(
        show("link label = string"),
        norm(
            r#"
            Scope[pack:test,
                Link[label, Type[_, Primitive(string)]],
            ]
        "#
        ),
    );
}

#[test]
fn link_single_ref() {
    assert_eq!(
        show("link engine = postgresql"),
        norm(
            r#"
            Scope[pack:test,
                Link[engine, Type[_, Ref(postgresql)]],
            ]
        "#
        ),
    );
}

#[test]
fn link_ref_union() {
    assert_eq!(
        show("link manage = self | provider | cloud"),
        norm(
            r#"
            Scope[pack:test,
                Link[manage, Type[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]],
            ]
        "#
        ),
    );
}

#[test]
fn link_list_single_ref() {
    assert_eq!(
        show("link access = [ service ]"),
        norm(
            r#"
            Scope[pack:test,
                Link[access, Type[_, List[Type[_, Ref(service)]]]],
            ]
        "#
        ),
    );
}

#[test]
fn link_list_union() {
    assert_eq!(
        show("link access = [ service | database ]"),
        norm(
            r#"
            Scope[pack:test,
                Link[access, Type[_, List[Type[_, Enum[Ref(service) | Ref(database)]]]]],
            ]
        "#
        ),
    );
}

#[test]
fn link_list_with_optional_ref_seg() {
    assert_eq!(
        show("link access = [ service:(port) | database ]"),
        norm(
            r#"
            Scope[pack:test,
                Link[access, Type[_, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]]],
            ]
        "#
        ),
    );
}

#[test]
fn link_typed_path() {
    assert_eq!(
        show("link region = type:region:type:zone"),
        norm(
            r#"
            Scope[pack:test,
                Link[region, Type[_, Ref(type:region:type:zone)]],
            ]
        "#
        ),
    );
}

#[test]
fn link_inline_named_type() {
    assert_eq!(
        show("link scaling = type scaling = { link min = integer  link max = integer }"),
        norm(
            r#"
            Scope[pack:test,
                Link[scaling, Type[scaling, Struct[Link[min, Type[_, Primitive(integer)]], Link[max, Type[_, Primitive(integer)]]]]],
            ]
        "#
        ),
    );
}

#[test]
fn inst_no_fields() {
    assert_eq!(
        show("service svc-api {}"),
        norm(
            r#"
            Scope[pack:test,
                Inst[service, svc-api],
            ]
        "#
        ),
    );
}

#[test]
fn inst_no_braces() {
    assert_eq!(
        show("service svc-api"),
        norm(
            r#"
            Scope[pack:test,
                Inst[service, svc-api],
            ]
        "#
        ),
    );
}

#[test]
fn inst_single_field() {
    assert_eq!(
        show("service svc-api { image: svc-api:prod }"),
        norm(
            r#"
            Scope[pack:test,
                Inst[service, svc-api, Field[image, Ref(svc-api:prod)]],
            ]
        "#
        ),
    );
}

#[test]
fn inst_multiple_fields() {
    assert_eq!(
        show("service svc-api { image: svc-api:prod  port: grpc }"),
        norm(
            r#"
            Scope[pack:test,
                Inst[service, svc-api, Field[image, Ref(svc-api:prod)], Field[port, Ref(grpc)]],
            ]
        "#
        ),
    );
}

#[test]
fn inst_list_field() {
    assert_eq!(
        show("service svc { access: [ svc-b svc-c ] }"),
        norm(
            r#"
            Scope[pack:test,
                Inst[service, svc, Field[access, List[Ref(svc-b), Ref(svc-c)]]],
            ]
        "#
        ),
    );
}

#[test]
fn inst_string_field() {
    assert_eq!(
        show(r#"service svc { label: "hello world" }"#),
        norm(
            r#"
            Scope[pack:test,
                Inst[service, svc, Field[label, Str("hello world")]],
            ]
        "#
        ),
    );
}

#[test]
fn inst_integer_field() {
    assert_eq!(
        show("service svc { zone: 5 }"),
        norm(
            r#"
            Scope[pack:test,
                Inst[service, svc, Field[zone, Ref(5)]],
            ]
        "#
        ),
    );
}

#[test]
fn deploy_no_fields() {
    assert_eq!(
        show("deploy stack to aws as prod {}"),
        norm(
            r#"
            Scope[pack:test,
                Deploy[stack, aws, prod],
            ]
        "#
        ),
    );
}

#[test]
fn deploy_with_ref_segments() {
    assert_eq!(
        show("deploy stack to aws:eu-central as prd {}"),
        norm(
            r#"
            Scope[pack:test,
                Deploy[stack, aws:eu-central, prd],
            ]
        "#
        ),
    );
}

#[test]
fn deploy_with_fields() {
    assert_eq!(
        show("deploy stack to aws as prod { region: eu-central:3 }"),
        norm(
            r#"
            Scope[pack:test,
                Deploy[stack, aws, prod, Field[region, Ref(eu-central:3)]],
            ]
        "#
        ),
    );
}

#[test]
fn multiple_defs() {
    let src = r#"
        type zone   = 1 | 2 | 3
        type region = eu-central | eu-west
        link access = [ service:(port) | database ]
        service svc { image: svc:prod }
        deploy svc to aws as prod {}
    "#;
    assert_eq!(
        show(src),
        norm(
            r#"
            Scope[pack:test,
                Type[zone, Enum[Ref(1) | Ref(2) | Ref(3)]],
                Type[region, Enum[Ref(eu-central) | Ref(eu-west)]],
                Link[access, Type[_, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]]],
                Inst[service, svc, Field[image, Ref(svc:prod)]],
                Deploy[svc, aws, prod],
            ]
        "#
        ),
    );
}

#[test]
fn stdlib_subset() {
    let src = r#"
        type zone   = 1 | 2 | 3 | 4 | 5
        type region = eu-central | eu-west | us-east | us-west | ap-southeast
        link access = [ service:(port) | database ]
        type database = {
          link manage  = self | provider | cloud
          link engine  = postgresql | mongodb
          link version = string
        }
        type service = {
          type port   = grpc | http
          link image  = reference
          link access = [ service:(port) | database ]
          link scaling = type scaling = {
            link min = integer
            link max = integer
          }
        }
        link region = type:region:type:zone
    "#;
    assert_eq!(
        show(src),
        norm(
            r#"
            Scope[pack:test,
                Type[zone, Enum[Ref(1) | Ref(2) | Ref(3) | Ref(4) | Ref(5)]],
                Type[region, Enum[Ref(eu-central) | Ref(eu-west) | Ref(us-east) | Ref(us-west) | Ref(ap-southeast)]],
                Link[access, Type[_, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]]],
                Type[database, Struct[Link[manage, Type[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]], Link[engine, Type[_, Enum[Ref(postgresql) | Ref(mongodb)]]], Link[version, Type[_, Primitive(string)]]]],
                Type[service, Struct[Link[image, Type[_, Primitive(reference)]], Link[access, Type[_, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]]], Link[scaling, Type[scaling, Struct[Link[min, Type[_, Primitive(integer)]], Link[max, Type[_, Primitive(integer)]]]]]]],
                Link[region, Type[_, Ref(type:region:type:zone)]],
                Scope[type:database],
                Scope[type:service,
                    Type[port, Enum[Ref(grpc) | Ref(http)]],
                    Scope[type:scaling],
                ],
            ]
        "#
        ),
    );
}

#[test]
fn error_unexpected_top_level() {
    let out = show("= foo");
    assert!(out.contains("ERR:"), "expected an error, got: {}", out);
}

#[test]
fn error_collected_continue_parsing() {
    let out = show("!! bad\ntype x = a | b");
    assert!(
        out.contains("Type[x, Enum[Ref(a) | Ref(b)]]"),
        "expected recovered def in: {}",
        out
    );
    assert!(out.contains("ERR:"), "expected error in: {}", out);
}

#[test]
fn ref_optional_segment() {
    assert_eq!(
        show("link foo = svc:(grpc)"),
        norm(
            r#"
            Scope[pack:test,
                Link[foo, Type[_, Ref(svc:grpc?)]],
            ]
        "#
        ),
    );
}

#[test]
fn ref_multi_segment_value() {
    assert_eq!(
        show("service svc { image: my-org:my-svc:v2 }"),
        norm(
            r#"
            Scope[pack:test,
                Inst[service, svc, Field[image, Ref(my-org:my-svc:v2)]],
            ]
        "#
        ),
    );
}

#[test]
fn multi_unit_shared_path() {
    use golden_parse_helpers::show_scope;
    use ground_compile::ast::{AstScopeId, ParseReq, ParseUnit};
    use ground_compile::parse::parse;

    let req = ParseReq {
        units: vec![
            ParseUnit {
                name: "web".into(),
                path: vec!["infra".into()],
                src: "link image = reference".into(),
            },
            ParseUnit {
                name: "db".into(),
                path: vec!["infra".into()],
                src: "link engine = string".into(),
            },
        ],
    };
    let res = parse(req);
    // scopes[0]=root, scopes[1]=infra, scopes[2]=web, scopes[3]=db
    assert_eq!(
        norm(&show_scope(&res.scopes, AstScopeId(1))),
        norm(
            r#"
            Scope[pack:infra,
                Scope[pack:web,
                    Link[image, Type[_, Primitive(reference)]],
                ],
                Scope[pack:db,
                    Link[engine, Type[_, Primitive(string)]],
                ],
            ]
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
                Type[service, Struct[Link[sidecar, Type[sidecar, Struct[Link[service, Type[_, Ref(type:service:port?)]]]]]]],
                Inst[service, upstream],
                Inst[service, my-svc, Field[sidecar, Struct[Field[service, Ref(upstream:grpc)]]]],
                Scope[type:service,
                    Type[port, Enum[Ref(grpc) | Ref(http)]],
                    Scope[type:sidecar],
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
                Type[scaling, Struct[Link[min, Type[_, Primitive(integer)]], Link[max, Type[_, Primitive(integer)]]]],
                Type[svc, Struct[Link[scaling, Type[_, Ref(scaling)]]]],
                Inst[svc, my-svc, Field[scaling, Struct[Field[min, Ref(1)], Field[max, Ref(10)]]]],
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
                Type[scaling, Struct[Link[min, Type[_, Primitive(integer)]], Link[max, Type[_, Primitive(integer)]]]],
                Type[svc, Struct[Link[scaling, Type[_, Ref(scaling)]]]],
                Inst[scaling, my-scaling, Field[min, Ref(1)], Field[max, Ref(10)]],
                Inst[svc, my-svc, Field[scaling, Ref(my-scaling)]],
                Scope[type:scaling],
                Scope[type:svc],
            ]
        "#
        ),
    );
}

// ---------------------------------------------------------------------------
// Anonymous list link instantiation
// ---------------------------------------------------------------------------

#[test]
fn inst_anon_list_space_separated() {
    // Space-separated items → two separate Anon fields, each a plain Ref.
    assert_eq!(
        show("stack my-stack { svc-a  svc-b }"),
        norm(r#"
            Scope[pack:test,
              Inst[stack, my-stack, Anon[Ref(svc-a)], Anon[Ref(svc-b)]],
            ]
        "#),
    );
}

#[test]
fn inst_anon_list_bracket_wrapped() {
    // Bracket-wrapped items → single Anon field containing a List value.
    assert_eq!(
        show("stack my-stack { [ svc-a  svc-b ] }"),
        norm(r#"
            Scope[pack:test,
              Inst[stack, my-stack, Anon[List[Ref(svc-a), Ref(svc-b)]]],
            ]
        "#),
    );
}

#[test]
fn inst_duplicate_named_field_allowed_by_parser() {
    // Parser does not deduplicate — both Field entries are preserved as-is.
    assert_eq!(
        show(r#"db my-db { engine: "pg"  engine: "mysql" }"#),
        norm(r#"
            Scope[pack:test,
              Inst[db, my-db, Field[engine, Str("pg")], Field[engine, Str("mysql")]],
            ]
        "#),
    );
}

#[test]
fn inst_duplicate_anon_field_allowed_by_parser() {
    // Space-separated bare refs each become a separate Anon — no deduplication.
    assert_eq!(
        show(r#"stack my-stack { svc-a  svc-a }"#),
        norm(r#"
            Scope[pack:test,
              Inst[stack, my-stack, Anon[Ref(svc-a)], Anon[Ref(svc-a)]],
            ]
        "#),
    );
}

// ---------------------------------------------------------------------------
// Use statements
// ---------------------------------------------------------------------------

#[test]
fn use_bare_name() {
    assert_eq!(
        show("use std"),
        norm(r#"
            Scope[pack:test,
                Use[std],
            ]
        "#),
    );
}

#[test]
fn use_pack_qualified() {
    assert_eq!(
        show("use pack:std"),
        norm(r#"
            Scope[pack:test,
                Use[pack:std],
            ]
        "#),
    );
}

#[test]
fn use_type_specific() {
    assert_eq!(
        show("use pack:std:type:service"),
        norm(r#"
            Scope[pack:test,
                Use[pack:std:type:service],
            ]
        "#),
    );
}

#[test]
fn use_wildcard() {
    assert_eq!(
        show("use pack:std:*"),
        norm(r#"
            Scope[pack:test,
                Use[pack:std:*],
            ]
        "#),
    );
}

#[test]
fn use_type_wildcard() {
    assert_eq!(
        show("use pack:std:type:*"),
        norm(r#"
            Scope[pack:test,
                Use[pack:std:type:*],
            ]
        "#),
    );
}

#[test]
fn use_alongside_defs() {
    assert_eq!(
        show(r#"
            use pack:std:type:service
            type stack = { link name = string }
        "#),
        norm(r#"
            Scope[pack:test,
                Use[pack:std:type:service],
                Type[stack, Struct[Link[name, Type[_, Primitive(string)]]]],
                Scope[type:stack],
            ]
        "#),
    );
}

#[test]
fn use_then_qualified_inst() {
    assert_eq!(
        show("use std\nstd:service api {}"),
        norm(r#"
            Scope[pack:test,
                Use[std],
                Inst[std:service, api],
            ]
        "#),
    );
}

// ---------------------------------------------------------------------------
// Type hints on struct values
// ---------------------------------------------------------------------------

#[test]
fn inst_struct_value_with_type_hint() {
    assert_eq!(
        show(r#"
            type scaling = {
                link min = integer
                link max = integer
            }
            type svc = { link scaling = scaling }
            svc my-svc {
                scaling: type:scaling { min: 2  max: 10 }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                Type[scaling, Struct[Link[min, Type[_, Primitive(integer)]], Link[max, Type[_, Primitive(integer)]]]],
                Type[svc, Struct[Link[scaling, Type[_, Ref(scaling)]]]],
                Inst[svc, my-svc, Field[scaling, Struct[Hint(type:scaling), Field[min, Ref(2)], Field[max, Ref(10)]]]],
                Scope[type:scaling],
                Scope[type:svc],
            ]
        "#),
    );
}

#[test]
fn inst_struct_value_bare_hint() {
    // Hint without type: prefix is also valid
    assert_eq!(
        show(r#"
            type scaling = {
                link min = integer
                link max = integer
            }
            type svc = { link scaling = scaling }
            svc my-svc {
                scaling: scaling { min: 2  max: 10 }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                Type[scaling, Struct[Link[min, Type[_, Primitive(integer)]], Link[max, Type[_, Primitive(integer)]]]],
                Type[svc, Struct[Link[scaling, Type[_, Ref(scaling)]]]],
                Inst[svc, my-svc, Field[scaling, Struct[Hint(scaling), Field[min, Ref(2)], Field[max, Ref(10)]]]],
                Scope[type:scaling],
                Scope[type:svc],
            ]
        "#),
    );
}
