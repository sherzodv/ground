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

        let res = compile(CompileReq {
            units: vec![Unit {
                name: path.to_str().unwrap().to_string(),
                path: vec![],
                src:  input.to_string(),
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
