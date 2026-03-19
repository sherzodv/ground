pub mod terra_ops;

use ground_compile::{CompileRes, Deploy, AsmInst, AsmValue};
use ground_gen::{render, merge_json};
use serde_json::{json, Map, Value};

pub use ground_gen::GenError;

const ROOT_TPL:         &str = include_str!("templates/root.json.tera");
const SVC_TPL:          &str = include_str!("templates/type_service.json.tera");
const DB_TPL:           &str = include_str!("templates/type_database.json.tera");
const LINK_SVC_DB_TPL:  &str = include_str!("templates/link_access_service_database.json.tera");
const LINK_SVC_SVC_TPL: &str = include_str!("templates/link_access_service_service.json.tera");

pub fn generate(res: &CompileRes) -> Result<String, GenError> {
    let mut frags: Vec<String> = Vec::new();

    for deploy in &res.deploys {
        let deploy_ctx = deploy_to_ctx(deploy);

        let instances_ctx: Vec<Value> = deploy.members.iter()
            .filter_map(|r| res.symbol.get(&r.name))
            .map(|i| Value::Object(inst_to_ctx(i)))
            .collect();

        // Root
        let rendered = render(ROOT_TPL, &json!({ "deploy": deploy_ctx, "instances": instances_ctx }))?;
        push_nonempty(&mut frags, rendered);

        // Type hooks
        for inst_ref in &deploy.members {
            let tpl = match inst_ref.type_name.as_str() {
                "service"  => SVC_TPL,
                "database" => DB_TPL,
                _          => continue,
            };
            if let Some(inst) = res.symbol.get(&inst_ref.name) {
                let rendered = render(tpl, &json!({ "deploy": deploy_ctx, "instance": inst_to_ctx(inst) }))?;
                push_nonempty(&mut frags, rendered);
            }
        }

        // Link hooks — access field
        for inst_ref in &deploy.members {
            let Some(inst) = res.symbol.get(&inst_ref.name) else { continue };
            for field in &inst.fields {
                if field.name != "access" { continue; }
                if let AsmValue::List(items) = &field.value {
                    for item in items {
                        let (target_name, extra): (&str, &[AsmValue]) = match item {
                            AsmValue::InstRef(ir)  => (&ir.name, &[]),
                            AsmValue::Path(segs)   => match segs.first() {
                                Some(AsmValue::InstRef(ir)) => (&ir.name, &segs[1..]),
                                _                           => continue,
                            },
                            _ => continue,
                        };
                        if let Some(target) = res.symbol.get(target_name) {
                            let tpl = match target.type_name.as_str() {
                                "database" => LINK_SVC_DB_TPL,
                                "service"  => LINK_SVC_SVC_TPL,
                                _          => continue,
                            };
                            let rendered = render(tpl, &json!({
                                "deploy":   deploy_ctx,
                                "source":   inst_to_ctx(inst),
                                "target":   inst_to_ctx(target),
                                "segments": segments_ctx(extra),
                            }))?;
                            push_nonempty(&mut frags, rendered);
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

fn deploy_to_ctx(d: &Deploy) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("alias".into(),    json!(d.name));
    m.insert("provider".into(), json!(d.target.join(":")));
    m.insert("name".into(),     json!(d.inst.name));
    for f in &d.fields { m.insert(f.name.clone(), asm_value_to_json(&f.value)); }
    m
}

fn inst_to_ctx(i: &AsmInst) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("type_name".into(), json!(i.type_name));
    m.insert("name".into(),      json!(i.name));
    for f in &i.fields { m.insert(f.name.clone(), asm_value_to_json(&f.value)); }
    m
}

fn asm_value_to_json(v: &AsmValue) -> Value {
    match v {
        AsmValue::Str(s)      => json!(s),
        AsmValue::Int(n)      => json!(n),
        AsmValue::Ref(s)      => json!(s),
        AsmValue::Variant(gv) => json!(gv.value),
        AsmValue::InstRef(ir) => json!({ "type_name": ir.type_name, "name": ir.name }),
        AsmValue::Inst(gi)    => Value::Object(gi.fields.iter().map(|f| (f.name.clone(), asm_value_to_json(&f.value))).collect()),
        AsmValue::Path(segs)  => Value::Array(segs.iter().map(asm_value_to_json).collect()),
        AsmValue::List(items) => Value::Array(items.iter().map(asm_value_to_json).collect()),
    }
}

fn segments_ctx(segs: &[AsmValue]) -> Map<String, Value> {
    segs.iter().enumerate()
        .map(|(i, v)| (format!("seg{i}"), asm_value_to_json(v)))
        .collect()
}
