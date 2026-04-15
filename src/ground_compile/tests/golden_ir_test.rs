/// Golden tests for the resolve pass (`ground_compile::resolve`).
///
/// Each test calls `show(input)` which parses + resolves the source and returns
/// a compact, position-free string of the resulting IR.  Multi-unit tests use
/// `show_multi(units)`.  Error assertions live in `golden_ir_error_test.rs`.
///
/// Output format conventions:
///
///   Top-level entries
///   -----------------
///   Type#N[name, body]                            — type arena entry at index N
///   Link#N[name, type]                            — link arena entry at index N (struct body or pack-level)
///   Inst#N[Type#N, name, fields…]                 — instance arena entry at index N
///   Inst#N[Type#N, name, hint=h, fields…]         — same, with a type hint (bare or type:-prefixed)
///   Inst#N[Type#N, _]                             — anonymous inline instance (no source name)
///   Deploy[what, target, name, fields…]           — deploy statement
///
///   Link type bodies
///   ----------------
///   Prim(string|integer|reference)  — primitive link type
///   IrRef[segs…]                    — resolved reference (single type or typed path)
///   IrRef[Enum(Type#N):Enum(Type#N)]— typed path: each segment is a resolved type
///   List[IrRef[…] | IrRef[…]]       — list link type (union of patterns)
///
///   Reference segment kinds (inside IrRef[…])
///   ------------------------------------------
///   Enum(Type#N)    — segment resolved to an enum type
///   Struct(Type#N)  — segment resolved to a struct type
///   (seg)           — optional segment
///   plain-string    — unresolved plain text (e.g. brace-group passthrough: `{this:name}`)
///
///   Field values
///   ------------
///   Str("…")                  — string literal
///   Int(N)                    — integer literal
///   Ref("…")                  — reference-typed field value (incl. gen-time brace-group passthrough)
///   Variant(Type#N, "name")   — resolved enum variant
///   Inst(Inst#N)              — resolved instance reference
///   val:val                   — colon-separated path value (typed-path field)
///   List[val, val]            — list field value
///   Field[Link#N, value]      — field entry resolved to a specific link
#[path = "helpers/golden_ir_helpers.rs"] mod golden_ir_helpers;
use golden_ir_helpers::{norm, show, show_multi};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[test]
fn single_enum_type() {
    assert_eq!(
        show("zone = 1 | 2 | 3"),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
            ]
        "##),
    );
}

#[test]
fn enum_typed_struct_ref_variants() {
    assert_eq!(
        show(r##"
            num  = { val = integer }
            add  = { lhs = string }
            expr = type:num | type:add
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[num, Struct[Link#0[val, Prim(integer)]]],
                Type#1[add, Struct[Link#1[lhs, Prim(string)]]],
                Type#2[expr, Enum[Struct(Type#0)|Struct(Type#1)]],
            ]
        "##),
    );
}

#[test]
fn enum_mixed_plain_and_typed_ref() {
    assert_eq!(
        show(r##"
            leaf = { val = integer }
            tree = leaf-val | type:leaf
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[leaf, Struct[Link#0[val, Prim(integer)]]],
                Type#1[tree, Enum[leaf-val|Struct(Type#0)]],
            ]
        "##),
    );
}

#[test]
fn enum_typed_enum_ref_variant() {
    assert_eq!(
        show(r##"
            zone = 1 | 2 | 3
            loc  = type:zone
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[loc, Enum[1|2|3]],
            ]
        "##),
    );
}

#[test]
fn struct_type_primitive_link() {
    assert_eq!(
        show("counter = { count = integer }"),
        norm(r##"
            Scope[pack:test,
                Type#0[counter, Struct[Link#0[count, Prim(integer)]]],
            ]
        "##),
    );
}

#[test]
fn struct_type_enum_ref_link() {
    assert_eq!(
        show(r##"
            zone = 1 | 2 | 3
            host = { zone = zone }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[host, Struct[Link#0[zone, IrRef[Enum(Type#0)]]]],
            ]
        "##),
    );
}

#[test]
fn struct_type_struct_ref_link() {
    assert_eq!(
        show(r##"
            db  = { engine = string }
            svc = { db = db }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[db, Struct[Link#0[engine, Prim(string)]]],
                Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]],
            ]
        "##),
    );
}

#[test]
fn inline_anonymous_enum_hoisted() {
    // In new syntax the anonymous enum type lives in the pack scope, not a sub-scope.
    assert_eq!(
        show("database = { manage = self | provider | cloud }"),
        norm(r##"
            Scope[pack:test,
                Type#0[database, Struct[Link#0[manage, IrRef[Enum(Type#1)]]]],
                Type#1[_, Enum[self|provider|cloud]],
            ]
        "##),
    );
}

#[test]
fn inline_named_struct_hoisted() {
    // In new syntax the inner struct type is anonymous (no 'type name =' prefix);
    // it lives in the pack scope rather than a nested type sub-scope.
    assert_eq!(
        show(r##"
            svc = {
                scaling = { min = integer  max = integer }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[svc, Struct[Link#2[scaling, IrRef[Struct(Type#1)]]]],
                Type#1[_, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]],
            ]
        "##),
    );
}

#[test]
fn struct_with_inline_type_def() {
    // def inside a struct body creates a named type in scope; field is still Link#0.
    assert_eq!(
        show(r##"
            service = {
                def port  = grpc | http
                image = reference
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[port, Enum[grpc|http]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Top-level links
// ---------------------------------------------------------------------------

#[test]
fn top_level_link_primitive() {
    assert_eq!(
        show("link image = reference"),
        norm(r##"
            Scope[pack:test,
                Link#0[image, Prim(reference)],
            ]
        "##),
    );
}

#[test]
fn top_level_link_enum_ref() {
    assert_eq!(
        show(r##"
            type zone = 1 | 2 | 3
            link zone = zone
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Link#0[zone, IrRef[Enum(Type#0)]],
            ]
        "##),
    );
}

#[test]
fn top_level_link_typed_path() {
    assert_eq!(
        show(r##"
            type zone   = 1 | 2 | 3
            type region = eu-central | eu-west
            link location = type:region:type:zone
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[region, Enum[eu-central|eu-west]],
                Link#0[location, IrRef[Enum(Type#1):Enum(Type#0)]],
            ]
        "##),
    );
}

#[test]
fn top_level_link_list() {
    assert_eq!(
        show(r##"
            type service  = { link image  = reference }
            type database = { link engine = string    }
            link access   = [ service:(port) | database ]
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[service, Struct[Link#1[image, Prim(reference)]]],
                Type#1[database, Struct[Link#2[engine, Prim(string)]]],
                Link#0[access, List[IrRef[Struct(Type#0):(port)] | IrRef[Struct(Type#1)]]],
                Scope[type:service],
                Scope[type:database],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Instance field resolution
// ---------------------------------------------------------------------------

#[test]
fn inst_integer_field() {
    assert_eq!(
        show(r##"
            counter = { count = integer }
            counter c { count: 42 }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[counter, Struct[Link#0[count, Prim(integer)]]],
                Inst#0[Type#0, c, Field[Link#0, Int(42)]],
            ]
        "##),
    );
}

#[test]
fn inst_string_field() {
    assert_eq!(
        show(r##"
            svc = { label = string }
            svc my-svc { label: "hello" }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[svc, Struct[Link#0[label, Prim(string)]]],
                Inst#0[Type#0, my-svc, Field[Link#0, Str("hello")]],
            ]
        "##),
    );
}

#[test]
fn inst_reference_field() {
    // reference-typed link: string literal stays as Ref(...), not Str(...)
    assert_eq!(
        show(r##"
            svc = { image = reference }
            svc my-svc { image: "nginx" }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[svc, Struct[Link#0[image, Prim(reference)]]],
                Inst#0[Type#0, my-svc, Field[Link#0, Ref("nginx")]],
            ]
        "##),
    );
}

#[test]
fn inst_enum_field() {
    assert_eq!(
        show(r##"
            zone = 1 | 2 | 3
            host = { zone = zone }
            host my-host { zone: 2 }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[host, Struct[Link#0[zone, IrRef[Enum(Type#0)]]]],
                Inst#0[Type#1, my-host, Field[Link#0, Variant(Type#0, "2")]],
            ]
        "##),
    );
}

#[test]
fn inst_struct_ref_field() {
    assert_eq!(
        show(r##"
            db  = { engine = string }
            svc = { db = db }
            db  my-db  { engine: "pg" }
            svc my-svc { db: my-db }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[db, Struct[Link#0[engine, Prim(string)]]],
                Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#0, my-db, Field[Link#0, Str("pg")]],
                Inst#1[Type#1, my-svc, Field[Link#1, Inst(Inst#0)]],
            ]
        "##),
    );
}

#[test]
fn inst_forward_reference() {
    assert_eq!(
        show(r##"
            db  = { engine = string }
            svc = { db = db }
            svc my-svc { db: my-db }
            db  my-db  { engine: "pg" }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[db, Struct[Link#0[engine, Prim(string)]]],
                Type#1[svc, Struct[Link#1[db, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#1, my-svc, Field[Link#1, Inst(Inst#1)]],
                Inst#1[Type#0, my-db, Field[Link#0, Str("pg")]],
            ]
        "##),
    );
}

#[test]
fn inst_typed_path_field() {
    assert_eq!(
        show(r##"
            zone   = 1 | 2 | 3
            region = eu-central | eu-west
            svc    = { location = type:region:type:zone }
            svc s { location: eu-central:2 }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[region, Enum[eu-central|eu-west]],
                Type#2[svc, Struct[Link#0[location, IrRef[Enum(Type#1):Enum(Type#0)]]]],
                Inst#0[Type#2, s, Field[Link#0, Variant(Type#1, "eu-central"):Variant(Type#0, "2")]],
            ]
        "##),
    );
}

#[test]
fn inst_list_field() {
    assert_eq!(
        show(r##"
            port     = grpc | http
            service  = { port = port }
            database = { engine = string }
            stack    = { access = [ service:(port) | database ] }
            service  svc-a { port: grpc   }
            database db-a  { engine: "pg" }
            stack    my-stack { access: [ svc-a:grpc  db-a ] }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[port, Enum[grpc|http]],
                Type#1[service, Struct[Link#0[port, IrRef[Enum(Type#0)]]]],
                Type#2[database, Struct[Link#1[engine, Prim(string)]]],
                Type#3[stack, Struct[Link#2[access, List[IrRef[Struct(Type#1):(Enum(Type#0))] | IrRef[Struct(Type#2)]]]]],
                Inst#0[Type#1, svc-a, Field[Link#0, Variant(Type#0, "grpc")]],
                Inst#1[Type#2, db-a, Field[Link#1, Str("pg")]],
                Inst#2[Type#3, my-stack, Field[Link#2, List[Inst(Inst#0):Variant(Type#0, "grpc"), Inst(Inst#1)]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Typed enum variant values
// ---------------------------------------------------------------------------

#[test]
fn inst_typed_enum_variant_named_ref() {
    // Named instance ref against a typed variant — hint not needed, inferred from inst type.
    assert_eq!(
        show(r##"
            num  = { val = integer }
            expr = type:num
            host = { e = type:expr }
            num  my-num { val: 5 }
            host h      { e: my-num }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[num, Struct[Link#0[val, Prim(integer)]]],
                Type#1[expr, Enum[Struct(Type#0)]],
                Type#2[host, Struct[Link#1[e, IrRef[Enum(Type#1)]]]],
                Inst#0[Type#0, my-num, Field[Link#0, Int(5)]],
                Inst#1[Type#2, h, Field[Link#1, Variant(Type#1, Inst(Inst#0))]],
            ]
        "##),
    );
}

#[test]
fn inst_typed_enum_variant_struct_with_hint() {
    // Struct literal with type hint against a typed enum variant.
    assert_eq!(
        show(r##"
            num  = { val = integer }
            expr = type:num
            host = { e = type:expr }
            host h { e: num { val: 5 } }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[num, Struct[Link#0[val, Prim(integer)]]],
                Type#1[expr, Enum[Struct(Type#0)]],
                Type#2[host, Struct[Link#1[e, IrRef[Enum(Type#1)]]]],
                Inst#0[Type#2, h, Field[Link#1, Variant(Type#1, Inst(Inst#1))]],
                Inst#1[Type#0, _, hint=num, Field[Link#0, Int(5)]],
            ]
        "##),
    );
}

#[test]
fn inst_typed_enum_variant_disambiguates_by_inst_type() {
    // Two typed variants — named ref selects the correct one by instance type.
    assert_eq!(
        show(r##"
            num  = { val = integer }
            add  = { lhs = string  rhs = string }
            expr = type:num | type:add
            host = { e = type:expr }
            num  my-num { val: 5 }
            host h      { e: my-num }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[num, Struct[Link#0[val, Prim(integer)]]],
                Type#1[add, Struct[Link#1[lhs, Prim(string)], Link#2[rhs, Prim(string)]]],
                Type#2[expr, Enum[Struct(Type#0)|Struct(Type#1)]],
                Type#3[host, Struct[Link#3[e, IrRef[Enum(Type#2)]]]],
                Inst#0[Type#0, my-num, Field[Link#0, Int(5)]],
                Inst#1[Type#3, h, Field[Link#3, Variant(Type#2, Inst(Inst#0))]],
            ]
        "##),
    );
}

#[test]
fn inst_plain_variant_in_mixed_enum_unchanged() {
    // Plain string value in an enum that also has typed variants — unchanged behaviour.
    assert_eq!(
        show(r##"
            num    = { val = integer }
            status = active | type:num
            host   = { s = type:status }
            host h { s: active }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[num, Struct[Link#0[val, Prim(integer)]]],
                Type#1[status, Enum[active|Struct(Type#0)]],
                Type#2[host, Struct[Link#1[s, IrRef[Enum(Type#1)]]]],
                Inst#0[Type#2, h, Field[Link#1, Variant(Type#1, "active")]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Inline struct values
// ---------------------------------------------------------------------------

#[test]
fn inst_inline_struct_value() {
    assert_eq!(
        show(r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            svc my-svc {
                scaling: {
                    min: 1
                    max: 10
                }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]],
                Type#1[svc, Struct[Link#2[scaling, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#1, my-svc, Field[Link#2, Inst(Inst#1)]],
                Inst#1[Type#0, _, Field[Link#0, Int(1)], Field[Link#1, Int(10)]],
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
                Type#0[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]],
                Type#1[svc, Struct[Link#2[scaling, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#0, my-scaling, Field[Link#0, Int(1)], Field[Link#1, Int(10)]],
                Inst#1[Type#1, my-svc, Field[Link#2, Inst(Inst#0)]],
            ]
        "##),
    );
}

#[test]
fn inst_inline_struct_with_type_hint() {
    assert_eq!(
        show(r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            svc my-svc {
                scaling: type:scaling { min: 2  max: 10 }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]],
                Type#1[svc, Struct[Link#2[scaling, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#1, my-svc, Field[Link#2, Inst(Inst#1)]],
                Inst#1[Type#0, _, hint=scaling, Field[Link#0, Int(2)], Field[Link#1, Int(10)]],
            ]
        "##),
    );
}

#[test]
fn inst_inline_struct_bare_hint() {
    // Bare hint (without type: prefix) resolves identically to type:scaling hint.
    assert_eq!(
        show(r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            svc my-svc {
                scaling: scaling { min: 2  max: 10 }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[scaling, Struct[Link#0[min, Prim(integer)], Link#1[max, Prim(integer)]]],
                Type#1[svc, Struct[Link#2[scaling, IrRef[Struct(Type#0)]]]],
                Inst#0[Type#1, my-svc, Field[Link#2, Inst(Inst#1)]],
                Inst#1[Type#0, _, hint=scaling, Field[Link#0, Int(2)], Field[Link#1, Int(10)]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Anonymous link
// ---------------------------------------------------------------------------

#[test]
fn struct_anonymous_link_list_inst() {
    assert_eq!(
        show(r##"
            service  = { image = reference }
            database = { engine = string }
            stack    = { = [ service | database ] }
            service  svc-a    { image: "nginx" }
            database db-a     { engine: "pg"   }
            stack    my-stack { svc-a  db-a }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[database, Struct[Link#1[engine, Prim(string)]]],
                Type#2[stack, Struct[Link#2[_, List[IrRef[Struct(Type#0)] | IrRef[Struct(Type#1)]]]]],
                Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]],
                Inst#1[Type#1, db-a, Field[Link#1, Str("pg")]],
                Inst#2[Type#2, my-stack, Field[Link#2, List[Inst(Inst#0), Inst(Inst#1)]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// List field aggregation
// ---------------------------------------------------------------------------

#[test]
fn anon_list_field_multiple_values_gathered() {
    assert_eq!(
        show(r##"
            service = { image = reference }
            stack   = { = [ service ] }
            service svc-a { image: "nginx"  }
            service svc-b { image: "apache" }
            stack my-stack { svc-a svc-b }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[stack, Struct[Link#1[_, List[IrRef[Struct(Type#0)]]]]],
                Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]],
                Inst#1[Type#0, svc-b, Field[Link#0, Ref("apache")]],
                Inst#2[Type#1, my-stack, Field[Link#1, List[Inst(Inst#0), Inst(Inst#1)]]],
            ]
        "##),
    );
}

#[test]
fn named_list_field_multiple_values_gathered() {
    assert_eq!(
        show(r##"
            service = { image = reference }
            stack   = { services = [ service ] }
            service svc-a { image: "nginx"  }
            service svc-b { image: "apache" }
            stack my-stack { services: svc-a services: svc-b }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[stack, Struct[Link#1[services, List[IrRef[Struct(Type#0)]]]]],
                Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]],
                Inst#1[Type#0, svc-b, Field[Link#0, Ref("apache")]],
                Inst#2[Type#1, my-stack, Field[Link#1, List[Inst(Inst#0), Inst(Inst#1)]]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Group ref reduction ({this:xxx})
// ---------------------------------------------------------------------------

#[test]
fn this_ref_reduces_string_field() {
    // {this:id} in a reference-typed field reduces to the value of 'id' on the same instance.
    assert_eq!(
        show(r##"
            svc = { id = string  image = reference }
            svc my-svc { id: "api"  image: {this:id}:latest }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[svc, Struct[Link#0[id, Prim(string)], Link#1[image, Prim(reference)]]],
                Inst#0[Type#0, my-svc, Field[Link#0, Str("api")], Field[Link#1, Ref("api:latest")]],
            ]
        "##),
    );
}

#[test]
fn this_ref_reduces_reference_field() {
    // {this:name} reduces to the full reference value of 'name', then :latest is appended as a segment.
    assert_eq!(
        show(r##"
            service = { name = reference  image = reference }
            service api { name: boo:foo  image: {this:name}:latest }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[service, Struct[Link#0[name, Prim(reference)], Link#1[image, Prim(reference)]]],
                Inst#0[Type#0, api, Field[Link#0, Ref("boo:foo")], Field[Link#1, Ref("boo:foo:latest")]],
            ]
        "##),
    );
}

#[test]
fn this_ref_trailing_atom_concatenated() {
    // {this:id}-sg: trailing atom is concatenated with the reduced value.
    assert_eq!(
        show(r##"
            svc = { id = string  sg = reference }
            svc my-svc { id: "api"  sg: {this:id}-sg }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[svc, Struct[Link#0[id, Prim(string)], Link#1[sg, Prim(reference)]]],
                Inst#0[Type#0, my-svc, Field[Link#0, Str("api")], Field[Link#1, Ref("api-sg")]],
            ]
        "##),
    );
}

#[test]
fn this_ref_reduces_enum_field() {
    // {this:zone} reduces to the enum variant string, then the resulting ref resolves normally.
    assert_eq!(
        show(r##"
            zone = 1 | 2 | 3
            svc  = { zone = zone  image = reference }
            svc my-svc { zone: 2  image: {this:zone}:latest }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[svc, Struct[Link#0[zone, IrRef[Enum(Type#0)]], Link#1[image, Prim(reference)]]],
                Inst#0[Type#1, my-svc, Field[Link#0, Variant(Type#0, "2")], Field[Link#1, Ref("2:latest")]],
            ]
        "##),
    );
}

#[test]
fn this_ref_reduction_then_symbol_resolve() {
    // {this:region} reduces to "eu-central", then symbol resolution resolves it to Variant.
    assert_eq!(
        show(r##"
            region  = eu-central | eu-west
            service = { region = region  location = region }
            service api { region: eu-central  location: {this:region} }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[region, Enum[eu-central|eu-west]],
                Type#1[service, Struct[Link#0[region, IrRef[Enum(Type#0)]], Link#1[location, IrRef[Enum(Type#0)]]]],
                Inst#0[Type#1, api, Field[Link#0, Variant(Type#0, "eu-central")], Field[Link#1, Variant(Type#0, "eu-central")]],
            ]
        "##),
    );
}

#[test]
fn this_ref_reduces_integer_field() {
    // {this:count} reduces to the integer string representation.
    assert_eq!(
        show(r##"
            svc = { count = integer  image = reference }
            svc my-svc { count: 42  image: {this:count}:latest }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[svc, Struct[Link#0[count, Prim(integer)], Link#1[image, Prim(reference)]]],
                Inst#0[Type#0, my-svc, Field[Link#0, Int(42)], Field[Link#1, Ref("42:latest")]],
            ]
        "##),
    );
}

#[test]
fn this_ref_forward_ref_resolves() {
    // {this:id} resolves even when 'id' is defined after 'image' in source order.
    assert_eq!(
        show(r##"
            svc = { image = reference  id = string }
            svc my-svc { image: {this:id}:latest  id: "api" }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[svc, Struct[Link#0[image, Prim(reference)], Link#1[id, Prim(string)]]],
                Inst#0[Type#0, my-svc, Field[Link#0, Ref("api:latest")], Field[Link#1, Str("api")]],
            ]
        "##),
    );
}

#[test]
fn group_passthrough_unresolvable() {
    // {role:arn} — not a {this:xxx} pattern — passes through as Ref("{role:arn}"), no error.
    assert_eq!(
        show(r##"
            svc = { arn = reference }
            svc my-svc { arn: {role:arn} }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[svc, Struct[Link#0[arn, Prim(reference)]]],
                Inst#0[Type#0, my-svc, Field[Link#0, Ref("{role:arn}")]],
            ]
        "##),
    );
}

#[test]
fn group_passthrough_on_enum_link() {
    // {this:zone} on an enum link when 'zone' is not yet resolved — passes through, no error.
    assert_eq!(
        show(r##"
            zone = 1 | 2 | 3
            svc  = { location = zone  image = reference }
            svc my-svc { location: {this:zone}  image: "nginx" }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[zone, Enum[1|2|3]],
                Type#1[svc, Struct[Link#0[location, IrRef[Enum(Type#0)]], Link#1[image, Prim(reference)]]],
                Inst#0[Type#1, my-svc, Field[Link#0, Ref("{this:zone}")], Field[Link#1, Ref("nginx")]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Use / pack imports
// ---------------------------------------------------------------------------

#[test]
fn use_pack_name_import() {
    // `use pack:std` registers std in the local packs map; std types visible in IR.
    assert_eq!(
        show_multi(vec![
            ("std", vec![], "service = { image = reference }"),
            ("app", vec![], "use pack:std"),
        ]),
        norm(r##"
            Scope[pack:std,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
            ]
            Scope[pack:app]
        "##),
    );
}

#[test]
fn use_type_specific_import() {
    // `use pack:std:type:service` brings `service` into scope unqualified.
    assert_eq!(
        show_multi(vec![
            ("std", vec![], "service = { image = reference }"),
            ("app", vec![], r##"
                use pack:std:type:service
                service my-svc { image: "nginx" }
            "##),
        ]),
        norm(r##"
            Scope[pack:std,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
            ]
            Scope[pack:app,
                Inst#0[Type#0, my-svc, Field[Link#0, Ref("nginx")]],
            ]
        "##),
    );
}

#[test]
fn use_wildcard_type_import() {
    // `use pack:std:type:*` brings all types from std into scope.
    assert_eq!(
        show_multi(vec![
            ("std", vec![], r##"
                service  = { image = reference }
                database = { engine = string }
            "##),
            ("app", vec![], r##"
                use pack:std:type:*
                service my-svc { image: "nginx" }
                database my-db { engine: "pg" }
            "##),
        ]),
        norm(r##"
            Scope[pack:std,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Type#1[database, Struct[Link#1[engine, Prim(string)]]],
            ]
            Scope[pack:app,
                Inst#0[Type#0, my-svc, Field[Link#0, Ref("nginx")]],
                Inst#1[Type#1, my-db, Field[Link#1, Str("pg")]],
            ]
        "##),
    );
}

#[test]
fn use_wildcard_all_import() {
    // `use pack:std:*` brings types AND instances from std into scope.
    assert_eq!(
        show_multi(vec![
            ("std", vec![], r##"
                service = { image = reference }
                service svc-a { image: "nginx" }
            "##),
            ("app", vec![], r##"
                use pack:std:*
                stack = { svc = type:service }
                stack my-stack { svc: svc-a }
            "##),
        ]),
        norm(r##"
            Scope[pack:std,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Inst#0[Type#0, svc-a, Field[Link#0, Ref("nginx")]],
            ]
            Scope[pack:app,
                Type#1[stack, Struct[Link#1[svc, IrRef[Struct(Type#0)]]]],
                Inst#1[Type#1, my-stack, Field[Link#1, Inst(Inst#0)]],
            ]
        "##),
    );
}

#[test]
fn type_and_link_same_name_no_ambiguity() {
    // A type and a link with the same name can be imported together without
    // conflict — they are resolved through separate lookup functions.
    // (pkg still uses old syntax to test the type+link coexistence behaviour.)
    assert_eq!(
        show_multi(vec![
            ("pkg", vec![], r##"
                type region = north | south
                link region = string
            "##),
            ("app", vec![], r##"
                use pack:pkg:*
                place = { region = string }
                place home { region: "x" }
            "##),
        ]),
        norm(r##"
            Scope[pack:pkg,
                Type#0[region, Enum[north|south]],
                Link#0[region, Prim(string)],
            ]
            Scope[pack:app,
                Type#1[place, Struct[Link#1[region, Prim(string)]]],
                Inst#0[Type#1, home, Field[Link#1, Str("x")]],
            ]
        "##),
    );
}

#[test]
fn use_qualified_import_bypasses_shadowed_name() {
    // `outer` defines `service`; `inner` (nested under outer) defines its own
    // `service` which shadows the outer one via parent-chain lookup.
    // `app` (nested under inner) explicitly imports `pack:outer:type:service`,
    // bypassing the shadowing — the instance resolves to outer's type, not inner's.
    assert_eq!(
        show_multi(vec![
            ("outer", vec![],                 "service = { image = reference }"),
            ("inner", vec!["outer"],          "service = { name = string }"),
            ("app",   vec!["outer", "inner"], r##"
                use pack:outer:type:service
                service my-svc { image: "nginx" }
            "##),
        ]),
        norm(r##"
            Scope[pack:outer,
                Type#0[service, Struct[Link#0[image, Prim(reference)]]],
                Scope[pack:inner,
                    Type#1[service, Struct[Link#1[name, Prim(string)]]],
                    Scope[pack:app,
                        Inst#0[Type#0, my-svc, Field[Link#0, Ref("nginx")]],
                    ],
                ],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Type function definitions
// ---------------------------------------------------------------------------

#[test]
fn type_fn_resolves_named() {
    assert_eq!(
        show(r##"
            aws_sg = { name = string }
            stack  = { sg_name = string }
            type stack_gen(s: stack) = {
                sg: aws_sg { name: "static-sg" }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[aws_sg, Struct[Link#0[name, Prim(string)]]],
                Type#1[stack, Struct[Link#1[sg_name, Prim(string)]]],
                TypeFn#0[stack_gen(s:Type#1), sg:Type#0[name=Str("static-sg")]],
            ]
        "##),
    );
}

#[test]
fn type_fn_param_ref_as_opaque() {
    // Param ref `{s:name}-sg` kept as opaque IrValue::Ref for ASM-time substitution.
    assert_eq!(
        show(r##"
            aws_sg  = { name = string }
            service = { port = integer }
            type svc_gen(s: service) = {
                sg: aws_sg { name: {s:name}-sg }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[aws_sg, Struct[Link#0[name, Prim(string)]]],
                Type#1[service, Struct[Link#1[port, Prim(integer)]]],
                TypeFn#0[svc_gen(s:Type#1), sg:Type#0[name=Ref("{s:name}-sg")]],
            ]
        "##),
    );
}

#[test]
fn type_fn_arbitrary_ref_opaque() {
    // Arbitrary ref expressions kept as opaque strings.
    assert_eq!(
        show(r##"
            aws_cluster = { name = string }
            stack       = { region = string }
            type stack_gen(s: stack) = {
                cluster: aws_cluster { name: {s:deploy:alias} }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[aws_cluster, Struct[Link#0[name, Prim(string)]]],
                Type#1[stack, Struct[Link#1[region, Prim(string)]]],
                TypeFn#0[stack_gen(s:Type#1), cluster:Type#0[name=Ref("{s:deploy:alias}")]],
            ]
        "##),
    );
}

#[test]
fn type_fn_anonymous_one_param() {
    // Anonymous 1-param type fn — name shown as `_`.
    assert_eq!(
        show(r##"
            aws_sg  = { name = string }
            service = { port = integer }
            type (s: service) = {
                sg: aws_sg { name: {s:name}-sg }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[aws_sg, Struct[Link#0[name, Prim(string)]]],
                Type#1[service, Struct[Link#1[port, Prim(integer)]]],
                TypeFn#0[_(s:Type#1), sg:Type#0[name=Ref("{s:name}-sg")]],
            ]
        "##),
    );
}

#[test]
fn type_fn_multi_entry_sibling_ref() {
    // Multi-entry type fn; sibling refs like `{role:arn}` kept as opaque Ref.
    assert_eq!(
        show(r##"
            aws_role = { arn      = string }
            aws_task = { role_arn = string }
            service  = { name     = string }
            type svc_gen(s: service) = {
                role: aws_role { arn:      {s:name}-role }
                task: aws_task { role_arn: {role:arn}    }
            }
        "##),
        norm(r##"
            Scope[pack:test,
                Type#0[aws_role, Struct[Link#0[arn, Prim(string)]]],
                Type#1[aws_task, Struct[Link#1[role_arn, Prim(string)]]],
                Type#2[service, Struct[Link#2[name, Prim(string)]]],
                TypeFn#0[svc_gen(s:Type#2), role:Type#0[arn=Ref("{s:name}-role")], task:Type#1[role_arn=Ref("{role:arn}")]],
            ]
        "##),
    );
}

// ---------------------------------------------------------------------------
// Regression / integration
// ---------------------------------------------------------------------------

#[test]
fn inline_named_type_with_typed_path_ref() {
    assert_eq!(
        show(r##"
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
        "##),
        norm(r##"
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
        "##),
    );
}
