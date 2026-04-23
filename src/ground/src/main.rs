mod ops_display;

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process,
};

#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

use ground_be_terra::terra_ops::{self, Action, AttrVal, OpsEvent};
use ground_compile::{
    compile, format_source, render as compile_render, render_ctx_for_plan, AsmDef, AsmDefRef,
    AsmValue, CompileReq, CompileRes, RenderTarget, RenderUnit, TemplateUnit, Unit,
};
use ground_run::RunEvent;
use ops_display::{Op, TerraEnricher};
use serde_json::Value;

const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.as_slice() {
        [cmd] if cmd == "init" => cmd_init(false),
        [cmd, flag] if cmd == "init" && flag == "--git-ignore" => cmd_init(true),
        [cmd, sub, key] if cmd == "config" && sub == "target" => cmd_config_set_target(key),
        [cmd, sub] if cmd == "config" && sub == "target" => cmd_config_get_target(),
        [cmd, sub] if cmd == "tf" && sub == "check" => cmd_tf_check(),
        [cmd, sub, source, target] if cmd == "tf" && sub == "rename" => {
            cmd_tf_rename(source, target)
        }
        [cmd, sub] if cmd == "tf" && sub == "migrate" => cmd_tf_migrate(),
        [cmd] if cmd == "gen" => cmd_gen(None),
        [cmd, flag, val] if cmd == "gen" && flag == "--target" => cmd_gen(Some(val.as_str())),
        [cmd, sub] if cmd == "gen" && sub == "types" => cmd_gen_types(),
        [cmd] if cmd == "fmt" => cmd_fmt(),
        [cmd, ..] if cmd == "fmt" => cmd_fmt(),
        [cmd] if cmd == "status" => cmd_status(),
        [cmd, name] if cmd == "asm" => cmd_asm(name),
        [cmd, sub] if cmd == "lsp" && sub == "start" => cmd_lsp_start(),
        [cmd, sub] if cmd == "lsp" && sub == "stop" => cmd_lsp_stop(),
        [cmd] if cmd == "plan" => cmd_plan_ls(),
        [cmd, sub] if cmd == "plan" && sub == "ls" => cmd_plan_ls(),
        [cmd, name] if cmd == "plan" => cmd_plan(name, false),
        [cmd, name, flag] if cmd == "plan" && (flag == "--verbose" || flag == "-v") => {
            cmd_plan(name, true)
        }
        [cmd, name] if cmd == "apply" => cmd_apply(name, false),
        [cmd, name, flag] if cmd == "apply" && (flag == "--verbose" || flag == "-v") => {
            cmd_apply(name, true)
        }
        _ => {
            eprintln!("usage:");
            eprintln!("  ground init [--git-ignore]");
            eprintln!("  ground config target [<backend>:<type>]");
            eprintln!("  ground gen [--target <backend>:<type>]");
            eprintln!("  ground gen types");
            eprintln!("  ground tf check");
            eprintln!("  ground tf rename <source> <target>");
            eprintln!("  ground tf migrate");
            eprintln!("  ground fmt");
            eprintln!("  ground status");
            eprintln!("  ground asm <plan>");
            eprintln!("  ground lsp start");
            eprintln!("  ground lsp stop");
            eprintln!("  ground plan ls");
            eprintln!("  ground plan <name> [--verbose|-v]");
            eprintln!("  ground apply <name> [--verbose|-v]");
            process::exit(1);
        }
    }
}

fn cmd_init(git_ignore: bool) {
    if let Err(e) = fs::create_dir(".ground") {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            eprintln!("error: {e}");
            process::exit(1);
        }
    }

    let settings_path = ".ground/settings.json";
    if !Path::new(settings_path).exists() {
        if let Err(e) = fs::write(settings_path, "{}\n") {
            eprintln!("error: {settings_path}: {e}");
            process::exit(1);
        }
    }

    println!("initialized .ground/");
    println!("run `ground config target <backend>:<type>` to set the render target");

    if git_ignore {
        let needed = [
            ".ground/**/.terraform/",
            ".ground/**/terraform.tfstate",
            ".ground/**/terraform.tfstate.backup",
        ];

        let existing = fs::read_to_string(".gitignore").unwrap_or_default();
        let to_add: Vec<&str> = needed
            .iter()
            .filter(|entry| !existing.lines().any(|l| l.trim() == **entry))
            .copied()
            .collect();

        if to_add.is_empty() {
            println!(".gitignore already up to date");
        } else {
            let mut content = existing;
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str("# ground\n");
            for entry in &to_add {
                content.push_str(entry);
                content.push('\n');
                println!(".gitignore  + {entry}");
            }
            if let Err(e) = fs::write(".gitignore", content) {
                eprintln!("error: .gitignore: {e}");
                process::exit(1);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Settings + target
// ---------------------------------------------------------------------------

fn settings_path(root: &Path) -> PathBuf {
    root.join(".ground/settings.json")
}

fn read_settings(root: &Path) -> Value {
    match fs::read_to_string(settings_path(root)) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(Value::Object(Default::default())),
        Err(_) => Value::Object(Default::default()),
    }
}

fn write_settings(root: &Path, value: &Value) {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into());
    let mut out = pretty;
    out.push('\n');
    if let Err(e) = fs::write(settings_path(root), out) {
        eprintln!("error: {}: {e}", settings_path(root).display());
        process::exit(1);
    }
}

fn read_target(root: &Path) -> RenderTarget {
    let settings = read_settings(root);
    let Some(raw) = settings.get("target").and_then(Value::as_str) else {
        eprintln!("error: no target set — run `ground config target <backend>:<type>`");
        process::exit(1);
    };
    RenderTarget::parse(raw).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        process::exit(1);
    })
}

fn cmd_config_get_target() {
    with_project_root(|root| {
        let settings = read_settings(root);
        match settings.get("target").and_then(Value::as_str) {
            Some(v) => println!("{v}"),
            None => {
                eprintln!("error: no target set");
                process::exit(1);
            }
        }
    });
}

fn cmd_config_set_target(value: &str) {
    if let Err(e) = RenderTarget::parse(value) {
        eprintln!("error: {e}");
        process::exit(1);
    }
    with_project_root(|root| {
        let mut settings = read_settings(root);
        let obj = settings
            .as_object_mut()
            .expect("settings.json must be a JSON object");
        obj.insert("target".into(), Value::String(value.into()));
        write_settings(root, &settings);
        println!("target = {value}");
    });
}

struct PlanStamp {
    alias: String,
    kind: String,
    state: Option<String>,
    backend_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StateBinding {
    plan: String,
    target: String,
    dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrackedStateOrphan {
    code: String,
    plan: String,
    dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StateRegistryMismatch {
    recorded_plan: String,
    actual_plan: String,
    dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TfCheckRes {
    untracked_orphans: Vec<String>,
    tracked_orphans: Vec<TrackedStateOrphan>,
    registry_mismatches: Vec<StateRegistryMismatch>,
}

fn states_path(root: &Path) -> PathBuf {
    root.join(".ground/states.json")
}

fn read_state_bindings(root: &Path) -> Result<Vec<StateBinding>, String> {
    let path = states_path(root);
    let Ok(src) = fs::read_to_string(&path) else {
        return Ok(vec![]);
    };
    let json: Value = serde_json::from_str(&src).map_err(|e| format!("{}: {e}", path.display()))?;
    let entries = match &json {
        Value::Array(items) => items,
        Value::Object(map) => match map.get("states") {
            Some(Value::Array(items)) => items,
            Some(_) => {
                return Err(format!(
                    "{}: expected top-level array or {{\"states\": [...]}}",
                    path.display()
                ));
            }
            None => return Ok(vec![]),
        },
        _ => {
            return Err(format!(
                "{}: expected top-level array or {{\"states\": [...]}}",
                path.display()
            ));
        }
    };

    let mut out = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        let Some(obj) = entry.as_object() else {
            return Err(format!(
                "{}: entry {} must be an object",
                path.display(),
                idx
            ));
        };
        let Some(plan) = obj.get("plan").and_then(Value::as_str) else {
            return Err(format!(
                "{}: entry {} missing string field 'plan'",
                path.display(),
                idx
            ));
        };
        let Some(target) = obj.get("target").and_then(Value::as_str) else {
            return Err(format!(
                "{}: entry {} missing string field 'target'",
                path.display(),
                idx
            ));
        };
        let Some(dir) = obj.get("dir").and_then(Value::as_str) else {
            return Err(format!(
                "{}: entry {} missing string field 'dir'",
                path.display(),
                idx
            ));
        };
        out.push(StateBinding {
            plan: plan.to_string(),
            target: target.to_string(),
            dir: dir.to_string(),
        });
    }
    Ok(out)
}

fn write_state_bindings(root: &Path, bindings: &[StateBinding]) -> Result<(), String> {
    let path = states_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let json = Value::Array(
        bindings
            .iter()
            .map(|binding| {
                Value::Object(serde_json::Map::from_iter([
                    ("plan".to_string(), Value::String(binding.plan.clone())),
                    ("target".to_string(), Value::String(binding.target.clone())),
                    ("dir".to_string(), Value::String(binding.dir.clone())),
                ]))
            })
            .collect(),
    );
    let mut out =
        serde_json::to_string_pretty(&json).map_err(|e| format!("{}: {e}", path.display()))?;
    out.push('\n');
    fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

fn target_repr(target: &RenderTarget) -> String {
    format!("{}:{}", target.backend, target.out_type)
}

fn selected_full_plan_name(res: &CompileRes, plan_name: &str) -> Option<String> {
    res.plans
        .iter()
        .find(|plan| plan.name == plan_name)
        .map(full_plan_name)
}

fn upsert_state_binding_for_plan_dir(
    root: &Path,
    full_plan: &str,
    target: &RenderTarget,
    dir: &str,
) -> Result<(), String> {
    let mut bindings = read_state_bindings(root)?;
    let target = target_repr(target);

    let mut updated = false;
    bindings.retain(|binding| {
        if binding.dir == dir {
            if !updated {
                updated = true;
                true
            } else {
                false
            }
        } else if binding.plan == full_plan {
            false
        } else {
            true
        }
    });

    if let Some(binding) = bindings.iter_mut().find(|binding| binding.dir == dir) {
        binding.plan = full_plan.to_string();
        binding.target = target;
    } else {
        bindings.push(StateBinding {
            plan: full_plan.to_string(),
            target,
            dir: dir.to_string(),
        });
    }
    bindings.sort_by(|a, b| a.dir.cmp(&b.dir).then(a.plan.cmp(&b.plan)));
    write_state_bindings(root, &bindings)
}

fn prune_non_stateful_state_bindings(root: &Path) -> Result<Vec<String>, String> {
    let mut bindings = read_state_bindings(root)?;
    let stateful_dirs: std::collections::HashSet<String> =
        tf_stateful_dir_paths(root).into_iter().collect();
    let before = bindings.len();
    let removed: Vec<String> = bindings
        .iter()
        .filter(|binding| !stateful_dirs.contains(&binding.dir))
        .map(|binding| binding.dir.clone())
        .collect();
    if removed.is_empty() {
        return Ok(vec![]);
    }
    bindings.retain(|binding| stateful_dirs.contains(&binding.dir));
    if bindings.len() != before {
        write_state_bindings(root, &bindings)?;
    }
    Ok(removed)
}

fn stateful_artifact_names() -> &'static [&'static str] {
    &[
        ".terraform",
        ".terraform.lock.hcl",
        "terraform.tfstate",
        "terraform.tfstate.backup",
    ]
}

fn dir_has_stateful_artifacts(dir: &Path) -> bool {
    stateful_artifact_names().iter().any(|name| {
        let path = dir.join(name);
        path.is_dir() || path.is_file()
    })
}

fn move_stateful_artifacts(src_dir: &Path, dst_dir: &Path) -> Result<(), String> {
    if dir_has_stateful_artifacts(dst_dir) {
        return Err(format!(
            "{} already has Terraform stateful artifacts",
            dst_dir.display()
        ));
    }
    fs::create_dir_all(dst_dir).map_err(|e| format!("{}: {e}", dst_dir.display()))?;
    for name in stateful_artifact_names() {
        let src = src_dir.join(name);
        if !src.exists() {
            continue;
        }
        let dst = dst_dir.join(name);
        fs::rename(&src, &dst)
            .map_err(|e| format!("failed to move {} -> {}: {e}", src.display(), dst.display()))?;
    }
    if src_dir.exists()
        && fs::read_dir(src_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false)
    {
        let _ = fs::remove_dir(src_dir);
    }
    Ok(())
}

fn rename_tracked_orphan_to_plan(
    root: &Path,
    res: &CompileRes,
    target: &RenderTarget,
    source_code: &str,
    target_plan_name: &str,
) -> Result<(String, String, String), String> {
    let check = tf_check(root, res)?;
    let Some(orphan) = check
        .tracked_orphans
        .iter()
        .find(|orphan| orphan.code == source_code)
    else {
        let available = check
            .tracked_orphans
            .iter()
            .map(|o| o.code.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return if available.is_empty() {
            Err("no tracked Terraform orphaned state found".into())
        } else {
            Err(format!(
                "unknown rename source '{}'; available sources: {}",
                source_code, available
            ))
        };
    };

    let Some(full_target_plan) = selected_full_plan_name(res, target_plan_name) else {
        let available = res
            .plans
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return if available.is_empty() {
            Err(format!("plan '{}' not found", target_plan_name))
        } else {
            Err(format!(
                "plan '{}' not found; available plans: {}",
                target_plan_name, available
            ))
        };
    };

    let target_dir = format!("{}/{}", render_out_dir(target), target_plan_name);
    let src_dir = root.join(&orphan.dir);
    let dst_dir = root.join(&target_dir);
    if orphan.dir != target_dir {
        move_stateful_artifacts(&src_dir, &dst_dir)?;
    }
    upsert_state_binding_for_plan_dir(root, &full_target_plan, target, &target_dir)?;
    let _ = prune_non_stateful_state_bindings(root)?;
    Ok((orphan.plan.clone(), full_target_plan, target_dir))
}

fn full_plan_name(plan: &ground_compile::PlanRoot) -> String {
    if plan.pack_path.is_empty() {
        plan.name.clone()
    } else {
        format!("{}:{}", plan.pack_path.join(":"), plan.name)
    }
}

fn tf_stateful_dir_paths(root: &Path) -> Vec<String> {
    let terra_root = root.join(".ground/tf");
    let entries = match fs::read_dir(&terra_root) {
        Ok(entries) => entries,
        Err(_) => return vec![],
    };

    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let is_stateful = path.join(".terraform").is_dir()
            || path.join("terraform.tfstate").is_file()
            || path.join("terraform.tfstate.backup").is_file();
        if !is_stateful {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        paths.push(rel.to_string_lossy().replace('\\', "/"));
    }
    paths.sort();
    paths
}

fn orphan_code_for_dir(dir: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in dir.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:05x}", hash & 0xfffff)
}

fn tf_check(root: &Path, res: &CompileRes) -> Result<TfCheckRes, String> {
    let stateful_dirs = tf_stateful_dir_paths(root);
    let bindings = read_state_bindings(root)?;
    let current_plans: std::collections::HashSet<String> =
        res.plans.iter().map(full_plan_name).collect();
    let current_plan_dirs: std::collections::HashSet<String> = res
        .plans
        .iter()
        .map(|plan| format!(".ground/tf/{}", plan.name))
        .collect();
    let current_plan_by_dir: std::collections::HashMap<String, String> = res
        .plans
        .iter()
        .map(|plan| (format!(".ground/tf/{}", plan.name), full_plan_name(plan)))
        .collect();
    let binding_by_dir: std::collections::HashMap<&str, &StateBinding> = bindings
        .iter()
        .map(|binding| (binding.dir.as_str(), binding))
        .collect();

    let untracked_orphans = stateful_dirs
        .iter()
        .filter(|dir| {
            !binding_by_dir.contains_key(dir.as_str()) && !current_plan_dirs.contains(dir.as_str())
        })
        .cloned()
        .collect();

    let stateful_dir_set: std::collections::HashSet<&str> =
        stateful_dirs.iter().map(|dir| dir.as_str()).collect();
    let mut registry_mismatches: Vec<StateRegistryMismatch> = bindings
        .iter()
        .filter_map(|binding| {
            let actual_plan = current_plan_by_dir.get(&binding.dir)?;
            if &binding.plan == actual_plan || !stateful_dir_set.contains(binding.dir.as_str()) {
                return None;
            }
            Some(StateRegistryMismatch {
                recorded_plan: binding.plan.clone(),
                actual_plan: actual_plan.clone(),
                dir: binding.dir.clone(),
            })
        })
        .collect();
    registry_mismatches.sort_by(|a, b| a.dir.cmp(&b.dir));

    let mut tracked_rows: Vec<(String, String)> = bindings
        .iter()
        .filter(|binding| {
            binding.target.starts_with("tf:")
                && stateful_dir_set.contains(binding.dir.as_str())
                && !current_plan_by_dir.contains_key(&binding.dir)
                && !current_plans.contains(&binding.plan)
        })
        .map(|binding| (binding.plan.clone(), binding.dir.clone()))
        .collect();
    tracked_rows.sort();
    let tracked_orphans = tracked_rows
        .into_iter()
        .map(|(plan, dir)| TrackedStateOrphan {
            code: orphan_code_for_dir(&dir),
            plan,
            dir,
        })
        .collect();

    Ok(TfCheckRes {
        untracked_orphans,
        tracked_orphans,
        registry_mismatches,
    })
}

fn print_tf_check_report(check: &TfCheckRes) {
    if !check.untracked_orphans.is_empty() {
        eprintln!("error: found untracked Terraform orphaned state");
        for dir in &check.untracked_orphans {
            eprintln!("- {dir}: requires manual fix");
        }
        if !check.tracked_orphans.is_empty() || !check.registry_mismatches.is_empty() {
            eprintln!();
        }
    }

    if !check.tracked_orphans.is_empty() {
        eprintln!("error: found tracked Terraform orphaned state");
        for orphan in &check.tracked_orphans {
            eprintln!(
                "- {}  {} -> {}: requires manual fix or rename `ground tf rename {} <target>`",
                orphan.code, orphan.plan, orphan.dir, orphan.code
            );
        }
        if !check.registry_mismatches.is_empty() {
            eprintln!();
        }
    }

    if !check.registry_mismatches.is_empty() {
        eprintln!("warning: found Terraform state registry mismatches");
        for mismatch in &check.registry_mismatches {
            eprintln!(
                "- {}: recorded as {} but current plan is {}",
                mismatch.dir, mismatch.recorded_plan, mismatch.actual_plan
            );
        }
        eprintln!("these will be fixed automatically on `ground plan`");
    }
}

fn tf_check_has_blockers(check: &TfCheckRes) -> bool {
    !check.untracked_orphans.is_empty() || !check.tracked_orphans.is_empty()
}

fn fix_registry_mismatches(root: &Path, target: &RenderTarget, check: &TfCheckRes) {
    for mismatch in &check.registry_mismatches {
        upsert_state_binding_for_plan_dir(root, &mismatch.actual_plan, target, &mismatch.dir)
            .unwrap_or_else(|e| {
                eprintln!("error: {e}");
                process::exit(1);
            });
        eprintln!(
            "info: updated state registry for {}: {} -> {}",
            mismatch.dir, mismatch.recorded_plan, mismatch.actual_plan
        );
    }
    for removed in prune_non_stateful_state_bindings(root).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        process::exit(1);
    }) {
        eprintln!("info: removed stale state registry record for {}", removed);
    }
}

fn cmd_tf_check() {
    with_project_root(|root| {
        let res = do_compile(root, true);
        let check = tf_check(root, &res).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            process::exit(1);
        });
        if check.untracked_orphans.is_empty()
            && check.tracked_orphans.is_empty()
            && check.registry_mismatches.is_empty()
        {
            println!("tf check: ok");
            return;
        }

        print_tf_check_report(&check);

        if tf_check_has_blockers(&check) {
            process::exit(1);
        }
    });
}

fn cmd_tf_rename(source: &str, target_plan: &str) {
    with_project_root(|root| {
        let res = do_compile(root, true);
        let target = read_target(root);
        ensure_tf_backend(&target, "ground tf rename");

        let (from_plan, to_plan, dir) =
            rename_tracked_orphan_to_plan(root, &res, &target, source, target_plan).unwrap_or_else(
                |e| {
                    eprintln!("error: {e}");
                    process::exit(1);
                },
            );

        println!("renamed Terraform state binding");
        println!("- source: {source}");
        println!("- from:   {from_plan}");
        println!("- to:     {to_plan}");
        println!("- dir:    {dir}");
    });
}

fn cmd_tf_migrate() {
    with_project_root(|root| {
        let target = read_target(root);
        ensure_tf_backend(&target, "ground tf migrate");

        let outputs = do_render(root, &do_compile(root, true), &target);
        write_render_units(root, &target, &outputs);

        let stamps = read_plan_stamps(&root.join(render_out_dir(&target)));
        let migratable: Vec<&PlanStamp> = stamps
            .iter()
            .filter(|stamp| {
                stamp.kind == "state_store"
                    && stamp.state.as_deref() == Some("remote")
                    && stamp.backend_present
            })
            .collect();

        match migratable.len() {
            0 => {
                eprintln!("error: no migratable state found");
                process::exit(1);
            }
            1 => {}
            _ => {
                eprintln!("error: multiple migratable states found");
                for stamp in migratable {
                    eprintln!("  {}", stamp.alias);
                }
                process::exit(1);
            }
        }

        let stamp = migratable[0];
        println!("state migration candidate: {}", stamp.alias);
        println!("this will migrate local terraform state to the configured remote backend");
        print!("type yes to continue: ");
        let _ = io::Write::flush(&mut io::stdout());

        let mut answer = String::new();
        if io::stdin().read_line(&mut answer).is_err() || answer.trim() != "yes" {
            eprintln!("aborted");
            process::exit(1);
        }

        let (res, outputs, target, plan_name) = compile_and_gen(root, &stamp.alias);
        let provider = "aws".to_string();
        let lookup = build_lookup(&res);
        write_render_units(root, &target, &outputs);

        let dir = root.join(render_out_dir(&target)).join(&plan_name);
        let Some(rx) = terra_ops::migrate_state(&dir).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            process::exit(1);
        }) else {
            println!("no migration needed for plan {plan_name}");
            return;
        };

        let mut enricher = TerraEnricher::new(
            plan_name.clone(),
            Op::Init,
            provider,
            String::new(),
            lookup,
            true,
        );
        if !run_events(rx, &mut enricher) {
            eprintln!("error: terraform migrate failed");
            process::exit(1);
        }
    });
}

fn read_plan_stamps(terra_root: &Path) -> Vec<PlanStamp> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(terra_root) {
        Ok(entries) => entries,
        Err(_) => return out,
    };

    for entry in entries.flatten() {
        let path = entry.path().join(".ground-plan.json");
        let Ok(src) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<Value>(&src) else {
            continue;
        };
        let Some(alias) = json
            .get("alias")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            continue;
        };
        let Some(kind) = json
            .get("kind")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            continue;
        };
        let state = json
            .get("state")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let backend_present = json
            .get("backend_present")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        out.push(PlanStamp {
            alias,
            kind,
            state,
            backend_present,
        });
    }

    out
}

fn read_backend_summary(dir: &Path) -> Option<(String, String, String)> {
    let src = fs::read_to_string(dir.join("main.tf.json")).ok()?;
    let json = serde_json::from_str::<Value>(&src).ok()?;
    let s3 = json.get("terraform")?.get("backend")?.get("s3")?;
    let bucket = s3.get("bucket")?.as_str()?.to_string();
    let key = s3.get("key")?.as_str()?.to_string();
    let region = s3.get("region")?.as_str()?.to_string();
    Some((bucket, key, region))
}

// ---------------------------------------------------------------------------
// Compile .grd files and run code generation
// ---------------------------------------------------------------------------

/// Collect all `.grd` files under `root` recursively.
/// Each file's pack path is derived from its directory relative to `root`.
fn collect_grd_files(root: &Path) -> Vec<Unit> {
    let mut units = Vec::new();
    collect_grd_recursive(root, root, &mut units);
    units
}

fn collect_grd_recursive(root: &Path, dir: &Path, units: &mut Vec<Unit>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("warning: cannot read {:?}: {e}", dir);
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip hidden directories (.ground, .git, .direnv …).
        let is_hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |n| n.starts_with('.'));
        if path.is_dir() && !is_hidden {
            collect_grd_recursive(root, &path, units);
        } else if path.extension().map_or(false, |e| e == "grd") {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            // Pack path = directory components (not the filename).
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
            // Unit name: the file stem unless it is "pack" (pack.grd merges into
            // its directory scope; all other named files create a named sub-pack).
            let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let name = if stem == "pack" {
                String::new()
            } else {
                stem.to_string()
            };
            match fs::read_to_string(&path) {
                Ok(src) => {
                    let ts_src = path
                        .with_extension("ts")
                        .to_str()
                        .and_then(|p| fs::read_to_string(p).ok());
                    units.push(Unit {
                        name,
                        path: pack_path,
                        src,
                        ts_src,
                    });
                }
                Err(e) => {
                    eprintln!("error: {}: {e}", path.display());
                    process::exit(1);
                }
            }
        }
    }
}

fn resolve_project_path(root: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn source_root(root: &Path) -> PathBuf {
    let settings = read_settings(root);
    if let Some(raw) = settings.get("src").and_then(Value::as_str) {
        return resolve_project_path(root, raw);
    }

    let default_src = root.join("src");
    if default_src.is_dir() {
        default_src
    } else {
        root.to_path_buf()
    }
}

fn templates_root(root: &Path) -> PathBuf {
    let settings = read_settings(root);
    if let Some(raw) = settings.get("templates").and_then(Value::as_str) {
        return resolve_project_path(root, raw);
    }
    source_root(root)
}

fn collect_templates(root: &Path) -> Vec<TemplateUnit> {
    let templates_root = templates_root(root);
    let mut units = Vec::new();
    collect_templates_recursive(&templates_root, &templates_root, &mut units);
    units
}

fn collect_templates_recursive(root: &Path, dir: &Path, units: &mut Vec<TemplateUnit>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("warning: cannot read {:?}: {e}", dir);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let is_hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));

        if path.is_dir() && !is_hidden {
            collect_templates_recursive(root, &path, units);
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) != Some("tera") {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(&path);
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

        let file = rel
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let id = rel
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(":");

        match fs::read_to_string(&path) {
            Ok(content) => units.push(TemplateUnit {
                path: pack_path,
                file,
                id,
                content,
            }),
            Err(e) => {
                eprintln!("error: {}: {e}", path.display());
                process::exit(1);
            }
        }
    }
}

/// Compile all .grd files under `root`, print errors and exit on failure.
fn do_compile(root: &Path, require_plans: bool) -> CompileRes {
    let src_root = source_root(root);
    let units = collect_grd_files(&src_root);

    if units.is_empty() {
        eprintln!("no .grd files found in {}", src_root.display());
        process::exit(1);
    }

    let unit_names: Vec<String> = units.iter().map(|u| u.name.clone()).collect();

    let res = compile(CompileReq { units });

    if !res.errors.is_empty() {
        for e in &res.errors {
            if let Some(loc) = &e.loc {
                let name = unit_names
                    .get(loc.unit.as_usize())
                    .map(|s| s.as_str())
                    .unwrap_or("?");
                eprintln!("error: {}:{}:{}: {}", name, loc.line, loc.col, e.message);
            } else {
                eprintln!("error: {}", e.message);
            }
        }
        process::exit(1);
    }

    if require_plans && res.plans.is_empty() {
        eprintln!("no plan declarations found — nothing to generate");
        process::exit(1);
    }

    res
}

fn compile_and_gen(
    root: &Path,
    plan_name: &str,
) -> (CompileRes, Vec<RenderUnit>, RenderTarget, String) {
    let res = do_compile(root, true);
    let target = read_target(root);
    ensure_tf_backend(&target, "ground plan/apply");
    let outputs = do_render(root, &res, &target);

    let Some(plan) = res.plans.iter().find(|p| p.name == plan_name) else {
        let available = res
            .plans
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("error: plan '{plan_name}' not found");
        if !available.is_empty() {
            eprintln!("available plans: {available}");
        }
        process::exit(1);
    };

    let selected_plan = plan.name.clone();
    let plan_outputs: Vec<RenderUnit> = outputs
        .into_iter()
        .filter(|u| u.plan == selected_plan)
        .collect();

    (res, plan_outputs, target, selected_plan)
}

fn do_render(_root: &Path, res: &CompileRes, target: &RenderTarget) -> Vec<RenderUnit> {
    let templates = collect_templates(_root);
    let rres = compile_render(res, target, &templates);
    if !rres.errors.is_empty() {
        for e in &rres.errors {
            eprintln!("error: {}", e.message);
        }
        process::exit(1);
    }
    rres.units
}

fn render_out_dir(target: &RenderTarget) -> String {
    format!(".ground/{}", target.backend)
}

fn ensure_tf_backend(target: &RenderTarget, cmd: &str) {
    if target.backend != "tf" {
        eprintln!(
            "error: {cmd} currently supports only target backend \"tf\", found \"{}\"",
            target.backend
        );
        process::exit(1);
    }
}

fn write_render_units(root: &Path, target: &RenderTarget, outputs: &[RenderUnit]) {
    let base = root.join(render_out_dir(target));
    for output in outputs {
        let out_path = base.join(&output.plan).join(&output.file);
        if let Some(parent) = out_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("error: {}: {e}", parent.display());
                process::exit(1);
            }
        }
        if let Err(e) = fs::write(&out_path, &output.content) {
            eprintln!("error: {}: {e}", out_path.display());
            process::exit(1);
        }
    }
}

fn cmd_gen(target_override: Option<&str>) {
    with_project_root(|root| {
        let res = do_compile(root, true);
        let target = match target_override {
            Some(raw) => RenderTarget::parse(raw).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                process::exit(1);
            }),
            None => read_target(root),
        };
        let outputs = do_render(root, &res, &target);
        let out_dir = render_out_dir(&target);

        for output in &outputs {
            write_render_units(root, &target, std::slice::from_ref(output));
            println!("wrote {out_dir}/{}/{}", output.plan, output.file);
        }
    });
}

fn cmd_plan_ls() {
    with_project_root(|root| {
        let res = do_compile(root, true);
        for plan in &res.plans {
            println!("{}", plan.name);
        }
    });
}

fn collect_grd_paths_recursive(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_grd_path_recursive(root, root, &mut paths);
    paths
}

fn collect_grd_path_recursive(root: &Path, dir: &Path, paths: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("warning: cannot read {:?}: {e}", dir);
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if path.is_dir() && !is_hidden {
            collect_grd_path_recursive(root, &path, paths);
        } else if path.extension().map_or(false, |e| e == "grd") {
            paths.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start.canonicalize().ok()?;
    loop {
        if cur.join(".ground").is_dir() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

fn with_project_root(action: impl FnOnce(&Path)) {
    let Some(root) = find_project_root(Path::new(".")) else {
        eprintln!("warning: no project found");
        return;
    };
    action(&root);
}

fn cmd_fmt() {
    with_project_root(|root| {
        let src_root = source_root(root);
        let files = collect_grd_paths_recursive(&src_root);
        if files.is_empty() {
            eprintln!("warning: no .grd files found in {}", src_root.display());
            return;
        }

        for rel in files {
            let path = src_root.join(&rel);
            let src = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {}: {e}", path.display());
                    process::exit(1);
                }
            };
            let formatted = match format_source(&src) {
                Ok(s) => s,
                Err(errors) => {
                    for err in errors {
                        eprintln!("error: {}: {}", path.display(), err);
                    }
                    process::exit(1);
                }
            };
            if src != formatted {
                if let Err(e) = fs::write(&path, formatted) {
                    eprintln!("error: {}: {e}", path.display());
                    process::exit(1);
                }
                println!("{}", rel.display());
            }
        }
    });
}

fn cmd_status() {
    with_project_root(|root| println!("{}", root.display()));
}

fn cmd_asm(plan_name: &str) {
    with_project_root(|root| {
        let res = do_compile(root, true);
        let Some(_plan) = res.plans.iter().find(|p| p.name == plan_name) else {
            let available = res
                .plans
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("error: plan '{plan_name}' not found");
            if !available.is_empty() {
                eprintln!("available plans: {available}");
            }
            process::exit(1);
        };

        let json = render_ctx_for_plan(&res, plan_name).unwrap_or_else(|| {
            eprintln!("error: plan '{plan_name}' not found");
            process::exit(1);
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&json).unwrap_or_else(|e| {
                eprintln!("error: failed to encode asm json: {e}");
                process::exit(1);
            })
        );
    });
}

fn cmd_lsp_start() {
    if let Err(e) = ground_lsp::start() {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn cmd_lsp_stop() {
    if let Err(e) = ground_lsp::stop() {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn write_type_units(root: &Path, type_units: &[ground_compile::TypeUnit]) {
    for unit in type_units {
        let out_path = root.join(".ground/types").join(&unit.file);
        if let Some(parent) = out_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("error: {}: {e}", parent.display());
                process::exit(1);
            }
        }
        if let Err(e) = fs::write(&out_path, &unit.content) {
            eprintln!("error: {}: {e}", out_path.display());
            process::exit(1);
        }
    }
}

fn cmd_gen_types() {
    with_project_root(|root| {
        let res = do_compile(root, false);

        for unit in &res.type_units {
            write_type_units(root, std::slice::from_ref(unit));
            println!("wrote .ground/types/{}", unit.file);
        }
    });
}

// ---------------------------------------------------------------------------
// Ground entity lookup
// ---------------------------------------------------------------------------

fn build_lookup(res: &CompileRes) -> Vec<(String, String)> {
    let mut lookup = Vec::new();
    for def in &res.defs {
        let alias_u = def.name.replace('-', "_");
        let pfx_u = def
            .fields
            .iter()
            .find(|f| f.name == "prefix")
            .and_then(|f| match &f.value {
                AsmValue::Str(s) | AsmValue::Ref(s) => Some(s.replace('-', "_")),
                _ => None,
            })
            .unwrap_or_default();
        for inst_ref in collect_members(def) {
            let inst_u = inst_ref.name.replace('-', "_");
            let key = format!("{pfx_u}{alias_u}_{inst_u}");
            let label = format!("{}:{}", inst_ref.type_name, inst_ref.name);
            lookup.push((key, label));
        }
    }
    lookup
}

fn collect_members(def: &AsmDef) -> Vec<AsmDefRef> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(def.name.clone());
    for f in &def.fields {
        collect_refs_deep(&f.value, &mut out, &mut seen);
    }
    out
}

fn collect_refs_deep(
    v: &AsmValue,
    out: &mut Vec<AsmDefRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    match v {
        AsmValue::DefRef(r) => {
            if seen.insert(r.name.clone()) {
                out.push(r.clone());
            }
        }
        AsmValue::List(items) => {
            for i in items {
                collect_refs_deep(i, out, seen);
            }
        }
        AsmValue::Path(segs) => {
            for s in segs {
                collect_refs_deep(s, out, seen);
            }
        }
        AsmValue::Def(def) => {
            for f in &def.fields {
                collect_refs_deep(&f.value, out, seen);
            }
        }
        AsmValue::Variant(v) => {
            if let Some(p) = &v.payload {
                collect_refs_deep(p, out, seen);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Action display helpers
// ---------------------------------------------------------------------------

fn action_glyph(action: &ground_be_terra::terra_ops::Action) -> (&'static str, &'static str) {
    use ground_be_terra::terra_ops::Action::*;
    match action {
        Create => ("+", GREEN),
        Update => ("~", YELLOW),
        Replace => ("±", YELLOW),
        Delete => ("-", RED),
    }
}

fn dominant_verb(
    changes: &[&ground_be_terra::terra_ops::ResourceChange],
) -> (&'static str, &'static str) {
    use ground_be_terra::terra_ops::Action::*;
    let create = changes.iter().any(|c| matches!(c.action, Create));
    let delete = changes.iter().any(|c| matches!(c.action, Delete));
    let modify = changes.iter().any(|c| matches!(c.action, Update | Replace));
    match (create, delete, modify) {
        (true, false, false) => ("create", GREEN),
        (false, true, false) => ("delete", RED),
        _ => ("modify", YELLOW),
    }
}

// ---------------------------------------------------------------------------
// ground plan
// ---------------------------------------------------------------------------

fn render(ev: &ops_display::DisplayEvent) {
    println!("{}", ev.message);
    if let Some(detail) = &ev.detail {
        println!("  {detail}");
    }
}

fn run_events(
    rx: std::sync::mpsc::Receiver<RunEvent<OpsEvent>>,
    enricher: &mut TerraEnricher,
) -> bool {
    let mut ok = true;
    for event in rx {
        if let RunEvent::Exited(ref s) = event {
            ok = s.success;
        }
        for d in enricher.enrich(&event) {
            render(&d);
        }
    }
    ok
}

fn run_init_if_needed(dir: &Path, plan_name: &str, provider: &str, verbose: bool) {
    use ground_be_terra::terra_ops;

    let init = terra_ops::init_if_needed(dir);
    let Some(rx) = (match init {
        Ok(rx) => rx,
        Err(terra_ops::OpsError::Other(msg)) if msg == "state migration required" => {
            let backend = read_backend_summary(dir);
            eprintln!("error: plan '{plan_name}' requires Terraform state migration");
            eprintln!();
            eprintln!("reason:");
            eprintln!("- this plan was previously initialized with local state");
            eprintln!("- it now has a remote backend configured");
            eprintln!();
            eprintln!("migration candidate:");
            eprintln!("- plan: {plan_name}");
            if let Some((bucket, key, region)) = backend {
                eprintln!("- bucket: {bucket}");
                eprintln!("- key: {key}");
                eprintln!("- region: {region}");
            }
            eprintln!();
            eprintln!("`ground tf migrate` will:");
            eprintln!("- ask for confirmation");
            eprintln!("- run `terraform init -migrate-state`");
            eprintln!("- move local state into the remote backend");
            eprintln!();
            eprintln!("`ground tf migrate` will not apply infrastructure changes.");
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    }) else {
        return;
    };

    let mut enricher = TerraEnricher::new(
        plan_name.to_string(),
        Op::Init,
        provider.to_string(),
        String::new(),
        vec![],
        verbose,
    );
    if !run_events(rx, &mut enricher) {
        eprintln!("error: terraform init failed");
        process::exit(1);
    }
}

fn fmt_attr_val(val: &AttrVal) -> String {
    match val {
        AttrVal::Scalar(s) => s.clone(),
        AttrVal::Unknown => "(known after apply)".to_string(),
        AttrVal::Sensitive => "(sensitive value)".to_string(),
        AttrVal::Null => "null".to_string(),
        AttrVal::Block(_) => "{...}".to_string(),
        AttrVal::List(items) => format!("[{} items]", items.len()),
    }
}

fn display_attr(val: &AttrVal, key: &str, glyph: &str, color: &str, indent: usize) {
    let pad = " ".repeat(indent);
    match val {
        AttrVal::Null => {}
        AttrVal::Block(pairs) if pairs.is_empty() => {}
        AttrVal::List(items) if items.is_empty() => {}
        AttrVal::Block(pairs) => {
            println!("  {pad}{color}{glyph}{RESET} {DIM}{key}{RESET} = {{");
            for (k, v) in pairs {
                display_attr(v, k, glyph, color, indent + 4);
            }
            println!("  {pad}  }}");
        }
        AttrVal::List(items) => {
            println!("  {pad}{color}{glyph}{RESET} {DIM}{key}{RESET} = [");
            for item in items {
                match item {
                    AttrVal::Block(pairs) if !pairs.is_empty() => {
                        println!("    {pad}{color}{glyph}{RESET} {{");
                        for (k, v) in pairs {
                            display_attr(v, k, glyph, color, indent + 8);
                        }
                        println!("    {pad}  }},");
                    }
                    _ => println!(
                        "    {pad}{color}{glyph}{RESET} {DIM}{}{RESET},",
                        fmt_attr_val(item)
                    ),
                }
            }
            println!("  {pad}  ]");
        }
        _ => {
            println!(
                "  {pad}{color}{glyph}{RESET} {DIM}{key}{RESET} = {DIM}{}{RESET}",
                fmt_attr_val(val)
            );
        }
    }
}

fn display_resource_attrs(change: &terra_ops::ResourceChange) {
    match change.action {
        Action::Create | Action::Replace => {
            for a in &change.attrs {
                if let Some(val) = &a.after {
                    display_attr(val, &a.key, "+", GREEN, 4);
                }
            }
        }
        Action::Delete => {
            for a in &change.attrs {
                if let Some(val) = &a.before {
                    display_attr(val, &a.key, "-", RED, 4);
                }
            }
        }
        Action::Update => {
            for a in &change.attrs {
                if let (Some(bv), Some(av)) = (&a.before, &a.after) {
                    if bv != av {
                        println!("      {YELLOW}~{RESET} {DIM}{}{RESET} = {DIM}{}{RESET} {DIM}->{RESET} {DIM}{}{RESET}",
                            a.key, fmt_attr_val(bv), fmt_attr_val(av));
                    }
                }
            }
        }
    }
}

fn display_plan_summary(
    summary: &ground_be_terra::terra_ops::PlanSummary,
    res: &CompileRes,
    plan_name: &str,
    provider: &str,
    verbose: bool,
) {
    use ground_be_terra::terra_ops;
    use std::collections::BTreeMap;

    let lookup = build_lookup(res);
    let ground_entity = |resource_name: &str| -> String {
        for (underscored, label) in &lookup {
            if resource_name == underscored.as_str()
                || resource_name.starts_with(&format!("{underscored}_"))
            {
                return label.clone();
            }
        }
        format!("plan:{plan_name}")
    };

    // Group by ground entity name
    let mut groups: BTreeMap<String, Vec<&terra_ops::ResourceChange>> = BTreeMap::new();
    for c in &summary.changes {
        groups
            .entry(ground_entity(&c.resource_name))
            .or_default()
            .push(c);
    }

    println!();
    println!("{BOLD}plan {plan_name}{RESET} {DIM}→ {provider}{RESET}");
    println!();

    if groups.is_empty() {
        println!("{DIM}no changes{RESET}");
    } else {
        for (entity, changes) in &groups {
            let (verb, color) = dominant_verb(changes);
            println!("{color}{verb}{RESET} {BOLD}{entity}{RESET}");
            for c in changes {
                let (glyph, gcolor) = action_glyph(&c.action);
                println!(
                    "  {gcolor}{glyph}{RESET} {DIM}{}.{}{RESET}",
                    c.resource_type, c.resource_name
                );
                if verbose {
                    display_resource_attrs(c);
                }
            }
            println!();
        }
    }

    let (cr, up, de) = (summary.creates(), summary.updates(), summary.destroys());
    println!("{GREEN}create {cr}{RESET}  {YELLOW}modify {up}{RESET}  {RED}delete {de}{RESET}");
    println!();
}

fn cmd_apply(plan_name: &str, verbose: bool) {
    use ground_be_terra::terra_ops;

    with_project_root(|root| {
        let (res, outputs, target, plan_name) = compile_and_gen(root, plan_name);
        let provider = "aws".to_string();
        let lookup = build_lookup(&res);
        let check = tf_check(root, &res).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            process::exit(1);
        });
        if !check.untracked_orphans.is_empty()
            || !check.tracked_orphans.is_empty()
            || !check.registry_mismatches.is_empty()
        {
            print_tf_check_report(&check);
            if !check.registry_mismatches.is_empty() {
                fix_registry_mismatches(root, &target, &check);
            }
            if tf_check_has_blockers(&check) {
                process::exit(1);
            }
        }

        write_render_units(root, &target, &outputs);
        let full_plan =
            selected_full_plan_name(&res, &plan_name).unwrap_or_else(|| plan_name.clone());
        let state_dir = format!("{}/{}", render_out_dir(&target), plan_name);
        upsert_state_binding_for_plan_dir(root, &full_plan, &target, &state_dir).unwrap_or_else(
            |e| {
                eprintln!("error: {e}");
                process::exit(1);
            },
        );

        let dir = root.join(render_out_dir(&target)).join(&plan_name);
        run_init_if_needed(&dir, &plan_name, &provider, verbose);

        let rx = terra_ops::apply(&dir).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            process::exit(1);
        });
        let mut enricher = TerraEnricher::new(
            plan_name.clone(),
            Op::Apply,
            provider.clone(),
            String::new(),
            lookup,
            verbose,
        );
        if !run_events(rx, &mut enricher) {
            eprintln!("error: terraform apply failed");
            process::exit(1);
        }
    });
}

fn cmd_plan(plan_name: &str, verbose: bool) {
    use ground_be_terra::terra_ops;

    with_project_root(|root| {
        let (res, outputs, target, plan_name) = compile_and_gen(root, plan_name);
        let provider = "aws".to_string();
        let lookup = build_lookup(&res);
        let check = tf_check(root, &res).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            process::exit(1);
        });
        if !check.untracked_orphans.is_empty()
            || !check.tracked_orphans.is_empty()
            || !check.registry_mismatches.is_empty()
        {
            print_tf_check_report(&check);
            if !check.registry_mismatches.is_empty() {
                fix_registry_mismatches(root, &target, &check);
            }
            if tf_check_has_blockers(&check) {
                process::exit(1);
            }
        }

        write_render_units(root, &target, &outputs);
        let full_plan =
            selected_full_plan_name(&res, &plan_name).unwrap_or_else(|| plan_name.clone());
        let state_dir = format!("{}/{}", render_out_dir(&target), plan_name);
        upsert_state_binding_for_plan_dir(root, &full_plan, &target, &state_dir).unwrap_or_else(
            |e| {
                eprintln!("error: {e}");
                process::exit(1);
            },
        );

        let dir = root.join(render_out_dir(&target)).join(&plan_name);
        run_init_if_needed(&dir, &plan_name, &provider, verbose);

        let rx = terra_ops::plan(&dir).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            process::exit(1);
        });
        let mut enricher = TerraEnricher::new(
            plan_name.clone(),
            Op::Plan,
            provider.clone(),
            String::new(),
            lookup,
            verbose,
        );

        for event in rx {
            match event {
                RunEvent::Exited(s) if !s.success => {
                    eprintln!("error: terraform plan failed");
                    process::exit(1);
                }
                RunEvent::Line(OpsEvent::PlanReady { summary }) => {
                    display_plan_summary(&summary, &res, &plan_name, &provider, verbose);
                }
                other => {
                    for d in enricher.enrich(&other) {
                        render(&d);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ground-{name}-{nanos}"));
        fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    fn test_compile_res(plan_names: &[&str]) -> CompileRes {
        CompileRes {
            units: vec![],
            defs: vec![],
            plans: plan_names
                .iter()
                .map(|name| ground_compile::PlanRoot {
                    name: (*name).to_string(),
                    def_idx: 0,
                    pack_path: vec![],
                    scope: ground_compile::ScopeId(0),
                    unit: None,
                })
                .collect(),
            scopes: vec![],
            type_units: vec![],
            errors: vec![],
        }
    }

    fn write_states_json(root: &Path, src: &str) {
        let path = states_path(root);
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        fs::write(path, src).unwrap();
    }

    fn read_states_json(root: &Path) -> String {
        fs::read_to_string(states_path(root)).unwrap()
    }

    #[test]
    fn tf_check_reports_untracked_stateful_dirs() {
        let root = temp_test_dir("tf-check-untracked");
        fs::create_dir_all(root.join(".ground/tf/orphan/.terraform")).unwrap();

        let check = tf_check(&root, &test_compile_res(&["test:current"])).unwrap();
        assert_eq!(
            check,
            TfCheckRes {
                untracked_orphans: vec![".ground/tf/orphan".to_string()],
                tracked_orphans: vec![],
                registry_mismatches: vec![],
            }
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tf_check_ignores_untracked_stateful_dir_for_current_plan() {
        let root = temp_test_dir("tf-check-current-plan-dir");
        fs::create_dir_all(root.join(".ground/tf/current/.terraform")).unwrap();

        let check = tf_check(&root, &test_compile_res(&["current"])).unwrap();
        assert_eq!(
            check,
            TfCheckRes {
                untracked_orphans: vec![],
                tracked_orphans: vec![],
                registry_mismatches: vec![],
            }
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tf_check_reports_tracked_state_json_orphans_with_codes() {
        let root = temp_test_dir("tf-check-tracked");
        fs::create_dir_all(root.join(".ground/tf/old-a/.terraform")).unwrap();
        fs::create_dir_all(root.join(".ground/tf/old-b/.terraform")).unwrap();
        write_states_json(
            &root,
            r#"
            [
              { "plan": "test:old-a", "target": "tf:json", "dir": ".ground/tf/old-a" },
              { "plan": "test:old-b", "target": "tf:json", "dir": ".ground/tf/old-b" }
            ]
            "#,
        );

        let check = tf_check(&root, &test_compile_res(&["test:current"])).unwrap();
        assert_eq!(check.untracked_orphans, Vec::<String>::new());
        assert_eq!(
            check.tracked_orphans,
            vec![
                TrackedStateOrphan {
                    code: orphan_code_for_dir(".ground/tf/old-a"),
                    plan: "test:old-a".to_string(),
                    dir: ".ground/tf/old-a".to_string(),
                },
                TrackedStateOrphan {
                    code: orphan_code_for_dir(".ground/tf/old-b"),
                    plan: "test:old-b".to_string(),
                    dir: ".ground/tf/old-b".to_string(),
                }
            ]
        );
        assert_eq!(
            check.registry_mismatches,
            Vec::<StateRegistryMismatch>::new()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tf_check_ignores_state_json_entries_without_stateful_dir() {
        let root = temp_test_dir("tf-check-no-state");
        fs::create_dir_all(root.join(".ground/tf/old")).unwrap();
        write_states_json(
            &root,
            r#"
            [
              { "plan": "test:old", "target": "tf:json", "dir": ".ground/tf/old" }
            ]
            "#,
        );

        let check = tf_check(&root, &test_compile_res(&["test:current"])).unwrap();
        assert_eq!(
            check,
            TfCheckRes {
                untracked_orphans: vec![],
                tracked_orphans: vec![],
                registry_mismatches: vec![],
            }
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tf_check_reports_registry_mismatch_for_current_plan_dir() {
        let root = temp_test_dir("tf-check-registry-mismatch");
        fs::create_dir_all(root.join(".ground/tf/current/.terraform")).unwrap();
        write_states_json(
            &root,
            r#"
            [
              { "plan": "test:boo", "target": "tf:json", "dir": ".ground/tf/current" }
            ]
            "#,
        );

        let check = tf_check(&root, &test_compile_res(&["current"])).unwrap();
        assert_eq!(check.untracked_orphans, Vec::<String>::new());
        assert_eq!(check.tracked_orphans, Vec::<TrackedStateOrphan>::new());
        assert_eq!(
            check.registry_mismatches,
            vec![StateRegistryMismatch {
                recorded_plan: "test:boo".to_string(),
                actual_plan: "current".to_string(),
                dir: ".ground/tf/current".to_string(),
            }]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upsert_state_binding_adds_missing_record() {
        let root = temp_test_dir("state-upsert-add");
        let target = RenderTarget {
            backend: "tf".into(),
            out_type: "json".into(),
        };

        upsert_state_binding_for_plan_dir(
            &root,
            "test:ground-test-platform",
            &target,
            ".ground/tf/ground-test-platform",
        )
        .unwrap();

        let src = read_states_json(&root);
        assert!(src.contains("\"plan\": \"test:ground-test-platform\""));
        assert!(src.contains("\"target\": \"tf:json\""));
        assert!(src.contains("\"dir\": \".ground/tf/ground-test-platform\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upsert_state_binding_rewrites_stale_binding_for_same_dir() {
        let root = temp_test_dir("state-upsert-rewrite");
        write_states_json(
            &root,
            r#"
            [
              { "plan": "test:boo", "target": "tf:json", "dir": ".ground/tf/ground-test-platform" }
            ]
            "#,
        );
        let target = RenderTarget {
            backend: "tf".into(),
            out_type: "json".into(),
        };

        upsert_state_binding_for_plan_dir(
            &root,
            "test:ground-test-platform",
            &target,
            ".ground/tf/ground-test-platform",
        )
        .unwrap();

        let src = read_states_json(&root);
        assert!(src.contains("\"plan\": \"test:ground-test-platform\""));
        assert!(!src.contains("\"plan\": \"test:boo\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fix_registry_mismatches_rewrites_states_json() {
        let root = temp_test_dir("fix-registry-mismatches");
        fs::create_dir_all(root.join(".ground/tf/current/.terraform")).unwrap();
        write_states_json(
            &root,
            r#"
            [
              { "plan": "test:boo", "target": "tf:json", "dir": ".ground/tf/current" }
            ]
            "#,
        );
        let target = RenderTarget {
            backend: "tf".into(),
            out_type: "json".into(),
        };
        let check = tf_check(&root, &test_compile_res(&["current"])).unwrap();

        fix_registry_mismatches(&root, &target, &check);

        let src = read_states_json(&root);
        assert!(src.contains("\"plan\": \"current\""));
        assert!(!src.contains("\"plan\": \"test:boo\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fix_registry_mismatches_prunes_non_stateful_registry_records() {
        let root = temp_test_dir("fix-registry-mismatches-prune");
        fs::create_dir_all(root.join(".ground/tf/current/.terraform")).unwrap();
        write_states_json(
            &root,
            r#"
            [
              { "plan": "test:boo", "target": "tf:json", "dir": ".ground/tf/current" },
              { "plan": "test:old", "target": "tf:json", "dir": ".ground/tf/old" }
            ]
            "#,
        );
        let target = RenderTarget {
            backend: "tf".into(),
            out_type: "json".into(),
        };
        let check = tf_check(&root, &test_compile_res(&["current"])).unwrap();

        fix_registry_mismatches(&root, &target, &check);

        let src = read_states_json(&root);
        assert!(src.contains("\"dir\": \".ground/tf/current\""));
        assert!(!src.contains("\"dir\": \".ground/tf/old\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rename_tracked_orphan_to_plan_moves_stateful_artifacts_and_updates_registry() {
        let root = temp_test_dir("rename-tracked-orphan");
        fs::create_dir_all(root.join(".ground/tf/old/.terraform")).unwrap();
        fs::write(root.join(".ground/tf/old/terraform.tfstate"), "{}").unwrap();
        fs::write(root.join(".ground/tf/old/.terraform.lock.hcl"), "# lock").unwrap();
        fs::create_dir_all(root.join(".ground/tf/current")).unwrap();
        fs::write(root.join(".ground/tf/current/main.tf.json"), "{}").unwrap();
        write_states_json(
            &root,
            r#"
            [
              { "plan": "test:old", "target": "tf:json", "dir": ".ground/tf/old" }
            ]
            "#,
        );

        let res = test_compile_res(&["current"]);
        let target = RenderTarget {
            backend: "tf".into(),
            out_type: "json".into(),
        };
        let source = orphan_code_for_dir(".ground/tf/old");

        let (from, to, dir) =
            rename_tracked_orphan_to_plan(&root, &res, &target, &source, "current").unwrap();

        assert_eq!(from, "test:old");
        assert_eq!(to, "current");
        assert_eq!(dir, ".ground/tf/current");
        assert!(root.join(".ground/tf/current/.terraform").is_dir());
        assert!(root.join(".ground/tf/current/terraform.tfstate").is_file());
        assert!(root
            .join(".ground/tf/current/.terraform.lock.hcl")
            .is_file());
        assert!(!root.join(".ground/tf/old/terraform.tfstate").exists());
        let src = read_states_json(&root);
        assert!(src.contains("\"plan\": \"current\""));
        assert!(src.contains("\"dir\": \".ground/tf/current\""));
        assert!(!src.contains("\"plan\": \"test:old\""));
        assert!(!src.contains("\"dir\": \".ground/tf/old\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rename_tracked_orphan_to_plan_rejects_stateful_target_dir() {
        let root = temp_test_dir("rename-tracked-orphan-stateful-target");
        fs::create_dir_all(root.join(".ground/tf/old/.terraform")).unwrap();
        fs::write(root.join(".ground/tf/old/terraform.tfstate"), "{}").unwrap();
        fs::create_dir_all(root.join(".ground/tf/current/.terraform")).unwrap();
        write_states_json(
            &root,
            r#"
            [
              { "plan": "test:old", "target": "tf:json", "dir": ".ground/tf/old" }
            ]
            "#,
        );

        let res = test_compile_res(&["current"]);
        let target = RenderTarget {
            backend: "tf".into(),
            out_type: "json".into(),
        };
        let source = orphan_code_for_dir(".ground/tf/old");

        let err = rename_tracked_orphan_to_plan(&root, &res, &target, &source, "current")
            .expect_err("rename should fail when target dir is already stateful");
        assert!(err.contains("already has Terraform stateful artifacts"));

        let _ = fs::remove_dir_all(root);
    }
}
