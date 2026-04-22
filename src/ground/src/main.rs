mod ops_display;

use std::{env, fs, io, path::{Path, PathBuf}, process};

use ground_be_terra::JsonUnit;
use ground_compile::{compile, format_source, CompileReq, CompileRes, AsmDef, AsmDefRef, AsmValue, Unit};
use ground_run::RunEvent;
use ground_be_terra::terra_ops::{self, Action, AttrVal, OpsEvent};
use ops_display::{Op, TerraEnricher};
use serde_json::Value;

const GREEN:  &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED:    &str = "\x1b[31m";
const BOLD:   &str = "\x1b[1m";
const DIM:    &str = "\x1b[2m";
const RESET:  &str = "\x1b[0m";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.as_slice() {
        [cmd, sub] if cmd == "tf" && sub == "init"                                => cmd_tf_init(false),
        [cmd, sub, flag] if cmd == "tf" && sub == "init" && flag == "--git-ignore" => cmd_tf_init(true),
        [cmd, sub] if cmd == "tf" && sub == "migrate"                              => cmd_tf_migrate(),
        [cmd, sub] if cmd == "gen" && sub == "terra"                              => cmd_gen_terra(),
        [cmd, sub] if cmd == "gen" && sub == "types"                              => cmd_gen_types(),
        [cmd] if cmd == "fmt"                                                     => cmd_fmt(),
        [cmd, ..] if cmd == "fmt"                                                 => cmd_fmt(),
        [cmd] if cmd == "status"                                                  => cmd_status(),
        [cmd, sub] if cmd == "lsp" && sub == "start"                              => cmd_lsp_start(),
        [cmd, sub] if cmd == "lsp" && sub == "stop"                               => cmd_lsp_stop(),
        [cmd] if cmd == "plan"                                                     => cmd_plan_ls(),
        [cmd, sub] if cmd == "plan" && sub == "ls"                                 => cmd_plan_ls(),
        [cmd, name] if cmd == "plan"                                               => cmd_plan(name, false),
        [cmd, name, flag] if cmd == "plan" && (flag == "--verbose" || flag == "-v") => cmd_plan(name, true),
        [cmd, name] if cmd == "apply"                                              => cmd_apply(name, false),
        [cmd, name, flag] if cmd == "apply" && (flag == "--verbose" || flag == "-v") => cmd_apply(name, true),
        _ => {
            eprintln!("usage:");
            eprintln!("  ground tf init [--git-ignore]");
            eprintln!("  ground tf migrate");
            eprintln!("  ground gen terra");
            eprintln!("  ground gen types");
            eprintln!("  ground fmt");
            eprintln!("  ground status");
            eprintln!("  ground lsp start");
            eprintln!("  ground lsp stop");
            eprintln!("  ground plan ls");
            eprintln!("  ground plan <name> [--verbose|-v]");
            eprintln!("  ground apply <name> [--verbose|-v]");
            process::exit(1);
        }
    }
}

fn cmd_tf_init(git_ignore: bool) {
    if let Err(e) = fs::create_dir(".ground") {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            eprintln!("error: {e}");
            process::exit(1);
        }
    }

    let settings_path = ".ground/settings.json";
    let settings = "{\n  \"provider\": \"aws\"\n}\n";

    if let Err(e) = fs::write(settings_path, settings) {
        eprintln!("error: {settings_path}: {e}");
        process::exit(1);
    }

    println!("initialized .ground/");

    if git_ignore {
        let needed = [
            ".ground/terra/**/.terraform/",
            ".ground/terra/**/terraform.tfstate",
            ".ground/terra/**/terraform.tfstate.backup",
        ];

        let existing = fs::read_to_string(".gitignore").unwrap_or_default();
        let to_add: Vec<&str> = needed.iter()
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

struct PlanStamp {
    alias: String,
    kind: String,
    state: Option<String>,
    backend_present: bool,
}

fn cmd_tf_migrate() {
    with_project_root(|root| {
        let res = do_compile(root, true);
        let outputs = match ground_be_terra::generate_each(&res) {
            Ok(o)  => o,
            Err(e) => { eprintln!("error: {e}"); process::exit(1); }
        };
        write_plan_units(root, &outputs);

        let stamps = read_plan_stamps(&root.join(".ground/terra"));
        let migratable: Vec<&PlanStamp> = stamps.iter()
            .filter(|stamp| stamp.kind == "state_store" && stamp.state.as_deref() == Some("remote") && stamp.backend_present)
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

        let (res, outputs, plan_name) = compile_and_gen(root, &stamp.alias);
        let provider = "aws".to_string();
        let lookup = build_lookup(&res);
        write_plan_units(root, &outputs);

        let dir = root.join(".ground/terra").join(&plan_name);
        let Some(rx) = terra_ops::migrate_state(&dir)
            .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); })
        else {
            println!("no migration needed for plan {plan_name}");
            return;
        };

        let mut enricher = TerraEnricher::new(plan_name.clone(), Op::Init, provider, String::new(), lookup, true);
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
        let Ok(src) = fs::read_to_string(&path) else { continue; };
        let Ok(json) = serde_json::from_str::<Value>(&src) else { continue; };
        let Some(alias) = json.get("alias").and_then(|v| v.as_str()).map(str::to_string) else { continue; };
        let Some(kind) = json.get("kind").and_then(|v| v.as_str()).map(str::to_string) else { continue; };
        let state = json.get("state").and_then(|v| v.as_str()).map(str::to_string);
        let backend_present = json.get("backend_present").and_then(|v| v.as_bool()).unwrap_or(false);
        out.push(PlanStamp { alias, kind, state, backend_present });
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
        Ok(e)  => e,
        Err(e) => { eprintln!("warning: cannot read {:?}: {e}", dir); return; }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip hidden directories (.ground, .git, .direnv …).
        let is_hidden = path.file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |n| n.starts_with('.'));
        if path.is_dir() && !is_hidden {
            collect_grd_recursive(root, &path, units);
        } else if path.extension().map_or(false, |e| e == "grd") {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            // Pack path = directory components (not the filename).
            let pack_path: Vec<String> = rel.parent()
                .map(|p| p.components()
                    .filter_map(|c| match c {
                        std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                        _ => None,
                    })
                    .collect())
                .unwrap_or_default();
            // Unit name: the file stem unless it is "pack" (pack.grd merges into
            // its directory scope; all other named files create a named sub-pack).
            let stem = rel.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let name = if stem == "pack" {
                String::new()
            } else {
                stem.to_string()
            };
            match fs::read_to_string(&path) {
                Ok(src) => {
                    let ts_src = path.with_extension("ts")
                        .to_str()
                        .and_then(|p| fs::read_to_string(p).ok());
                    units.push(Unit { name, path: pack_path, src, ts_src });
                }
                Err(e) => { eprintln!("error: {}: {e}", path.display()); process::exit(1); }
            }
        }
    }
}

/// Compile all .grd files under `root`, print errors and exit on failure.
fn do_compile(root: &Path, require_plans: bool) -> CompileRes {
    let units = collect_grd_files(root);

    if units.is_empty() {
        eprintln!("no .grd files found in project");
        process::exit(1);
    }

    let unit_names: Vec<String> = units.iter().map(|u| u.name.clone()).collect();

    let res = compile(CompileReq { units });

    if !res.errors.is_empty() {
        for e in &res.errors {
            if let Some(loc) = &e.loc {
                let name = unit_names.get(loc.unit as usize).map(|s| s.as_str()).unwrap_or("?");
                eprintln!("error: {}:{}:{}: {}", name, loc.line, loc.col, e.message);
            } else {
                eprintln!("error: {}", e.message);
            }
        }
        process::exit(1);
    }

    if require_plans && res.defs.is_empty() {
        eprintln!("no plan declarations found — nothing to generate");
        process::exit(1);
    }

    res
}

fn compile_and_gen(root: &Path, plan_name: &str) -> (CompileRes, Vec<JsonUnit>, String) {
    let res = do_compile(root, true);

    let outputs = match ground_be_terra::generate_each(&res) {
        Ok(o)  => o,
        Err(e) => { eprintln!("error: {e}"); process::exit(1); }
    };

    let Some(def) = res.defs.iter().find(|d| d.name == plan_name) else {
        let available = res.defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", ");
        eprintln!("error: plan '{plan_name}' not found");
        if !available.is_empty() {
            eprintln!("available plans: {available}");
        }
        process::exit(1);
    };

    let selected_plan = def.name.clone();
    let plan_outputs: Vec<JsonUnit> = outputs.into_iter()
        .filter(|u| u.file.starts_with(&format!("{selected_plan}/")))
        .collect();

    (res, plan_outputs, selected_plan)
}

fn write_plan_units(root: &Path, outputs: &[JsonUnit]) {
    for output in outputs {
        let out_path = root.join(".ground/terra").join(&output.file);
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

fn cmd_gen_terra() {
    with_project_root(|root| {
        let res = do_compile(root, true);

        let outputs = match ground_be_terra::generate_each(&res) {
            Ok(o)  => o,
            Err(e) => { eprintln!("error: {e}"); process::exit(1); }
        };

        for output in &outputs {
            write_plan_units(root, std::slice::from_ref(output));
            println!("wrote .ground/terra/{}", output.file);
        }
    });
}

fn cmd_plan_ls() {
    with_project_root(|root| {
        let res = do_compile(root, true);
        for def in &res.defs {
            println!("{}", def.name);
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
        Err(e) => { eprintln!("warning: cannot read {:?}: {e}", dir); return; }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_hidden = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
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
        let files = collect_grd_paths_recursive(root);
        if files.is_empty() {
            eprintln!("warning: no .grd files found in project");
            return;
        }

        for rel in files {
            let path = root.join(&rel);
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
        let pfx_u = def.fields.iter()
            .find(|f| f.name == "prefix")
            .and_then(|f| match &f.value {
                AsmValue::Str(s) | AsmValue::Ref(s) => Some(s.replace('-', "_")),
                _ => None,
            })
            .unwrap_or_default();
        for inst_ref in collect_members(def) {
            let inst_u = inst_ref.name.replace('-', "_");
            let key    = format!("{pfx_u}{alias_u}_{inst_u}");
            let label  = format!("{}:{}", inst_ref.type_name, inst_ref.name);
            lookup.push((key, label));
        }
    }
    lookup
}

fn collect_members(def: &AsmDef) -> Vec<AsmDefRef> {
    let mut out  = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(def.name.clone());
    for f in &def.fields {
        collect_refs_deep(&f.value, &mut out, &mut seen);
    }
    out
}

fn collect_refs_deep(
    v:      &AsmValue,
    out:    &mut Vec<AsmDefRef>,
    seen:   &mut std::collections::HashSet<String>,
) {
    match v {
        AsmValue::DefRef(r) => {
            if seen.insert(r.name.clone()) {
                out.push(r.clone());
            }
        }
        AsmValue::List(items) => { for i in items { collect_refs_deep(i, out, seen); } }
        AsmValue::Path(segs) => { for s in segs { collect_refs_deep(s, out, seen); } }
        AsmValue::Def(def) => { for f in &def.fields { collect_refs_deep(&f.value, out, seen); } }
        AsmValue::Variant(v) => { if let Some(p) = &v.payload { collect_refs_deep(p, out, seen); } }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Action display helpers
// ---------------------------------------------------------------------------

fn action_glyph(action: &ground_be_terra::terra_ops::Action) -> (&'static str, &'static str) {
    use ground_be_terra::terra_ops::Action::*;
    match action {
        Create  => ("+", GREEN),
        Update  => ("~", YELLOW),
        Replace => ("±", YELLOW),
        Delete  => ("-", RED),
    }
}

fn dominant_verb(changes: &[&ground_be_terra::terra_ops::ResourceChange]) -> (&'static str, &'static str) {
    use ground_be_terra::terra_ops::Action::*;
    let create = changes.iter().any(|c| matches!(c.action, Create));
    let delete = changes.iter().any(|c| matches!(c.action, Delete));
    let modify = changes.iter().any(|c| matches!(c.action, Update | Replace));
    match (create, delete, modify) {
        (true,  false, false) => ("create", GREEN),
        (false, true,  false) => ("delete", RED),
        _                     => ("modify", YELLOW),
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

fn run_events(rx: std::sync::mpsc::Receiver<RunEvent<OpsEvent>>, enricher: &mut TerraEnricher) -> bool {
    let mut ok = true;
    for event in rx {
        if let RunEvent::Exited(ref s) = event { ok = s.success; }
        for d in enricher.enrich(&event) { render(&d); }
    }
    ok
}

fn run_init_if_needed(
    dir: &Path,
    plan_name: &str,
    provider: &str,
    verbose: bool,
) {
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

    let mut enricher = TerraEnricher::new(plan_name.to_string(), Op::Init, provider.to_string(), String::new(), vec![], verbose);
    if !run_events(rx, &mut enricher) {
        eprintln!("error: terraform init failed");
        process::exit(1);
    }
}

fn fmt_attr_val(val: &AttrVal) -> String {
    match val {
        AttrVal::Scalar(s)   => s.clone(),
        AttrVal::Unknown     => "(known after apply)".to_string(),
        AttrVal::Sensitive   => "(sensitive value)".to_string(),
        AttrVal::Null        => "null".to_string(),
        AttrVal::Block(_)    => "{...}".to_string(),
        AttrVal::List(items) => format!("[{} items]", items.len()),
    }
}

fn display_attr(val: &AttrVal, key: &str, glyph: &str, color: &str, indent: usize) {
    let pad = " ".repeat(indent);
    match val {
        AttrVal::Null                             => {}
        AttrVal::Block(pairs) if pairs.is_empty() => {}
        AttrVal::List(items)  if items.is_empty() => {}
        AttrVal::Block(pairs) => {
            println!("  {pad}{color}{glyph}{RESET} {DIM}{key}{RESET} = {{");
            for (k, v) in pairs { display_attr(v, k, glyph, color, indent + 4); }
            println!("  {pad}  }}");
        }
        AttrVal::List(items) => {
            println!("  {pad}{color}{glyph}{RESET} {DIM}{key}{RESET} = [");
            for item in items {
                match item {
                    AttrVal::Block(pairs) if !pairs.is_empty() => {
                        println!("    {pad}{color}{glyph}{RESET} {{");
                        for (k, v) in pairs { display_attr(v, k, glyph, color, indent + 8); }
                        println!("    {pad}  }},");
                    }
                    _ => println!("    {pad}{color}{glyph}{RESET} {DIM}{}{RESET},", fmt_attr_val(item)),
                }
            }
            println!("  {pad}  ]");
        }
        _ => {
            println!("  {pad}{color}{glyph}{RESET} {DIM}{key}{RESET} = {DIM}{}{RESET}", fmt_attr_val(val));
        }
    }
}

fn display_resource_attrs(change: &terra_ops::ResourceChange) {
    match change.action {
        Action::Create | Action::Replace => {
            for a in &change.attrs {
                if let Some(val) = &a.after { display_attr(val, &a.key, "+", GREEN, 4); }
            }
        }
        Action::Delete => {
            for a in &change.attrs {
                if let Some(val) = &a.before { display_attr(val, &a.key, "-", RED, 4); }
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
    summary:    &ground_be_terra::terra_ops::PlanSummary,
    res:        &CompileRes,
    plan_name:  &str,
    provider:   &str,
    verbose:    bool,
) {
    use std::collections::BTreeMap;
    use ground_be_terra::terra_ops;

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
        groups.entry(ground_entity(&c.resource_name)).or_default().push(c);
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
                println!("  {gcolor}{glyph}{RESET} {DIM}{}.{}{RESET}", c.resource_type, c.resource_name);
                if verbose { display_resource_attrs(c); }
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
        let (res, outputs, plan_name) = compile_and_gen(root, plan_name);
        let provider = "aws".to_string();
        let lookup = build_lookup(&res);

        write_plan_units(root, &outputs);

        let dir = root.join(".ground/terra").join(&plan_name);
        run_init_if_needed(&dir, &plan_name, &provider, verbose);

        let rx = terra_ops::apply(&dir)
            .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); });
        let mut enricher = TerraEnricher::new(plan_name.clone(), Op::Apply, provider.clone(), String::new(), lookup, verbose);
        if !run_events(rx, &mut enricher) {
            eprintln!("error: terraform apply failed");
            process::exit(1);
        }
    });
}

fn cmd_plan(plan_name: &str, verbose: bool) {
    use ground_be_terra::terra_ops;

    with_project_root(|root| {
        let (res, outputs, plan_name) = compile_and_gen(root, plan_name);
        let provider = "aws".to_string();
        let lookup = build_lookup(&res);

        write_plan_units(root, &outputs);

        let dir = root.join(".ground/terra").join(&plan_name);
        run_init_if_needed(&dir, &plan_name, &provider, verbose);

        let rx = terra_ops::plan(&dir)
            .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); });
        let mut enricher = TerraEnricher::new(plan_name.clone(), Op::Plan, provider.clone(), String::new(), lookup, verbose);

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
                    for d in enricher.enrich(&other) { render(&d); }
                }
            }
        }
    });
}
