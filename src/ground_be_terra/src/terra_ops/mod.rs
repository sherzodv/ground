mod parser;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde_json::Value;

use ground_run::{RunEvent, RunError};
pub use ground_run::ExitStatus;

use parser::{Mode, TfEvent, TfParser};

// -- public types ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action { Create, Update, Replace, Delete }

pub struct ResourceChange {
    pub action:        Action,
    pub resource_type: String,
    pub resource_name: String,
}

pub struct PlanSummary {
    pub changes: Vec<ResourceChange>,
}

impl PlanSummary {
    pub fn creates(&self)  -> usize { self.changes.iter().filter(|c| matches!(c.action, Action::Create)).count() }
    pub fn updates(&self)  -> usize { self.changes.iter().filter(|c| matches!(c.action, Action::Update | Action::Replace)).count() }
    pub fn destroys(&self) -> usize { self.changes.iter().filter(|c| matches!(c.action, Action::Delete)).count() }
}

pub enum OpsEvent {
    ProviderReady    { name: String, version: String },
    InitDone,

    TerraformReady   { version: String },
    Refreshing       { address: String },
    RefreshDone      { address: String },
    Computing,
    ReadingPlan,
    ResourceQueued   { address: String, action: Action },
    ResourceApplying { address: String, action: Action },
    ResourceDone     { address: String, action: Action, elapsed_secs: u32 },
    ResourceFailed   { address: String, reason: String },
    PlanReady        { summary: PlanSummary },
    ApplyDone,

    DriftDetected    { address: String, action: Action },
    Warning          { message: String, detail: Option<String>, address: Option<String> },
}

#[derive(Debug)]
pub enum OpsError {
    Run(RunError),
    Other(String),
}

impl std::fmt::Display for OpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpsError::Run(e)   => write!(f, "{e}"),
            OpsError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl From<RunError> for OpsError {
    fn from(e: RunError) -> Self { OpsError::Run(e) }
}

// -- public API --------------------------------------------------------------

pub fn init(dir: &Path) -> Result<Receiver<RunEvent<OpsEvent>>, OpsError> {
    let chdir = format!("-chdir={}", dir.to_str().unwrap_or("."));
    let raw = ground_run::spawn(
        Command::new("terraform")
            .args([&chdir, "init", "-input=false"]),
        TfParser { mode: Mode::Init },
    )?;

    Ok(map_events(raw, |ev| match ev {
        TfEvent::InitProviderDownload { name, version } =>
            Some(OpsEvent::ProviderReady { name, version }),
        TfEvent::InitComplete =>
            Some(OpsEvent::InitDone),
        TfEvent::Diagnostic { severity, summary, detail, address } if severity == "error" =>
            Some(OpsEvent::Warning { message: summary, detail, address }),
        _ => None,
    }))
}

pub fn plan(dir: &Path) -> Result<Receiver<RunEvent<OpsEvent>>, OpsError> {
    let chdir = format!("-chdir={}", dir.to_str().unwrap_or("."));
    let raw = ground_run::spawn(
        Command::new("terraform")
            .args([&chdir, "plan", "-json", "-input=false", "-out=.tfplan"]),
        TfParser { mode: Mode::PlanApply },
    )?;

    let dir = dir.to_path_buf();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        for event in raw {
            match event {
                RunEvent::Spawned     => { let _ = tx.send(RunEvent::Spawned); }
                RunEvent::Stderr(s)   => { let _ = tx.send(RunEvent::Stderr(s)); }
                RunEvent::Line(ev)    => {
                    if let Some(ops) = tf_to_ops(ev) {
                        let _ = tx.send(RunEvent::Line(ops));
                    }
                }
                RunEvent::Exited(status) => {
                    if status.success {
                        let _ = tx.send(RunEvent::Line(OpsEvent::ReadingPlan));
                        match show_json(&dir) {
                            Ok(summary) => { let _ = tx.send(RunEvent::Line(OpsEvent::PlanReady { summary })); }
                            Err(e)      => { let _ = tx.send(RunEvent::Line(OpsEvent::Warning { message: e, detail: None, address: None })); }
                        }
                        let _ = std::fs::remove_file(dir.join(".tfplan"));
                    }
                    let _ = tx.send(RunEvent::Exited(status));
                }
            }
        }
    });

    Ok(rx)
}

pub fn apply(dir: &Path) -> Result<Receiver<RunEvent<OpsEvent>>, OpsError> {
    let chdir = format!("-chdir={}", dir.to_str().unwrap_or("."));
    let raw = ground_run::spawn(
        Command::new("terraform")
            .args([&chdir, "apply", "-json", "-input=false", "-auto-approve"]),
        TfParser { mode: Mode::PlanApply },
    )?;

    Ok(map_events(raw, |ev| match ev {
        TfEvent::ChangeSummary => Some(OpsEvent::ApplyDone),
        ev => tf_to_ops(ev),
    }))
}

// -- helpers -----------------------------------------------------------------

fn tf_to_ops(ev: TfEvent) -> Option<OpsEvent> {
    match ev {
        TfEvent::TerraformVersion { version } =>
            Some(OpsEvent::TerraformReady { version }),
        TfEvent::RefreshStart { address } =>
            Some(OpsEvent::Refreshing { address }),
        TfEvent::RefreshComplete { address } =>
            Some(OpsEvent::RefreshDone { address }),
        TfEvent::PlannedChangesStart =>
            Some(OpsEvent::Computing),
        TfEvent::ResourcePlanned { address, action } =>
            Some(OpsEvent::ResourceQueued { address, action }),
        TfEvent::ResourceApplying { address, action } =>
            Some(OpsEvent::ResourceApplying { address, action }),
        TfEvent::ResourceDone { address, action, elapsed_secs } =>
            Some(OpsEvent::ResourceDone { address, action, elapsed_secs }),
        TfEvent::ResourceErrored { address, message } =>
            Some(OpsEvent::ResourceFailed { address, reason: message }),
        TfEvent::ResourceDrift { address, action } =>
            Some(OpsEvent::DriftDetected { address, action }),
        TfEvent::Diagnostic { summary, detail, address, .. } =>
            Some(OpsEvent::Warning { message: summary, detail, address }),
        _ => None,
    }
}

/// Wraps a raw `Receiver<RunEvent<TfEvent>>` and maps each `Line` through `f`,
/// forwarding lifecycle events (Spawned, Stderr, Exited) unchanged.
fn map_events<F>(
    raw: Receiver<RunEvent<TfEvent>>,
    f:   F,
) -> Receiver<RunEvent<OpsEvent>>
where
    F: Fn(TfEvent) -> Option<OpsEvent> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for event in raw {
            let out = match event {
                RunEvent::Spawned     => Some(RunEvent::Spawned),
                RunEvent::Stderr(s)   => Some(RunEvent::Stderr(s)),
                RunEvent::Line(ev)    => f(ev).map(RunEvent::Line),
                RunEvent::Exited(s)   => Some(RunEvent::Exited(s)),
            };
            if let Some(e) = out {
                if tx.send(e).is_err() { break; }
            }
        }
    });
    rx
}

/// Runs `terraform show -json .tfplan` synchronously and parses a PlanSummary.
fn show_json(dir: &PathBuf) -> Result<PlanSummary, String> {
    let chdir = format!("-chdir={}", dir.to_str().unwrap_or("."));
    let out = Command::new("terraform")
        .args([&chdir, "show", "-json", ".tfplan"])
        .output()
        .map_err(|e| format!("terraform show failed: {e}"))?;

    if !out.status.success() {
        return Err("terraform show failed".into());
    }

    let json: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("failed to parse plan output: {e}"))?;

    let mut changes = Vec::new();
    for rc in json["resource_changes"].as_array().unwrap_or(&vec![]) {
        let actions: Vec<&str> = rc["change"]["actions"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let action = match actions.as_slice() {
            ["no-op"] | ["read"]       => continue,
            ["create"]                 => Action::Create,
            ["update"]                 => Action::Update,
            ["delete"]                 => Action::Delete,
            ["delete", "create"]
            | ["create", "delete"]     => Action::Replace,
            _                          => continue,
        };

        changes.push(ResourceChange {
            action,
            resource_type: rc["type"].as_str().unwrap_or("").to_string(),
            resource_name: rc["name"].as_str().unwrap_or("").to_string(),
        });
    }

    Ok(PlanSummary { changes })
}
