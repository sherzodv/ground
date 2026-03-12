mod ops_display;

use std::{env, fs, path::Path, process};

use ground_run::RunEvent;
use ground_be_terra::terra_ops::OpsEvent;
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
        [cmd] if cmd == "init"                                          => cmd_init(false),
        [cmd, flag] if cmd == "init" && flag == "--git-ignore"          => cmd_init(true),
        [cmd, sub] if cmd == "gen" && sub == "terra"                    => cmd_gen_terra(),
        [cmd] if cmd == "plan"                                          => cmd_plan(),
        _ => {
            eprintln!("usage:");
            eprintln!("  ground init [--git-ignore]");
            eprintln!("  ground gen terra");
            eprintln!("  ground plan");
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

fn load_stacks() -> Vec<(String, ground_core::low::Plan)> {
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

    let sources: Vec<(&str, &str)> = sources_data.iter().map(|(p, c)| (p.as_str(), c.as_str())).collect();

    let spec = match ground_parse::parse(&sources) {
        Ok(s)   => s,
        Err(es) => { for e in es { eprintln!("{e}"); } process::exit(1); }
    };

    let stacks = match ground_core::compile::compile(&spec) {
        Ok(s)   => s,
        Err(es) => { for e in es { eprintln!("error: {e}"); } process::exit(1); }
    };

    if stacks.is_empty() {
        eprintln!("no deploy declarations found — nothing to generate");
        process::exit(1);
    }

    stacks
}

fn write_stack(stack_name: &str, plan: &ground_core::low::Plan) {
    let dir      = format!(".ground/terra/{stack_name}");
    let out_path = format!("{dir}/main.tf.json");

    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("error: {dir}: {e}"); process::exit(1);
    }

    let json = ground_be_terra::terra_gen::aws::generate(plan);
    if let Err(e) = fs::write(&out_path, &json) {
        eprintln!("error: {out_path}: {e}"); process::exit(1);
    }
}

fn cmd_gen_terra() {
    for (stack_name, plan) in load_stacks() {
        write_stack(&stack_name, &plan);
        println!("wrote .ground/terra/{stack_name}/main.tf.json");
    }
}

// -- Ground entity classification --------------------------------------------

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum GroundKind {
    Service(String),
    Stack(String),
    Region(String),
}

fn classify(rtype: &str, rname: &str, plan: &ground_core::low::Plan) -> GroundKind {
    if rtype == "aws_ecs_cluster" {
        return GroundKind::Stack(plan.stack_name.clone());
    }
    for wl in &plan.workloads {
        let s = wl.name.replace(['-', '/'], "_");
        if rname == s
            || rname.starts_with(&format!("{s}_"))
            || rname == format!("_ground_{s}")
        {
            return GroundKind::Service(wl.name.clone());
        }
    }
    GroundKind::Region(plan.region_name.clone())
}

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

// -- ground plan -------------------------------------------------------------

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

fn display_plan_summary(
    summary:    &ground_be_terra::terra_ops::PlanSummary,
    stack_name: &str,
    plan:       &ground_core::low::Plan,
) {
    use std::collections::BTreeMap;
    use ground_be_terra::terra_ops;

    let mut groups: BTreeMap<GroundKind, Vec<&terra_ops::ResourceChange>> = BTreeMap::new();
    for c in &summary.changes {
        groups
            .entry(classify(&c.resource_type, &c.resource_name, plan))
            .or_default()
            .push(c);
    }

    println!();
    println!("{BOLD}stack {stack_name}{RESET} {DIM}→ {}{RESET}", plan.provider_name);
    println!("{DIM}region {}{RESET}", plan.region_name);
    println!("{DIM}env    {}{RESET}", plan.env_name);
    println!("{DIM}group  {}{RESET}", plan.group_name);
    println!();

    if groups.is_empty() {
        println!("{DIM}no changes{RESET}");
    } else {
        for (kind, changes) in &groups {
            let (verb, color) = dominant_verb(changes);
            let label = match kind {
                GroundKind::Service(name) => format!("service {name}"),
                GroundKind::Stack(name)   => format!("stack {name}"),
                GroundKind::Region(name)  => format!("region {name}"),
            };
            println!("{color}{verb}{RESET} {BOLD}{label}{RESET}");
            let w = changes.iter().map(|c| c.resource_type.len()).max().unwrap_or(0);
            for c in changes {
                let (glyph, gcolor) = action_glyph(&c.action);
                println!("  {gcolor}{glyph}{RESET} {DIM}{:<w$}  {}{RESET}", c.resource_type, c.resource_name);
            }
            println!();
        }
    }

    let (c, u, d) = (summary.creates(), summary.updates(), summary.destroys());
    println!("{GREEN}create {c}{RESET}  {YELLOW}modify {u}{RESET}  {RED}delete {d}{RESET}");
    println!();
}

fn cmd_plan() {
    use ground_be_terra::terra_ops;

    for (stack_name, plan) in load_stacks() {
        write_stack(&stack_name, &plan);

        let dir = Path::new(".ground/terra").join(&stack_name);

        let rx = terra_ops::init(&dir)
            .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); });
        let mut enricher = TerraEnricher::new(stack_name.clone(), Op::Init, plan.provider_name.clone(), plan.region_name.clone());
        if !run_events(rx, &mut enricher) {
            eprintln!("error: terraform init failed");
            process::exit(1);
        }

        let rx = terra_ops::plan(&dir)
            .unwrap_or_else(|e| { eprintln!("error: {e}"); process::exit(1); });
        let mut enricher = TerraEnricher::new(stack_name.clone(), Op::Plan, plan.provider_name.clone(), plan.region_name.clone());

        for event in rx {
            match event {
                RunEvent::Exited(s) if !s.success => {
                    eprintln!("error: terraform plan failed");
                    process::exit(1);
                }
                RunEvent::Line(OpsEvent::PlanReady { summary }) => {
                    display_plan_summary(&summary, &stack_name, &plan);
                }
                other => {
                    for d in enricher.enrich(&other) { render(&d); }
                }
            }
        }
    }
}
