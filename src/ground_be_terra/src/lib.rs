pub mod terra_ops;

use ground_compile::{CompileRes, Deploy, AsmExpansion, AsmOutput, AsmLinkOutput, AsmValue};
use ground_gen::{render, merge_json};
use serde_json::{json, Map, Value};

pub use ground_gen::GenError;

const ROOT_TPL:          &str = include_str!("templates/root.json.tera");
const IAM_ROLE_TPL:      &str = include_str!("templates/aws_iam_role.json.tera");
const IAM_ATTACH_TPL:    &str = include_str!("templates/aws_iam_role_policy_attachment.json.tera");
const SG_TPL:            &str = include_str!("templates/aws_security_group.json.tera");
const SG_EGRESS_TPL:     &str = include_str!("templates/aws_vpc_security_group_egress_rule.json.tera");
const SG_INGRESS_TPL:    &str = include_str!("templates/aws_vpc_security_group_ingress_rule.json.tera");
const LOG_GROUP_TPL:     &str = include_str!("templates/aws_cloudwatch_log_group.json.tera");
const ECS_TD_TPL:        &str = include_str!("templates/aws_ecs_task_definition.json.tera");
const ECS_SVC_TPL:       &str = include_str!("templates/aws_ecs_service.json.tera");
const RAND_PW_TPL:       &str = include_str!("templates/random_password.json.tera");
const DB_SNG_TPL:        &str = include_str!("templates/aws_db_subnet_group.json.tera");
const DB_INST_TPL:       &str = include_str!("templates/aws_db_instance.json.tera");

pub fn generate(res: &CompileRes) -> Result<String, GenError> {
    let mut frags: Vec<String> = Vec::new();

    for deploy in &res.deploys {
        let deploy_ctx = deploy_to_ctx(deploy);

        // Root template — always rendered.
        let rendered = render(ROOT_TPL, &json!({ "deploy": deploy_ctx }))?;
        push_nonempty(&mut frags, rendered);

        // Walk the expansion tree produced by type function firing.
        if let Some(exp) = &deploy.expansion {
            walk_expansion(exp, &deploy_ctx, &mut frags)?;
        }
    }

    merge_json(frags)
}

/// Recursively walk an `AsmExpansion` tree and render outputs via Tera templates.
fn walk_expansion(
    exp:        &AsmExpansion,
    deploy_ctx: &Map<String, Value>,
    frags:      &mut Vec<String>,
) -> Result<(), GenError> {
    // 1-param outputs: one entry per fired type-function entry.
    for output in &exp.outputs {
        if let Some(tpl) = load_template(&output.scope, &output.vendor_type) {
            let ctx = output_to_ctx(output);
            let rendered = render(tpl, &json!({ "deploy": deploy_ctx, "output": ctx }))?;
            push_nonempty(frags, rendered);
        }
    }

    // 2-param link outputs.
    for link_out in &exp.link_outs {
        render_link_out(link_out, deploy_ctx, frags)?;
    }

    // Children (recursively expanded linked instances).
    for child in &exp.children {
        walk_expansion(child, deploy_ctx, frags)?;
    }

    Ok(())
}

fn render_link_out(
    link_out:   &AsmLinkOutput,
    deploy_ctx: &Map<String, Value>,
    frags:      &mut Vec<String>,
) -> Result<(), GenError> {
    for output in &link_out.outputs {
        if let Some(tpl) = load_template(&output.scope, &output.vendor_type) {
            let ctx = json!({
                "deploy": deploy_ctx,
                "from":   { "type_name": link_out.from.type_name, "name": link_out.from.name },
                "to":     { "type_name": link_out.to.type_name,   "name": link_out.to.name   },
                "output": output_to_ctx(output),
            });
            let rendered = render(tpl, &ctx)?;
            push_nonempty(frags, rendered);
        }
    }
    Ok(())
}

/// Load a Tera template by scope path + vendor type name.
fn load_template(scope: &[String], vendor_type: &str) -> Option<&'static str> {
    let _ = scope;
    match vendor_type {
        "aws_iam_role"                        => Some(IAM_ROLE_TPL),
        "aws_iam_role_policy_attachment"      => Some(IAM_ATTACH_TPL),
        "aws_security_group"                  => Some(SG_TPL),
        "aws_vpc_security_group_egress_rule"  => Some(SG_EGRESS_TPL),
        "aws_vpc_security_group_ingress_rule" => Some(SG_INGRESS_TPL),
        "aws_cloudwatch_log_group"            => Some(LOG_GROUP_TPL),
        "aws_ecs_task_definition"             => Some(ECS_TD_TPL),
        "aws_ecs_service"                     => Some(ECS_SVC_TPL),
        "random_password"                     => Some(RAND_PW_TPL),
        "aws_db_subnet_group"                 => Some(DB_SNG_TPL),
        "aws_db_instance"                     => Some(DB_INST_TPL),
        _ => None,
    }
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

fn output_to_ctx(o: &AsmOutput) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("alias".into(),       json!(o.alias));
    m.insert("vendor_type".into(), json!(o.vendor_type));
    for f in &o.fields { m.insert(f.name.clone(), asm_value_to_json(&f.value)); }
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
