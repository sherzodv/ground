pub mod terra_ops;

use ground_compile::{AsmDef, AsmValue, CompileRes};
use ground_gen::{merge_json, render, RenderReq, TeraUnit};
use serde_json::{json, Map, Value};

pub use ground_gen::{GenError, JsonUnit};

const MANIFEST_TPL: &str = include_str!("templates/manifest.json.tera");
const MAIN_TPL: &str = include_str!("templates/main.tf.json.tera");
const ROOT_TPL: &str = include_str!("templates/root.json.tera");

pub fn generate(res: &CompileRes) -> Result<String, GenError> {
    let units = generate_each(res)?;
    merge_json(units.into_iter().map(|u| u.content).collect())
}

pub fn generate_each(res: &CompileRes) -> Result<Vec<JsonUnit>, GenError> {
    let mut out = Vec::new();
    let render_req = RenderReq {
        entry: "manifest.json.tera".into(),
        units: template_units(),
    };

    for def in &res.defs {
        let deploy = def_to_ctx(def);
        let rendered = render(&render_req, &json!({ "deploy": deploy }))?;
        out.extend(rendered.into_iter().filter(|unit| !unit.content.trim().is_empty()));
    }

    Ok(out)
}

fn template_units() -> Vec<TeraUnit> {
    vec![
        TeraUnit { file: "manifest.json.tera".into(), template: MANIFEST_TPL.into() },
        TeraUnit { file: "main.tf.json.tera".into(), template: MAIN_TPL.into() },
        TeraUnit { file: "root.json.tera".into(), template: ROOT_TPL.into() },
    ]
}

fn def_to_ctx(def: &AsmDef) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("alias".into(),    json!(def.name));
    m.insert("provider".into(), json!("aws"));
    m.insert("name".into(),     json!(def.name));
    for f in &def.fields {
        m.insert(f.name.clone(), asm_value_to_json_local(&f.value));
    }
    m
}

fn asm_value_to_json_local(v: &AsmValue) -> Value {
    use serde_json::Value as V;
    match v {
        AsmValue::Str(s)      => json!(s),
        AsmValue::Int(n)      => json!(n),
        AsmValue::Bool(b)     => json!(b),
        AsmValue::Ref(s)      => json!(s),
        AsmValue::Variant(gv) => json!(gv.value),
        AsmValue::DefRef(r)   => json!({ "type_name": r.type_name, "name": r.name }),
        AsmValue::Def(def)    => V::Object(def.fields.iter().map(|f| (f.name.clone(), asm_value_to_json_local(&f.value))).collect()),
        AsmValue::Path(segs)  => V::Array(segs.iter().map(asm_value_to_json_local).collect()),
        AsmValue::List(items) => V::Array(items.iter().map(asm_value_to_json_local).collect()),
    }
}
