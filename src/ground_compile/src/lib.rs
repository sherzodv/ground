pub mod asm;
pub mod ast;
pub mod fmt;
pub mod ir;
pub mod parse;
pub mod render;
pub mod resolve;
mod ts_gen;

pub use asm::{asm_value_to_json, AsmDef, AsmDefRef, AsmField, AsmValue, AsmVariant};
pub use ast::UnitId;
pub use fmt::format_source;
pub use ir::ScopeId;
pub use render::{render, RenderRes, RenderTarget, RenderUnit, TemplateUnit};

// ---------------------------------------------------------------------------
// Public input shapes
// ---------------------------------------------------------------------------

pub struct CompileReq {
    pub units: Vec<Unit>,
}

pub struct Unit {
    pub name: String,
    pub path: Vec<String>,
    pub src: String,
    /// Optional TypeScript source co-located with this unit.
    /// Mapper functions defined in `src` are implemented here.
    pub ts_src: Option<String>,
}

// ---------------------------------------------------------------------------
// Public output shapes
// ---------------------------------------------------------------------------

pub struct CompileError {
    pub message: String,
    pub loc: Option<ErrorLoc>,
}

pub struct ErrorLoc {
    pub unit: UnitId,
    pub line: u32,
    pub col: u32,
    /// True if `line`/`col` are inside the unit's `.ts` source; false for `.grd`.
    pub in_ts: bool,
}

pub struct CompileRes {
    pub units: Vec<UnitId>,
    pub defs: Vec<AsmDef>,
    pub plans: Vec<PlanRoot>,
    pub scopes: Vec<ir::IrScope>,
    pub type_units: Vec<TypeUnit>,
    pub errors: Vec<CompileError>,
}

/// A planned root — one per `plan` declaration in the source. `def_idx`
/// indexes `CompileRes::defs`. `pack_path` is the pack where the `plan`
/// statement was written; it drives render-manifest lookup.
#[derive(Debug, Clone)]
pub struct PlanRoot {
    pub name: String,
    pub def_idx: usize,
    pub pack_path: Vec<String>,
    pub scope: ScopeId,
    /// Source unit where the `plan` declaration lives. Useful for error
    /// locations; optional to let non-source-driven callers omit it.
    pub unit: Option<UnitId>,
}

pub struct AnalysisRes {
    pub units: Vec<UnitId>,
    pub parse: ast::ParseRes,
    pub ir: ir::IrRes,
    pub type_units: Vec<TypeUnit>,
    pub errors: Vec<CompileError>,
}

pub struct TypeUnit {
    pub file: String,
    pub content: String,
}

fn type_unit_file(path: &[String], name: &str) -> String {
    let mut out = path.join("/");
    if !out.is_empty() {
        out.push('/');
    }
    let stem = if name.is_empty() { "pack" } else { name };
    out.push_str(stem);
    out.push_str(".gen.d.ts");
    out
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

struct Prepared {
    unit_ids: Vec<UnitId>,
    parse_res: ast::ParseRes,
    ir: ir::IrRes,
    type_units: Vec<TypeUnit>,
    errors: Vec<CompileError>,
    full_ts: String,
}

fn prepare(req: CompileReq) -> Prepared {
    let mut pre_errors = vec![];
    let units: Vec<ast::ParseUnit> = req
        .units
        .into_iter()
        .enumerate()
        .map(|(idx, u)| {
            let declared_pack = validate_compile_unit_pack(
                UnitId(idx as u32),
                &u.path,
                &u.name,
                &u.src,
                &mut pre_errors,
            );
            ast::ParseUnit {
                name: u.name,
                path: u.path,
                declared_pack,
                src: u.src,
                ts_src: u.ts_src,
            }
        })
        .collect();

    let unit_ids: Vec<UnitId> = (0..units.len()).map(|i| UnitId(i as u32)).collect();

    let type_unit_files: Vec<String> = units
        .iter()
        .map(|u| type_unit_file(&u.path, &u.name))
        .collect();

    // Keep sources for error location resolution before moving units into parse.
    let srcs: Vec<String> = units.iter().map(|u| u.src.clone()).collect();

    let parse_req = ast::ParseReq { units };

    // Build user_ts by concatenating each unit's ts_src with "\n\n" separators,
    // tracking the 1-based line number where each unit's content starts in the blob
    // so TS diagnostics can be mapped back to per-unit (UnitId, line, col).
    let mut ts_parts: Vec<&str> = Vec::new();
    let mut ts_unit_starts: Vec<(UnitId, u32)> = Vec::new();
    let mut cur_line: u32 = 1;
    for (idx, u) in parse_req.units.iter().enumerate() {
        if let Some(ts) = u.ts_src.as_deref() {
            if !ts_parts.is_empty() {
                cur_line += 2; // "\n\n" separator
            }
            ts_unit_starts.push((UnitId(idx as u32), cur_line));
            cur_line += ts.bytes().filter(|&b| b == b'\n').count() as u32;
            ts_parts.push(ts);
        }
    }
    let user_ts: String = ts_parts.join("\n\n");

    let parse_res = parse::parse(parse_req);
    let ir = resolve::resolve(parse_res.clone());

    let mut errors: Vec<CompileError> = ir
        .errors
        .iter()
        .map(|e| {
            let loc = srcs.get(e.loc.unit.as_usize()).map(|src| {
                let (line, col) = offset_to_line_col(src, e.loc.start);
                ErrorLoc {
                    unit: e.loc.unit,
                    line,
                    col,
                    in_ts: false,
                }
            });
            CompileError {
                message: e.message.clone(),
                loc,
            }
        })
        .collect();
    errors.extend(pre_errors);

    // Don't lower if the IR has errors — it may be in an invalid state.
    if !errors.is_empty() {
        return Prepared {
            unit_ids,
            parse_res,
            ir,
            type_units: vec![],
            errors,
            full_ts: String::new(),
        };
    }

    // Generate TypeScript interface declarations and type-compatibility assertions.
    let generated_dts = ts_gen::gen_mapper_interfaces(&ir);
    let type_units = ts_gen::gen_mapper_interfaces_by_unit(&ir)
        .into_iter()
        .filter_map(|(unit, content)| {
            let file = type_unit_files.get(unit.as_usize())?.clone();
            Some(TypeUnit { file, content })
        })
        .collect();
    let tc_assertions = ts_gen::gen_typecheck_assertions(&ir, &user_ts);

    // Append assertions to user_ts so TypeScript verifies each mapper implementation
    // is assignable to its declared I/O signature, even without explicit annotations.
    let user_ts_for_check = if tc_assertions.is_empty() {
        user_ts.clone()
    } else {
        format!("{user_ts}\n{tc_assertions}")
    };

    if !user_ts.is_empty() {
        match ground_ts::typecheck::typecheck(&generated_dts, &user_ts_for_check) {
            Ok(diags) => {
                let ts_errors: Vec<CompileError> = diags
                    .iter()
                    .filter(|d| d.category == 1) // 1 = error
                    .map(|d| CompileError {
                        message: d.message.clone(),
                        loc: ts_diag_to_loc(d, &ts_unit_starts),
                    })
                    .collect();
                if !ts_errors.is_empty() {
                    return Prepared {
                        unit_ids,
                        parse_res,
                        ir,
                        type_units: vec![],
                        errors: ts_errors,
                        full_ts: String::new(),
                    };
                }
            }
            Err(e) => {
                let msg = format!("TypeScript type-check engine error: {e}");
                return Prepared {
                    unit_ids,
                    parse_res,
                    ir,
                    type_units: vec![],
                    errors: vec![CompileError {
                        message: msg,
                        loc: None,
                    }],
                    full_ts: String::new(),
                };
            }
        }
    }

    let full_ts = if generated_dts.is_empty() {
        user_ts
    } else {
        format!("{}\n\n{}", generated_dts, user_ts)
    };

    Prepared {
        unit_ids,
        parse_res,
        ir,
        type_units,
        errors,
        full_ts,
    }
}

pub fn analyze(req: CompileReq) -> AnalysisRes {
    let prepared = prepare(req);
    AnalysisRes {
        units: prepared.unit_ids,
        parse: prepared.parse_res,
        ir: prepared.ir,
        type_units: prepared.type_units,
        errors: prepared.errors,
    }
}

pub fn compile(req: CompileReq) -> CompileRes {
    let prepared = prepare(req);

    if !prepared.errors.is_empty() {
        return CompileRes {
            units: prepared.unit_ids,
            defs: vec![],
            plans: vec![],
            scopes: vec![],
            type_units: prepared.type_units,
            errors: prepared.errors,
        };
    }

    let ctx = asm::lower(&prepared.ir, &prepared.full_ts);

    // `asm::lower` iterates `ir.defs` in order and keeps planned defs only, so
    // the i-th planned IR def corresponds to `ctx.defs[i]`.
    let plans: Vec<PlanRoot> = prepared
        .ir
        .defs
        .iter()
        .filter(|d| d.planned)
        .enumerate()
        .map(|(idx, ir_def)| {
            let unit = ir_def.loc.unit;
            let scope = ir_def.scope;
            let pack_path = scope_pack_path(&prepared.ir.scopes, scope);
            PlanRoot {
                name: ir_def.name.clone(),
                def_idx: idx,
                pack_path,
                scope,
                unit: Some(unit),
            }
        })
        .collect();

    CompileRes {
        units: prepared.unit_ids,
        defs: ctx.defs,
        plans,
        scopes: prepared.ir.scopes,
        type_units: prepared.type_units,
        errors: prepared.errors,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn offset_to_line_col(src: &str, offset: u32) -> (u32, u32) {
    let offset = (offset as usize).min(src.len());
    let before = &src[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
    let col = before.rfind('\n').map_or(offset, |p| offset - p - 1) as u32 + 1;
    (line, col)
}

fn validate_compile_unit_pack(
    unit: UnitId,
    path: &[String],
    name: &str,
    src: &str,
    errors: &mut Vec<CompileError>,
) -> Option<Vec<String>> {
    let (items, parse_errors) = parse::parse_file_items(src, unit);
    if !parse_errors.is_empty() {
        return None;
    }

    let Some(first_idx) = items
        .iter()
        .position(|item| !matches!(item, ast::AstItem::Comment(_)))
    else {
        errors.push(CompileError {
            message: "compile unit must start with `pack ...`".into(),
            loc: Some(ErrorLoc {
                unit,
                line: 1,
                col: 1,
                in_ts: false,
            }),
        });
        return None;
    };

    let ast::AstItem::Pack(pack) = &items[first_idx] else {
        errors.push(CompileError {
            message: "compile unit must start with `pack ...`".into(),
            loc: Some(ErrorLoc {
                unit,
                line: 1,
                col: 1,
                in_ts: false,
            }),
        });
        return None;
    };

    if pack.inner.defs.is_some() {
        let (line, col) = offset_to_line_col(src, pack.loc.start);
        errors.push(CompileError {
            message: "compile unit pack declaration must be bare `pack ...`".into(),
            loc: Some(ErrorLoc {
                unit,
                line,
                col,
                in_ts: false,
            }),
        });
        return None;
    }

    let mut declared = vec![];
    for seg in &pack.inner.path.inner.segments {
        let Some(plain) = seg.inner.as_plain() else {
            let (line, col) = offset_to_line_col(src, seg.loc.start);
            errors.push(CompileError {
                message: "compile unit pack path must use plain segments".into(),
                loc: Some(ErrorLoc {
                    unit,
                    line,
                    col,
                    in_ts: false,
                }),
            });
            return None;
        };
        if seg.inner.is_opt || plain == "*" || plain == "pack" || plain == "def" {
            let (line, col) = offset_to_line_col(src, seg.loc.start);
            errors.push(CompileError {
                message: "compile unit pack path must use plain segments".into(),
                loc: Some(ErrorLoc {
                    unit,
                    line,
                    col,
                    in_ts: false,
                }),
            });
            return None;
        }
        declared.push(plain.to_string());
    }

    let mut base = path.to_vec();
    if !name.is_empty() {
        base.push(name.to_string());
    }
    if !declared.starts_with(&base) {
        let (line, col) = offset_to_line_col(src, pack.loc.start);
        let expected = if base.is_empty() {
            "<root>".into()
        } else {
            base.join(":")
        };
        let got = if declared.is_empty() {
            "<root>".into()
        } else {
            declared.join(":")
        };
        errors.push(CompileError {
            message: format!(
                "compile unit pack '{}' must match file path prefix '{}'",
                got, expected
            ),
            loc: Some(ErrorLoc {
                unit,
                line,
                col,
                in_ts: false,
            }),
        });
        return None;
    }

    Some(declared)
}

fn scope_pack_path(scopes: &[ir::IrScope], mut scope: ScopeId) -> Vec<String> {
    let mut out = Vec::new();
    loop {
        let Some(sc) = scopes.get(scope.0 as usize) else {
            break;
        };
        if matches!(sc.kind, ir::ScopeKind::Pack) {
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
    out
}

/// Map a TypeScript diagnostic back to a per-unit (UnitId, line, col) using
/// the per-unit start-line table built when assembling `user_ts`.
///
/// Only `user.ts` diagnostics can be mapped. `decls.gen.d.ts` errors or
/// diagnostics without line info return `None` — they'll surface as
/// workspace-level messages without a squiggle.
fn ts_diag_to_loc(
    d: &ground_ts::typecheck::TsDiagnostic,
    ts_unit_starts: &[(UnitId, u32)],
) -> Option<ErrorLoc> {
    if d.file.as_deref() != Some("user.ts") {
        return None;
    }
    let line = d.line?;
    let col = d.col.unwrap_or(1);
    // Find the last unit whose start_line <= line.
    let (unit, start_line) = ts_unit_starts
        .iter()
        .rev()
        .copied()
        .find(|(_, start)| *start <= line)?;
    Some(ErrorLoc {
        unit,
        line: line - start_line + 1,
        col,
        in_ts: true,
    })
}
