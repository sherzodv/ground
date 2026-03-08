use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

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

pub fn init(dir: &Path) -> Result<(), String> {
    let ok = Command::new("terraform")
        .args(["init", "-input=false"])
        .current_dir(dir)
        .status()
        .map_err(|e| format!("terraform not found: {e}"))?
        .success();
    if ok { Ok(()) } else { Err("terraform init failed".into()) }
}

pub fn plan(dir: &Path) -> Result<PlanSummary, String> {
    let ok = Command::new("terraform")
        .args(["plan", "-input=false", "-out=.tfplan", "-compact-warnings"])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("terraform not found: {e}"))?
        .status.success();

    if !ok {
        // re-run without silencing to surface the error
        Command::new("terraform")
            .args(["plan", "-input=false"])
            .current_dir(dir)
            .status().ok();
        return Err("terraform plan failed".into());
    }

    let out = Command::new("terraform")
        .args(["show", "-json", ".tfplan"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("terraform show failed: {e}"))?;

    let _ = std::fs::remove_file(dir.join(".tfplan"));

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
            ["no-op"] | ["read"]          => continue,
            ["create"]                    => Action::Create,
            ["update"]                    => Action::Update,
            ["delete"]                    => Action::Delete,
            ["delete", "create"]
            | ["create", "delete"]        => Action::Replace,
            _                             => continue,
        };

        changes.push(ResourceChange {
            action,
            resource_type: rc["type"].as_str().unwrap_or("").to_string(),
            resource_name: rc["name"].as_str().unwrap_or("").to_string(),
        });
    }

    Ok(PlanSummary { changes })
}
