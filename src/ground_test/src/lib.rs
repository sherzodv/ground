/// Golden render tests: exercise the full compiler pipeline through
/// `ground_compile::render` against on-disk fixtures.
///
/// Each sub-directory of `fixtures/` is one test case containing:
///   test.grd              — Ground source
///   test.ts               — optional TypeScript hooks
///   main.<backend>.<type>.tera — manifest entry for the pack
///   *.tera                — helper templates loaded with the manifest
///   expected/<file>       — expected rendered output tree
///
/// The test harness picks `<backend>` and `<type>` from the single
/// `main.*.*.tera` file in the fixture. Templates live in the (empty) root
/// pack, which matches the plan's pack.
///
/// To (re)generate golden files:
///   UPDATE_GOLDENS=1 cargo test -p ground_test
#[cfg(test)]
mod golden {
    use ground_compile::{compile, render, CompileReq, RenderTarget, TemplateUnit, Unit};
    use std::{fs, path::Path};

    fn run_fixture(dir: &Path) -> Result<(), String> {
        let grd_path = dir.join("test.grd");
        let src =
            fs::read_to_string(&grd_path).map_err(|e| format!("{}: {e}", grd_path.display()))?;
        let ts_src = fs::read_to_string(dir.join("test.ts")).ok();

        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
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

        let (templates, target) = load_templates(dir)?;

        let rres = render(&res, &target, &templates);
        if !rres.errors.is_empty() {
            let msgs: Vec<_> = rres.errors.iter().map(|e| e.message.as_str()).collect();
            return Err(format!(
                "{}: render errors: {}",
                dir.display(),
                msgs.join("; ")
            ));
        }

        let mut actual_files: Vec<(String, String)> = rres
            .units
            .into_iter()
            .map(|u| (format!("{}/{}", u.plan, u.file), u.content))
            .collect();
        actual_files.sort_by(|a, b| a.0.cmp(&b.0));

        let expected_root = dir.join("expected");

        if std::env::var("UPDATE_GOLDENS").is_ok() {
            if expected_root.exists() {
                let _ = fs::remove_dir_all(&expected_root);
            }
            for (name, content) in &actual_files {
                let out = expected_root.join(name);
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                fs::write(&out, content).map_err(|e| format!("{}: {e}", out.display()))?;
            }
            return Ok(());
        }

        let mut expected_files = Vec::new();
        if expected_root.exists() {
            collect_expected_files(&expected_root, &expected_root, &mut expected_files)
                .map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        expected_files.sort();

        let expected_names: Vec<&str> = expected_files.iter().map(|(n, _)| n.as_str()).collect();
        let actual_names: Vec<&str> = actual_files.iter().map(|(n, _)| n.as_str()).collect();
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

    fn load_templates(dir: &Path) -> Result<(Vec<TemplateUnit>, RenderTarget), String> {
        let mut templates = Vec::new();
        let mut entry: Option<(String, String)> = None;
        collect_templates(dir, dir, &mut templates, &mut entry)?;

        let (_, target_str) = entry.ok_or_else(|| {
            format!(
                "{}: no manifest (main.<backend>.<type>.tera) found",
                dir.display()
            )
        })?;

        let target = RenderTarget::parse(&target_str).map_err(|e| e.to_string())?;
        Ok((templates, target))
    }

    fn collect_templates(
        root: &Path,
        dir: &Path,
        out: &mut Vec<TemplateUnit>,
        entry: &mut Option<(String, String)>,
    ) -> Result<(), String> {
        for item in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let path = item.map_err(|e| e.to_string())?.path();
            if path.file_name().and_then(|n| n.to_str()) == Some("expected") {
                continue;
            }
            if path.is_dir() {
                collect_templates(root, &path, out, entry)?;
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("tera") {
                continue;
            }

            let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
            let file = rel
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let pack_path: Vec<String> = rel
                .parent()
                .map(|p| {
                    p.components()
                        .filter_map(|c| match c {
                            std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            let content =
                fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;

            if let Some((backend, out_type)) = parse_manifest_name(&file) {
                if let Some((prev, _)) = entry {
                    return Err(format!(
                        "{}: multiple manifests in fixture ({prev}, {file})",
                        root.display()
                    ));
                }
                *entry = Some((file.clone(), format!("{backend}:{out_type}")));
            }

            out.push(TemplateUnit {
                path: pack_path,
                file,
                content,
            });
        }

        Ok(())
    }

    /// Parse `main.<backend>.<out_type>.tera` → `Some((backend, out_type))`.
    fn parse_manifest_name(file: &str) -> Option<(&str, &str)> {
        let rest = file.strip_prefix("main.")?;
        let rest = rest.strip_suffix(".tera")?;
        let (backend, out_type) = rest.split_once('.')?;
        if backend.is_empty() || out_type.is_empty() {
            return None;
        }
        if backend.contains('.') || out_type.contains('.') {
            return None;
        }
        Some((backend, out_type))
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
