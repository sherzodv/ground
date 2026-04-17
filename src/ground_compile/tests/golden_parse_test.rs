/// Golden tests for the parser (`ground_compile::parse`).
///
/// Each test calls `show(input)` which parses the source as a single unit
/// named "test" and returns a compact, position-free string of the scope tree.
/// Scopes are visible: output is always wrapped in `Scope[pack:test, ...]`.
/// Errors are surfaced as `ERR: <message>` lines at the end.

#[path = "helpers/golden_parse_helpers.rs"] mod golden_parse_helpers;
use golden_parse_helpers::{norm, show};

// ---------------------------------------------------------------------------
// Basics
// ---------------------------------------------------------------------------

#[test]
fn empty_file() {
    assert_eq!(show(""), "Scope[pack:test]");
}

#[test]
fn line_comment_ignored() {
    assert_eq!(
        show(r##"
            # this is a comment
            x = a | b
        "##),
        norm(r##"
            Scope[pack:test,
                Def[x, Enum[Ref(a) | Ref(b)]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[test]
fn integer_enum() {
    assert_eq!(
        show("zone = 1 | 2 | 3 | 4 | 5"),
        norm(r##"
            Scope[pack:test,
                Def[zone, Enum[Ref(1) | Ref(2) | Ref(3) | Ref(4) | Ref(5)]],
            ]
        "##),
    );
}

#[test]
fn ident_enum() {
    assert_eq!(
        show("region = eu-central | eu-west | us-east | us-west | ap-southeast"),
        norm(r##"
            Scope[pack:test,
                Def[region, Enum[Ref(eu-central) | Ref(eu-west) | Ref(us-east) | Ref(us-west) | Ref(ap-southeast)]],
            ]
        "##),
    );
}

#[test]
fn type_enum_typed_ref_variants() {
    assert_eq!(
        show("boo = type:foo | type:goo"),
        norm(r##"
            Scope[pack:test,
                Def[boo, Enum[Ref(type:foo) | Ref(type:goo)]],
            ]
        "##),
    );
}

#[test]
fn type_enum_mixed_plain_and_typed_ref() {
    assert_eq!(
        show("boo = plain | type:foo"),
        norm(r##"
            Scope[pack:test,
                Def[boo, Enum[Ref(plain) | Ref(type:foo)]],
            ]
        "##),
    );
}

#[test]
fn struct_1() {
    assert_eq!(
        show("def database"),
        norm(r##"
            Scope[pack:test,
                Def[database, Unit],
            ]
        "##),
    );
}

#[test]
fn struct_2() {
    assert_eq!(
        show("def database = {}"),
        norm(r##"
            Scope[pack:test,
                Def[database, Struct[]],
            ]
        "##),
    );
}

#[test]
fn struct_3() {
    assert_eq!(
        show("def database = unit"),
        norm(r##"
            Scope[pack:test,
                Def[database, Ref(unit)],
            ]
        "##),
    );
}

#[test]
fn struct_4() {
    assert_eq!(
        show("def database unit = unit"),
        norm(r##"
            Scope[pack:test,
                Def[database, Ref(unit)],
            ]
        "##),
    );
}

#[test]
fn struct_type_primitive_link() {
    assert_eq!(
        show("database = { engine = string }"),
        norm(r##"
            Scope[pack:test,
                Def[database, Struct[Field[engine, Type[_, Primitive(string)]]]],
            ]
        "##),
    );
}

#[test]
fn struct_type_link_union() {
    assert_eq!(
        show("database = { manage = self | provider | cloud }"),
        norm(r##"
            Scope[pack:test,
                Def[database, Struct[Field[manage, Type[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]]]],
            ]
        "##),
    );
}

#[test]
fn struct_type_anon_link() {
    assert_eq!(
        show("stack = { = service | database }"),
        norm(r##"
            Scope[pack:test,
                Def[stack, Struct[Field[_, Type[_, Enum[Ref(service) | Ref(database)]]]]],
            ]
        "##),
    );
}

#[test]
fn struct_type_with_inline_type() {
    assert_eq!(
        show("service = { def port = grpc | http }"),
        norm(r##"
            Scope[pack:test,
                Def[service, Struct[Def[port, Enum[Ref(grpc) | Ref(http)]]]],
            ]
        "##),
    );
}

#[test]
fn struct_type_with_inline_link() {
    assert_eq!(
        show(r##"
            service = {
                image = reference
                scaling = {
                    min = integer
                }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Def[service, Struct[Field[image, Type[_, Primitive(reference)]], Field[scaling, Type[_, Struct[Field[min, Type[_, Primitive(integer)]]]]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------

#[test]
fn link_primitive() {
    assert_eq!(
        show("image = reference"),
        norm(r##"
            Scope[pack:test,
                Def[image, Primitive(reference)],
            ]
        "##),
    );
    assert_eq!(
        show("count = integer"),
        norm(r##"
            Scope[pack:test,
                Def[count, Primitive(integer)],
            ]
        "##),
    );
    assert_eq!(
        show("label = string"),
        norm(r##"
            Scope[pack:test,
                Def[label, Primitive(string)],
            ]
        "##),
    );
}

#[test]
fn link_single_ref() {
    assert_eq!(
        show("engine = postgresql"),
        norm(r##"
            Scope[pack:test,
                Def[engine, Ref(postgresql)],
            ]
        "##),
    );
}

#[test]
fn link_ref_union() {
    assert_eq!(
        show("manage = self | provider | cloud"),
        norm(r##"
            Scope[pack:test,
                Def[manage, Enum[Ref(self) | Ref(provider) | Ref(cloud)]],
            ]
        "##),
    );
}

#[test]
fn link_list_single_ref() {
    assert_eq!(
        show("access = [ service ]"),
        norm(r##"
            Scope[pack:test,
                Def[access, List[Type[_, Ref(service)]]],
            ]
        "##),
    );
}

#[test]
fn link_list_union() {
    assert_eq!(
        show("access = [ service | database ]"),
        norm(r##"
            Scope[pack:test,
                Def[access, List[Type[_, Enum[Ref(service) | Ref(database)]]]],
            ]
        "##),
    );
}

#[test]
fn link_list_with_optional_ref_seg() {
    assert_eq!(
        show("access = [ service:(port) | database ]"),
        norm(r##"
            Scope[pack:test,
                Def[access, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]],
            ]
        "##),
    );
}

#[test]
fn link_typed_path() {
    assert_eq!(
        show("region = type:region:type:zone"),
        norm(r##"
            Scope[pack:test,
                Def[region, Ref(type:region:type:zone)],
            ]
        "##),
    );
}

#[test]
fn link_inline_named_type() {
    assert_eq!(
        show("scaling = { min = integer  max = integer }"),
        norm(r##"
            Scope[pack:test,
                Def[scaling, Struct[Field[min, Type[_, Primitive(integer)]], Field[max, Type[_, Primitive(integer)]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

#[test]
fn ref_optional_segment() {
    assert_eq!(
        show("foo = svc:(grpc)"),
        norm(r##"
            Scope[pack:test,
                Def[foo, Ref(svc:grpc?)],
            ]
        "##),
    );
}

#[test]
fn ref_multi_segment_value() {
    assert_eq!(
        show("service svc { image: my-org:my-svc:v2 }"),
        norm(r##"
            Scope[pack:test,
                Inst[service, svc, Field[image, Ref(my-org:my-svc:v2)]],
            ]
        "##),
    );
}

#[test]
fn brace_group_ref_simple() {
    // {role:arn} → Group segment preserving inner structure
    assert_eq!(
        show("svc my-svc { arn: {role:arn} }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[arn, Ref({role:arn})]],
            ]
        "##),
    );
}

#[test]
fn brace_group_ref_with_trailing() {
    // {this:name}-sg → Group segment + plain trailing atom
    assert_eq!(
        show("svc my-svc { name: {this:name}-sg }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[name, Ref({this:name}-sg)]],
            ]
        "##),
    );
}

#[test]
fn brace_group_ref_colon_after() {
    // {role:arn}:suffix → Group segment + colon-separated plain segment
    assert_eq!(
        show("svc my-svc { name: {role:arn}:suffix }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[name, Ref({role:arn}:suffix)]],
            ]
        "##),
    );
}

#[test]
fn brace_group_ref_adjacent() {
    // {a:b}{c:d} → two adjacent Group segments, no colon separator needed
    assert_eq!(
        show("svc my-svc { name: {a:b}{c:d} }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[name, Ref({a:b}:{c:d})]],
            ]
        "##),
    );
}

#[test]
fn brace_group_ref_in_list() {
    // [{sg:id}] → list element is a Group ref
    assert_eq!(
        show("svc my-svc { ids: [{sg:id}] }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[ids, List[Ref({sg:id})]]],
            ]
        "##),
    );
}

#[test]
fn brace_group_does_not_affect_struct_body() {
    // Plain { field: value } still parses as struct body, not brace group
    assert_eq!(
        show("svc my-svc { cfg: { x: 5 } }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[cfg, Struct[Field[x, Ref(5)]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Instances
// ---------------------------------------------------------------------------

#[test]
fn inst_no_fields() {
    assert_eq!(
        show("service svc-api {}"),
        norm(r##"
            Scope[pack:test,
                Inst[service, svc-api],
            ]
        "##),
    );
}

#[test]
fn inst_no_braces() {
    assert_eq!(
        show("service svc-api"),
        norm(r##"
            Scope[pack:test,
                Inst[service, svc-api],
            ]
        "##),
    );
}

#[test]
fn inst_single_field() {
    assert_eq!(
        show("service svc-api { image: svc-api:prod }"),
        norm(r##"
            Scope[pack:test,
                Inst[service, svc-api, Field[image, Ref(svc-api:prod)]],
            ]
        "##),
    );
}

#[test]
fn inst_multiple_fields() {
    assert_eq!(
        show("service svc-api { image: svc-api:prod  port: grpc }"),
        norm(r##"
            Scope[pack:test,
                Inst[service, svc-api, Field[image, Ref(svc-api:prod)], Field[port, Ref(grpc)]],
            ]
        "##),
    );
}

#[test]
fn inst_string_field() {
    assert_eq!(
        show(r##"service svc { label: "hello world" }"##),
        norm(r##"
            Scope[pack:test,
                Inst[service, svc, Field[label, Str("hello world")]],
            ]
        "##),
    );
}

#[test]
fn inst_integer_field() {
    assert_eq!(
        show("service svc { zone: 5 }"),
        norm(r##"
            Scope[pack:test,
                Inst[service, svc, Field[zone, Ref(5)]],
            ]
        "##),
    );
}

#[test]
fn inst_list_field() {
    assert_eq!(
        show("service svc { access: [ svc-b svc-c ] }"),
        norm(r##"
            Scope[pack:test,
                Inst[service, svc, Field[access, List[Ref(svc-b), Ref(svc-c)]]],
            ]
        "##),
    );
}

#[test]
fn inst_inline_struct_value() {
    assert_eq!(
        show(r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            svc my-svc { scaling: { min: 1  max: 10 } }
        "##),
        norm(r##"
            Scope[pack:test,
                Def[scaling, Struct[Field[min, Type[_, Primitive(integer)]], Field[max, Type[_, Primitive(integer)]]]],
                Def[svc, Struct[Field[scaling, Type[_, Ref(scaling)]]]],
                Inst[svc, my-svc, Field[scaling, Struct[Field[min, Ref(1)], Field[max, Ref(10)]]]],
            ]
        "##),
    );
}

#[test]
fn inst_struct_as_field_value() {
    assert_eq!(
        show(r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            scaling my-scaling { min: 1  max: 10 }
            svc     my-svc     { scaling: my-scaling }
        "##),
        norm(r##"
            Scope[pack:test,
                Def[scaling, Struct[Field[min, Type[_, Primitive(integer)]], Field[max, Type[_, Primitive(integer)]]]],
                Def[svc, Struct[Field[scaling, Type[_, Ref(scaling)]]]],
                Inst[scaling, my-scaling, Field[min, Ref(1)], Field[max, Ref(10)]],
                Inst[svc, my-svc, Field[scaling, Ref(my-scaling)]],
            ]
        "##),
    );
}

#[test]
fn inst_anon_list_space_separated() {
    // Space-separated items → two separate Anon fields, each a plain Ref.
    assert_eq!(
        show("stack my-stack { svc-a  svc-b }"),
        norm(r##"
            Scope[pack:test,
              Inst[stack, my-stack, Anon[Ref(svc-a)], Anon[Ref(svc-b)]],
            ]
        "##),
    );
}

#[test]
fn inst_anon_list_bracket_wrapped() {
    // Bracket-wrapped items → single Anon field containing a List value.
    assert_eq!(
        show("stack my-stack { [ svc-a  svc-b ] }"),
        norm(r##"
            Scope[pack:test,
              Inst[stack, my-stack, Anon[List[Ref(svc-a), Ref(svc-b)]]],
            ]
        "##),
    );
}

#[test]
fn inst_duplicate_named_field_allowed_by_parser() {
    // Parser does not deduplicate — both Field entries are preserved as-is.
    assert_eq!(
        show(r##"db my-db { engine: "pg"  engine: "mysql" }"##),
        norm(r##"
            Scope[pack:test,
              Inst[db, my-db, Field[engine, Str("pg")], Field[engine, Str("mysql")]],
            ]
        "##),
    );
}

#[test]
fn inst_duplicate_anon_field_allowed_by_parser() {
    // Space-separated bare refs each become a separate Anon — no deduplication.
    assert_eq!(
        show(r##"stack my-stack { svc-a  svc-a }"##),
        norm(r##"
            Scope[pack:test,
              Inst[stack, my-stack, Anon[Ref(svc-a)], Anon[Ref(svc-a)]],
            ]
        "##),
    );
}

#[test]
fn inst_struct_value_with_type_hint() {
    assert_eq!(
        show(r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            svc my-svc { scaling: type:scaling { min: 2  max: 10 } }
        "##),
        norm(r##"
            Scope[pack:test,
                Def[scaling, Struct[Field[min, Type[_, Primitive(integer)]], Field[max, Type[_, Primitive(integer)]]]],
                Def[svc, Struct[Field[scaling, Type[_, Ref(scaling)]]]],
                Inst[svc, my-svc, Field[scaling, Struct[Hint(type:scaling), Field[min, Ref(2)], Field[max, Ref(10)]]]],
            ]
        "##),
    );
}

#[test]
fn inst_struct_value_bare_hint() {
    // Hint without type: prefix is also valid
    assert_eq!(
        show(r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            svc my-svc { scaling: scaling { min: 2  max: 10 } }
        "##),
        norm(r##"
            Scope[pack:test,
                Def[scaling, Struct[Field[min, Type[_, Primitive(integer)]], Field[max, Type[_, Primitive(integer)]]]],
                Def[svc, Struct[Field[scaling, Type[_, Ref(scaling)]]]],
                Inst[svc, my-svc, Field[scaling, Struct[Hint(scaling), Field[min, Ref(2)], Field[max, Ref(10)]]]],
            ]
        "##),
    );
}

#[test]
fn inst_struct_field_with_brace_group_list() {
    // Brace-group refs inside a list inside a nested struct body
    assert_eq!(
        show("svc my-svc { cfg: { ids: [{sg:id}] } }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[cfg, Struct[Field[ids, List[Ref({sg:id})]]]]],
            ]
        "##),
    );
}

#[test]
fn inst_list_field_brace_group_with_trailing() {
    // List element that is a brace-group ref with a trailing plain atom
    assert_eq!(
        show("svc my-svc { names: [{this:name}-sg] }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[names, List[Ref({this:name}-sg)]]],
            ]
        "##),
    );
}

#[test]
fn inst_struct_value_nested_type_hint() {
    // Type hint on a struct that is itself a field value inside an outer hinted struct
    assert_eq!(
        show("svc my-svc { outer: outer_type { inner: inner_type { x: 1 } } }"),
        norm(r##"
            Scope[pack:test,
                Inst[svc, my-svc, Field[outer, Struct[Hint(outer_type), Field[inner, Struct[Hint(inner_type), Field[x, Ref(1)]]]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Use statements
// ---------------------------------------------------------------------------

#[test]
fn use_bare_name() {
    assert_eq!(
        show("use std"),
        norm(r##"
            Scope[pack:test,
                Use[std],
            ]
        "##),
    );
}

#[test]
fn use_pack_qualified() {
    assert_eq!(
        show("use pack:std"),
        norm(r##"
            Scope[pack:test,
                Use[pack:std],
            ]
        "##),
    );
}

#[test]
fn use_type_specific() {
    assert_eq!(
        show("use pack:std:type:service"),
        norm(r##"
            Scope[pack:test,
                Use[pack:std:type:service],
            ]
        "##),
    );
}

#[test]
fn use_wildcard() {
    assert_eq!(
        show("use pack:std:*"),
        norm(r##"
            Scope[pack:test,
                Use[pack:std:*],
            ]
        "##),
    );
}

#[test]
fn use_type_wildcard() {
    assert_eq!(
        show("use pack:std:type:*"),
        norm(r##"
            Scope[pack:test,
                Use[pack:std:type:*],
            ]
        "##),
    );
}

#[test]
fn use_alongside_defs() {
    assert_eq!(
        show(r##"
            use pack:std:type:service
            stack = { name = string }
        "##),
        norm(r##"
            Scope[pack:test,
                Use[pack:std:type:service],
                Def[stack, Struct[Field[name, Type[_, Primitive(string)]]]],
            ]
        "##),
    );
}

#[test]
fn use_then_qualified_inst() {
    assert_eq!(
        show(r##"
            use std
            std:service api {}
        "##),
        norm(r##"
            Scope[pack:test,
                Use[std],
                Inst[std:service, api],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Multi-unit scope
// ---------------------------------------------------------------------------

#[test]
fn multi_unit_shared_path() {
    use golden_parse_helpers::show_scope;
    use ground_compile::ast::{AstScopeId, ParseReq, ParseUnit};
    use ground_compile::parse::parse;

    let req = ParseReq {
        units: vec![
            ParseUnit { name: "web".into(), path: vec!["infra".into()], src: "image = reference".into(), ts_src: None },
            ParseUnit { name: "db".into(),  path: vec!["infra".into()], src: "engine = string".into(),   ts_src: None },
        ],
    };
    let res = parse(req);
    // scopes[0]=root, scopes[1]=infra, scopes[2]=web, scopes[3]=db
    assert_eq!(
        norm(&show_scope(&res.scopes, AstScopeId(1))),
        norm(r##"
            Scope[pack:infra,
                Scope[pack:web,
                    Def[image, Primitive(reference)],
                ],
                Scope[pack:db,
                    Def[engine, Primitive(string)],
                ],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Regression / integration
// ---------------------------------------------------------------------------

#[test]
fn stdlib_subset() {
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
        norm(r##"
            Scope[pack:test,
                Def[zone, Enum[Ref(1) | Ref(2) | Ref(3) | Ref(4) | Ref(5)]],
                Def[region, Enum[Ref(eu-central) | Ref(eu-west) | Ref(us-east) | Ref(us-west) | Ref(ap-southeast)]],
                Def[access, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]],
                Def[database, Struct[Field[manage, Type[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]], Field[engine, Type[_, Enum[Ref(postgresql) | Ref(mongodb)]]], Field[version, Type[_, Primitive(string)]]]],
                Def[service, Struct[Def[port, Enum[Ref(grpc) | Ref(http)]], Field[image, Type[_, Primitive(reference)]], Field[access, Type[_, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]]], Field[scaling, Type[_, Struct[Field[min, Type[_, Primitive(integer)]], Field[max, Type[_, Primitive(integer)]]]]]]],
                Def[region_path, Ref(type:region:type:zone)],
            ]
        "##),
    );
}

#[test]
fn inline_named_type_with_typed_path_ref() {
    assert_eq!(
        show(r##"
            service = {
                def port   = grpc | http
                sidecar = {
                    service = type:service:(port)
                }
            }
            service upstream {}
            service my-svc {
                sidecar: {
                    service: upstream:grpc
                }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Def[service, Struct[Def[port, Enum[Ref(grpc) | Ref(http)]], Field[sidecar, Type[_, Struct[Field[service, Type[_, Ref(type:service:port?)]]]]]]],
                Inst[service, upstream],
                Inst[service, my-svc, Field[sidecar, Struct[Field[service, Ref(upstream:grpc)]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Gen definitions
// ---------------------------------------------------------------------------

#[test]
fn type_fn_named_static_field() {
    assert_eq!(
        show(r#"
            type stack_gen(s: stack) = {
              vpc: aws_vpc { cidr_block: "10.0.0.0/16" }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                TypeFn[stack_gen(s:stack), TypeFn[vpc=Struct[Hint(aws_vpc), Field[cidr_block, Str("10.0.0.0/16")]]]],
            ]
        "#),
    );
}

#[test]
fn type_fn_named_interp_ref() {
    // Brace-group ref in type fn body rendered as Ref at parse level.
    assert_eq!(
        show(r#"
            type stack_gen(s: stack) = {
              cluster: aws_ecs_cluster { name: {s:deploy:alias} }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                TypeFn[stack_gen(s:stack), TypeFn[cluster=Struct[Hint(aws_ecs_cluster), Field[name, Ref({s:deploy:alias})]]]],
            ]
        "#),
    );
}

#[test]
fn type_fn_named_interp_with_trailing() {
    // Brace-group ref with trailing atom: {s:name}-sg.
    assert_eq!(
        show(r#"
            type svc_gen(s: service) = {
              sg: aws_sg { name: {s:name}-sg }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                TypeFn[svc_gen(s:service), TypeFn[sg=Struct[Hint(aws_sg), Field[name, Ref({s:name}-sg)]]]],
            ]
        "#),
    );
}

#[test]
fn type_fn_anonymous_one_param() {
    // Anonymous 1-param type fn — no name.
    assert_eq!(
        show(r#"
            type (s: service) = {
              rule: aws_rule { from_port: {s:from:port} }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                TypeFn[_(s:service), TypeFn[rule=Struct[Hint(aws_rule), Field[from_port, Ref({s:from:port})]]]],
            ]
        "#),
    );
}

#[test]
fn type_fn_multi_entry() {
    assert_eq!(
        show(r#"
            type svc_gen(s: service) = {
              role: aws_iam_role { name: {s:name}-role }
              task: aws_ecs_task { execution_role_arn: {role:arn} }
            }
        "#),
        norm(r#"
            Scope[pack:test,
                TypeFn[svc_gen(s:service), TypeFn[role=Struct[Hint(aws_iam_role), Field[name, Ref({s:name}-role)]], task=Struct[Hint(aws_ecs_task), Field[execution_role_arn, Ref({role:arn})]]]],
            ]
        "#),
    );
}

#[test]
fn type_fn_in_subpack() {
    // Pack structure from file paths — use show_multi with path segments.
    use golden_parse_helpers::show_multi;
    assert_eq!(
        show_multi(vec![
            ("ecs", vec!["aws"], r#"
                type stack_gen(s: stack) = {
                  vpc: aws_vpc { cidr_block: "10.0.0.0/16" }
                }
            "#),
        ]),
        norm(r#"
            Scope[pack:aws,
                Scope[pack:ecs,
                    TypeFn[stack_gen(s:stack), TypeFn[vpc=Struct[Hint(aws_vpc), Field[cidr_block, Str("10.0.0.0/16")]]]],
                ],
            ]
        "#),
    );
}

#[test]
fn type_fn_empty_body() {
    assert_eq!(
        show("type stack_gen(s: stack) = { }"),
        norm(r#"
            Scope[pack:test,
                TypeFn[stack_gen(s:stack), TypeFn[]],
            ]
        "#),
    );
}

// ---------------------------------------------------------------------------
// New syntax — keyword-free defs
// ---------------------------------------------------------------------------

#[test]
fn top_level_enum_no_kw() {
    assert_eq!(
        show("port = http | grpc"),
        norm(r##"
            Scope[pack:test,
                Def[port, Enum[Ref(http) | Ref(grpc)]],
            ]
        "##),
    );
}

#[test]
fn top_level_ref_no_kw() {
    assert_eq!(
        show("x = service"),
        norm(r##"
            Scope[pack:test,
                Def[x, Ref(service)],
            ]
        "##),
    );
}

#[test]
fn top_level_list_no_kw() {
    assert_eq!(
        show("ports = [ port ]"),
        norm(r##"
            Scope[pack:test,
                Def[ports, List[Type[_, Ref(port)]]],
            ]
        "##),
    );
}

#[test]
fn top_level_struct_no_kw() {
    assert_eq!(
        show("service = { port = grpc | http }"),
        norm(r##"
            Scope[pack:test,
                Def[service, Struct[Field[port, Type[_, Enum[Ref(grpc) | Ref(http)]]]]],
            ]
        "##),
    );
}

#[test]
fn top_level_struct_multiple_fields_no_kw() {
    assert_eq!(
        show("service = { port = grpc | http  image = reference }"),
        norm(r##"
            Scope[pack:test,
                Def[service, Struct[Field[port, Type[_, Enum[Ref(grpc) | Ref(http)]]], Field[image, Type[_, Primitive(reference)]]]],
            ]
        "##),
    );
}

#[test]
fn top_level_struct_anon_field() {
    assert_eq!(
        show("stack = { = [ service | database ] }"),
        norm(r##"
            Scope[pack:test,
                Def[stack, Struct[Field[_, Type[_, List[Type[_, Enum[Ref(service) | Ref(database)]]]]]]],
            ]
        "##),
    );
}

#[test]
fn top_level_struct_optional_ref_field() {
    assert_eq!(
        show("service = { access = [ service:(port) | database ] }"),
        norm(r##"
            Scope[pack:test,
                Def[service, Struct[Field[access, Type[_, List[Type[_, Enum[Ref(service:port?) | Ref(database)]]]]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// New syntax — def keyword
// ---------------------------------------------------------------------------

#[test]
fn def_unit_bare() {
    assert_eq!(
        show("def secret"),
        norm(r##"
            Scope[pack:test,
                Def[secret, Unit],
            ]
        "##),
    );
}

#[test]
fn def_enum() {
    assert_eq!(
        show("def port = http | grpc"),
        norm(r##"
            Scope[pack:test,
                Def[port, Enum[Ref(http) | Ref(grpc)]],
            ]
        "##),
    );
}

#[test]
fn def_struct() {
    assert_eq!(
        show("def service = { port = grpc | http }"),
        norm(r##"
            Scope[pack:test,
                Def[service, Struct[Field[port, Type[_, Enum[Ref(grpc) | Ref(http)]]]]],
            ]
        "##),
    );
}

#[test]
fn def_with_input_no_hook() {
    assert_eq!(
        show("def node { name = string } = { ep = endpoint }"),
        norm(r##"
            Scope[pack:test,
                Def[node, Input[Field[name, Type[_, Primitive(string)]]], Struct[Field[ep, Type[_, Ref(endpoint)]]]],
            ]
        "##),
    );
}

#[test]
fn def_with_input_and_hook() {
    assert_eq!(
        show("def node { name = string } = make_node { ep = endpoint }"),
        norm(r##"
            Scope[pack:test,
                Def[node, Input[Field[name, Type[_, Primitive(string)]]], make_node, Struct[Field[ep, Type[_, Ref(endpoint)]]]],
            ]
        "##),
    );
}

#[test]
fn def_with_multi_input_and_hook() {
    assert_eq!(
        show("def make_service { svc = service  d = deploy } = make_service { sg = aws_security_group }"),
        norm(r##"
            Scope[pack:test,
                Def[make_service, Input[Field[svc, Type[_, Ref(service)]], Field[d, Type[_, Ref(deploy)]]], make_service, Struct[Field[sg, Type[_, Ref(aws_security_group)]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// New syntax — def in struct body (nested named def)
// ---------------------------------------------------------------------------

#[test]
fn struct_body_nested_def() {
    assert_eq!(
        show("s = { def scaling = { min = integer  max = integer } }"),
        norm(r##"
            Scope[pack:test,
                Def[s, Struct[Def[scaling, Struct[Field[min, Type[_, Primitive(integer)]], Field[max, Type[_, Primitive(integer)]]]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// New syntax — def: qualifier in refs
// ---------------------------------------------------------------------------

#[test]
fn def_qualifier_in_type_expr() {
    assert_eq!(
        show("stack = { = [ def:service | def:database ] }"),
        norm(r##"
            Scope[pack:test,
                Def[stack, Struct[Field[_, Type[_, List[Type[_, Enum[Ref(def:service) | Ref(def:database)]]]]]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// New syntax — pack declarations
// ---------------------------------------------------------------------------

#[test]
fn pack_bare() {
    assert_eq!(
        show("pack std:aws"),
        norm(r##"
            Scope[pack:test,
                Pack[std:aws],
            ]
        "##),
    );
}

#[test]
fn pack_bare_single_segment() {
    assert_eq!(
        show("pack std"),
        norm(r##"
            Scope[pack:test,
                Pack[std],
            ]
        "##),
    );
}

#[test]
fn pack_inline_with_body() {
    assert_eq!(
        show("pack std:aws { port = http | grpc }"),
        norm(r##"
            Scope[pack:test,
                Pack[std:aws,
                    Def[port, Enum[Ref(http) | Ref(grpc)]],
                ],
            ]
        "##),
    );
}

#[test]
fn pack_nested() {
    assert_eq!(
        show("pack std { pack aws { port = http | grpc } }"),
        norm(r##"
            Scope[pack:test,
                Pack[std,
                    Pack[aws,
                        Def[port, Enum[Ref(http) | Ref(grpc)]],
                    ],
                ],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// New syntax — plan declarations
// ---------------------------------------------------------------------------

#[test]
fn plan_bare() {
    assert_eq!(
        show("plan prd-eu"),
        norm(r##"
            Scope[pack:test,
                Plan[prd-eu],
            ]
        "##),
    );
}

#[test]
fn plan_with_args() {
    assert_eq!(
        show("plan prd-eu { region: eu-central }"),
        norm(r##"
            Scope[pack:test,
                Plan[prd-eu, Field[region, Ref(eu-central)]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// New syntax — backward compat: instances and use still work
// ---------------------------------------------------------------------------

#[test]
fn inst_with_new_syntax_context() {
    assert_eq!(
        show(r##"
            service = { port = grpc | http }
            service api { port: grpc }
        "##),
        norm(r##"
            Scope[pack:test,
                Def[service, Struct[Field[port, Type[_, Enum[Ref(grpc) | Ref(http)]]]]],
                Inst[service, api, Field[port, Ref(grpc)]],
            ]
        "##),
    );
}

#[test]
fn use_with_new_syntax() {
    assert_eq!(
        show(r##"
            use std
            service = { port = grpc | http }
        "##),
        norm(r##"
            Scope[pack:test,
                Use[std],
                Def[service, Struct[Field[port, Type[_, Enum[Ref(grpc) | Ref(http)]]]]],
            ]
        "##),
    );
}
