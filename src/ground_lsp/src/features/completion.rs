use serde_json::Value;

use crate::util::{completion_item, dedupe_completion_items, line_prefix, text_doc_uri_and_pos};
use crate::workspace::{lookup_pack_path, Workspace};
use ground_compile::ir::{IrRes, ScopeId};

pub fn completion(workspace: &Workspace, params: &Value) -> Vec<Value> {
    let Some((uri, pos)) = text_doc_uri_and_pos(params) else {
        return vec![];
    };
    let Some(text) = workspace.text_for_uri(&uri) else {
        return vec![];
    };
    if uri.ends_with(".ts") {
        return vec![];
    }
    let prefix_line = line_prefix(text, pos.line as usize, pos.character as usize);
    let mut items = vec![];
    for kw in ["def", "plan", "pack", "use", "via"] {
        items.push(completion_item(kw, 14));
    }
    for builtin in [
        "string",
        "integer",
        "boolean",
        "reference",
        "ipv4",
        "ipv4net",
    ] {
        items.push(completion_item(builtin, 7));
    }
    if let Some((analysis, scope)) = workspace.analysis_and_scope_for_uri(&uri) {
        if prefix_line.trim_start().starts_with("use") {
            items.extend(use_completion(
                &analysis.res.ir,
                scope,
                prefix_line.trim_start(),
            ));
        } else {
            for name in visible_names(&analysis.res.ir, scope) {
                items.push(completion_item(&name, 6));
            }
        }
    }
    dedupe_completion_items(items)
}

fn use_completion(ir: &IrRes, scope: ScopeId, trimmed_line: &str) -> Vec<Value> {
    let mut items = vec![];
    let after_use = trimmed_line.strip_prefix("use").unwrap_or("").trim_start();
    if after_use.is_empty() {
        items.push(completion_item("pack", 14));
        items.extend(pack_items(visible_pack_names(ir, scope), ""));
        return items;
    }

    let (path, force_pack) = if let Some(rest) = after_use.strip_prefix("pack:") {
        (rest, true)
    } else {
        (after_use, false)
    };

    let ends_with_colon = path.ends_with(':');
    let raw_segments: Vec<&str> = if path.is_empty() {
        vec![]
    } else {
        path.split(':').collect()
    };
    let (complete, current_prefix): (Vec<&str>, &str) = if ends_with_colon {
        (
            raw_segments.into_iter().filter(|s| !s.is_empty()).collect(),
            "",
        )
    } else if let Some((last, rest)) = raw_segments.split_last() {
        (
            rest.iter().copied().filter(|s| !s.is_empty()).collect(),
            last,
        )
    } else {
        (vec![], "")
    };

    let (pack_scope, pack_prefix_len) = longest_pack_prefix(ir, scope, &complete);
    let remaining = &complete[pack_prefix_len..];
    let def_mode = remaining.first() == Some(&"def");

    if complete.is_empty() && !force_pack {
        if "pack".starts_with(current_prefix) {
            items.push(completion_item("pack", 14));
        }
        items.extend(pack_items(visible_pack_names(ir, scope), current_prefix));
        return items;
    }

    let Some(pack_scope) = pack_scope else {
        items.extend(pack_items(visible_pack_names(ir, scope), current_prefix));
        return items;
    };

    if !def_mode && "def".starts_with(current_prefix) {
        items.push(completion_item("def", 14));
    }
    if "*".starts_with(current_prefix) {
        items.push(completion_item("*", 14));
    }
    items.extend(pack_items(child_pack_names(ir, pack_scope), current_prefix));
    items.extend(name_items(child_name_names(ir, pack_scope), current_prefix));
    items
}

fn longest_pack_prefix<'a>(
    ir: &IrRes,
    scope: ScopeId,
    complete: &'a [&'a str],
) -> (Option<ScopeId>, usize) {
    for len in (1..=complete.len()).rev() {
        if let Some(found) = lookup_pack_path(ir, scope, &complete[..len]) {
            return (Some(found), len);
        }
    }
    (None, 0)
}

fn visible_pack_names(ir: &IrRes, mut scope: ScopeId) -> Vec<String> {
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    loop {
        let Some(sc) = ir.scopes.get(scope.0 as usize) else {
            break;
        };
        for name in sc.packs.keys() {
            if seen.insert(name.clone()) {
                out.push(name.clone());
            }
        }
        let Some(parent) = sc.parent else {
            break;
        };
        scope = parent;
    }
    out.sort();
    out
}

fn child_pack_names(ir: &IrRes, scope: ScopeId) -> Vec<String> {
    let mut out: Vec<String> = ir
        .scopes
        .get(scope.0 as usize)
        .map(|s| s.packs.keys().cloned().collect())
        .unwrap_or_default();
    out.sort();
    out
}

fn child_name_names(ir: &IrRes, scope: ScopeId) -> Vec<String> {
    let mut out: Vec<String> = ir
        .scopes
        .get(scope.0 as usize)
        .map(|s| {
            let mut names: Vec<String> = s.defs.keys().chain(s.shapes.keys()).cloned().collect();
            names.sort();
            names.dedup();
            names
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

fn pack_items(names: Vec<String>, prefix: &str) -> Vec<Value> {
    names
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .map(|name| completion_item(&name, 9))
        .collect()
}

fn name_items(names: Vec<String>, prefix: &str) -> Vec<Value> {
    names
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .map(|name| completion_item(&name, 6))
        .collect()
}

fn visible_names(ir: &IrRes, mut scope: ScopeId) -> Vec<String> {
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    loop {
        let Some(sc) = ir.scopes.get(scope.0 as usize) else {
            break;
        };
        for name in sc
            .packs
            .keys()
            .chain(sc.shapes.keys())
            .chain(sc.defs.keys())
        {
            if seen.insert(name.clone()) {
                out.push(name.clone());
            }
        }
        let Some(parent) = sc.parent else {
            break;
        };
        scope = parent;
    }
    out.sort();
    out
}
