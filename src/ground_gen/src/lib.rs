use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tera::{Filter, Tera};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeraUnit {
    pub file: String,
    pub template: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonUnit {
    pub file: String,
    pub content: String,
    pub attrs: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderReq {
    pub entry: String,
    pub units: Vec<TeraUnit>,
    pub pretty_print: bool,
}

#[derive(Debug)]
pub enum GenError {
    Render { cause: String },
    Manifest { cause: String },
    Merge { cause: String },
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenError::Render { cause } => write!(f, "template render error: {cause}"),
            GenError::Manifest { cause } => write!(f, "manifest error: {cause}"),
            GenError::Merge { cause } => write!(f, "merge error: {cause}"),
        }
    }
}

impl std::error::Error for GenError {}

pub fn render(req: &RenderReq, ctx: &Value) -> Result<Vec<JsonUnit>, GenError> {
    let mut tera = Tera::default();
    tera.register_filter("skip", SkipFilter);
    tera.register_filter("pick", PickFilter);
    tera.add_raw_templates(
        req.units
            .iter()
            .map(|u| (u.file.as_str(), u.template.as_str()))
            .collect::<Vec<_>>(),
    )
    .map_err(|e| GenError::Render {
        cause: e.to_string(),
    })?;
    let tera_ctx = tera::Context::from_value(ctx.clone()).map_err(|e| GenError::Render {
        cause: e.to_string(),
    })?;

    let manifest = tera
        .render(&req.entry, &tera_ctx)
        .map_err(|e| GenError::Render {
            cause: format!("{e:?}"),
        })?;
    let manifest_value: Value =
        serde_json::from_str(&manifest).map_err(|e| GenError::Manifest {
            cause: format!("{e}: {manifest}"),
        })?;
    let files = manifest_value
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| GenError::Manifest {
            cause: "missing 'files' array".into(),
        })?;

    files
        .iter()
        .map(|file| {
            let attrs = file
                .as_object()
                .cloned()
                .ok_or_else(|| GenError::Manifest {
                    cause: "file entry must be an object".into(),
                })?;
            let path =
                file.get("file")
                    .and_then(Value::as_str)
                    .ok_or_else(|| GenError::Manifest {
                        cause: "file entry missing string 'file'".into(),
                    })?;
            let template = file
                .get("template")
                .and_then(Value::as_str)
                .ok_or_else(|| GenError::Manifest {
                    cause: "file entry missing string 'template'".into(),
                })?;
            let content = tera
                .render(template, &tera_ctx)
                .map_err(|e| GenError::Render {
                    cause: format!("{e:?}"),
                })?;
            let content = if req.pretty_print && path.ends_with(".json") {
                let value: Value =
                    serde_json::from_str(&content).map_err(|e| GenError::Render {
                        cause: format!("pretty-print json parse error: {e}: {content}"),
                    })?;
                serde_json::to_string_pretty(&value).map_err(|e| GenError::Render {
                    cause: format!("pretty-print json encode error: {e}"),
                })?
            } else {
                content
            };
            Ok(JsonUnit {
                file: path.into(),
                content,
                attrs,
            })
        })
        .collect()
}

pub fn merge_json(frags: Vec<String>) -> Result<String, GenError> {
    let mut acc = serde_json::json!({});
    for frag in frags {
        let trimmed = frag.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(trimmed).map_err(|e| GenError::Merge {
            cause: format!("{e}: {trimmed}"),
        })?;
        deep_merge(&mut acc, v);
    }
    serde_json::to_string_pretty(&acc).map_err(|e| GenError::Merge {
        cause: e.to_string(),
    })
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                deep_merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (base, overlay) => *base = overlay,
    }
}

struct SkipFilter;
struct PickFilter;

impl Filter for SkipFilter {
    fn filter(&self, value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
        filter_fields(value, args, false)
    }
}

impl Filter for PickFilter {
    fn filter(&self, value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
        filter_fields(value, args, true)
    }
}

fn filter_fields(value: &Value, args: &HashMap<String, Value>, keep: bool) -> tera::Result<Value> {
    let names = filter_names(args)?;
    let names: HashSet<&str> = names.iter().map(String::as_str).collect();
    let arr = field_array(value)?;
    let out = arr
        .iter()
        .filter_map(|item| {
            let name = item.get("name").and_then(Value::as_str)?;
            let contains = names.contains(name);
            if contains == keep {
                Some(item.clone())
            } else {
                None
            }
        })
        .collect();
    Ok(Value::Array(out))
}

fn filter_names(args: &HashMap<String, Value>) -> tera::Result<Vec<String>> {
    let Some(raw) = args.get("names") else {
        return Err("missing filter arg `names`".into());
    };
    let Some(arr) = raw.as_array() else {
        return Err("filter arg `names` must be an array of strings".into());
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(name) = item.as_str() else {
            return Err("filter arg `names` must be an array of strings".into());
        };
        out.push(name.to_string());
    }
    Ok(out)
}

fn field_array(value: &Value) -> tera::Result<&Vec<Value>> {
    if let Some(arr) = value.as_array() {
        return Ok(arr);
    }
    value
        .get("as_arr")
        .and_then(Value::as_array)
        .ok_or_else(|| "filter input must be a field array or object with `as_arr`".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn render_manifest_drives_file_templates() {
        let req = RenderReq {
            entry: "manifest.json.tera".into(),
            units: vec![
                TeraUnit {
                    file: "manifest.json.tera".into(),
                    template: r#"{
  "files": [
    { "file": "services/{{ deploy.name }}/summary.txt", "template": "summary.txt.tera" },
    { "file": "services/{{ deploy.name }}/values.txt", "template": "values.txt.tera" }
  ]
}"#.into(),
                },
                TeraUnit {
                    file: "_shared.tera".into(),
                    template: r#"
{% macro summary(deploy) -%}
name={{ deploy.name }}
enabled={{ deploy.enabled }}
{%- endmacro summary %}

{% macro values(deploy) -%}
port={{ deploy.port }}
region={{ deploy.region }}
{%- endmacro values %}
"#.into(),
                },
                TeraUnit {
                    file: "summary.txt.tera".into(),
                    template: r#"{% import "_shared.tera" as shared -%}{{ shared::summary(deploy=deploy) }}"#.into(),
                },
                TeraUnit {
                    file: "values.txt.tera".into(),
                    template: r#"{% import "_shared.tera" as shared -%}{{ shared::values(deploy=deploy) }}"#.into(),
                },
            ],
            pretty_print: false,
        };

        let out = render(
            &req,
            &json!({
                "deploy": {
                    "name": "api",
                    "enabled": true,
                    "port": 8080,
                    "region": "eu-central-1"
                }
            }),
        )
        .unwrap();

        assert_eq!(
            out,
            vec![
                JsonUnit {
                    file: "services/api/summary.txt".into(),
                    content: "name=api\nenabled=true".into(),
                    attrs: serde_json::from_value(json!({
                        "file": "services/api/summary.txt",
                        "template": "summary.txt.tera"
                    }))
                    .unwrap(),
                },
                JsonUnit {
                    file: "services/api/values.txt".into(),
                    content: "port=8080\nregion=eu-central-1".into(),
                    attrs: serde_json::from_value(json!({
                        "file": "services/api/values.txt",
                        "template": "values.txt.tera"
                    }))
                    .unwrap(),
                },
            ]
        );
    }

    #[test]
    fn render_pretty_prints_json_when_requested() {
        let req = RenderReq {
            entry: "manifest.json.tera".into(),
            units: vec![
                TeraUnit {
                    file: "manifest.json.tera".into(),
                    template:
                        r#"{ "files": [ { "file": "main.json", "template": "main.json.tera" } ] }"#
                            .into(),
                },
                TeraUnit {
                    file: "main.json.tera".into(),
                    template: r#"{"b":1,"a":{"x":true}}"#.into(),
                },
            ],
            pretty_print: true,
        };

        let out = render(&req, &json!({})).unwrap();
        assert_eq!(out.len(), 1);
        let actual: Value = serde_json::from_str(&out[0].content).unwrap();
        assert_eq!(actual, json!({ "b": 1, "a": { "x": true } }));
    }

    #[test]
    fn render_supports_colon_template_names() {
        let req = RenderReq {
            entry: "test:main.tf.json.tera".into(),
            units: vec![
                TeraUnit {
                    file: "test:main.tf.json.tera".into(),
                    template: r#"{ "files": [ { "file": "main.tf.json", "template": "std:aws:tf:vpc.tf.json.tera" } ] }"#.into(),
                },
                TeraUnit {
                    file: "std:aws:tf:vpc.tf.json.tera".into(),
                    template: r#"{"ok":true}"#.into(),
                },
            ],
            pretty_print: true,
        };

        let out = render(&req, &json!({})).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file, "main.tf.json");
        assert_eq!(out[0].content, "{\n  \"ok\": true\n}");
    }

    #[test]
    fn render_skip_filter_accepts_def_object() {
        let req = RenderReq {
            entry: "manifest.json.tera".into(),
            units: vec![
                TeraUnit {
                    file: "manifest.json.tera".into(),
                    template:
                        r#"{ "files": [ { "file": "main.txt", "template": "main.txt.tera" } ] }"#
                            .into(),
                },
                TeraUnit {
                    file: "main.txt.tera".into(),
                    template: r#"{% for field in deploy | skip(names=["vpc"]) -%}{{ field.name }}{% if not loop.last %},{% endif %}{%- endfor %}"#.into(),
                },
            ],
            pretty_print: false,
        };

        let out = render(
            &req,
            &json!({
                "deploy": {
                    "name": "app",
                    "as_arr": [
                        { "name": "vpc", "value": "main" },
                        { "name": "cidr_block", "value": "10.0.1.0/24" },
                        { "name": "tags", "value": { "Name": "app" } }
                    ]
                }
            }),
        )
        .unwrap();

        assert_eq!(out[0].content, "cidr_block,tags");
    }

    #[test]
    fn render_pick_filter_accepts_array_input() {
        let req = RenderReq {
            entry: "manifest.json.tera".into(),
            units: vec![
                TeraUnit {
                    file: "manifest.json.tera".into(),
                    template:
                        r#"{ "files": [ { "file": "main.txt", "template": "main.txt.tera" } ] }"#
                            .into(),
                },
                TeraUnit {
                    file: "main.txt.tera".into(),
                    template: r#"{% for field in fields | pick(names=["tags"]) -%}{{ field.value.Name }}{%- endfor %}"#.into(),
                },
            ],
            pretty_print: false,
        };

        let out = render(
            &req,
            &json!({
                "fields": [
                    { "name": "cidr_block", "value": "10.0.1.0/24" },
                    { "name": "tags", "value": { "Name": "app" } }
                ]
            }),
        )
        .unwrap();

        assert_eq!(out[0].content, "app");
    }
}
