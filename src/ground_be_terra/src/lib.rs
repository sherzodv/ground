pub mod terra_ops;

use ground_core::{Spec, Instance, DeployInstance, ResolvedValue, ScalarValue, ListEntry};
use ground_gen::{render, merge_json};
use serde_json::{json, Map, Value};

pub use ground_gen::GenError;

const ROOT_TPL:         &str = include_str!("templates/root.json.tera");
const SVC_TPL:          &str = include_str!("templates/type_service.json.tera");
const DB_TPL:           &str = include_str!("templates/type_database.json.tera");
const LINK_SVC_DB_TPL:  &str = include_str!("templates/link_access_service_database.json.tera");
const LINK_SVC_SVC_TPL: &str = include_str!("templates/link_access_service_service.json.tera");

fn stack_members<'a>(deploy: &DeployInstance, instances: &'a [Instance]) -> Vec<&'a Instance> {
    let stack_name = deploy.name.as_str();

    let stack = match instances.iter().find(|i| i.type_name == "stack" && i.name == stack_name) {
        Some(s) => s,
        None    => return vec![],
    };

    let mut member_names = std::collections::HashSet::new();
    for field in &stack.fields {
        match &field.value {
            ResolvedValue::Scalar(ScalarValue::InstanceRef { name, .. }) => { member_names.insert(name.clone()); }
            ResolvedValue::List(entries) => {
                for entry in entries {
                    if let Some(ScalarValue::InstanceRef { name, .. }) = entry.segments.first() {
                        member_names.insert(name.clone());
                    }
                }
            }
            _ => {}
        }
    }

    instances.iter()
        .filter(|i| i.type_name != "stack" && member_names.contains(&i.name))
        .collect()
}

pub fn generate(spec: &Spec) -> Result<String, GenError> {
    let mut frags: Vec<String> = Vec::new();

    for deploy in &spec.deploys {
        let deploy_ctx = deploy_to_ctx(deploy);
        let members    = stack_members(deploy, &spec.instances);

        let instances_ctx: Vec<Value> = members.iter()
            .map(|i| Value::Object(instance_to_ctx(i)))
            .collect();

        // Root
        let rendered = render(ROOT_TPL, &json!({ "deploy": deploy_ctx, "instances": instances_ctx }))?;
        push_nonempty(&mut frags, rendered);

        // Type hooks
        for inst in &members {
            let tpl = match inst.type_name.as_str() {
                "service"  => SVC_TPL,
                "database" => DB_TPL,
                _          => continue,
            };
            let rendered = render(tpl, &json!({ "deploy": deploy_ctx, "instance": instance_to_ctx(inst) }))?;
            push_nonempty(&mut frags, rendered);
        }

        // Link hooks — access field
        for inst in &members {
            for field in &inst.fields {
                if field.link_name != "access" { continue; }
                if let ResolvedValue::List(entries) = &field.value {
                    for entry in entries {
                        if let Some(ScalarValue::InstanceRef { name: target_name, .. }) = entry.segments.first() {
                            if let Some(target) = members.iter().find(|i| &i.name == target_name) {
                                let tpl = match target.type_name.as_str() {
                                    "database" => LINK_SVC_DB_TPL,
                                    "service"  => LINK_SVC_SVC_TPL,
                                    _          => continue,
                                };
                                let segs = segments_ctx(&entry.segments[1..]);
                                let rendered = render(tpl, &json!({
                                    "deploy":    deploy_ctx,
                                    "source":    instance_to_ctx(inst),
                                    "target":    instance_to_ctx(target),
                                    "segments":  segs,
                                }))?;
                                push_nonempty(&mut frags, rendered);
                            }
                        }
                    }
                }
            }
        }
    }

    merge_json(frags)
}

fn push_nonempty(frags: &mut Vec<String>, s: String) {
    if !s.trim().is_empty() { frags.push(s); }
}

// --- context helpers ---

fn deploy_to_ctx(d: &DeployInstance) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("name".into(),     json!(d.name));
    m.insert("provider".into(), json!(d.provider));
    m.insert("alias".into(),    json!(d.alias));
    for f in &d.fields { m.insert(f.link_name.clone(), resolved_to_json(&f.value)); }
    m
}

fn instance_to_ctx(i: &Instance) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("type_name".into(), json!(i.type_name));
    m.insert("name".into(),      json!(i.name));
    for f in &i.fields { m.insert(f.link_name.clone(), resolved_to_json(&f.value)); }
    m
}

fn resolved_to_json(v: &ResolvedValue) -> Value {
    match v {
        ResolvedValue::Scalar(s)       => scalar_to_json(s),
        ResolvedValue::Composite(fs)   => Value::Object(fs.iter().map(|f| (f.link_name.clone(), resolved_to_json(&f.value))).collect()),
        ResolvedValue::List(entries)   => Value::Array(entries.iter().map(entry_to_json).collect()),
    }
}

fn entry_to_json(e: &ListEntry) -> Value {
    if e.segments.len() == 1 { scalar_to_json(&e.segments[0]) }
    else { Value::Array(e.segments.iter().map(scalar_to_json).collect()) }
}

fn scalar_to_json(s: &ScalarValue) -> Value {
    match s {
        ScalarValue::Int(n)                              => json!(n),
        ScalarValue::Bool(b)                             => json!(b),
        ScalarValue::Str(s) | ScalarValue::Ref(s) | ScalarValue::Enum(s) => json!(s),
        ScalarValue::InstanceRef { type_name, name }     => json!({ "type_name": type_name, "name": name }),
    }
}

fn segments_ctx(segs: &[ScalarValue]) -> Map<String, Value> {
    segs.iter().enumerate()
        .map(|(i, s)| (format!("seg{i}"), scalar_to_json(s)))
        .collect()
}
