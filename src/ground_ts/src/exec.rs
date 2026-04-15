/// Execute TypeScript hook functions embedded in-process via deno_core + V8.
///
/// Workflow:
///   1. Strip `export` modifiers so functions become globals (no module loader needed).
///   2. Transpile TypeScript → JavaScript using deno_ast (swc-backed).
///   3. Load the JS as a classic script in a fresh JsRuntime.
///   4. Call the named function with the JSON-encoded input.
///   5. Return the JSON-encoded output.
///
/// Constraints for hook source files:
///   - No `import` statements (no module resolution in this embedded runtime).
///   - Functions may use `interface`/`type` freely — they are erased by the transpiler.

use anyhow::{anyhow, Result};
use deno_ast::{EmitOptions, MediaType, ParseParams, TranspileModuleOptions, TranspileOptions};
use deno_core::{JsRuntime, RuntimeOptions};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Call a single TypeScript hook function.
///
/// * `ts_src`     — full TypeScript source defining the hook (and any helpers).
/// * `fn_name`    — name of the function to call (must be defined at top level).
/// * `input_json` — JSON string passed as the sole argument.
///
/// Returns the JSON string produced by `JSON.stringify(fn(input))`.
pub fn call_hook(ts_src: &str, fn_name: &str, input_json: &str) -> Result<String> {
    let js = ts_to_js(ts_src)?;

    let mut rt = JsRuntime::new(RuntimeOptions::default());

    // Load the hook definitions as a classic script.
    rt.execute_script("<hooks>", js)?;

    // Call: JSON.stringify( globalThis["fn_name"](input) )
    let call = format!(
        "JSON.stringify(globalThis[{fn_name:?}]({input_json}))"
    );
    let handle = rt.execute_script("<call>", call)?;

    // Extract the JSON string from the V8 result.
    deno_core::scope!(scope, rt);
    let local = handle.open(scope);

    if local.is_null_or_undefined() {
        return Err(anyhow!(
            "hook '{fn_name}' returned null/undefined (did you forget a return statement?)"
        ));
    }

    Ok(local.to_string(scope)
        .ok_or_else(|| anyhow!("hook '{fn_name}' result could not be converted to string"))?
        .to_rust_string_lossy(scope))
}

// ---------------------------------------------------------------------------
// TypeScript → JavaScript
// ---------------------------------------------------------------------------

/// Transpile TypeScript to plain JavaScript.
///
/// `export` modifiers on functions/consts are stripped first so the output is a
/// classic script with top-level bindings reachable via `globalThis`.
/// `interface`, `type`, and `export interface`/`export type` declarations are
/// erased entirely by the swc transpiler — they produce no JS output.
pub fn ts_to_js(ts_src: &str) -> Result<String> {
    // Strip export modifiers — functions/consts become plain globals.
    // Interface/type declarations produce no JS output regardless, so
    // `export interface Foo {}` → `interface Foo {}` → (erased by transpiler).
    let stripped = ts_src
        .replace("export function ",  "function ")
        .replace("export async function ", "async function ")
        .replace("export const ",    "const ")
        .replace("export let ",      "let ")
        .replace("export var ",      "var ");

    let parsed = deno_ast::parse_module(ParseParams {
        specifier: "file:///hook.ts".parse().unwrap(),
        text:       Arc::from(stripped.as_str()),
        media_type: MediaType::TypeScript,
        capture_tokens: false,
        scope_analysis: false,
        maybe_syntax: None,
    })?;

    let emit = parsed
        .transpile(
            &TranspileOptions::default(),
            &TranspileModuleOptions::default(),
            &EmitOptions {
                source_map: deno_ast::SourceMapOption::None,
                ..Default::default()
            },
        )?
        .into_source();

    Ok(emit.text)
}
