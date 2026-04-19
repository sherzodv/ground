/// Golden tests for parser error cases (`ground_compile::parse`).
///
/// Each test calls `show(input)` and asserts that the output contains
/// one or more `ERR: ...` lines.  The parser is error-tolerant — it
/// continues after failures — so some valid output may precede the errors.

#[path = "helpers/golden_parse_helpers.rs"] mod golden_parse_helpers;
use golden_parse_helpers::show;

// ---------------------------------------------------------------------------
// Moved from golden_parse_test.rs
// ---------------------------------------------------------------------------

#[test]
fn error_001() {
    let out = show("type foo ?");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn error_002() {
    let out = show("??? garbage");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn error_003() {
    // Parser emits an error for the bad token but continues and parses the
    // valid def that follows.
    let out = show(r##"
        ???
        x = a | b
    "##);
    assert!(out.contains("ERR:"), "expected error, got: {out}");
    assert!(out.contains("Def[x,"), "expected recovery parse, got: {out}");
}

// ---------------------------------------------------------------------------
// New coverage
// ---------------------------------------------------------------------------

#[test]
fn error_004() {
    // List opened but never closed; parser should surface an error.
    let out = show("service my-svc { access: [svc-b }");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn error_005() {
    // Field key present but no value; parser should surface an error.
    let out = show("service my-svc { image: }");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}

#[test]
fn error_006() {
    // Shorthand initial defs require `field = type` entries in the block.
    let out = show("service { port }");
    assert!(out.contains("ERR:"), "expected error, got: {out}");
}
