use serde_json::Value;
use tera::Tera;

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
}
