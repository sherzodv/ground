/// Golden tests for the ASM lowering pass (`ground_compile::asm`).
///
/// ASM is plan-driven: without `plan` declarations it produces no output.
/// These tests compare the full normalized ASM output.
#[path = "helpers/golden_asm_helpers.rs"] mod golden_asm_helpers;
use golden_asm_helpers::{norm, show, show_with_ts};

// ---------------------------------------------------------------------------
// Basic
// ---------------------------------------------------------------------------

#[test]
fn basic_001() {
    assert_eq!(
        show(
            r##"
            service = { image = reference }
            api = service { image: nginx }
        "##
        ),
        "",
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
            Def[api = service { image: Ref(nginx) }]
        "##
        ),
    );
}

#[test]
fn plan_002() {
    assert_eq!(
        show(
            r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            plan my-svc = svc { scaling: def:scaling { min: 2  max: 10 } }
        "##
        ),
        norm(
            r##"
            Def[my-svc = svc { scaling: Def[_ = scaling hint: scaling { min: Int(2), max: Int(10) }] }]
        "##
        ),
    );
}

#[test]
fn plan_003() {
    assert_eq!(
        show(
            r##"
            service = { image = reference }
            plan api = service { image: nginx }
        "##
        ),
        norm(
            r##"
            Def[api = service { image: Ref(nginx) }]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Mapper
// ---------------------------------------------------------------------------

#[test]
fn mapper_001() {
    let grd = r#"
        def tag = make_tag { name = string  value = string  enabled = boolean }
        plan ground-managed = tag {}
    "#;
    let ts = r#"
        function make_tag(_i) {
            return { name: "ground-managed", value: "true", enabled: true };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[ground-managed = tag { name: Str("ground-managed"), value: Str("true"), enabled: Bool(true) }]
        "##
        ),
    );
}

#[test]
fn mapper_002() {
    let grd = r#"
        endpoint = { host = string  port = integer }
        def node { name = string } = make_node { ep = endpoint }
        plan api = node { name: "api" }
    "#;
    let ts = r#"
        function make_node(i) {
            return { ep: { host: i.name + ".internal", port: 8080 } };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[api = node { name: Str("api"), ep: Def[_ =  { host: Str("api.internal"), port: Int(8080) }] }]
        "##
        ),
    );
}

#[test]
fn mapper_003() {
    let grd = r#"
        def tags { prefix = string  count = integer } = make_tags { items = string }
        plan my-tags = tags { prefix: "svc"  count: 3 }
    "#;
    let ts = r#"
        function make_tags(i) {
            const items = [];
            for (let k = 0; k < i.count; k++) items.push(i.prefix + "-" + k);
            return { items };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[my-tags = tags { prefix: Str("svc"), count: Int(3), items: List[Str("svc-0"), Str("svc-1"), Str("svc-2")] }]
        "##
        ),
    );
}

#[test]
fn mapper_004() {
    let grd = r#"
        def rectangle { width = integer  height = integer } = mk_rect { area = integer }
        plan r1 {} = rectangle { width: 1  height: 2 }
    "#;
    let ts = r#"
        function mk_rect(i) {
            return { area: i.width * i.height };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[r1 = rectangle { width: Int(1), height: Int(2), area: Int(2) }]
        "##
        ),
    );
}

#[test]
fn mapper_005() {
    let grd = r#"
        def rectangle { width = integer  height = integer } = mk_rect { area = integer }
        plan r1 {} = rectangle { width: 1  height: 2 }
    "#;
    let ts = r#"
        function mk_rect(i) {
            return { area: i.width * i.height };
        }
        function rectangle(resolved, _input) {
            return { area: resolved.area + 10 };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[r1 = rectangle { width: Int(1), height: Int(2), area: Int(12) }]
        "##
        ),
    );
}
