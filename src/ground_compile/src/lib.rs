pub mod ast;
pub mod ir;
pub mod parse;
pub mod resolve;
pub mod asm;
pub mod fmt;
mod ts_gen;

pub use asm::{AsmDef, AsmField, AsmValue, AsmVariant, AsmDefRef, asm_value_to_json};
pub use fmt::format_source;

// ---------------------------------------------------------------------------
// Public input shapes
// ---------------------------------------------------------------------------

pub struct CompileReq {
    pub units: Vec<Unit>,
}

pub struct Unit {
    pub name:   String,
    pub path:   Vec<String>,
    pub src:    String,
    /// Optional TypeScript source co-located with this unit.
    /// Mapper functions defined in `src` are implemented here.
    pub ts_src: Option<String>,
}

// ---------------------------------------------------------------------------
// Public output shapes
// ---------------------------------------------------------------------------

pub struct CompileError {
    pub message: String,
    pub loc:     Option<ErrorLoc>,
}

pub struct ErrorLoc {
    pub unit: u32,
    pub line: u32,
    pub col:  u32,
}

pub struct CompileRes {
    pub defs:    Vec<AsmDef>,
    pub type_units: Vec<TypeUnit>,
    pub errors:  Vec<CompileError>,
}

pub struct AnalysisRes {
    pub parse:      ast::ParseRes,
    pub ir:         ir::IrRes,
    pub type_units: Vec<TypeUnit>,
    pub errors:     Vec<CompileError>,
}

pub struct TypeUnit {
    pub file:    String,
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
    parse_res:  ast::ParseRes,
    ir:         ir::IrRes,
    type_units: Vec<TypeUnit>,
    errors:     Vec<CompileError>,
    full_ts:    String,
}

fn prepare(req: CompileReq) -> Prepared {
    let units: Vec<ast::ParseUnit> = req.units.into_iter().map(|u| ast::ParseUnit {
        name: u.name, path: u.path, src: u.src, ts_src: u.ts_src,
    }).collect();

    let type_unit_files: Vec<String> = units.iter()
        .map(|u| type_unit_file(&u.path, &u.name))
        .collect();

    // Keep sources for error location resolution before moving units into parse.
    let srcs: Vec<String> = units.iter().map(|u| u.src.clone()).collect();

    let parse_req = ast::ParseReq { units };

    let user_ts: String = parse_req.units.iter()
        .filter_map(|u| u.ts_src.as_deref())
        .collect::<Vec<_>>()
        .join("\n\n");

    let parse_res = parse::parse(parse_req);
    let ir        = resolve::resolve(parse_res.clone());

    let errors: Vec<CompileError> = ir.errors.iter()
        .map(|e| {
            let loc = srcs.get(e.loc.unit as usize).map(|src| {
                let (line, col) = offset_to_line_col(src, e.loc.start);
                ErrorLoc { unit: e.loc.unit, line, col }
            });
            CompileError { message: e.message.clone(), loc }
        })
        .collect();

    // Don't lower if the IR has errors — it may be in an invalid state.
    if !errors.is_empty() {
        return Prepared { parse_res, ir, type_units: vec![], errors, full_ts: String::new() };
    }

    // Generate TypeScript interface declarations and type-compatibility assertions.
    let generated_dts    = ts_gen::gen_mapper_interfaces(&ir);
    let type_units = ts_gen::gen_mapper_interfaces_by_unit(&ir).into_iter()
        .filter_map(|(unit, content)| {
            let file = type_unit_files.get(unit as usize)?.clone();
            Some(TypeUnit { file, content })
        })
        .collect();
    let tc_assertions    = ts_gen::gen_typecheck_assertions(&ir, &user_ts);

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
                let ts_errors: Vec<CompileError> = diags.iter()
                    .filter(|d| d.category == 1) // 1 = error
                    .map(|d| CompileError { message: d.message.clone(), loc: None })
                    .collect();
                if !ts_errors.is_empty() {
                    return Prepared { parse_res, ir, type_units: vec![], errors: ts_errors, full_ts: String::new() };
                }
            }
            Err(e) => {
                let msg = format!("TypeScript type-check engine error: {e}");
                return Prepared {
                    parse_res,
                    ir,
                    type_units: vec![],
                    errors: vec![CompileError { message: msg, loc: None }],
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

    Prepared { parse_res, ir, type_units, errors, full_ts }
}

pub fn analyze(req: CompileReq) -> AnalysisRes {
    let prepared = prepare(req);
    AnalysisRes {
        parse: prepared.parse_res,
        ir: prepared.ir,
        type_units: prepared.type_units,
        errors: prepared.errors,
    }
}

pub fn compile(req: CompileReq) -> CompileRes {
    let prepared = prepare(req);

    if !prepared.errors.is_empty() {
        return CompileRes { defs: vec![], type_units: prepared.type_units, errors: prepared.errors };
    }

    let ctx = asm::lower(&prepared.ir, &prepared.full_ts);
    CompileRes { defs: ctx.defs, type_units: prepared.type_units, errors: prepared.errors }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn offset_to_line_col(src: &str, offset: u32) -> (u32, u32) {
    let offset = (offset as usize).min(src.len());
    let before = &src[..offset];
    let line   = before.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
    let col    = before.rfind('\n').map_or(offset, |p| offset - p - 1) as u32 + 1;
    (line, col)
}
