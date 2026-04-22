use ground_be_terra::terra_ops::{Action, OpsEvent};
use ground_run::RunEvent;

const GREEN:  &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED:    &str = "\x1b[31m";
const BOLD:   &str = "\x1b[1m";
const DIM:    &str = "\x1b[2m";
const RESET:  &str = "\x1b[0m";

pub struct DisplayEvent {
    pub message: String,
    pub detail:  Option<String>,
}

pub enum Op { Init, Plan, Apply }

pub struct TerraEnricher {
    pub plan:        String,
    pub op:          Op,
    pub provider:    String,
    pub region:      String,
    pub verbose:     bool,
    lookup:          Vec<(String, String)>,  // (underscored_tf_name, "type:name")
    refresh_started: bool,
    refresh_done:    bool,
    drift_buffer:    Vec<(String, Action)>,  // (tf_address, action)
}

impl TerraEnricher {
    pub fn new(plan: String, op: Op, provider: String, region: String, lookup: Vec<(String, String)>, verbose: bool) -> Self {
        Self { plan, op, provider, region, verbose, lookup, refresh_started: false, refresh_done: false, drift_buffer: vec![] }
    }

    pub fn enrich(&mut self, event: &RunEvent<OpsEvent>) -> Vec<DisplayEvent> {
        match event {
            RunEvent::Spawned => {
                let (op_label, cmd) = match self.op {
                    Op::Init  => ("init",  "terraform init"),
                    Op::Plan  => ("plan",  "terraform plan"),
                    Op::Apply => ("apply", "terraform apply"),
                };
                let mut v = vec![];
                if matches!(self.op, Op::Plan) {
                    v.push(msg(format!("{DIM}running in plan mode, no changes will be made{RESET}")));
                }
                v.push(msg(format!("{op_label} for plan {GREEN}{BOLD}{}{RESET} on {} / {}", self.plan, self.provider, self.region)));
                v.push(msg(format!("{DIM}running {cmd}{RESET}")));
                v
            }

            RunEvent::Raw(line) => if self.verbose {
                vec![msg(format!("{DIM}{line}{RESET}"))]
            } else {
                vec![]
            },

            RunEvent::Line(ev) => self.enrich_ops(ev),

            RunEvent::Stderr(s) => vec![msg(format!("{DIM}{s}{RESET}"))],

            RunEvent::Exited(s) if !s.success =>
                vec![msg(format!("{RED}failed{RESET}"))],

            RunEvent::Exited(_) => vec![],
        }
    }

    fn enrich_ops(&mut self, ev: &OpsEvent) -> Vec<DisplayEvent> {
        // inject "state refresh complete" banner when leaving refresh phase
        let mut prefix: Vec<DisplayEvent> = if self.refresh_started
            && !self.refresh_done
            && !matches!(ev, OpsEvent::Refreshing { .. } | OpsEvent::RefreshDone { .. })
        {
            self.refresh_done = true;
            vec![msg(format!("{DIM}state refresh complete{RESET}"))]
        } else {
            vec![]
        };

        let mut out = match ev {
            OpsEvent::TerraformReady { version } =>
                vec![msg(format!("{DIM}terraform {version} ready{RESET}"))],

            OpsEvent::Refreshing { address } => {
                let mut v = vec![];
                if !self.refresh_started {
                    self.refresh_started = true;
                    v.push(msg(format!("{DIM}starting state refresh{RESET}")));
                }
                v.push(msg(format!("{DIM}  ↻ refreshing {address}{RESET}")));
                v
            }

            OpsEvent::RefreshDone { .. } => vec![],

            OpsEvent::Computing => {
                let mut v = self.flush_drift();
                v.push(msg(format!("{DIM}computing plan{RESET}")));
                v
            }

            OpsEvent::ReadingPlan => {
                let mut v = self.flush_drift();  // fallback: flush any drift not caught at Computing
                v.push(msg(format!("{DIM}running terraform show -json .tfplan{RESET}")));
                v
            }

            OpsEvent::ProviderReady { name, version } =>
                vec![msg(format!("{DIM}installing provider {name} {version}{RESET}"))],

            OpsEvent::InitDone =>
                vec![msg(format!("{GREEN}terraform init complete{RESET}"))],

            OpsEvent::ResourceQueued { .. } => vec![],

            OpsEvent::ResourceApplying { address, action } => {
                let (glyph, color) = action_style(action);
                vec![msg(format!("  {color}{glyph}{RESET} {address}…"))]
            }

            OpsEvent::ResourceDone { address, action, elapsed_secs } => {
                let (glyph, color) = action_style(action);
                vec![msg(format!("  {color}{glyph}{RESET} {address}  {DIM}({elapsed_secs}s){RESET}"))]
            }

            OpsEvent::ResourceFailed { address, reason } => vec![DisplayEvent {
                message: format!("  {RED}✗{RESET} {address}"),
                detail:  Some(format!("{RED}{reason}{RESET}")),
            }],

            OpsEvent::DriftDetected { address, action } => {
                self.drift_buffer.push((address.clone(), *action));
                vec![]
            }

            OpsEvent::Warning { message, detail, address } => {
                let mut lines = vec![];
                if let Some(a) = address { lines.push(format!("with {a}")); }
                if let Some(d) = detail  { lines.push(d.clone()); }
                vec![DisplayEvent {
                    message: format!("{YELLOW}warning:{RESET} {message}"),
                    detail:  if lines.is_empty() { None } else { Some(lines.join("\n  ")) },
                }]
            }

            OpsEvent::ApplyDone => {
                let mut v = self.flush_drift();  // fallback: flush any drift not caught at Computing
                v.push(msg(format!("{GREEN}apply complete{RESET}")));
                v
            }

            OpsEvent::PlanReady { .. } => vec![],
        };

        prefix.append(&mut out);
        prefix
    }

    fn ground_entity(&self, tf_address: &str) -> String {
        // tf_address is "resource_type.resource_name" — extract the name part
        let resource_name = tf_address.splitn(2, '.').nth(1).unwrap_or(tf_address);
        for (underscored, label) in &self.lookup {
            if resource_name == underscored.as_str()
                || resource_name.starts_with(&format!("{underscored}_"))
            {
                return label.clone();
            }
        }
        format!("plan:{}", self.plan)
    }

    fn flush_drift(&mut self) -> Vec<DisplayEvent> {
        if self.drift_buffer.is_empty() { return vec![]; }

        // Group by ground entity, preserving first-seen order
        let drained: Vec<(String, Action)> = self.drift_buffer.drain(..).collect();
        let mut groups: Vec<(String, Vec<(String, Action)>)> = Vec::new();
        for (address, action) in drained {
            let entity = self.ground_entity(&address);
            if let Some(g) = groups.iter_mut().find(|(e, _)| e == &entity) {
                g.1.push((address, action));
            } else {
                groups.push((entity, vec![(address, action)]));
            }
        }

        let mut v = vec![
            DisplayEvent {
                message: format!("{YELLOW}drift detected — changes made outside terraform{RESET}"),
                detail:  None,
            },
            msg(String::new()),
        ];
        for (entity, changes) in &groups {
            v.push(msg(format!("  {YELLOW}~{RESET} {BOLD}{entity}{RESET}")));
            for (address, action) in changes {
                let (glyph, color) = action_style(action);
                v.push(msg(format!("      {color}{glyph}{RESET} {DIM}{address}{RESET}")));
            }
        }
        v.push(msg(String::new()));
        v
    }
}

fn action_style(action: &Action) -> (&'static str, &'static str) {
    match action {
        Action::Create  => ("+", GREEN),
        Action::Update  => ("~", YELLOW),
        Action::Replace => ("±", YELLOW),
        Action::Delete  => ("-", RED),
    }
}

fn msg(message: String) -> DisplayEvent {
    DisplayEvent { message, detail: None }
}
