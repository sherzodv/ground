/// Golden render tests: .grd + .ts (optional) + one entry .tera + helper .tera files
///
/// Each sub-directory of `fixtures/` is one test case containing:
///   test.grd    — Ground source
///   test.ts     — TypeScript hooks (optional)
///   manifest.json.tera — entry Tera template to render/assert
///   *.tera      — helper templates loaded together into one Tera instance
///   test.golden — expected rendered output
///
/// The template receives a `defs` array (see `compile_res_to_ctx`).
///
/// To (re)generate golden files:
///   UPDATE_GOLDENS=1 cargo test -p ground_test
#[cfg(test)]
mod golden {
    use ground_compile::{
        asm_value_to_json, compile, AsmDef, AsmField, CompileReq, CompileRes, Unit,
    };
    use ground_gen::{render as render_units, RenderReq, TeraUnit};
    use serde_json::{json, Value};
    use std::{fs, path::Path};
    use tera::Tera;

    // -----------------------------------------------------------------------
    // Context builder
    // -----------------------------------------------------------------------

    fn def_to_json(def: &AsmDef) -> Value {
        json!({
            "type_name": def.type_name,
            "name":      def.name,
            "type_hint": def.type_hint,
            "fields":    fields_to_json(&def.fields),
        })
    }

    fn fields_to_json(fields: &[AsmField]) -> Vec<Value> {
        fields
            .iter()
            .map(|f| {
                json!({
                    "name":  f.name,
                    "value": asm_value_to_json(&f.value),
                })
            })
            .collect()
    }

    fn compile_res_to_ctx(res: &CompileRes) -> Value {
        json!({
            "defs": res.defs.iter().map(def_to_json).collect::<Vec<_>>(),
        })
    }

    // -----------------------------------------------------------------------
    // Per-fixture runner
    // -----------------------------------------------------------------------

    /// Load all `*.tera` files from `dir` into one Tera instance keyed by
    /// filename, so `{% include "helper.tera" %}` and friends work naturally.
    fn load_tera_dir(dir: &Path) -> Result<Tera, String> {
        let mut tera = Tera::default();
        let mut units = Vec::new();
        for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let path = item.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("tera") {
                continue;
            }
            let src = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            units.push((name, src));
        }
        tera.add_raw_templates(
            units
                .iter()
                .map(|(name, src)| (name.as_str(), src.as_str()))
                .collect::<Vec<_>>(),
        )
        .map_err(|e| e.to_string())?;
        Ok(tera)
    }

    fn load_units(dir: &Path) -> Result<Vec<TeraUnit>, String> {
        let mut units = Vec::new();
        let mut entries: Vec<_> = fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("tera"))
            .collect();
        entries.sort();

        for path in entries {
            let file = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let template = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            units.push(TeraUnit { file, template });
        }

        Ok(units)
    }

    fn render_dir(dir: &Path, ctx: &Value) -> Result<String, String> {
        let tera = load_tera_dir(dir)?;
        let tera_ctx = tera::Context::from_value(ctx.clone()).map_err(|e| e.to_string())?;
        tera.render("manifest.json.tera", &tera_ctx)
            .map_err(|e| e.to_string())
    }

    fn run_fixture(dir: &Path) -> Result<(), String> {
        let grd_path = dir.join("test.grd");
        let src =
            fs::read_to_string(&grd_path).map_err(|e| format!("{}: {e}", grd_path.display()))?;

        let ts_src = fs::read_to_string(dir.join("test.ts")).ok();

        let res = compile(CompileReq {
            units: vec![Unit {
                name: dir.file_name().unwrap().to_string_lossy().into_owned(),
                path: vec![],
                src,
                ts_src,
            }],
        });

        if !res.errors.is_empty() {
            let msgs: Vec<_> = res.errors.iter().map(|e| e.message.as_str()).collect();
            return Err(format!(
                "{}: compile errors: {}",
                dir.display(),
                msgs.join("; ")
            ));
        }

        let ctx = compile_res_to_ctx(&res);
        let actual =
            render_dir(dir, &ctx).map_err(|e| format!("{}: render error: {e}", dir.display()))?;

        let golden_path = dir.join("test.golden");

        if std::env::var("UPDATE_GOLDENS").is_ok() {
            fs::write(&golden_path, &actual)
                .map_err(|e| format!("{}: {e}", golden_path.display()))?;
            return Ok(());
        }

        let expected = fs::read_to_string(&golden_path).map_err(|_| {
            format!(
                "{}: test.golden missing — run with UPDATE_GOLDENS=1 to generate",
                dir.display()
            )
        })?;

        if actual != expected {
            return Err(format!(
                "{}: output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
                dir.display(),
                expected,
                actual,
            ));
        }

        let units =
            load_units(dir).map_err(|e| format!("{}: load units error: {e}", dir.display()))?;
        let rendered = render_units(
            &RenderReq {
                entry: "manifest.json.tera".into(),
                units,
            },
            &ctx,
        )
        .map_err(|e| format!("{}: render engine error: {e}", dir.display()))?;

        let expected_root = dir.join("expected");
        let mut expected_files = Vec::new();
        if expected_root.exists() {
            collect_expected_files(&expected_root, &expected_root, &mut expected_files)
                .map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        expected_files.sort();

        let mut actual_files: Vec<(String, String)> =
            rendered.into_iter().map(|u| (u.file, u.content)).collect();
        actual_files.sort_by(|a, b| a.0.cmp(&b.0));

        let expected_names: Vec<String> = expected_files
            .iter()
            .map(|(name, _content): &(String, String)| name.clone())
            .collect();
        let actual_names: Vec<String> = actual_files
            .iter()
            .map(|(name, _content): &(String, String)| name.clone())
            .collect();
        if actual_names != expected_names {
            return Err(format!(
                "{}: rendered files mismatch\n--- expected ---\n{:?}\n--- actual ---\n{:?}",
                dir.display(),
                expected_names,
                actual_names,
            ));
        }

        for ((name, actual), (_, expected)) in actual_files.iter().zip(expected_files.iter()) {
            if normalize_trailing_newlines(actual) != normalize_trailing_newlines(expected) {
                return Err(format!(
                    "{}: rendered file mismatch in {}\n--- expected ---\n{}\n--- actual ---\n{}",
                    dir.display(),
                    name,
                    expected,
                    actual,
                ));
            }
        }

        Ok(())
    }

    fn collect_expected_files(
        root: &Path,
        dir: &Path,
        out: &mut Vec<(String, String)>,
    ) -> Result<(), String> {
        let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_expected_files(root, &path, out)?;
            } else {
                let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
                let name = rel.to_string_lossy().replace('\\', "/");
                let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                out.push((name, content));
            }
        }
        Ok(())
    }

    fn normalize_trailing_newlines(s: &str) -> &str {
        s.trim_end_matches('\n')
    }

    // -----------------------------------------------------------------------
    // Test entry point
    // -----------------------------------------------------------------------

    #[test]
    fn golden_tests() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let entries = fs::read_dir(&fixtures)
            .unwrap_or_else(|_| panic!("fixtures dir not found: {}", fixtures.display()));

        let mut failures = Vec::new();
        let mut count = 0;

        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() && path.join("test.grd").exists() {
                count += 1;
                if let Err(e) = run_fixture(&path) {
                    failures.push(e);
                }
            }
        }

        assert!(
            count > 0,
            "no test fixtures found in {}",
            fixtures.display()
        );
        if !failures.is_empty() {
            panic!(
                "{} fixture(s) failed:\n\n{}",
                failures.len(),
                failures.join("\n\n")
            );
        }
    }
}
