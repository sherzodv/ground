use std::io;
use std::path::Path;

use serde_json::{json, Value};
use url::Url;

#[derive(Clone, Copy)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

pub fn text_doc_uri_and_pos(params: &Value) -> Option<(String, Position)> {
    let uri = params.get("textDocument")?.get("uri")?.as_str()?.to_string();
    let pos = params.get("position")?;
    Some((uri, Position {
        line: pos.get("line")?.as_u64()? as u32,
        character: pos.get("character")?.as_u64()? as u32,
    }))
}

pub fn line_prefix(src: &str, line: usize, col: usize) -> String {
    src.lines().nth(line).unwrap_or("").chars().take(col).collect()
}

pub fn token_at(src: &str, line: usize, col: usize) -> Option<String> {
    let line_str = src.lines().nth(line)?;
    let chars: Vec<char> = line_str.chars().collect();
    if col > chars.len() { return None; }
    let is_tok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '*' );
    let mut start = col.min(chars.len());
    while start > 0 && is_tok(chars[start - 1]) { start -= 1; }
    let mut end = col.min(chars.len());
    while end < chars.len() && is_tok(chars[end]) { end += 1; }
    (start < end).then(|| chars[start..end].iter().collect())
}

pub fn range_from_offsets(src: &str, start: usize, end: usize) -> Value {
    let (sl, sc) = offset_to_line_col(src, start);
    let (el, ec) = offset_to_line_col(src, end);
    json!({
        "start": { "line": sl as u32, "character": sc as u32 },
        "end": { "line": el as u32, "character": ec as u32 },
    })
}

pub fn offset_to_line_col(src: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(src.len());
    let before = &src[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count();
    let col = before.rfind('\n').map_or(offset, |p| offset - p - 1);
    (line, col)
}

pub fn file_uri(path: &Path) -> io::Result<String> {
    Url::from_file_path(path)
        .map(|u| u.to_string())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid file path {}", path.display())))
}

pub fn completion_item(label: &str, kind: u32) -> Value {
    json!({ "label": label, "kind": kind })
}

pub fn dedupe_completion_items(items: Vec<Value>) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for item in items {
        let Some(label) = item.get("label").and_then(Value::as_str) else { continue; };
        if seen.insert(label.to_string()) {
            out.push(item);
        }
    }
    out
}
