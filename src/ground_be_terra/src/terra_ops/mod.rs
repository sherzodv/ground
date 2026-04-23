mod parser;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde_json::Value;

pub use ground_run::ExitStatus;
use ground_run::{RunError, RunEvent};

use parser::{Mode, TfEvent, TfParser};

// -- public types ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    Replace,
    Delete,
}

/// One attribute value as it appears in a plan diff.
#[derive(Debug, Clone, PartialEq)]
pub enum AttrVal {
    Scalar(String),                // concrete value (pre-formatted)
    Unknown,                       // (known after apply)
    Sensitive,                     // (sensitive value)
    Null,                          // null / absent
    Block(Vec<(String, AttrVal)>), // nested object/block (sorted keys)
    List(Vec<AttrVal>),            // list / set / tuple
}

/// How a single attribute changes across a plan action.
pub struct AttrChange {
    pub key: String,
    pub before: Option<AttrVal>,
    pub after: Option<AttrVal>,
}

pub struct ResourceChange {
    pub action: Action,
    pub resource_type: String,
    pub resource_name: String,
    pub attrs: Vec<AttrChange>,
}

pub struct PlanSummary {
    pub changes: Vec<ResourceChange>,
}

impl PlanSummary {
    pub fn creates(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| matches!(c.action, Action::Create))
            .count()
    }
    pub fn updates(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| matches!(c.action, Action::Update | Action::Replace))
            .count()
    }
    pub fn destroys(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| matches!(c.action, Action::Delete))
            .count()
    }
}

pub enum OpsEvent {
    ProviderReady {
        name: String,
        version: String,
    },
    InitDone,

    TerraformReady {
        version: String,
    },
    Refreshing {
        address: String,
    },
    RefreshDone {
        address: String,
    },
    Computing,
    ReadingPlan,
    ResourceQueued {
        address: String,
        action: Action,
    },
    ResourceApplying {
        address: String,
        action: Action,
    },
    ResourceDone {
        address: String,
        action: Action,
        elapsed_secs: u32,
    },
    ResourceFailed {
        address: String,
        reason: String,
    },
    PlanReady {
        summary: PlanSummary,
    },
    ApplyDone,

    DriftDetected {
        address: String,
        action: Action,
    },
    Warning {
        message: String,
        detail: Option<String>,
        address: Option<String>,
    },
}

#[derive(Debug)]
pub enum OpsError {
    Run(RunError),
    Other(String),
}

impl std::fmt::Display for OpsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpsError::Run(e) => write!(f, "{e}"),
            OpsError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl From<RunError> for OpsError {
    fn from(e: RunError) -> Self {
        OpsError::Run(e)
    }
}

// -- public API --------------------------------------------------------------

enum InitMode {
    Init,
    MigrateState,
}

pub fn init_if_needed(dir: &Path) -> Result<Option<Receiver<RunEvent<OpsEvent>>>, OpsError> {
    let Some(mode) = init_mode(dir) else {
        return Ok(None);
    };

    match mode {
        InitMode::Init => run_init(dir, false).map(Some),
        InitMode::MigrateState => Err(OpsError::Other("state migration required".into())),
    }
}

pub fn migrate_state(dir: &Path) -> Result<Option<Receiver<RunEvent<OpsEvent>>>, OpsError> {
    let Some(mode) = init_mode(dir) else {
        return Ok(None);
    };

    match mode {
        InitMode::MigrateState => run_init(dir, true).map(Some),
        InitMode::Init => Err(OpsError::Other(
            "state migration is not available for this plan yet".into(),
        )),
    }
}

fn run_init(dir: &Path, migrate_state: bool) -> Result<Receiver<RunEvent<OpsEvent>>, OpsError> {
    let chdir = format!("-chdir={}", dir.to_str().unwrap_or("."));
    let mut cmd = Command::new("terraform");
    cmd.arg(&chdir).arg("init");
    if migrate_state {
        cmd.arg("-migrate-state").arg("-force-copy");
    }
    cmd.arg("-input=false");
    let raw = ground_run::spawn(&mut cmd, TfParser { mode: Mode::Init })?;

    let dir = dir.to_path_buf();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for event in raw {
            let out = match event {
                RunEvent::Spawned => Some(RunEvent::Spawned),
                RunEvent::Raw(s) => Some(RunEvent::Raw(s)),
                RunEvent::Stderr(s) => Some(RunEvent::Stderr(s)),
                RunEvent::Line(ev) => match ev {
                    TfEvent::InitProviderDownload { name, version } => {
                        Some(RunEvent::Line(OpsEvent::ProviderReady { name, version }))
                    }
                    TfEvent::InitComplete => Some(RunEvent::Line(OpsEvent::InitDone)),
                    TfEvent::Diagnostic {
                        severity,
                        summary,
                        detail,
                        address,
                    } if severity == "error" => Some(RunEvent::Line(OpsEvent::Warning {
                        message: summary,
                        detail,
                        address,
                    })),
                    _ => None,
                },
                RunEvent::Exited(status) => {
                    if status.success {
                        let _ = write_init_stamp(&dir);
                    }
                    Some(RunEvent::Exited(status))
                }
            };
            if let Some(e) = out {
                if tx.send(e).is_err() {
                    break;
                }
            }
        }
    });

    Ok(rx)
}

pub fn plan(dir: &Path) -> Result<Receiver<RunEvent<OpsEvent>>, OpsError> {
    let chdir = format!("-chdir={}", dir.to_str().unwrap_or("."));
    let raw = ground_run::spawn(
        Command::new("terraform").args([&chdir, "plan", "-json", "-input=false", "-out=.tfplan"]),
        TfParser {
            mode: Mode::PlanApply,
        },
    )?;

    let dir = dir.to_path_buf();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        for event in raw {
            match event {
                RunEvent::Spawned => {
                    let _ = tx.send(RunEvent::Spawned);
                }
                RunEvent::Raw(s) => {
                    let _ = tx.send(RunEvent::Raw(s));
                }
                RunEvent::Stderr(s) => {
                    let _ = tx.send(RunEvent::Stderr(s));
                }
                RunEvent::Line(ev) => {
                    if let Some(ops) = tf_to_ops(ev) {
                        let _ = tx.send(RunEvent::Line(ops));
                    }
                }
                RunEvent::Exited(status) => {
                    if status.success {
                        let _ = tx.send(RunEvent::Line(OpsEvent::ReadingPlan));
                        match show_json(&dir) {
                            Ok(summary) => {
                                let _ = tx.send(RunEvent::Line(OpsEvent::PlanReady { summary }));
                            }
                            Err(e) => {
                                let _ = tx.send(RunEvent::Line(OpsEvent::Warning {
                                    message: e,
                                    detail: None,
                                    address: None,
                                }));
                            }
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
        Command::new("terraform").args([&chdir, "apply", "-json", "-input=false", "-auto-approve"]),
        TfParser {
            mode: Mode::PlanApply,
        },
    )?;

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for event in raw {
            match event {
                RunEvent::Spawned => {
                    let _ = tx.send(RunEvent::Spawned);
                }
                RunEvent::Raw(s) => {
                    let _ = tx.send(RunEvent::Raw(s));
                }
                RunEvent::Stderr(s) => {
                    let _ = tx.send(RunEvent::Stderr(s));
                }
                RunEvent::Line(ev) => {
                    if let Some(ops) = tf_to_ops(ev) {
                        let _ = tx.send(RunEvent::Line(ops));
                    }
                }
                RunEvent::Exited(status) => {
                    if status.success {
                        let _ = tx.send(RunEvent::Line(OpsEvent::ApplyDone));
                    }
                    let _ = tx.send(RunEvent::Exited(status));
                }
            }
        }
    });

    Ok(rx)
}

// -- helpers -----------------------------------------------------------------

fn tf_to_ops(ev: TfEvent) -> Option<OpsEvent> {
    match ev {
        TfEvent::TerraformVersion { version } => Some(OpsEvent::TerraformReady { version }),
        TfEvent::RefreshStart { address } => Some(OpsEvent::Refreshing { address }),
        TfEvent::RefreshComplete { address } => Some(OpsEvent::RefreshDone { address }),
        TfEvent::PlannedChangesStart => Some(OpsEvent::Computing),
        TfEvent::ResourcePlanned { address, action } => {
            Some(OpsEvent::ResourceQueued { address, action })
        }
        TfEvent::ResourceApplying { address, action } => {
            Some(OpsEvent::ResourceApplying { address, action })
        }
        TfEvent::ResourceDone {
            address,
            action,
            elapsed_secs,
        } => Some(OpsEvent::ResourceDone {
            address,
            action,
            elapsed_secs,
        }),
        TfEvent::ResourceErrored { address, message } => Some(OpsEvent::ResourceFailed {
            address,
            reason: message,
        }),
        TfEvent::ResourceDrift { address, action } => {
            Some(OpsEvent::DriftDetected { address, action })
        }
        TfEvent::Diagnostic {
            summary,
            detail,
            address,
            ..
        } => Some(OpsEvent::Warning {
            message: summary,
            detail,
            address,
        }),
        _ => None,
    }
}

const INIT_STAMP_FILE: &str = ".ground-init.json";

fn init_mode(dir: &Path) -> Option<InitMode> {
    if !dir.join(".terraform").is_dir() {
        return Some(InitMode::Init);
    }

    let Some(current) = load_terraform_block(dir) else {
        return Some(InitMode::Init);
    };

    let stamp_path = dir.join(INIT_STAMP_FILE);
    let Ok(saved) = fs::read_to_string(stamp_path) else {
        return Some(InitMode::Init);
    };

    let Ok(saved_json) = serde_json::from_str::<Value>(&saved) else {
        return Some(InitMode::Init);
    };

    if saved_json == current {
        return None;
    }

    if should_migrate_state(dir, &saved_json, &current) {
        return Some(InitMode::MigrateState);
    }

    Some(InitMode::Init)
}

fn write_init_stamp(dir: &Path) -> Result<(), String> {
    let Some(current) = load_terraform_block(dir) else {
        return Err("missing terraform config for init stamp".into());
    };
    let current =
        serde_json::to_string(&current).map_err(|e| format!("failed to encode init stamp: {e}"))?;
    fs::write(dir.join(INIT_STAMP_FILE), current)
        .map_err(|e| format!("failed to write init stamp: {e}"))
}

fn load_terraform_block(dir: &Path) -> Option<Value> {
    let main_tf = fs::read_to_string(dir.join("main.tf.json")).ok()?;
    let json: Value = serde_json::from_str(&main_tf).ok()?;
    Some(json.get("terraform").cloned().unwrap_or(Value::Null))
}

fn has_backend(terraform: &Value) -> bool {
    terraform.get("backend").is_some_and(|v| v.is_object())
}

fn should_migrate_state(dir: &Path, previous: &Value, current: &Value) -> bool {
    has_backend(current) && !has_backend(previous) && dir.join("terraform.tfstate").is_file()
}

/// Converts a JSON value into an `AttrVal`, resolving unknowns and sensitives.
fn parse_attr_val(val: &Value, unknown: &Value, sensitive: &Value) -> AttrVal {
    if unknown.as_bool() == Some(true) {
        return AttrVal::Unknown;
    }
    if sensitive.as_bool() == Some(true) {
        return AttrVal::Sensitive;
    }
    match val {
        Value::Null => AttrVal::Null,
        Value::String(s) => AttrVal::Scalar(format!("\"{s}\"")),
        Value::Number(n) => AttrVal::Scalar(n.to_string()),
        Value::Bool(b) => AttrVal::Scalar(b.to_string()),
        Value::Object(m) => {
            let mut pairs: Vec<(String, AttrVal)> = m
                .iter()
                .map(|(k, v)| {
                    let uk = unknown.get(k).unwrap_or(&Value::Null);
                    let sk = sensitive.get(k).unwrap_or(&Value::Null);
                    (k.clone(), parse_attr_val(v, uk, sk))
                })
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            AttrVal::Block(pairs)
        }
        Value::Array(arr) => {
            let items = arr
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let uk = if let Value::Array(ua) = unknown {
                        ua.get(i).unwrap_or(&Value::Null)
                    } else {
                        &Value::Null
                    };
                    let sk = if let Value::Array(sa) = sensitive {
                        sa.get(i).unwrap_or(&Value::Null)
                    } else {
                        &Value::Null
                    };
                    parse_attr_val(v, uk, sk)
                })
                .collect();
            AttrVal::List(items)
        }
    }
}

/// Builds a sorted list of `AttrChange`s from a single `resource_changes[].change` object.
fn build_attrs(change: &Value) -> Vec<AttrChange> {
    let before = &change["before"];
    let after = &change["after"];
    let unknown = &change["after_unknown"];
    let before_s = &change["before_sensitive"];
    let after_s = &change["after_sensitive"];

    let mut keys = std::collections::BTreeSet::new();
    if let Some(m) = before.as_object() {
        keys.extend(m.keys().cloned());
    }
    if let Some(m) = after.as_object() {
        keys.extend(m.keys().cloned());
    }

    keys.into_iter()
        .map(|key| {
            let bsk = before_s.get(&key).unwrap_or(&Value::Null);
            let ask = after_s.get(&key).unwrap_or(&Value::Null);
            let auk = unknown.get(&key).unwrap_or(&Value::Null);
            AttrChange {
                before: before
                    .get(&key)
                    .map(|v| parse_attr_val(v, &Value::Null, bsk)),
                after: after.get(&key).map(|v| parse_attr_val(v, auk, ask)),
                key,
            }
        })
        .collect()
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
        let actions: Vec<&str> = rc["change"]["actions"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let action = match actions.as_slice() {
            ["no-op"] | ["read"] => continue,
            ["create"] => Action::Create,
            ["update"] => Action::Update,
            ["delete"] => Action::Delete,
            ["delete", "create"] | ["create", "delete"] => Action::Replace,
            _ => continue,
        };

        changes.push(ResourceChange {
            action,
            resource_type: rc["type"].as_str().unwrap_or("").to_string(),
            resource_name: rc["name"].as_str().unwrap_or("").to_string(),
            attrs: build_attrs(&rc["change"]),
        });
    }

    Ok(PlanSummary { changes })
}

#[cfg(test)]
mod tests {
    use super::{has_backend, init_mode, write_init_stamp, InitMode};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ground-be-terra-{name}-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_main_tf(dir: &PathBuf, backend_key: Option<&str>) {
        let backend = backend_key
            .map(|backend_key| {
                format!(
                    r#",
    "backend": {{
      "s3": {{ "bucket": "b", "key": "{backend_key}", "region": "us-east-1" }}
    }}"#
                )
            })
            .unwrap_or_default();
        fs::write(
            dir.join("main.tf.json"),
            format!(
                r#"{{
  "terraform": {{
    "required_providers": {{
      "aws": {{ "source": "hashicorp/aws", "version": "~> 5.0" }}
    }}{backend}
  }}
}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn init_needed_when_uninitialized() {
        let dir = temp_dir("uninit");
        write_main_tf(&dir, Some("demo/a.tfstate"));
        assert!(matches!(init_mode(&dir), Some(InitMode::Init)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn skips_init_when_stamp_matches() {
        let dir = temp_dir("cached");
        write_main_tf(&dir, Some("demo/a.tfstate"));
        fs::create_dir_all(dir.join(".terraform")).unwrap();
        write_init_stamp(&dir).unwrap();
        assert!(init_mode(&dir).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reruns_init_when_backend_changes() {
        let dir = temp_dir("changed");
        write_main_tf(&dir, Some("demo/a.tfstate"));
        fs::create_dir_all(dir.join(".terraform")).unwrap();
        write_init_stamp(&dir).unwrap();
        write_main_tf(&dir, Some("demo/b.tfstate"));
        assert!(matches!(init_mode(&dir), Some(InitMode::Init)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn migrates_state_when_backend_is_new() {
        let dir = temp_dir("migrate");
        write_main_tf(&dir, None);
        fs::create_dir_all(dir.join(".terraform")).unwrap();
        fs::write(dir.join("terraform.tfstate"), "{}").unwrap();
        write_init_stamp(&dir).unwrap();
        write_main_tf(&dir, Some("demo/bootstrap.tfstate"));
        assert!(matches!(init_mode(&dir), Some(InitMode::MigrateState)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn backend_helper_detects_backend_block() {
        let with_backend = serde_json::json!({ "backend": { "s3": {} } });
        let without_backend = serde_json::json!({ "required_providers": {} });
        assert!(has_backend(&with_backend));
        assert!(!has_backend(&without_backend));
    }
}
