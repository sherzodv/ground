use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ground_compile::{
    analyze,
    ir::{DefId, IrLoc, IrPrimitive, IrRes, IrShapeBody, ScopeId},
    AnalysisRes, CompileError, CompileReq, ErrorLoc, Unit, UnitId,
};
use serde_json::{json, Value};

use crate::util::{file_uri, range_from_offsets, token_at, Position};

#[derive(Clone)]
pub struct UserFile {
    pub unit_id: UnitId,
    pub uri: String,
    pub pack_path: Vec<String>,
    pub name: String,
    pub grd_src: String,
    pub ts_uri: String,
    pub ts_src: Option<String>,
}

#[derive(Clone)]
pub struct TsFn {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

pub struct WorkspaceAnalysis {
    pub files: Vec<UserFile>,
    pub res: AnalysisRes,
    pub ts_scope_to_uri: HashMap<u32, String>,
    pub ts_functions_by_uri: HashMap<String, Vec<TsFn>>,
}

pub struct Workspace {
    pub root: PathBuf,
    pub open_docs: HashMap<String, String>,
    pub analysis: Option<WorkspaceAnalysis>,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            open_docs: HashMap::new(),
            analysis: None,
        }
    }

    pub fn did_open(&mut self, params: &Value) {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|v| v.get("uri"))
            .and_then(Value::as_str)
        else {
            return;
        };
        let Some(text) = params
            .get("textDocument")
            .and_then(|v| v.get("text"))
            .and_then(Value::as_str)
        else {
            return;
        };
        self.open_docs.insert(uri.to_string(), text.to_string());
    }

    pub fn did_change(&mut self, params: &Value) {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|v| v.get("uri"))
            .and_then(Value::as_str)
        else {
            return;
        };
        let Some(text) = params
            .get("contentChanges")
            .and_then(Value::as_array)
            .and_then(|v| v.last())
            .and_then(|v| v.get("text"))
            .and_then(Value::as_str)
        else {
            return;
        };
        self.open_docs.insert(uri.to_string(), text.to_string());
    }

    pub fn did_close(&mut self, params: &Value) {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|v| v.get("uri"))
            .and_then(Value::as_str)
        else {
            return;
        };
        self.open_docs.remove(uri);
    }

    pub fn reload(&mut self) -> io::Result<()> {
        let mut files = collect_user_files(&self.root, &self.open_docs)?;
        let units: Vec<Unit> = files
            .iter()
            .map(|f| Unit {
                name: f.name.clone(),
                path: f.pack_path.clone(),
                src: f.grd_src.clone(),
                ts_src: f.ts_src.clone(),
            })
            .collect();
        let res = analyze(CompileReq { units });
        for (file, unit_id) in files.iter_mut().zip(res.units.iter().copied()) {
            file.unit_id = unit_id;
        }
        let mut ts_scope_to_uri = HashMap::new();
        let mut ts_functions_by_uri = HashMap::new();
        for file in &files {
            if let Some(unit) = res.parse.units.get(file.unit_id.as_usize()) {
                ts_scope_to_uri.insert(unit.scope_id.0, file.ts_uri.clone());
            }
            if let Some(ts_src) = &file.ts_src {
                ts_functions_by_uri.insert(file.ts_uri.clone(), scan_ts_functions(ts_src));
            }
        }
        self.analysis = Some(WorkspaceAnalysis {
            files,
            res,
            ts_scope_to_uri,
            ts_functions_by_uri,
        });
        Ok(())
    }

    pub fn diagnostics_by_uri(&self) -> BTreeMap<String, Vec<Value>> {
        let mut out: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let Some(analysis) = &self.analysis else {
            return out;
        };
        for file in &analysis.files {
            out.entry(file.uri.clone()).or_default();
            if file.ts_src.is_some() {
                out.entry(file.ts_uri.clone()).or_default();
            }
        }
        for err in &analysis.res.errors {
            let Some(uri) = error_uri(analysis, err.loc.as_ref()) else {
                continue;
            };
            out.entry(uri).or_default().push(diagnostic_from_error(err));
        }
        out
    }

    pub fn text_for_uri(&self, uri: &str) -> Option<&str> {
        if let Some(text) = self.open_docs.get(uri) {
            return Some(text.as_str());
        }
        let analysis = self.analysis.as_ref()?;
        for file in &analysis.files {
            if file.uri == uri {
                return Some(file.grd_src.as_str());
            }
            if file.ts_uri == uri {
                return file.ts_src.as_deref();
            }
        }
        None
    }

    pub fn analysis_and_scope_for_uri(&self, uri: &str) -> Option<(&WorkspaceAnalysis, ScopeId)> {
        let analysis = self.analysis.as_ref()?;
        for file in &analysis.files {
            if file.uri == uri || file.ts_uri == uri {
                let scope = analysis
                    .res
                    .parse
                    .units
                    .get(file.unit_id.as_usize())?
                    .scope_id;
                return Some((analysis, ScopeId(scope.0)));
            }
        }
        None
    }

    pub fn analysis_scope_and_unit_for_uri(
        &self,
        uri: &str,
    ) -> Option<(&WorkspaceAnalysis, ScopeId, UnitId)> {
        let analysis = self.analysis.as_ref()?;
        for file in &analysis.files {
            if file.uri == uri || file.ts_uri == uri {
                let scope = analysis
                    .res
                    .parse
                    .units
                    .get(file.unit_id.as_usize())?
                    .scope_id;
                return Some((analysis, ScopeId(scope.0), file.unit_id));
            }
        }
        None
    }
}

pub fn resolve_ground_token(
    analysis: &WorkspaceAnalysis,
    scope: ScopeId,
    token: &str,
) -> Option<Value> {
    let ir = &analysis.res.ir;
    if token.contains(':') {
        let parts: Vec<&str> = token.split(':').collect();
        if parts.is_empty() {
            return None;
        }
        if parts.last() == Some(&"*") {
            return None;
        }
        if parts.len() > 1 {
            let pack = lookup_pack_path(ir, scope, &parts[..parts.len() - 1])?;
            if let Some(def_id) = lookup_def_in_scope(ir, pack, parts[parts.len() - 1]) {
                return location_from_loc(analysis, ir.defs.get(def_id.0 as usize)?.loc.clone());
            }
            if let Some(shape_id) = lookup_shape_in_scope(ir, pack, parts[parts.len() - 1]) {
                return location_from_loc(
                    analysis,
                    ir.shapes.get(shape_id.0 as usize)?.loc.clone(),
                );
            }
            if let Some(pack_scope) = lookup_pack_in_scope(ir, pack, parts[parts.len() - 1]) {
                return location_from_scope(analysis, pack_scope);
            }
        }
    }
    if let Some(def_id) = lookup_def_visible(ir, scope, token) {
        return location_from_loc(analysis, ir.defs.get(def_id.0 as usize)?.loc.clone());
    }
    if let Some(shape_id) = lookup_shape_visible(ir, scope, token) {
        return location_from_loc(analysis, ir.shapes.get(shape_id.0 as usize)?.loc.clone());
    }
    if let Some(pack_scope) = lookup_pack_visible(ir, scope, token) {
        return location_from_scope(analysis, pack_scope);
    }
    None
}

pub fn resolve_mapper_token(
    analysis: &WorkspaceAnalysis,
    scope: ScopeId,
    token: &str,
) -> Option<Value> {
    let ir = &analysis.res.ir;
    let mut cur = Some(scope);
    while let Some(scope_id) = cur {
        if let Some(uri) = analysis.ts_scope_to_uri.get(&scope_id.0) {
            if let Some(fns) = analysis.ts_functions_by_uri.get(uri) {
                if let Some(ts_fn) = fns.iter().find(|f| f.name == token) {
                    let src = text_for_analysis_uri(analysis, uri)?;
                    return Some(json!({
                        "uri": uri,
                        "range": range_from_offsets(src, ts_fn.start, ts_fn.end),
                    }));
                }
            }
        }
        cur = ir.scopes.get(scope_id.0 as usize).and_then(|s| s.parent);
    }
    None
}

pub fn describe_ground_token<'a>(
    analysis: &'a WorkspaceAnalysis,
    scope: ScopeId,
    token: &str,
) -> Option<(&'static str, String)> {
    let ir = &analysis.res.ir;
    if let Some(def_id) = lookup_def_visible(ir, scope, token) {
        let def = ir.defs.get(def_id.0 as usize)?;
        let shape = ir.shapes.get(def.shape_id.0 as usize)?;
        let body = match &shape.body {
            IrShapeBody::Unit => "unit".to_string(),
            IrShapeBody::Primitive(p) => format!("primitive `{}`", primitive_name(p)),
            IrShapeBody::Enum(vs) => format!("enum with {} variants", vs.len()),
            IrShapeBody::Struct(fs) => format!("struct with {} fields", fs.len()),
            IrShapeBody::Tuple(items) => format!("tuple with {} items", items.len()),
        };
        return Some(("Def", format!("`{}`\n\n{}", def.name, body)));
    }
    if let Some(shape_id) = lookup_shape_visible(ir, scope, token) {
        let shape = ir.shapes.get(shape_id.0 as usize)?;
        let body = match &shape.body {
            IrShapeBody::Unit => "unit".to_string(),
            IrShapeBody::Primitive(p) => format!("primitive `{}`", primitive_name(p)),
            IrShapeBody::Enum(vs) => format!("enum with {} variants", vs.len()),
            IrShapeBody::Struct(fs) => format!("struct with {} fields", fs.len()),
            IrShapeBody::Tuple(items) => format!("tuple with {} items", items.len()),
        };
        return Some(("Shape", body));
    }
    if lookup_pack_visible(ir, scope, token).is_some() {
        return Some(("Pack", format!("`{}`", token)));
    }
    None
}

fn primitive_name(p: &IrPrimitive) -> &'static str {
    match p {
        IrPrimitive::String => "string",
        IrPrimitive::Integer => "integer",
        IrPrimitive::Boolean => "boolean",
        IrPrimitive::Reference => "reference",
        IrPrimitive::Ipv4 => "ipv4",
        IrPrimitive::Ipv4Net => "ipv4net",
    }
}

pub fn definition_from_ts(
    analysis: &WorkspaceAnalysis,
    uri: &str,
    pos: Position,
) -> Option<Vec<Value>> {
    let src = text_for_analysis_uri(analysis, uri)?;
    let token = token_at(src, pos.line as usize, pos.character as usize)?;
    let defs: Vec<Value> = analysis
        .res
        .ir
        .defs
        .iter()
        .filter(|d| d.mapper_fn.as_deref() == Some(token.as_str()))
        .filter_map(|d| location_from_loc(analysis, d.loc.clone()))
        .collect();
    (!defs.is_empty()).then_some(defs)
}

pub fn lookup_pack_path(ir: &IrRes, scope: ScopeId, parts: &[&str]) -> Option<ScopeId> {
    if parts.is_empty() {
        return Some(scope);
    }
    let mut cur = lookup_pack_visible(ir, scope, parts[0])?;
    for part in &parts[1..] {
        cur = lookup_pack_in_scope(ir, cur, part)?;
    }
    Some(cur)
}

pub fn lookup_pack_visible(ir: &IrRes, mut scope: ScopeId, name: &str) -> Option<ScopeId> {
    loop {
        let sc = ir.scopes.get(scope.0 as usize)?;
        if let Some(found) = sc.packs.get(name) {
            return Some(*found);
        }
        let Some(parent) = sc.parent else {
            return None;
        };
        scope = parent;
    }
}

pub fn lookup_def_visible(ir: &IrRes, mut scope: ScopeId, name: &str) -> Option<DefId> {
    loop {
        let sc = ir.scopes.get(scope.0 as usize)?;
        if let Some(found) = sc.defs.get(name).and_then(|v| v.first()).copied() {
            return Some(found);
        }
        let Some(parent) = sc.parent else {
            return None;
        };
        scope = parent;
    }
}

pub fn lookup_shape_visible(
    ir: &IrRes,
    mut scope: ScopeId,
    name: &str,
) -> Option<ground_compile::ir::ShapeId> {
    loop {
        let sc = ir.scopes.get(scope.0 as usize)?;
        if let Some(found) = sc.shapes.get(name) {
            return Some(*found);
        }
        let Some(parent) = sc.parent else {
            return None;
        };
        scope = parent;
    }
}

fn lookup_pack_in_scope(ir: &IrRes, scope: ScopeId, name: &str) -> Option<ScopeId> {
    ir.scopes.get(scope.0 as usize)?.packs.get(name).copied()
}

fn lookup_def_in_scope(ir: &IrRes, scope: ScopeId, name: &str) -> Option<DefId> {
    ir.scopes
        .get(scope.0 as usize)?
        .defs
        .get(name)
        .and_then(|v| v.first())
        .copied()
}

fn lookup_shape_in_scope(
    ir: &IrRes,
    scope: ScopeId,
    name: &str,
) -> Option<ground_compile::ir::ShapeId> {
    ir.scopes.get(scope.0 as usize)?.shapes.get(name).copied()
}

pub fn scan_ts_functions(src: &str) -> Vec<TsFn> {
    let mut out = vec![];
    let mut search_from = 0usize;
    for needle in ["export function ", "function "] {
        while let Some(idx) = src[search_from..].find(needle) {
            let start = search_from + idx;
            let name_start = start + needle.len();
            let rest = &src[name_start..];
            let name_len = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .count();
            if name_len == 0 {
                search_from = name_start;
                continue;
            }
            let name = rest[..name_len].to_string();
            out.push(TsFn {
                name,
                start,
                end: name_start + name_len,
            });
            search_from = name_start + name_len;
        }
    }
    out.sort_by_key(|f| f.start);
    out.dedup_by(|a, b| a.name == b.name && a.start == b.start && a.end == b.end);
    out
}

fn collect_user_files(
    root: &Path,
    overrides: &HashMap<String, String>,
) -> io::Result<Vec<UserFile>> {
    let mut files = vec![];
    collect_user_files_recursive(root, root, overrides, &mut files)?;
    Ok(files)
}

fn collect_user_files_recursive(
    root: &Path,
    dir: &Path,
    overrides: &HashMap<String, String>,
    files: &mut Vec<UserFile>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let is_hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if path.is_dir() {
            if !is_hidden {
                collect_user_files_recursive(root, &path, overrides, files)?;
            }
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("grd") {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let pack_path: Vec<String> = rel
            .parent()
            .map(|p| {
                p.components()
                    .filter_map(|c| match c {
                        std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let name = if stem == "pack" {
            String::new()
        } else {
            stem.to_string()
        };
        let abs_path = fs::canonicalize(&path).unwrap_or(path.clone());
        let uri = file_uri(&abs_path)?;
        let grd_src = overrides
            .get(&uri)
            .cloned()
            .unwrap_or(fs::read_to_string(&path)?);
        let ts_path = path.with_extension("ts");
        let ts_abs = fs::canonicalize(&ts_path).unwrap_or(ts_path.clone());
        let ts_uri = file_uri(&ts_abs)?;
        let ts_src = if ts_path.exists() {
            Some(
                overrides
                    .get(&ts_uri)
                    .cloned()
                    .unwrap_or(fs::read_to_string(&ts_path)?),
            )
        } else {
            overrides.get(&ts_uri).cloned()
        };
        files.push(UserFile {
            unit_id: UnitId(0),
            uri,
            pack_path,
            name,
            grd_src,
            ts_uri,
            ts_src,
        });
    }
    Ok(())
}

pub fn diagnostic_from_error(err: &CompileError) -> Value {
    let range = match &err.loc {
        Some(loc) => {
            let line = loc.line.saturating_sub(1);
            let col = loc.col.saturating_sub(1);
            json!({
                "start": { "line": line, "character": col },
                "end": { "line": line, "character": col + 1 },
            })
        }
        None => json!({
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 1 },
        }),
    };
    json!({
        "range": range,
        "severity": 1,
        "source": "ground",
        "message": err.message,
    })
}

pub fn location_from_loc(analysis: &WorkspaceAnalysis, loc: IrLoc) -> Option<Value> {
    let uri = location_uri_for_loc(analysis, loc.clone())?;
    let src = text_for_analysis_uri(analysis, &uri)?;
    Some(json!({
        "uri": uri,
        "range": range_from_offsets(src, loc.start as usize, loc.end as usize),
    }))
}

fn location_from_scope(analysis: &WorkspaceAnalysis, scope: ScopeId) -> Option<Value> {
    let uri = analysis.ts_scope_to_uri.get(&scope.0)?.clone();
    let src = text_for_analysis_uri(analysis, &uri)?;
    Some(json!({
        "uri": uri,
        "range": range_from_offsets(src, 0, 0),
    }))
}

pub fn location_uri_for_loc(analysis: &WorkspaceAnalysis, loc: IrLoc) -> Option<String> {
    let file = analysis.files.iter().find(|f| f.unit_id == loc.unit)?;
    Some(file.uri.clone())
}

fn error_uri(analysis: &WorkspaceAnalysis, loc: Option<&ErrorLoc>) -> Option<String> {
    let loc = loc?;
    let file = analysis.files.iter().find(|f| f.unit_id == loc.unit)?;
    Some(if loc.in_ts {
        file.ts_uri.clone()
    } else {
        file.uri.clone()
    })
}

fn text_for_analysis_uri<'a>(analysis: &'a WorkspaceAnalysis, uri: &str) -> Option<&'a str> {
    for file in &analysis.files {
        if file.uri == uri {
            return Some(file.grd_src.as_str());
        }
        if file.ts_uri == uri {
            return file.ts_src.as_deref();
        }
    }
    None
}
