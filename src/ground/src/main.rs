mod ops_display;

use std::{env, fs, path::Path, process};

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
// Parse .grd files and run code generation
// ---------------------------------------------------------------------------

fn parse_and_gen() -> (ground_core::Spec, String, String) {
    let entries = match fs::read_dir(".") {
        Ok(e)  => e,
        Err(e) => { eprintln!("error reading current directory: {e}"); process::exit(1); }
    };

    let mut sources_data: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "grd") {
            let path_str = path.to_string_lossy().into_owned();
            match fs::read_to_string(&path) {
                Ok(content) => sources_data.push((path_str, content)),
                Err(e)      => { eprintln!("error: {path_str}: {e}"); process::exit(1); }
            }
        }
    }

    if sources_data.is_empty() {
        eprintln!("no .grd files found in current directory");
        process::exit(1);
    }

    let sources: Vec<(&str, &str)> = sources_data.iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();

    let spec = match ground_compile::compile(&sources) {
        Ok(s)   => s,
        Err(es) => { for e in es { eprintln!("{e}"); } process::exit(1); }
    };

    if spec.deploys.is_empty() {
        eprintln!("no deploy declarations found — nothing to generate");
        process::exit(1);
    }

    let json = match ground_be_terra::generate(&spec) {
        Ok(j)  => j,
        Err(e) => { eprintln!("error: {e}"); process::exit(1); }
    };

    // Use the first deploy's alias as the stack name
    let stack_name = spec.deploys[0].alias.clone();

    (spec, json, stack_name)
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
    let (_spec, json, stack_name) = parse_and_gen();
    write_stack(&stack_name, &json);
    println!("wrote .ground/terra/{stack_name}/main.tf.json");
}

// ---------------------------------------------------------------------------
// Ground entity lookup
// ---------------------------------------------------------------------------

fn build_lookup(spec: &ground_core::Spec) -> Vec<(String, String)> {
    use ground_core::ScalarValue;
    let mut lookup = Vec::new();
    for deploy in &spec.deploys {
        let alias_u = deploy.alias.replace('-', "_");
        let pfx_u = deploy.fields.iter()
            .find(|f| f.link_name == "prefix")
            .and_then(|f| if let ground_core::ResolvedValue::Scalar(ScalarValue::Ref(s) | ScalarValue::Str(s)) = &f.value { Some(s.replace('-', "_")) } else { None })
            .unwrap_or_default();
        for inst in &spec.instances {
            let inst_u = inst.name.replace('-', "_");
            let key   = format!("{pfx_u}{alias_u}_{inst_u}");
            let label = format!("{}:{}", inst.type_name, inst.name);
            lookup.push((key, label));
        }
    }
    lookup
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
    spec:       &ground_core::Spec,
    stack_name: &str,
    provider:   &str,
    verbose:    bool,
) {
    use std::collections::BTreeMap;
    use ground_be_terra::terra_ops;

    let lookup = build_lookup(spec);
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

    let (spec, json, stack_name) = parse_and_gen();
    let provider = "aws".to_string();
    let lookup = build_lookup(&spec);

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

    let (spec, json, stack_name) = parse_and_gen();
    let provider = "aws".to_string();
    let lookup = build_lookup(&spec);

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
                display_plan_summary(&summary, &spec, &stack_name, &provider, verbose);
            }
            other => {
                for d in enricher.enrich(&other) { render(&d); }
            }
        }
    }
}
