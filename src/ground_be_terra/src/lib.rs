pub mod terra_ops;

use ground_compile::{AsmDef, AsmValue, CompileRes};
use ground_gen::{merge_json, render, RenderReq, TeraUnit};
use serde_json::{json, Map, Value};

pub use ground_gen::{GenError, JsonUnit};

const MANIFEST_TPL: &str = include_str!("templates/manifest.json.tera");
const MAIN_TPL: &str = include_str!("templates/main.tf.json.tera");
const ROOT_TPL: &str = include_str!("templates/root.json.tera");
const AWS_CLOUDWATCH_LOG_GROUP_TPL: &str = include_str!("templates/aws_cloudwatch_log_group.json.tera");
const AWS_DB_INSTANCE_TPL: &str = include_str!("templates/aws_db_instance.json.tera");
const AWS_DB_SUBNET_GROUP_TPL: &str = include_str!("templates/aws_db_subnet_group.json.tera");
const AWS_ECS_SERVICE_TPL: &str = include_str!("templates/aws_ecs_service.json.tera");
const AWS_ECS_TASK_DEFINITION_TPL: &str = include_str!("templates/aws_ecs_task_definition.json.tera");
const AWS_IAM_ROLE_TPL: &str = include_str!("templates/aws_iam_role.json.tera");
const AWS_IAM_ROLE_POLICY_ATTACHMENT_TPL: &str = include_str!("templates/aws_iam_role_policy_attachment.json.tera");
const AWS_SECURITY_GROUP_TPL: &str = include_str!("templates/aws_security_group.json.tera");
const AWS_VPC_SECURITY_GROUP_EGRESS_RULE_TPL: &str = include_str!("templates/aws_vpc_security_group_egress_rule.json.tera");
const AWS_VPC_SECURITY_GROUP_INGRESS_RULE_TPL: &str = include_str!("templates/aws_vpc_security_group_ingress_rule.json.tera");
const LINK_ACCESS_SERVICE_DATABASE_TPL: &str = include_str!("templates/link_access_service_database.json.tera");
const LINK_ACCESS_SERVICE_SERVICE_TPL: &str = include_str!("templates/link_access_service_service.json.tera");
const RANDOM_PASSWORD_TPL: &str = include_str!("templates/random_password.json.tera");
const TYPE_DATABASE_TPL: &str = include_str!("templates/type_database.json.tera");
const TYPE_SERVICE_TPL: &str = include_str!("templates/type_service.json.tera");

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
        TeraUnit { file: "aws_cloudwatch_log_group.json.tera".into(), template: AWS_CLOUDWATCH_LOG_GROUP_TPL.into() },
        TeraUnit { file: "aws_db_instance.json.tera".into(), template: AWS_DB_INSTANCE_TPL.into() },
        TeraUnit { file: "aws_db_subnet_group.json.tera".into(), template: AWS_DB_SUBNET_GROUP_TPL.into() },
        TeraUnit { file: "aws_ecs_service.json.tera".into(), template: AWS_ECS_SERVICE_TPL.into() },
        TeraUnit { file: "aws_ecs_task_definition.json.tera".into(), template: AWS_ECS_TASK_DEFINITION_TPL.into() },
        TeraUnit { file: "aws_iam_role.json.tera".into(), template: AWS_IAM_ROLE_TPL.into() },
        TeraUnit { file: "aws_iam_role_policy_attachment.json.tera".into(), template: AWS_IAM_ROLE_POLICY_ATTACHMENT_TPL.into() },
        TeraUnit { file: "aws_security_group.json.tera".into(), template: AWS_SECURITY_GROUP_TPL.into() },
        TeraUnit { file: "aws_vpc_security_group_egress_rule.json.tera".into(), template: AWS_VPC_SECURITY_GROUP_EGRESS_RULE_TPL.into() },
        TeraUnit { file: "aws_vpc_security_group_ingress_rule.json.tera".into(), template: AWS_VPC_SECURITY_GROUP_INGRESS_RULE_TPL.into() },
        TeraUnit { file: "link_access_service_database.json.tera".into(), template: LINK_ACCESS_SERVICE_DATABASE_TPL.into() },
        TeraUnit { file: "link_access_service_service.json.tera".into(), template: LINK_ACCESS_SERVICE_SERVICE_TPL.into() },
        TeraUnit { file: "random_password.json.tera".into(), template: RANDOM_PASSWORD_TPL.into() },
        TeraUnit { file: "type_database.json.tera".into(), template: TYPE_DATABASE_TPL.into() },
        TeraUnit { file: "type_service.json.tera".into(), template: TYPE_SERVICE_TPL.into() },
    ]
}

fn def_to_ctx(def: &AsmDef) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("alias".into(),    json!(def.name));
    m.insert("provider".into(), json!("aws"));
    m.insert("name".into(),     json!(def.name));
    for f in &def.fields {
        let value = if f.name == "region" {
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
        AsmValue::Bool(b)     => json!(b),
        AsmValue::Ref(s)      => json!(s),
        AsmValue::Variant(gv) => json!(gv.value),
        AsmValue::DefRef(r)   => json!({ "type_name": r.type_name, "name": r.name }),
        AsmValue::Def(def)    => V::Object(def.fields.iter().map(|f| (f.name.clone(), asm_value_to_json_local(&f.value))).collect()),
        AsmValue::Path(segs)  => V::Array(segs.iter().map(asm_value_to_json_local).collect()),
        AsmValue::List(items) => V::Array(items.iter().map(asm_value_to_json_local).collect()),
    }
}
