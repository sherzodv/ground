use serde_json::{json, Value};

use crate::util::{text_doc_uri_and_pos, token_at};
use crate::workspace::{
    definition_from_ts, resolve_ground_token, resolve_mapper_token, Workspace,
};

pub fn definition(workspace: &Workspace, params: &Value) -> Option<Value> {
    let (uri, pos) = text_doc_uri_and_pos(params)?;
    let analysis = workspace.analysis.as_ref()?;
    if uri.ends_with(".ts") {
        return definition_from_ts(analysis, &uri, pos).map(|locs| json!(locs));
    }
    let text = workspace.text_for_uri(&uri)?;
    let token = token_at(text, pos.line as usize, pos.character as usize)?;
    let (_, scope) = workspace.analysis_and_scope_for_uri(&uri)?;

    if let Some(loc) = resolve_ground_token(analysis, scope, &token) {
        return Some(json!([loc]));
    }
    if let Some(loc) = resolve_mapper_token(analysis, scope, &token) {
        return Some(json!([loc]));
    }
    None
}
