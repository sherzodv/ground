pub mod terra_ops;

use ground_compile::{CompileRes, Plan, AsmValue};
use ground_gen::{render, merge_json};
use serde_json::{json, Map, Value};

pub use ground_gen::GenError;

const ROOT_TPL: &str = include_str!("templates/root.json.tera");

pub fn generate(res: &CompileRes) -> Result<String, GenError> {
    let mut frags: Vec<String> = Vec::new();

    for plan in &res.plans {
        let plan_ctx = plan_to_ctx(plan);
        let rendered = render(ROOT_TPL, &json!({ "deploy": plan_ctx }))?;
        push_nonempty(&mut frags, rendered);
    }

    merge_json(frags)
}

/// Generate Terraform JSON for each plan separately.
/// Returns a vec of `(plan_name, tf_json)` pairs.
pub fn generate_each(res: &CompileRes) -> Result<Vec<(String, String)>, GenError> {
    let mut out = Vec::new();
    for plan in &res.plans {
        let plan_ctx = plan_to_ctx(plan);
        let rendered = render(ROOT_TPL, &json!({ "deploy": plan_ctx }))?;
        if !rendered.trim().is_empty() {
            out.push((plan.name.clone(), rendered));
        }
    }
    Ok(out)
}

fn push_nonempty(frags: &mut Vec<String>, s: String) {
    if !s.trim().is_empty() { frags.push(s); }
}

// --- context helpers ---

fn plan_to_ctx(p: &Plan) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("alias".into(),    json!(p.name));
    m.insert("provider".into(), json!("aws"));
    m.insert("name".into(),     json!(p.root.name));
    for f in &p.fields {
        let value = if f.name == "region" {
            // Convert ["eu-central:1", ...] → [["eu-central", "1"], ...] for the template.
            if let AsmValue::List(items) = &f.value {
                let regions: Vec<Value> = items.iter().map(|item| {
                    let s = match item {
                        AsmValue::Str(s) | AsmValue::Ref(s) => s.as_str(),
                        _ => "",
                    };
                    let mut parts = s.splitn(2, ':');
                    let prefix = parts.next().unwrap_or(s);
                    let zone   = parts.next().unwrap_or("1");
                    json!([prefix, zone])
                }).collect();
                json!(regions)
            } else {
                asm_value_to_json_local(&f.value)
            }
        } else {
            asm_value_to_json_local(&f.value)
        };
        m.insert(f.name.clone(), value);
    }
    m
}

fn asm_value_to_json_local(v: &AsmValue) -> Value {
    use serde_json::Value as V;
    match v {
        AsmValue::Str(s)      => json!(s),
        AsmValue::Int(n)      => json!(n),
        AsmValue::Ref(s)      => json!(s),
        AsmValue::Variant(gv) => json!(gv.value),
        AsmValue::InstRef(ir) => json!({ "type_name": ir.type_name, "name": ir.name }),
        AsmValue::Inst(gi)    => V::Object(gi.fields.iter().map(|f| (f.name.clone(), asm_value_to_json_local(&f.value))).collect()),
        AsmValue::Path(segs)  => V::Array(segs.iter().map(asm_value_to_json_local).collect()),
        AsmValue::List(items) => V::Array(items.iter().map(asm_value_to_json_local).collect()),
    }
}
