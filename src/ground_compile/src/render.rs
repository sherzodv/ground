use std::collections::{BTreeMap, HashSet};

use ground_gen::{render as gen_render, GenError, RenderReq, TeraUnit};
use serde_json::{json, Value};

use crate::asm::{asm_value_to_json, AsmDef, AsmField};
use crate::{CompileError, CompileRes, ErrorLoc, PlanRoot};

/// Template file addressable by pack path. Collected by the CLI from disk
/// (or by tests from fixtures) and passed to `render` alongside a target.
#[derive(Debug, Clone)]
pub struct TemplateUnit {
    pub path: Vec<String>,
    pub file: String,
    pub id: String,
    pub content: String,
}

/// Rendering target — backend + output type, both opaque tokens used only to
/// select the manifest filename `main.<backend>.<out_type>.tera`.
#[derive(Debug, Clone)]
pub struct RenderTarget {
    pub backend: String,
    pub out_type: String,
}

impl RenderTarget {
    /// Parse `"<backend>:<out_type>"`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let (b, t) = s
            .split_once(':')
            .ok_or_else(|| format!("invalid target \"{s}\": expected \"<backend>:<type>\""))?;
        if b.is_empty() || t.is_empty() {
            return Err(format!(
                "invalid target \"{s}\": backend and type must be non-empty"
            ));
        }
        Ok(Self {
            backend: b.into(),
            out_type: t.into(),
        })
    }
}

/// One rendered file produced from a plan pack's manifest.
#[derive(Debug, Clone)]
pub struct RenderUnit {
    pub backend: String,
    pub out_type: String,
    pub plan: String,
    pub pack_path: Vec<String>,
    pub file: String,
    pub content: String,
}

pub struct RenderRes {
    pub units: Vec<RenderUnit>,
    pub errors: Vec<CompileError>,
}

pub fn render_ctx_for_plan(res: &CompileRes, plan_name: &str) -> Option<Value> {
    let plan = res.plans.iter().find(|p| p.name == plan_name)?;
    let plans: Vec<&PlanRoot> = res
        .plans
        .iter()
        .filter(|p| p.pack_path == plan.pack_path)
        .collect();
    Some(render_ctx_for_plans(res, &plans))
}

pub fn render(res: &CompileRes, target: &RenderTarget, templates: &[TemplateUnit]) -> RenderRes {
    let mut errors = Vec::new();
    let mut out = Vec::new();

    if res.plans.is_empty() {
        return RenderRes { units: out, errors };
    }

    let manifest_name = format!("main.{}.{}.tera", target.backend, target.out_type);

    let mut groups: BTreeMap<Vec<String>, Vec<&PlanRoot>> = BTreeMap::new();
    for p in &res.plans {
        groups.entry(p.pack_path.clone()).or_default().push(p);
    }

    for (pack, plans) in &groups {
        let pack_templates: Vec<&TemplateUnit> = templates
            .iter()
            .filter(|t| &t.path == pack && t.file == manifest_name)
            .collect();

        if pack_templates.is_empty() {
            errors.push(plan_pack_error(
                format!(
                    "no \"{manifest_name}\" found in pack \"{}\" (required by plan(s): {})",
                    pack_display(pack),
                    plans
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                plans,
            ));
            continue;
        }
        let manifest_template = pack_templates[0];

        let units = collect_all_templates(templates);

        let ctx = render_ctx_for_plans(res, plans);

        match gen_render(
            &RenderReq {
                entry: manifest_template.id.clone(),
                units,
                pretty_print: target.out_type == "json",
            },
            &ctx,
        ) {
            Ok(rendered) => {
                for u in rendered {
                    let Some(plan) = u.attrs.get("plan").and_then(Value::as_str) else {
                        errors.push(plan_pack_error(
                            format!(
                                "render error in pack \"{}\": manifest file entry for \"{}\" is missing string field \"plan\"",
                                pack_display(pack),
                                u.file,
                            ),
                            plans,
                        ));
                        continue;
                    };
                    if !plans.iter().any(|p| p.name == plan) {
                        errors.push(plan_pack_error(
                            format!(
                                "render error in pack \"{}\": manifest file entry for \"{}\" refers to unknown plan \"{}\"",
                                pack_display(pack),
                                u.file,
                                plan,
                            ),
                            plans,
                        ));
                        continue;
                    }
                    if u.content.trim().is_empty() {
                        continue;
                    }
                    out.push(RenderUnit {
                        backend: target.backend.clone(),
                        out_type: target.out_type.clone(),
                        plan: plan.into(),
                        pack_path: pack.clone(),
                        file: u.file,
                        content: u.content,
                    });
                }
            }
            Err(e) => errors.push(plan_pack_error(
                format_render_error(&pack_display(pack), &e),
                plans,
            )),
        }
    }

    RenderRes { units: out, errors }
}

fn render_ctx_for_plans(res: &CompileRes, plans: &[&PlanRoot]) -> Value {
    let defs_json: Vec<Value> = plans
        .iter()
        .map(|p| def_to_json(&res.defs[p.def_idx], &res.defs))
        .collect();

    let plans_json: Vec<Value> = plans
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "pack": p.pack_path,
            })
        })
        .collect();

    json!({
        "defs": defs_json,
        "plans": plans_json,
    })
}

fn collect_all_templates(templates: &[TemplateUnit]) -> Vec<TeraUnit> {
    templates
        .iter()
        .map(|t| TeraUnit {
            file: t.id.clone(),
            template: t.content.clone(),
        })
        .collect()
}

fn plan_pack_error(message: String, plans: &[&PlanRoot]) -> CompileError {
    let loc = plans.first().and_then(|p| {
        p.unit.map(|unit| ErrorLoc {
            unit,
            line: 1,
            col: 1,
            in_ts: false,
        })
    });
    CompileError { message, loc }
}

fn format_render_error(pack: &str, e: &GenError) -> String {
    format!("render error in pack \"{pack}\": {e}")
}

fn pack_display(pack: &[String]) -> String {
    if pack.is_empty() {
        "<root>".into()
    } else {
        pack.join(":")
    }
}

fn def_to_json(def: &AsmDef, defs: &[AsmDef]) -> Value {
    let mut seen = HashSet::new();
    def_to_json_inner(def, defs, &mut seen)
}

fn def_to_json_inner(def: &AsmDef, defs: &[AsmDef], seen: &mut HashSet<(String, String)>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("type_name".into(), Value::String(def.type_name.clone()));
    obj.insert("name".into(), Value::String(def.name.clone()));
    obj.insert(
        "type_hint".into(),
        def.type_hint
            .as_ref()
            .map(|v| Value::String(v.clone()))
            .unwrap_or(Value::Null),
    );
    obj.insert(
        "as_arr".into(),
        Value::Array(fields_to_json(&def.fields, defs, seen)),
    );
    obj.insert("as_obj".into(), fields_to_obj(&def.fields, defs, seen));

    Value::Object(obj)
}

fn fields_to_json(
    fields: &[AsmField],
    defs: &[AsmDef],
    seen: &mut HashSet<(String, String)>,
) -> Vec<Value> {
    fields
        .iter()
        .map(|f| {
            json!({
                "name": f.name,
                "value": value_to_json(&f.value, defs, seen),
            })
        })
        .collect()
}

fn fields_to_obj(
    fields: &[AsmField],
    defs: &[AsmDef],
    seen: &mut HashSet<(String, String)>,
) -> Value {
    let mut obj = serde_json::Map::new();
    for field in fields {
        let value = value_to_json(&field.value, defs, seen);
        obj.insert(field.name.clone(), value);
    }
    Value::Object(obj)
}

fn value_to_json(
    v: &crate::asm::AsmValue,
    defs: &[AsmDef],
    seen: &mut HashSet<(String, String)>,
) -> Value {
    match v {
        crate::asm::AsmValue::DefRef(r) => {
            let key = (r.type_name.clone(), r.name.clone());
            if seen.contains(&key) {
                return asm_value_to_json(v);
            }
            let Some(target) = defs
                .iter()
                .filter(|d| d.type_name == r.type_name && d.name == r.name)
                .max_by_key(|d| d.fields.len())
            else {
                return asm_value_to_json(v);
            };
            seen.insert(key.clone());
            let out = def_to_json_inner(target, defs, seen);
            seen.remove(&key);
            out
        }
        crate::asm::AsmValue::Def(d) => {
            if d.name == "_" && d.type_name.is_empty() {
                fields_to_obj(&d.fields, defs, seen)
            } else {
                def_to_json_inner(d, defs, seen)
            }
        }
        crate::asm::AsmValue::List(items) => list_to_json(items, defs, seen),
        crate::asm::AsmValue::Path(items) => Value::Array(
            items
                .iter()
                .map(|item| value_to_json(item, defs, seen))
                .collect(),
        ),
        crate::asm::AsmValue::Tuple(items) => tuple_to_json(items, defs, seen),
        _ => asm_value_to_json(v),
    }
}

fn list_to_json(
    items: &[crate::asm::AsmValue],
    defs: &[AsmDef],
    seen: &mut HashSet<(String, String)>,
) -> Value {
    if let Some(obj) = tuple_pair_list_to_obj(items, defs, seen) {
        return Value::Object(obj);
    }
    Value::Array(
        items
            .iter()
            .map(|item| value_to_json(item, defs, seen))
            .collect(),
    )
}

fn tuple_pair_list_to_obj(
    items: &[crate::asm::AsmValue],
    defs: &[AsmDef],
    seen: &mut HashSet<(String, String)>,
) -> Option<serde_json::Map<String, Value>> {
    let mut obj = serde_json::Map::new();
    for item in items {
        let crate::asm::AsmValue::Tuple(parts) = item else {
            return None;
        };
        if parts.len() != 2 {
            return None;
        }
        let Value::String(key) = value_to_json(&parts[0], defs, seen) else {
            return None;
        };
        obj.insert(key, value_to_json(&parts[1], defs, seen));
    }
    Some(obj)
}

fn tuple_to_json(
    items: &[crate::asm::AsmValue],
    defs: &[AsmDef],
    seen: &mut HashSet<(String, String)>,
) -> Value {
    let arr: Vec<Value> = items
        .iter()
        .map(|item| value_to_json(item, defs, seen))
        .collect();
    let mut obj = serde_json::Map::new();
    obj.insert("as_arr".into(), Value::Array(arr.clone()));
    for (idx, item) in arr.into_iter().enumerate() {
        obj.insert(format!("v{idx}"), item);
    }
    Value::Object(obj)
}
