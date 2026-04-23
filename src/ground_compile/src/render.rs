use std::collections::BTreeMap;

use ground_gen::{render as gen_render, GenError, RenderReq, TeraUnit};
use serde_json::{json, Value};

use crate::asm::{asm_value_to_json, AsmDef, AsmField};
use crate::ir::{IrScope, ScopeId};
use crate::{CompileError, CompileRes, ErrorLoc, PlanRoot};

/// Template file addressable by pack path. Collected by the CLI from disk
/// (or by tests from fixtures) and passed to `render` alongside a target.
#[derive(Debug, Clone)]
pub struct TemplateUnit {
    pub path: Vec<String>,
    pub file: String,
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
        let pack_templates: Vec<&TemplateUnit> =
            templates.iter().filter(|t| &t.path == pack).collect();

        if !pack_templates.iter().any(|t| t.file == manifest_name) {
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

        let Some(plan_scope) = plans.first().map(|p| p.scope) else {
            continue;
        };
        let visible_packs = visible_pack_paths(&res.scopes, plan_scope);
        let units =
            match collect_visible_templates(templates, &visible_packs, pack, &manifest_name, plans)
            {
                Ok(units) => units,
                Err(err) => {
                    errors.push(err);
                    continue;
                }
            };

        let defs_json: Vec<Value> = plans
            .iter()
            .map(|p| def_to_json(&res.defs[p.def_idx]))
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

        let ctx = json!({
            "defs": defs_json,
            "plans": plans_json,
        });

        match gen_render(
            &RenderReq {
                entry: manifest_name.clone(),
                units,
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

fn collect_visible_templates(
    templates: &[TemplateUnit],
    visible_packs: &[Vec<String>],
    manifest_pack: &[String],
    manifest_name: &str,
    plans: &[&PlanRoot],
) -> Result<Vec<TeraUnit>, CompileError> {
    let mut by_file: BTreeMap<String, (&TemplateUnit, Vec<String>)> = BTreeMap::new();

    for pack in visible_packs {
        for template in templates.iter().filter(|t| &t.path == pack) {
            if template.file == manifest_name && template.path != manifest_pack {
                continue;
            }

            match by_file.get(&template.file) {
                Some((prev, prev_pack)) if prev.path != template.path => {
                    return Err(plan_pack_error(
                        format!(
                            "render error in pack \"{}\": helper template \"{}\" is ambiguous between packs \"{}\" and \"{}\"",
                            pack_display(manifest_pack),
                            template.file,
                            pack_display(prev_pack),
                            pack_display(&template.path),
                        ),
                        plans,
                    ));
                }
                Some(_) => {}
                None => {
                    by_file.insert(template.file.clone(), (template, template.path.clone()));
                }
            }
        }
    }

    Ok(by_file
        .into_values()
        .map(|(t, _)| TeraUnit {
            file: t.file.clone(),
            template: t.content.clone(),
        })
        .collect())
}

fn visible_pack_paths(scopes: &[IrScope], mut scope: ScopeId) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    loop {
        if let Some(path) = scope_pack_path(scopes, scope) {
            if seen.insert(path.clone()) {
                out.push(path);
            }
        }

        let Some(sc) = scopes.get(scope.0 as usize) else {
            break;
        };

        let mut imported: Vec<_> = sc.packs.values().copied().collect();
        imported.sort_by_key(|sid| sid.0);
        for imported_scope in imported {
            if let Some(path) = scope_pack_path(scopes, imported_scope) {
                if seen.insert(path.clone()) {
                    out.push(path);
                }
            }
        }

        let Some(parent) = sc.parent else {
            break;
        };
        scope = parent;
    }

    out
}

fn scope_pack_path(scopes: &[IrScope], mut scope: ScopeId) -> Option<Vec<String>> {
    let mut out = Vec::new();

    loop {
        let sc = scopes.get(scope.0 as usize)?;
        if matches!(sc.kind, crate::ir::ScopeKind::Pack) {
            if let Some(name) = &sc.name {
                out.push(name.clone());
            }
        }
        let Some(parent) = sc.parent else {
            break;
        };
        scope = parent;
    }

    out.reverse();
    Some(out)
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

fn def_to_json(def: &AsmDef) -> Value {
    json!({
        "type_name": def.type_name,
        "name": def.name,
        "type_hint": def.type_hint,
        "fields": fields_to_json(&def.fields),
    })
}

fn fields_to_json(fields: &[AsmField]) -> Vec<Value> {
    fields
        .iter()
        .map(|f| {
            json!({
                "name": f.name,
                "value": asm_value_to_json(&f.value),
            })
        })
        .collect()
}
