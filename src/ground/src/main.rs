mod ops_display;

use std::{env, fs, path::{Path, PathBuf}, process};

use ground_compile::{compile, CompileReq, CompileRes, AsmValue, AsmInstRef, Symbol, Unit, STDLIB_UNIT_COUNT};
use ground_run::RunEvent;
use ground_be_terra::terra_ops::{self, Action, AttrVal, OpsEvent};
use ops_display::{Op, TerraEnricher};

const GREEN:  &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED:    &str = "\x1b[31m";
const BOLD:   &str = "\x1b[1m";
const DIM:    &str = "\x1b[2m";
const RESET:  &str = "\x1b[0m";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.as_slice() {
        [cmd] if cmd == "init"                                 => cmd_init(false),
        [cmd, flag] if cmd == "init" && flag == "--git-ignore" => cmd_init(true),
        [cmd, sub] if cmd == "gen" && sub == "terra"                              => cmd_gen_terra(),
        [cmd] if cmd == "plan"                                                     => cmd_plan(false),
        [cmd, flag] if cmd == "plan" && flag == "--verbose"                        => cmd_plan(true),
        [cmd] if cmd == "apply"                                                    => cmd_apply(false),
        [cmd, flag] if cmd == "apply" && flag == "--verbose"                       => cmd_apply(true),
        _ => {
            eprintln!("usage:");
            eprintln!("  ground init [--git-ignore]");
            eprintln!("  ground gen terra");
            eprintln!("  ground plan [--verbose]");
            eprintln!("  ground apply [--verbose]");
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

/// Compile all .grd files in the current directory, print errors and exit on failure.
fn do_compile() -> CompileRes {
    let units = collect_grd_files(&PathBuf::from("."));

    if units.is_empty() {
        eprintln!("no .grd files found in current directory");
        process::exit(1);
    }

    // stdlib units are prepended by compile(); their display names must come first.
    let mut unit_names: Vec<String> = vec![
        "<std>".into(),
        "<std:aws>".into(),
        "<std:aws:transform>".into(),
    ];
    debug_assert_eq!(unit_names.len(), STDLIB_UNIT_COUNT);
    unit_names.extend(units.iter().map(|u| u.name.clone()));

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

    if res.plans.is_empty() {
        eprintln!("no plan declarations found — nothing to generate");
        process::exit(1);
    }

    res
}

/// Compile and generate a merged Terraform JSON for the first deploy.
/// Used by `cmd_plan` and `cmd_apply` which operate on a single terraform workspace.
fn compile_and_gen() -> (CompileRes, String, String) {
    let res = do_compile();

    let json = match ground_be_terra::generate(&res) {
        Ok(j)  => j,
        Err(e) => { eprintln!("error: {e}"); process::exit(1); }
    };

    let stack_name = res.plans[0].name.clone();
    (res, json, stack_name)
}

fn write_stack(stack_name: &str, json: &str) {
    let dir      = format!(".ground/terra/{stack_name}");
    let out_path = format!("{dir}/main.tf.json");

    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("error: {dir}: {e}"); process::exit(1);
    }

    if let Err(e) = fs::write(&out_path, json) {
        eprintln!("error: {out_path}: {e}"); process::exit(1);
    }
}

fn cmd_gen_terra() {
    let res = do_compile();

    let outputs = match ground_be_terra::generate_each(&res) {
        Ok(o)  => o,
        Err(e) => { eprintln!("error: {e}"); process::exit(1); }
    };

    for (name, json) in &outputs {
        write_stack(name, json);
        println!("wrote .ground/terra/{name}/main.tf.json");
    }
}

// ---------------------------------------------------------------------------
// Ground entity lookup
// ---------------------------------------------------------------------------

fn build_lookup(res: &CompileRes) -> Vec<(String, String)> {
    let mut lookup = Vec::new();
    for plan in &res.plans {
        let alias_u = plan.name.replace('-', "_");
        let pfx_u = plan.fields.iter()
            .find(|f| f.name == "prefix")
            .and_then(|f| match &f.value {
                AsmValue::Str(s) | AsmValue::Ref(s) => Some(s.replace('-', "_")),
                _ => None,
            })
            .unwrap_or_default();
        for inst_ref in collect_members(&plan.root, &res.symbol) {
            let inst_u = inst_ref.name.replace('-', "_");
            let key    = format!("{pfx_u}{alias_u}_{inst_u}");
            let label  = format!("{}:{}", inst_ref.type_name, inst_ref.name);
            lookup.push((key, label));
        }
    }
    lookup
}

fn collect_members(inst: &ground_compile::AsmInst, symbol: &Symbol) -> Vec<AsmInstRef> {
    let mut out  = Vec::new();
    let mut seen = std::collections::HashSet::new();
    seen.insert(inst.name.clone());
    for f in &inst.fields {
        collect_refs_deep(&f.value, symbol, &mut out, &mut seen);
    }
    out
}

fn collect_refs_deep(
    v:      &AsmValue,
    symbol: &Symbol,
    out:    &mut Vec<AsmInstRef>,
    seen:   &mut std::collections::HashSet<String>,
) {
    match v {
        AsmValue::InstRef(r) => {
            if seen.insert(r.name.clone()) {
                out.push(r.clone());
                if let Some(inst) = symbol.get(&r.name) {
                    for f in &inst.fields {
                        collect_refs_deep(&f.value, symbol, out, seen);
                    }
                }
            }
        }
        AsmValue::List(items)  => { for i in items  { collect_refs_deep(i, symbol, out, seen); } }
        AsmValue::Path(segs)   => { for s in segs   { collect_refs_deep(s, symbol, out, seen); } }
        AsmValue::Inst(i)      => { for f in &i.fields { collect_refs_deep(&f.value, symbol, out, seen); } }
        AsmValue::Variant(v)   => { if let Some(p) = &v.payload { collect_refs_deep(p, symbol, out, seen); } }
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
    stack_name: &str,
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
        format!("stack:{stack_name}")
    };

    // Group by ground entity name
    let mut groups: BTreeMap<String, Vec<&terra_ops::ResourceChange>> = BTreeMap::new();
    for c in &summary.changes {
        groups.entry(ground_entity(&c.resource_name)).or_default().push(c);
    }

    println!();
    println!("{BOLD}stack {stack_name}{RESET} {DIM}→ {provider}{RESET}");
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

fn cmd_apply(verbose: bool) {
    use ground_be_terra::terra_ops;

    let (res, json, stack_name) = compile_and_gen();
    let provider = "aws".to_string();
    let lookup = build_lookup(&res);

    write_stack(&stack_name, &json);

    let dir = Path::new(".ground/terra").join(&stack_name);

    let rx = terra_ops::init(&dir)
        .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); });
    let mut enricher = TerraEnricher::new(stack_name.clone(), Op::Init, provider.clone(), String::new(), vec![], verbose);
    if !run_events(rx, &mut enricher) {
        eprintln!("error: terraform init failed");
        process::exit(1);
    }

    let rx = terra_ops::apply(&dir)
        .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); });
    let mut enricher = TerraEnricher::new(stack_name.clone(), Op::Apply, provider.clone(), String::new(), lookup, verbose);
    if !run_events(rx, &mut enricher) {
        eprintln!("error: terraform apply failed");
        process::exit(1);
    }
}

fn cmd_plan(verbose: bool) {
    use ground_be_terra::terra_ops;

    let (res, json, stack_name) = compile_and_gen();
    let provider = "aws".to_string();
    let lookup = build_lookup(&res);

    write_stack(&stack_name, &json);

    let dir = Path::new(".ground/terra").join(&stack_name);

    let rx = terra_ops::init(&dir)
        .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); });
    let mut enricher = TerraEnricher::new(stack_name.clone(), Op::Init, provider.clone(), String::new(), vec![], verbose);
    if !run_events(rx, &mut enricher) {
        eprintln!("error: terraform init failed");
        process::exit(1);
    }

    let rx = terra_ops::plan(&dir)
        .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); });
    let mut enricher = TerraEnricher::new(stack_name.clone(), Op::Plan, provider.clone(), String::new(), lookup, verbose);

    for event in rx {
        match event {
            RunEvent::Exited(s) if !s.success => {
                eprintln!("error: terraform plan failed");
                process::exit(1);
            }
            RunEvent::Line(OpsEvent::PlanReady { summary }) => {
                display_plan_summary(&summary, &res, &stack_name, &provider, verbose);
            }
            other => {
                for d in enricher.enrich(&other) { render(&d); }
            }
        }
    }
}
