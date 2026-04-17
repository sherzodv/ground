/// Embedded TypeScript type-checker using the bundled TypeScript compiler.
///
/// `typecheck(declarations_dts, user_ts)` runs TypeScript's `createProgram` API
/// inside a fresh V8 `JsRuntime` with two virtual files:
///   - `decls.gen.d.ts`  — generated interface + `declare function` declarations
///   - `user.ts`         — user-written hook implementations
///
/// Diagnostics are returned as a `Vec<TsDiagnostic>`. Only category-1 items
/// (errors) indicate compilation failures; the caller is responsible for
/// deciding which categories to surface.
///
/// Performance note: loading the 8.8 MB TypeScript bundle takes ~0.5–2 s on a
/// cold JsRuntime. The check runs once per `compile()` call.

use anyhow::{Result, anyhow};
use deno_core::{JsRuntime, RuntimeOptions};

// Bundled at compile time — no network access at runtime.
static TYPESCRIPT_JS: &str = include_str!("vendor/typescript.js");
static HARNESS_JS:    &str = include_str!("typecheck_harness.js");
/// TypeScript ES5 standard library declarations (Array, Object, JSON …).
static LIB_ES5_DTS:   &str = include_str!("vendor/lib.es5.d.ts");

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TsDiagnostic {
    /// Human-readable message (multi-line flattened).
    pub message:  String,
    /// TypeScript diagnostic code (e.g. 2322 for "Type X is not assignable to Y").
    pub code:     u32,
    /// 1 = error, 2 = warning, 3 = suggestion, 4 = message.
    pub category: u8,
    /// Virtual file name: `"decls.gen.d.ts"` or `"user.ts"`.
    pub file:     Option<String>,
    /// 1-based line number within `file`, if available.
    pub line:     Option<u32>,
    /// 1-based column number within `file`, if available.
    pub col:      Option<u32>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Type-check `user_ts` against the generated declarations in `declarations_dts`.
///
/// Returns an empty `Vec` if both strings are empty (no hooks, nothing to check).
/// Returns `Err` only when the type-checking *engine* itself fails (V8 crash,
/// harness bug); type errors are returned as `TsDiagnostic` items, not `Err`.
pub fn typecheck(declarations_dts: &str, user_ts: &str) -> Result<Vec<TsDiagnostic>> {
    if declarations_dts.is_empty() && user_ts.is_empty() {
        return Ok(vec![]);
    }

    let mut rt = JsRuntime::new(RuntimeOptions::default());

    // 1. Load the TypeScript compiler bundle (sets `var ts = …` as a global).
    rt.execute_script("<typescript>", TYPESCRIPT_JS)?;

    // 2. Load the harness (defines `globalThis.__groundTypecheck`).
    rt.execute_script("<harness>", HARNESS_JS)?;

    // 3. Call the harness with the three virtual file contents.
    let decls_json = serde_json::to_string(declarations_dts)?;
    let user_json  = serde_json::to_string(user_ts)?;
    let lib_json   = serde_json::to_string(LIB_ES5_DTS)?;
    let call: String = format!("globalThis.__groundTypecheck({decls_json}, {user_json}, {lib_json})");

    let handle = rt.execute_script("<typecheck>", call)?;

    // 4. Extract the JSON string result from V8.
    deno_core::scope!(scope, rt);
    let local = handle.open(scope);
    let json_str = local.to_string(scope)
        .ok_or_else(|| anyhow!("typecheck harness returned non-string"))?
        .to_rust_string_lossy(scope);

    // 5. Parse diagnostics from JSON array.
    let raw: Vec<serde_json::Value> = serde_json::from_str(&json_str)
        .map_err(|e| anyhow!("failed to parse typecheck output: {e}\n{json_str}"))?;

    let diags = raw.into_iter().map(|v| TsDiagnostic {
        message:  v["message"] .as_str() .unwrap_or("").to_owned(),
        code:     v["code"]    .as_u64()  .unwrap_or(0) as u32,
        category: v["category"].as_u64()  .unwrap_or(1) as u8,
        file:     v["file"]    .as_str()  .map(str::to_owned),
        line:     v["line"]    .as_u64()  .map(|n| n as u32),
        col:      v["col"]     .as_u64()  .map(|n| n as u32),
    }).collect();

    Ok(diags)
}
