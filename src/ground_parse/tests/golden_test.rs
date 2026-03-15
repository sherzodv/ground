/// Golden tests for the RFC-0005 parser (`ground_parse::parse2`).
///
/// Each test calls `show(input)` which parses the source and returns a compact,
/// position-free string representation of the resulting AST.  Errors are
/// surfaced as `ERR: <message>` lines at the end of the output.
///
/// To add a new test: write the input, run the test once (it will fail),
/// copy the `actual` side of the diff into the `expected` string, done.
mod golden_helpers;
use golden_helpers::{norm, show};

#[test]
fn empty_file() {
    assert_eq!(show(""), "");
}

#[test]
fn line_comment_ignored() {
    assert_eq!(
        show("# this is a comment\ntype x = a | b"),
        "TypeDef[x, Enum[Ref(a) | Ref(b)]]",
    );
}

#[test]
fn integer_enum() {
    assert_eq!(
        show("type zone = 1 | 2 | 3 | 4 | 5"),
        "TypeDef[zone, Enum[Ref(1) | Ref(2) | Ref(3) | Ref(4) | Ref(5)]]",
    );
}

#[test]
fn ident_enum() {
    assert_eq!(
        show("type region = eu-central | eu-west | us-east | us-west | ap-southeast"),
        "TypeDef[region, Enum[Ref(eu-central) | Ref(eu-west) | Ref(us-east) | Ref(us-west) | Ref(ap-southeast)]]",
    );
}

#[test]
fn struct_type_primitive_link() {
    assert_eq!(
        show("type database = { link engine = string }"),
        "TypeDef[database, Struct[LinkDef[engine, TypeDef[_, Primitive(string)]]]]",
    );
}

#[test]
fn struct_type_link_union() {
    assert_eq!(
        show("type database = { link manage = self | provider | cloud }"),
        "TypeDef[database, Struct[LinkDef[manage, TypeDef[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]]]]",
    );
}

#[test]
fn struct_type_with_inline_type() {
    assert_eq!(
        show("type service = { type port = grpc | http }"),
        "TypeDef[service, Struct[TypeDef[port, Enum[Ref(grpc) | Ref(http)]]]]",
    );
}

#[test]
fn struct_type_with_inline_link() {
    assert_eq!(
        show("type service = {\n  link image = reference\n  link scaling = type scaling = { link min = integer }\n}"),
        "TypeDef[service, Struct[LinkDef[image, TypeDef[_, Primitive(reference)]], LinkDef[scaling, TypeDef[scaling, Struct[LinkDef[min, TypeDef[_, Primitive(integer)]]]]]]]",
    );
}

#[test]
fn struct_type_bare_ref_is_error() {
    // Bare refs are no longer valid struct items; use anonymous links instead
    let out = show("type stack = { service database }");
    assert!(out.contains("ERR:"), "expected error, got: {}", out);
}

#[test]
fn struct_type_anon_link() {
    // Anonymous link for composition (no name, enum of refs)
    assert_eq!(
        show("type stack = { link = service | database }"),
        "TypeDef[stack, Struct[LinkDef[_, TypeDef[_, Enum[Ref(service) | Ref(database)]]]]]",
    );
}

#[test]
fn link_primitive() {
    assert_eq!(show("link image = reference"), "LinkDef[image, TypeDef[_, Primitive(reference)]]");
    assert_eq!(show("link count = integer"),   "LinkDef[count, TypeDef[_, Primitive(integer)]]");
    assert_eq!(show("link label = string"),    "LinkDef[label, TypeDef[_, Primitive(string)]]");
}

#[test]
fn link_single_ref() {
    assert_eq!(show("link engine = postgresql"), "LinkDef[engine, TypeDef[_, Ref(postgresql)]]");
}

#[test]
fn link_ref_union() {
    assert_eq!(
        show("link manage = self | provider | cloud"),
        "LinkDef[manage, TypeDef[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]]",
    );
}

#[test]
fn link_list_single_ref() {
    assert_eq!(
        show("link access = [ service ]"),
        "LinkDef[access, TypeDef[_, List[TypeDef[_, Ref(service)]]]]",
    );
}

#[test]
fn link_list_union() {
    assert_eq!(
        show("link access = [ service | database ]"),
        "LinkDef[access, TypeDef[_, List[TypeDef[_, Enum[Ref(service) | Ref(database)]]]]]",
    );
}

#[test]
fn link_list_with_optional_ref_seg() {
    assert_eq!(
        show("link access = [ service:(port) | database ]"),
        "LinkDef[access, TypeDef[_, List[TypeDef[_, Enum[Ref(service:port?) | Ref(database)]]]]]",
    );
}

#[test]
fn link_typed_path() {
    assert_eq!(
        show("link region = type:region:type:zone"),
        "LinkDef[region, TypeDef[_, Ref(type:region:type:zone)]]",
    );
}

#[test]
fn link_inline_named_type() {
    assert_eq!(
        show("link scaling = type scaling = { link min = integer  link max = integer }"),
        "LinkDef[scaling, TypeDef[scaling, Struct[LinkDef[min, TypeDef[_, Primitive(integer)]], LinkDef[max, TypeDef[_, Primitive(integer)]]]]]",
    );
}

#[test]
fn inst_no_fields() {
    assert_eq!(show("service svc-api {}"), "Inst[service, svc-api]");
}

#[test]
fn inst_no_braces() {
    assert_eq!(show("service svc-api"), "Inst[service, svc-api]");
}

#[test]
fn inst_single_field() {
    assert_eq!(
        show("service svc-api { image: svc-api:prod }"),
        "Inst[service, svc-api, Field[image, Ref(svc-api:prod)]]",
    );
}

#[test]
fn inst_multiple_fields() {
    assert_eq!(
        show("service svc-api { image: svc-api:prod  port: grpc }"),
        "Inst[service, svc-api, Field[image, Ref(svc-api:prod)], Field[port, Ref(grpc)]]",
    );
}

#[test]
fn inst_list_field() {
    assert_eq!(
        show("service svc { access: [ svc-b svc-c ] }"),
        "Inst[service, svc, Field[access, List[Ref(svc-b), Ref(svc-c)]]]",
    );
}

#[test]
fn inst_string_field() {
    assert_eq!(
        show(r#"service svc { label: "hello world" }"#),
        r#"Inst[service, svc, Field[label, Str("hello world")]]"#,
    );
}

#[test]
fn inst_integer_field() {
    assert_eq!(
        show("service svc { zone: 5 }"),
        "Inst[service, svc, Field[zone, Ref(5)]]",
    );
}

#[test]
fn deploy_no_fields() {
    assert_eq!(show("deploy stack to aws as prod {}"), "Deploy[stack, aws, prod]");
}

#[test]
fn deploy_with_ref_segments() {
    assert_eq!(show("deploy stack to aws:eu-central as prd {}"), "Deploy[stack, aws:eu-central, prd]");
}

#[test]
fn deploy_with_fields() {
    assert_eq!(
        show("deploy stack to aws as prod { region: eu-central:3 }"),
        "Deploy[stack, aws, prod, Field[region, Ref(eu-central:3)]]",
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
        norm(r#"
            TypeDef[zone, Enum[Ref(1) | Ref(2) | Ref(3)]]
            TypeDef[region, Enum[Ref(eu-central) | Ref(eu-west)]]
            LinkDef[access, TypeDef[_, List[TypeDef[_, Enum[Ref(service:port?) | Ref(database)]]]]]
            Inst[service, svc, Field[image, Ref(svc:prod)]]
            Deploy[svc, aws, prod]
        "#),
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
        norm(r#"
            TypeDef[zone, Enum[Ref(1) | Ref(2) | Ref(3) | Ref(4) | Ref(5)]]
            TypeDef[region, Enum[Ref(eu-central) | Ref(eu-west) | Ref(us-east) | Ref(us-west) | Ref(ap-southeast)]]
            LinkDef[access, TypeDef[_, List[TypeDef[_, Enum[Ref(service:port?) | Ref(database)]]]]]
            TypeDef[database, Struct[LinkDef[manage, TypeDef[_, Enum[Ref(self) | Ref(provider) | Ref(cloud)]]], LinkDef[engine, TypeDef[_, Enum[Ref(postgresql) | Ref(mongodb)]]], LinkDef[version, TypeDef[_, Primitive(string)]]]]
            TypeDef[service, Struct[TypeDef[port, Enum[Ref(grpc) | Ref(http)]], LinkDef[image, TypeDef[_, Primitive(reference)]], LinkDef[access, TypeDef[_, List[TypeDef[_, Enum[Ref(service:port?) | Ref(database)]]]]], LinkDef[scaling, TypeDef[scaling, Struct[LinkDef[min, TypeDef[_, Primitive(integer)]], LinkDef[max, TypeDef[_, Primitive(integer)]]]]]]]
            LinkDef[region, TypeDef[_, Ref(type:region:type:zone)]]
        "#),
    );
}

#[test]
fn error_unexpected_top_level() {
    let out = show("= foo");
    assert!(out.starts_with("ERR:"), "expected an error, got: {}", out);
}

#[test]
fn error_collected_continue_parsing() {
    let out = show("!! bad\ntype x = a | b");
    assert!(out.contains("TypeDef[x, Enum[Ref(a) | Ref(b)]]"), "expected recovered def in: {}", out);
    assert!(out.contains("ERR:"), "expected error in: {}", out);
}

#[test]
fn ref_optional_segment() {
    assert_eq!(
        show("link foo = svc:(grpc)"),
        "LinkDef[foo, TypeDef[_, Ref(svc:grpc?)]]",
    );
}

#[test]
fn ref_multi_segment_value() {
    assert_eq!(
        show("service svc { image: my-org:my-svc:v2 }"),
        "Inst[service, svc, Field[image, Ref(my-org:my-svc:v2)]]",
    );
}
