pub mod terra_ops;

use ground_compile::{AsmDef, AsmValue, CompileRes};
use ground_gen::{merge_json, render, RenderReq, TeraUnit};
use serde_json::{json, Map, Value};

pub use ground_gen::{GenError, JsonUnit};

const MANIFEST_TPL: &str = include_str!("templates/manifest.json.tera");
const MANIFEST_NETWORK_TPL: &str = include_str!("templates/manifest_network.json.tera");
const MANIFEST_STATE_TPL: &str = include_str!("templates/manifest_state.json.tera");
const MAIN_TPL: &str = include_str!("templates/main.tf.json.tera");
const NETWORK_TPL: &str = include_str!("templates/network.json.tera");
const PLAN_STAMP_TPL: &str = include_str!("templates/plan-stamp.json.tera");
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
        let entry = match def_kind(def).or_else(|| (def.type_name == "vpc").then_some("network")) {
            Some("state_store") => "manifest_state.json.tera",
            Some("network") => "manifest_network.json.tera",
            _ => "manifest.json.tera",
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
        TeraUnit { file: "manifest_network.json.tera".into(), template: MANIFEST_NETWORK_TPL.into() },
        TeraUnit { file: "manifest_state.json.tera".into(), template: MANIFEST_STATE_TPL.into() },
        TeraUnit { file: "main.tf.json.tera".into(), template: MAIN_TPL.into() },
        TeraUnit { file: "network.json.tera".into(), template: NETWORK_TPL.into() },
        TeraUnit { file: "plan-stamp.json.tera".into(), template: PLAN_STAMP_TPL.into() },
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
    if def.type_name == "vpc" && def_kind(def).is_none() {
        return vpc_def_to_ctx(def);
    }

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

fn vpc_def_to_ctx(def: &AsmDef) -> Map<String, Value> {
    let project = def_ref_name(def, "project").unwrap_or("ground");
    let region_values = def_list_strs(def, "region");
    let first_region = region_values.first().map(|s| s.as_str()).unwrap_or("us-east:1");
    let region_prefix = first_region.split(':').next().unwrap_or("us-east");
    let provider_region = aws_region(region_prefix);
    let project_bucket = sanitize_bucket(project);
    let project_key = sanitize_key(project);
    let stem = format!("{project_key}_network");
    let name_stem = format!("{project}-network");
    let egress = def_variant(def, "egress").unwrap_or("none");
    let has_igw = egress != "none";
    let zones = region_values
        .iter()
        .enumerate()
        .map(|(idx, raw)| {
            let zone_cfg = def_list_defs(def, "zones").get(idx).cloned().unwrap_or_default();
            let parsed = parse_zone(raw);
            let n = parsed.0;
            let az = parsed.1;
            let default_public = parsed.2;
            let default_private = parsed.3;
            let private_cidr = zone_cfg.get("private").and_then(Value::as_str).unwrap_or(&default_private).to_string();
            let public_cidr = if egress == "none" {
                None
            } else {
                Some(zone_cfg.get("public").and_then(Value::as_str).unwrap_or(&default_public).to_string())
            };
            let private_nat_key = match egress {
                "shared" => Some(format!("{stem}_nat")),
                "zone" => Some(format!("{stem}_nat_{n}")),
                _ => None,
            };

            json!({
                "n": n,
                "az": az,
                "private_cidr": private_cidr,
                "public_cidr": public_cidr,
                "pub_key": format!("{stem}_npub_{n}"),
                "pub_name": format!("{name_stem}-npub-{n}"),
                "priv_key": format!("{stem}_nprv_{n}"),
                "priv_name": format!("{name_stem}-nprv-{n}"),
                "rpub_key": format!("{stem}_rpub_{n}"),
                "rpub_name": format!("{name_stem}-rpub-{n}"),
                "rprv_key": format!("{stem}_rprv_{n}"),
                "rprv_name": format!("{name_stem}-rprv-{n}"),
                "rpub_default_key": format!("{stem}_rpub_{n}_default"),
                "rprv_default_key": format!("{stem}_rprv_{n}_default"),
                "private_nat_key": private_nat_key,
            })
        })
        .collect::<Vec<_>>();

    let nat_gateways = match egress {
        "shared" => vec![json!({
            "n": "1",
            "nat_key": format!("{stem}_nat"),
            "nat_name": format!("{name_stem}-nat"),
            "eip_key": format!("{stem}_nat_eip"),
            "public_subnet_key": format!("{stem}_npub_1"),
        })],
        "zone" => region_values
            .iter()
            .map(|raw| {
                let (n, _, _, _) = parse_zone(raw);
                json!({
                    "n": n,
                    "nat_key": format!("{stem}_nat_{n}"),
                    "nat_name": format!("{name_stem}-nat-{n}"),
                    "eip_key": format!("{stem}_nat_eip_{n}"),
                    "public_subnet_key": format!("{stem}_npub_{n}"),
                })
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    let mut m = Map::new();
    m.insert("alias".into(), json!(def.name));
    m.insert("provider".into(), json!("aws"));
    m.insert("name".into(), json!(def.name));
    m.insert("kind".into(), json!("network"));
    m.insert("provider_region".into(), json!(provider_region));
    m.insert("backend".into(), json!({
        "bucket": format!("{project_bucket}-tfstate"),
        "key": format!("{project_bucket}/network.tfstate"),
        "region": provider_region,
        "encrypt": true,
        "use_lockfile": true,
    }));
    m.insert("root".into(), json!({
        "vpc_key": format!("{stem}_vpc"),
        "vpc_name": format!("{name_stem}-vpc"),
        "cidr_block": def_str(def, "cidr").unwrap_or("10.0.0.0/16"),
        "enable_dns_support": def_bool(def, "dns").unwrap_or(true),
        "enable_dns_hostnames": def_bool(def, "dns").unwrap_or(true),
        "gw_key": format!("{stem}_gw"),
        "gw_name": format!("{name_stem}-gw"),
        "has_internet_gateway": has_igw,
    }));
    m.insert("zones".into(), Value::Array(zones));
    m.insert("nat_gateways".into(), Value::Array(nat_gateways));
    m
}

fn def_field<'a>(def: &'a AsmDef, name: &str) -> Option<&'a AsmValue> {
    def.fields.iter().find(|f| f.name == name).map(|f| &f.value)
}

fn def_ref_name<'a>(def: &'a AsmDef, name: &str) -> Option<&'a str> {
    match def_field(def, name)? {
        AsmValue::DefRef(r) => Some(r.name.as_str()),
        AsmValue::Ref(s) | AsmValue::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

fn def_variant<'a>(def: &'a AsmDef, name: &str) -> Option<&'a str> {
    match def_field(def, name)? {
        AsmValue::Variant(v) => Some(v.value.as_str()),
        AsmValue::Ref(s) | AsmValue::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

fn def_str<'a>(def: &'a AsmDef, name: &str) -> Option<&'a str> {
    match def_field(def, name)? {
        AsmValue::Str(s) | AsmValue::Ref(s) => Some(s.as_str()),
        _ => None,
    }
}

fn def_bool(def: &AsmDef, name: &str) -> Option<bool> {
    match def_field(def, name)? {
        AsmValue::Bool(v) => Some(*v),
        _ => None,
    }
}

fn def_list_strs(def: &AsmDef, name: &str) -> Vec<String> {
    match def_field(def, name) {
        Some(AsmValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                AsmValue::Str(s) | AsmValue::Ref(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn def_list_defs(def: &AsmDef, name: &str) -> Vec<Map<String, Value>> {
    match def_field(def, name) {
        Some(AsmValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                AsmValue::Def(inner) => Some(
                    inner
                        .fields
                        .iter()
                        .map(|f| (f.name.clone(), asm_value_to_json_local(&f.value)))
                        .collect(),
                ),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn sanitize_key(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_us = false;
    for ch in raw.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_us = false;
        } else if !prev_us {
            out.push('_');
            prev_us = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn sanitize_bucket(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn aws_region(prefix: &str) -> &'static str {
    match prefix {
        "us-east" => "us-east-1",
        "us-west" => "us-west-2",
        "eu-central" => "eu-central-1",
        "eu-west" => "eu-west-1",
        "ap-southeast" => "ap-southeast-1",
        "me-central" => "me-central-1",
        _ => "us-east-1",
    }
}

fn parse_zone(raw: &str) -> (String, String, String, String) {
    let mut parts = raw.splitn(2, ':');
    let prefix = parts.next().unwrap_or("us-east");
    let number = parts
        .next()
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(1);
    let letters = b"abcdefghijklmnopqrstuvwxyz";
    let letter = letters.get(number - 1).copied().unwrap_or(b'a') as char;
    let region = aws_region(prefix);
    let pub_idx = (number - 1) * 2;
    let priv_idx = pub_idx + 1;
    (
        number.to_string(),
        format!("{region}{letter}"),
        format!("10.0.{pub_idx}.0/24"),
        format!("10.0.{priv_idx}.0/24"),
    )
}

#[cfg(test)]
mod tests {
    use super::{generate, generate_each};
    use ground_compile::{compile, CompileReq, Unit};
    use ground_gen::{render, RenderReq};

    #[test]
    fn state_templates_render() {
        render(&RenderReq {
            entry: "manifest_state.json.tera".into(),
            units: super::template_units(),
        }, &serde_json::json!({ "deploy": { "alias": "bootstrap", "kind": "state_store", "provider_region": "us-east-1", "root": { "bucket_key": "k", "bucket_name": "b" } } }))
        .expect("templates should parse");
    }

    #[test]
    fn network_templates_render() {
        render(&RenderReq {
            entry: "manifest_network.json.tera".into(),
            units: super::template_units(),
        }, &serde_json::json!({ "deploy": { "alias": "platform", "kind": "network", "provider_region": "us-east-1", "root": { "vpc_key": "v", "vpc_name": "n", "cidr_block": "10.0.0.0/16", "enable_dns_support": true, "enable_dns_hostnames": true, "gw_key": "g", "gw_name": "gw", "has_internet_gateway": true }, "zones": [], "nat_gateways": [] } }))
        .expect("templates should parse");
    }

    #[test]
    fn manifest_emits_plan_stamp() {
        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
                path: vec![],
                src: r#"
                    use std:def:project
                    use std:tf:def:state_store

                    ground-test = project {}

                    plan bootstrap = state_store {
                      project: ground-test
                      state: remote
                      region: [ us-east:1 ]
                      encrypted: true
                    }
                "#.into(),
                ts_src: None,
            }],
        });

        let outputs = generate_each(&res).expect("terraform generation failed");
        assert!(outputs.iter().any(|u| u.file == "bootstrap/.ground-plan.json"));
    }

    #[test]
    fn generates_state_bootstrap_bucket() {
        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
                path: vec![],
                src: r#"
                    use std:def:project
                    use std:tf:def:state_store

                    ground-test = project {}

                    plan bootstrap = state_store {
                      project: ground-test
                      state: local
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
    fn generates_network_plan() {
        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
                path: vec![],
                src: r#"
                    use std:def:project
                    use std:platform:def:network_zone
                    use std:aws:tf:def:vpc

                    ground-test = project {}

                    plan platform = std:aws:tf:vpc {
                      project: ground-test
                      cidr: "10.42.0.0/16"
                      egress: shared
                      zones: [
                        network_zone {
                          private: "10.42.0.0/20"
                          public: "10.42.128.0/20"
                        }
                        network_zone {
                          private: "10.42.16.0/20"
                          public: "10.42.144.0/20"
                        }
                      ]
                      region: [ us-east:1 us-east:2 ]
                    }
                "#.into(),
                ts_src: None,
            }],
        });

        assert!(res.errors.is_empty(), "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let json = generate(&res).expect("terraform generation failed");
        assert!(json.contains("\"aws_vpc\""));
        assert!(json.contains("\"cidr_block\": \"10.42.0.0/16\""));
        assert!(json.contains("\"aws_nat_gateway\""));
        assert!(json.contains("\"bucket\": \"ground-test-tfstate\""));
        assert!(json.contains("\"key\": \"ground-test/network.tfstate\""));
    }

    #[test]
    fn state_store_remote_emits_backend() {
        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
                path: vec![],
                src: r#"
                    use std:def:project
                    use std:tf:def:state_store

                    ground-test = project {}

                    plan bootstrap = state_store {
                      project: ground-test
                      state: remote
                      region: [ us-east:1 ]
                      encrypted: true
                    }
                "#.into(),
                ts_src: None,
            }],
        });

        assert!(res.errors.is_empty(), "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let json = generate(&res).expect("terraform generation failed");
        assert!(json.contains("\"backend\""));
        assert!(json.contains("\"bucket\": \"ground-test-tfstate\""));
        assert!(json.contains("\"key\": \"ground-test/terraform.tfstate\""));
    }

    #[test]
    fn generates_deploy_backend_from_state() {
        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
                path: vec![],
                src: r#"
                    use std:def:project
                    use std:def:service
                    use std:def:compute_pool
                    use std:aws:tf:def:deploy

                    ground-test = project {}

                    def api {} = service {
                      port: http
                    }

                    def app {} = compute_pool {
                      services: [ api ]
                    }

                    plan prod = std:aws:tf:deploy {
                      project: ground-test
                      pool: app
                      region: [ us-east:1 ]
                    }
                "#.into(),
                ts_src: None,
            }],
        });

        assert!(res.errors.is_empty(), "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let json = generate(&res).expect("terraform generation failed");
        assert!(json.contains("\"backend\""));
        assert!(json.contains("\"s3\""));
        assert!(json.contains("\"bucket\": \"ground-test-tfstate\""));
        assert!(json.contains("\"key\": \"ground-test/app.tfstate\""));
        assert!(json.contains("\"use_lockfile\": true"));
    }

    #[test]
    fn deploy_uses_custom_service_image() {
        let res = compile(CompileReq {
            units: vec![Unit {
                name: "test".into(),
                path: vec![],
                src: r#"
                    use std:def:service
                    use std:def:project
                    use std:def:compute_pool
                    use std:aws:tf:def:deploy

                    ground-test = project {}

                    def api {} = service {
                      port: http
                      image: ealen/echo-server
                    }

                    def app {} = compute_pool {
                      services: [ api ]
                    }

                    plan prod = deploy {
                      project: ground-test
                      pool: app
                      region: [ us-east:1 ]
                    }
                "#.into(),
                ts_src: None,
            }],
        });

        assert!(res.errors.is_empty(), "compile errors: {:?}", res.errors.iter().map(|e| &e.message).collect::<Vec<_>>());

        let json = generate(&res).expect("terraform generation failed");
        assert!(json.contains("\\\"image\\\":\\\"ealen/echo-server\\\""));
    }
}
