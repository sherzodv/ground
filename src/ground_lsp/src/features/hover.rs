use serde_json::{json, Value};

use crate::util::{text_doc_uri_and_pos, token_at};
use crate::workspace::{describe_ground_token, Workspace};
use ground_compile::ir::IrDef;

pub fn hover(workspace: &Workspace, params: &Value) -> Option<Value> {
    let (uri, pos) = text_doc_uri_and_pos(params)?;
    let analysis = workspace.analysis.as_ref()?;
    if uri.ends_with(".ts") {
        let token = token_at(workspace.text_for_uri(&uri)?, pos.line as usize, pos.character as usize)?;
        let defs: Vec<&IrDef> = analysis.res.ir.defs.iter().filter(|d| d.mapper_fn.as_deref() == Some(token.as_str())).collect();
        if defs.is_empty() { return None; }
        let value = defs.iter().map(|d| format!("`{}` mapper for Ground def `{}`", token, d.name)).collect::<Vec<_>>().join("\n");
        return Some(json!({ "contents": { "kind": "markdown", "value": value } }));
    }
    let text = workspace.text_for_uri(&uri)?;
    let token = token_at(text, pos.line as usize, pos.character as usize)?;
    let (_, scope) = workspace.analysis_and_scope_for_uri(&uri)?;
    if let Some((kind, body)) = describe_ground_token(analysis, scope, &token) {
        return Some(json!({ "contents": { "kind": "markdown", "value": format!("**{}**\n\n{}", kind, body) } }));
    }
    None
}
