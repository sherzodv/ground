pub mod terra_ops;

use ground_compile::{AsmDef, AsmValue, CompileRes};
use ground_gen::{merge_json, render, RenderReq, TeraUnit};
use serde_json::{json, Map, Value};

pub use ground_gen::{GenError, JsonUnit};

const MANIFEST_TPL: &str = include_str!("templates/manifest.json.tera");
const MANIFEST_STATE_TPL: &str = include_str!("templates/manifest_state.json.tera");
const MAIN_TPL: &str = include_str!("templates/main.tf.json.tera");
const ROOT_TPL: &str = include_str!("templates/root.json.tera");
const STATE_TPL: &str = include_str!("templates/state.json.tera");

pub fn generate(res: &CompileRes) -> Result<String, GenError> {
    let units = generate_each(res)?;
    merge_json(units.into_iter().map(|u| u.content).collect())
}

pub fn generate_each(res: &CompileRes) -> Result<Vec<JsonUnit>, GenError> {
    let mut out = Vec::new();

    for def in &res.defs {
        let deploy = def_to_ctx(def);
        let entry = if def_kind(def) == Some("state") {
            "manifest_state.json.tera"
        } else {
            "manifest.json.tera"
        };
        let rendered = render(&RenderReq {
            entry: entry.into(),
            units: template_units(),
        }, &json!({ "deploy": deploy }))?;
        out.extend(rendered.into_iter().filter(|unit| !unit.content.trim().is_empty()));
    }

    Ok(out)
}

fn template_units() -> Vec<TeraUnit> {
    vec![
        TeraUnit { file: "manifest.json.tera".into(), template: MANIFEST_TPL.into() },
        TeraUnit { file: "manifest_state.json.tera".into(), template: MANIFEST_STATE_TPL.into() },
        TeraUnit { file: "main.tf.json.tera".into(), template: MAIN_TPL.into() },
        TeraUnit { file: "root.json.tera".into(), template: ROOT_TPL.into() },
        TeraUnit { file: "state.json.tera".into(), template: STATE_TPL.into() },
    ]
}

fn def_kind(def: &AsmDef) -> Option<&str> {
    def.fields.iter().find(|f| f.name == "kind").and_then(|f| match &f.value {
        AsmValue::Str(s) | AsmValue::Ref(s) => Some(s.as_str()),
        AsmValue::Variant(v) => Some(v.value.as_str()),
        _ => None,
    })
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

#[cfg(test)]
mod tests {
    use super::generate;
    use ground_compile::{compile, CompileReq, Unit};
    use ground_gen::{render, RenderReq};

    #[test]
    fn state_templates_render() {
        render(&RenderReq {
            entry: "manifest_state.json.tera".into(),
            units: super::template_units(),
        }, &serde_json::json!({ "deploy": { "alias": "bootstrap", "kind": "state", "provider_region": "us-east-1", "root": { "bucket_key": "k", "bucket_name": "b" } } }))
        .expect("templates should parse");
    }

    #[test]
    fn generates_state_bootstrap_bucket() {
        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
                path: vec![],
                src: r#"
                    use pack:std:def:*
                    use pack:std:platform:*

                    ground-test = project {}

                    plan bootstrap = state {
                      project: ground-test
                      region: [ us-east:1 ]
                      encrypted: true
                    }
                "#.into(),
                ts_src: None,
            }],
        });

        assert!(res.errors.is_empty(), "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let json = generate(&res).expect("terraform generation failed");
        assert!(json.contains("\"aws_s3_bucket\""));
        assert!(json.contains("\"bucket\": \"ground-test-tfstate\""));
        assert!(!json.contains("\"backend\""));
    }

    #[test]
    fn generates_deploy_backend_from_state() {
        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
                path: vec![],
                src: r#"
                    use pack:std:*

                    def api {} = service {
                      port: http
                    }

                    def app {} = compute_pool {
                      services: [ api ]
                    }

                    plan prod = std:aws:tf:deploy {
                      prefix: "core-"
                      pool: app
                      region: [ us-east:1 ]
                      state_backend: std:aws:tf:def:backend_s3 {
                        bucket: "demo-tfstate"
                        key: "demo/terraform.tfstate"
                        region: "us-east-1"
                        encrypt: true
                        use_lockfile: true
                      }
                    }
                "#.into(),
                ts_src: None,
            }],
        });

        assert!(res.errors.is_empty(), "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let json = generate(&res).expect("terraform generation failed");
        assert!(json.contains("\"backend\""));
        assert!(json.contains("\"s3\""));
        assert!(json.contains("\"bucket\": \"demo-tfstate\""));
        assert!(json.contains("\"key\": \"demo/terraform.tfstate\""));
        assert!(json.contains("\"use_lockfile\": true"));
    }
}
