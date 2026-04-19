/// Golden tests for the ASM lowering pass (`ground_compile::asm`).
///
/// Each test calls `show(input)` which parses + resolves + lowers the source
/// and returns a compact, position-free string of the resulting AsmRes.
#[path = "helpers/golden_asm_helpers.rs"] mod golden_asm_helpers;
use golden_asm_helpers::show_with_ts;

// ---------------------------------------------------------------------------
// Mapper execution tests
// ---------------------------------------------------------------------------

/// Coarse mapper: the mapper owns the complete output subtree (Case 2 from the book).
///   def node { name = string } = make_node { ep = endpoint }
/// The mapper returns { host, port } which become the ep fields.
#[test]
fn mapper_coarse_output() {
    let grd = r#"
        endpoint = { host = string  port = integer }
        def node { name = string } = make_node { ep = endpoint }
        api = node { name: "api" }
    "#;
    let ts = r#"
        function make_node(i) {
            return { ep: { host: i.name + ".internal", port: 8080 } };
        }
    "#;
    let out = show_with_ts(grd, ts);
    assert!(out.contains("ep=Inst"), "mapper output ep field must be present");
    assert!(out.contains("host=Str(\"api.internal\")"), "mapper must compute host");
    assert!(out.contains("port=Int(8080)"), "mapper must compute port");
}

/// Mapper with no inputs: output-only mapper fires with empty input object.
#[test]
fn mapper_no_inputs() {
    let grd = r#"
        def tag = make_tag { name = string  value = string }
        ground-managed = tag {}
    "#;
    let ts = r#"
        function make_tag(_i) {
            return { name: "ground-managed", value: "true" };
        }
    "#;
    let out = show_with_ts(grd, ts);
    assert!(out.contains("name=Str(\"ground-managed\")"), "mapper must produce name");
    assert!(out.contains("value=Str(\"true\")"), "mapper must produce value");
}

/// Mapper with input fields that produces a list output.
#[test]
fn mapper_list_output() {
    // prefix and count are INPUT fields (before =); items is the OUTPUT (returned by the mapper).
    let grd = r#"
        def tags { prefix = string  count = integer } = make_tags { items = string }
        my-tags = tags { prefix: "svc"  count: 3 }
    "#;
    let ts = r#"
        function make_tags(i) {
            const items = [];
            for (let k = 0; k < i.count; k++) items.push(i.prefix + "-" + k);
            return { items };
        }
    "#;
    let out = show_with_ts(grd, ts);
    assert!(out.contains("items=List"), "mapper list output must appear");
    assert!(out.contains("Str(\"svc-0\")"), "first list element must be present");
}
