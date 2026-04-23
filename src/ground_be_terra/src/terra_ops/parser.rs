use crate::terra_ops::Action;
use ground_run::{OutputParser, Source};
use serde_json::Value;

pub enum TfEvent {
    // init (best-effort text)
    InitProviderDownload {
        name: String,
        version: String,
    },
    InitComplete,

    // plan / apply (-json NDJSON)
    TerraformVersion {
        version: String,
    },
    RefreshStart {
        address: String,
    },
    RefreshComplete {
        address: String,
    },
    PlannedChangesStart,
    ResourcePlanned {
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
    ResourceErrored {
        address: String,
        message: String,
    },
    ResourceDrift {
        address: String,
        action: Action,
    },
    ChangeSummary,

    Diagnostic {
        severity: String,
        summary: String,
        detail: Option<String>,
        address: Option<String>,
    },
}

pub enum Mode {
    Init,
    PlanApply,
}

pub struct TfParser {
    pub mode: Mode,
}

impl OutputParser for TfParser {
    type Event = TfEvent;

    fn parse(&mut self, line: &str, source: Source) -> Option<TfEvent> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }

        match source {
            Source::Stderr => parse_diagnostic(line),
            Source::Stdout => match self.mode {
                Mode::Init => parse_init(line),
                Mode::PlanApply => parse_json(line),
            },
        }
    }
}

// -- init text parser --------------------------------------------------------

fn parse_init(line: &str) -> Option<TfEvent> {
    // "- Installing hashicorp/aws v5.0.0..."
    if let Some(rest) = line.strip_prefix("- Installing ") {
        let mut parts = rest.splitn(2, ' ');
        let name = parts.next().unwrap_or("").to_string();
        let version = parts
            .next()
            .unwrap_or("")
            .trim_start_matches('v')
            .trim_end_matches("...")
            .to_string();
        return Some(TfEvent::InitProviderDownload { name, version });
    }
    if line.contains("successfully initialized") {
        return Some(TfEvent::InitComplete);
    }
    None
}

// -- NDJSON parser -----------------------------------------------------------

fn parse_json(line: &str) -> Option<TfEvent> {
    let v: Value = serde_json::from_str(line).ok()?;

    match v["type"].as_str()? {
        "version" => {
            let ver = v["terraform"].as_str().unwrap_or("").to_string();
            Some(TfEvent::TerraformVersion { version: ver })
        }
        "refresh_start" => {
            let addr = v["hook"]["resource"]["addr"].as_str()?.to_string();
            Some(TfEvent::RefreshStart { address: addr })
        }
        "refresh_complete" => {
            let addr = v["hook"]["resource"]["addr"].as_str()?.to_string();
            Some(TfEvent::RefreshComplete { address: addr })
        }
        "planned_changes" => Some(TfEvent::PlannedChangesStart),
        "planned_change" => {
            let addr = v["change"]["resource"]["addr"].as_str()?.to_string();
            let action = parse_action(v["change"]["action"].as_str().unwrap_or(""))?;
            Some(TfEvent::ResourcePlanned {
                address: addr,
                action,
            })
        }
        "apply_start" => {
            let addr = v["hook"]["resource"]["addr"].as_str()?.to_string();
            let action = parse_action(v["hook"]["action"].as_str().unwrap_or(""))?;
            Some(TfEvent::ResourceApplying {
                address: addr,
                action,
            })
        }
        "apply_complete" => {
            let addr = v["hook"]["resource"]["addr"].as_str()?.to_string();
            let action = parse_action(v["hook"]["action"].as_str().unwrap_or(""))?;
            let elapsed = v["hook"]["elapsed_seconds"].as_f64().unwrap_or(0.0) as u32;
            Some(TfEvent::ResourceDone {
                address: addr,
                action,
                elapsed_secs: elapsed,
            })
        }
        "apply_errored" => {
            let addr = v["hook"]["resource"]["addr"].as_str()?.to_string();
            let msg = v["@message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            Some(TfEvent::ResourceErrored {
                address: addr,
                message: msg,
            })
        }
        "change_summary" => Some(TfEvent::ChangeSummary),
        "resource_drift" => {
            let addr = v["change"]["resource"]["addr"].as_str()?.to_string();
            let action = parse_action(v["change"]["action"].as_str().unwrap_or(""))?;
            Some(TfEvent::ResourceDrift {
                address: addr,
                action,
            })
        }
        "diagnostic" => {
            let severity = v["diagnostic"]["severity"]
                .as_str()
                .unwrap_or("error")
                .to_string();
            let summary = v["diagnostic"]["summary"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let detail = v["diagnostic"]["detail"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let address = v["diagnostic"]["address"].as_str().map(str::to_string);
            Some(TfEvent::Diagnostic {
                severity,
                summary,
                detail,
                address,
            })
        }
        _ => None,
    }
}

fn parse_action(s: &str) -> Option<Action> {
    match s {
        "create" => Some(Action::Create),
        "update" => Some(Action::Update),
        "delete" => Some(Action::Delete),
        "replace" => Some(Action::Replace),
        _ => None,
    }
}

fn parse_diagnostic(line: &str) -> Option<TfEvent> {
    // terraform may emit JSON diagnostics on stderr too
    if let Ok(v) = serde_json::from_str::<Value>(line) {
        if let Some(level) = v["@level"].as_str() {
            if matches!(level, "error" | "warn") {
                return Some(TfEvent::Diagnostic {
                    severity: level.to_string(),
                    summary: v["@message"].as_str().unwrap_or("").to_string(),
                    detail: None,
                    address: None,
                });
            }
        }
    }
    // plain text error lines
    if line.starts_with("Error:") || line.starts_with("Warning:") {
        let (severity, rest) = if line.starts_with("Error:") {
            ("error", line.trim_start_matches("Error:").trim())
        } else {
            ("warn", line.trim_start_matches("Warning:").trim())
        };
        return Some(TfEvent::Diagnostic {
            severity: severity.to_string(),
            summary: rest.to_string(),
            detail: None,
            address: None,
        });
    }
    None
}
