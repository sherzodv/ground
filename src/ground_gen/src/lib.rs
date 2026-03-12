use serde_json::Value;
use tera::Tera;

#[derive(Debug)]
pub enum GenError {
    Render { cause: String },
    Merge  { cause: String },
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenError::Render { cause } => write!(f, "template render error: {cause}"),
            GenError::Merge  { cause } => write!(f, "merge error: {cause}"),
        }
    }
}

impl std::error::Error for GenError {}

pub fn render(template: &str, ctx: &Value) -> Result<String, GenError> {
    let mut tera = Tera::default();
    tera.add_raw_template("t", template)
        .map_err(|e| GenError::Render { cause: e.to_string() })?;
    let tera_ctx = tera::Context::from_value(ctx.clone())
        .map_err(|e| GenError::Render { cause: e.to_string() })?;
    tera.render("t", &tera_ctx)
        .map_err(|e| GenError::Render { cause: e.to_string() })
}

pub fn merge_json(frags: Vec<String>) -> Result<String, GenError> {
    let mut acc = serde_json::json!({});
    for frag in frags {
        let trimmed = frag.trim();
        if trimmed.is_empty() { continue; }
        let v: Value = serde_json::from_str(trimmed)
            .map_err(|e| GenError::Merge { cause: format!("{e}: {trimmed}") })?;
        deep_merge(&mut acc, v);
    }
    serde_json::to_string_pretty(&acc)
        .map_err(|e| GenError::Merge { cause: e.to_string() })
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o { deep_merge(b.entry(k).or_insert(Value::Null), v); }
        }
        (base, overlay) => *base = overlay,
    }
}
