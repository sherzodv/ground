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

