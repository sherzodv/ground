/// Quadruple golden tests: .grd + .ts (optional) + .tera + .golden
///
/// Each sub-directory of `fixtures/` is one test case containing:
///   test.grd    — Ground source
///   test.ts     — TypeScript hooks (optional)
///   test.tera   — Tera template: declares what to render/assert
///   test.golden — expected rendered output
///
/// The template receives a `plans` array and a `symbol` array (see `compile_res_to_ctx`).
///
/// To (re)generate golden files:
///   UPDATE_GOLDENS=1 cargo test -p ground_test
#[cfg(test)]
mod golden {
    use std::{fs, path::Path};
    use ground_compile::{compile, CompileReq, Unit, CompileRes, AsmInst, AsmField, asm_value_to_json};
    use serde_json::{json, Value};
    use tera::Tera;

    // -----------------------------------------------------------------------
    // Context builder
    // -----------------------------------------------------------------------

    fn inst_to_json(inst: &AsmInst) -> Value {
        json!({
            "type_name": inst.type_name,
            "name":      inst.name,
            "type_hint": inst.type_hint,
            "fields":    fields_to_json(&inst.fields),
        })
    }

    fn fields_to_json(fields: &[AsmField]) -> Vec<Value> {
        fields.iter().map(|f| json!({
            "name":  f.name,
            "value": asm_value_to_json(&f.value),
        })).collect()
    }

    fn compile_res_to_ctx(res: &CompileRes) -> Value {
        json!({
            "plans": res.plans.iter().map(|p| json!({
                "name":      p.name,
                "root":      inst_to_json(&p.root),
                "fields":    fields_to_json(&p.fields),
                "reachable": p.reachable.iter().map(inst_to_json).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "symbol": res.symbol.instances.iter().map(inst_to_json).collect::<Vec<_>>(),
        })
    }

    // -----------------------------------------------------------------------
    // Per-fixture runner
    // -----------------------------------------------------------------------

    /// Load all `*.tera` files from `dir` into a Tera instance (by filename,
    /// so `{% include "helper.tera" %}` works), then render `entry`.
    fn render_dir(dir: &Path, entry: &str, ctx: &Value) -> Result<String, String> {
        let mut tera = Tera::default();
        for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let path = item.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("tera") { continue; }
            let src  = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            tera.add_raw_template(&name, &src).map_err(|e| e.to_string())?;
        }
        let tera_ctx = tera::Context::from_value(ctx.clone()).map_err(|e| e.to_string())?;
        tera.render(entry, &tera_ctx).map_err(|e| e.to_string())
    }

    fn run_fixture(dir: &Path) -> Result<(), String> {
        let grd_path = dir.join("test.grd");
        let src = fs::read_to_string(&grd_path)
            .map_err(|e| format!("{}: {e}", grd_path.display()))?;

        let ts_src = fs::read_to_string(dir.join("test.ts")).ok();

        let res = compile(CompileReq {
            units: vec![Unit {
                name:   dir.file_name().unwrap().to_string_lossy().into_owned(),
                path:   vec![],
                src,
                ts_src,
            }],
        });

        if !res.errors.is_empty() {
            let msgs: Vec<_> = res.errors.iter().map(|e| e.message.as_str()).collect();
            return Err(format!("{}: compile errors: {}", dir.display(), msgs.join("; ")));
        }

        let ctx    = compile_res_to_ctx(&res);
        let actual = render_dir(dir, "test.tera", &ctx)
            .map_err(|e| format!("{}: render error: {e}", dir.display()))?;

        let golden_path = dir.join("test.golden");

        if std::env::var("UPDATE_GOLDENS").is_ok() {
            fs::write(&golden_path, &actual)
                .map_err(|e| format!("{}: {e}", golden_path.display()))?;
            return Ok(());
        }

        let expected = fs::read_to_string(&golden_path)
            .map_err(|_| format!(
                "{}: test.golden missing — run with UPDATE_GOLDENS=1 to generate",
                dir.display()
            ))?;

        if actual != expected {
            return Err(format!(
                "{}: output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
                dir.display(), expected, actual,
            ));
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Test entry point
    // -----------------------------------------------------------------------

    #[test]
    fn golden_tests() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let entries  = fs::read_dir(&fixtures)
            .unwrap_or_else(|_| panic!("fixtures dir not found: {}", fixtures.display()));

        let mut failures = Vec::new();
        let mut count    = 0;

        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() && path.join("test.grd").exists() {
                count += 1;
                if let Err(e) = run_fixture(&path) {
                    failures.push(e);
                }
            }
        }

        assert!(count > 0, "no test fixtures found in {}", fixtures.display());
        if !failures.is_empty() {
            panic!("{} fixture(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
        }
    }
}

/// File-based golden tests.
///
/// Each `.md` file in `fixtures/` contains a ` ```ground ` block with the
/// input and a ` ```json ` block with the expected Terraform JSON output.
///
/// To regenerate expected output after a generator change:
///   UPDATE_FIXTURES=1 cargo test -- files
#[cfg(test)]
mod files {
    use std::{fs, path::Path};

    use ground_compile::{compile, CompileReq, Unit};
    use ground_be_terra::generate;
    use serde_json::Value;

    fn extract_block<'a>(content: &'a str, lang: &str) -> Option<&'a str> {
        let open = format!("```{lang}\n");
        let (_, after) = content.split_once(open.as_str())?;
        let end = after.find("\n```")?;
        Some(&after[..end])
    }

    fn update_json_block(content: &str, actual_str: &str) -> String {
        let open  = "```json\n";
        let close = "\n```";
        if let Some((before, after_open)) = content.split_once(open) {
            let after_close = after_open.find(close)
                .map(|i| &after_open[i + close.len()..])
                .unwrap_or("");
            format!("{before}{open}{actual_str}{close}{after_close}")
        } else {
            format!("{}\n{open}{actual_str}{close}\n", content.trim_end())
        }
    }

    fn run_fixture(path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;

        // Strip ## Explain section — human docs, not part of the test
        let testable = match content.split_once("\n## Explain") {
            Some((before, _)) => before,
            None              => &content,
        };

        let input = extract_block(testable, "ground")
            .ok_or_else(|| format!("{}: missing ```ground block", path.display()))?;

        // Load the co-located .ts file if present (same stem, .ts extension).
        let ts_src = path.with_extension("ts")
            .to_str()
            .and_then(|p| std::fs::read_to_string(p).ok());

        let res = compile(CompileReq {
            units: vec![Unit {
                name:   path.to_str().unwrap().to_string(),
                path:   vec![],
                src:    input.to_string(),
                ts_src,
            }],
        });
        if !res.errors.is_empty() {
            let msgs: Vec<_> = res.errors.iter().map(|e| e.message.as_str()).collect();
            return Err(format!("{}: {}", path.display(), msgs.join("; ")));
        }

        let actual_str = generate(&res)
            .map_err(|e| format!("{}: gen error: {e}", path.display()))?;

        if std::env::var("UPDATE_FIXTURES").is_ok() {
            let updated = update_json_block(&content, &actual_str);
            fs::write(path, updated).map_err(|e| format!("{}: {e}", path.display()))?;
            return Ok(());
        }

        let expected_str = extract_block(testable, "json")
            .ok_or_else(|| format!(
                "{}: missing ```json block; run with UPDATE_FIXTURES=1 to generate",
                path.display()
            ))?;

        if expected_str.trim().is_empty() {
            return Err(format!(
                "{}: ```json block is empty; run with UPDATE_FIXTURES=1 to generate",
                path.display()
            ));
        }

        let actual: Value   = serde_json::from_str(&actual_str)
            .map_err(|e| format!("{}: invalid actual JSON: {e}\n{actual_str}", path.display()))?;
        let expected: Value = serde_json::from_str(expected_str)
            .map_err(|e| format!("{}: invalid expected JSON: {e}", path.display()))?;

        if actual != expected {
            return Err(format!(
                "{}: output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
                path.display(),
                serde_json::to_string_pretty(&expected).unwrap(),
                serde_json::to_string_pretty(&actual).unwrap(),
            ));
        }

        Ok(())
    }

    #[test]
    fn fixture_files() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let entries  = fs::read_dir(&fixtures)
            .unwrap_or_else(|_| panic!("fixtures dir not found: {}", fixtures.display()));

        let mut failures = Vec::new();
        let mut count    = 0;

        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().map_or(false, |e| e == "md") {
                count += 1;
                if let Err(e) = run_fixture(&path) {
                    failures.push(e);
                }
            }
        }

        assert!(count > 0, "no .md files found in {}", fixtures.display());
        if !failures.is_empty() {
            panic!("{} fixture(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
        }
    }
}
