pub mod ast;
pub mod ir;
pub mod parse;
pub mod resolve;
pub mod asm;
mod ts_gen;

pub use asm::{AsmDef, AsmField, AsmValue, AsmVariant, AsmDefRef, asm_value_to_json};

// ---------------------------------------------------------------------------
// Embedded stdlib
// ---------------------------------------------------------------------------

const STD_GRD:               &str = include_str!("stdlib/std.grd");
const STD_AWS_PACK_GRD:      &str = include_str!("stdlib/aws/pack.grd");
const STD_AWS_PACK_TS:       &str = include_str!("stdlib/aws/pack.ts");

/// Number of units prepended by the compiler as stdlib.
/// Callers can use this to offset unit indices in error locations.
pub const STDLIB_UNIT_COUNT: usize = 2;

fn make_stdlib_parse_units() -> Vec<ast::ParseUnit> {
    vec![
        ast::ParseUnit { name: "std".into(),       path: vec![],                                    src: STD_GRD.into(),               ts_src: None },
        ast::ParseUnit { name: "".into(),           path: vec!["std".into(), "aws".into()],         src: STD_AWS_PACK_GRD.into(),      ts_src: Some(STD_AWS_PACK_TS.into()) },
    ]
}

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
    pub errors:  Vec<CompileError>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn compile(req: CompileReq) -> CompileRes {
    // Prepend stdlib units before user units, carrying ts_src with each unit.
    let mut units: Vec<ast::ParseUnit> = make_stdlib_parse_units();
    units.extend(req.units.into_iter().map(|u| ast::ParseUnit {
        name: u.name, path: u.path, src: u.src, ts_src: u.ts_src,
    }));

    // Keep sources for error location resolution before moving units into parse.
    let srcs: Vec<String> = units.iter().map(|u| u.src.clone()).collect();

    let parse_req = ast::ParseReq { units };

    // Collect TS source blobs — stdlib and user are kept separate.
    // Stdlib TS is trusted internal code; only user TS is type-checked.
    let stdlib_ts: String = parse_req.units.iter()
        .take(STDLIB_UNIT_COUNT)
        .filter_map(|u| u.ts_src.as_deref())
        .collect::<Vec<_>>()
        .join("\n\n");
    let user_ts: String = parse_req.units.iter()
        .skip(STDLIB_UNIT_COUNT)
        .filter_map(|u| u.ts_src.as_deref())
        .collect::<Vec<_>>()
        .join("\n\n");
    // Full blob used for execution (stdlib + user, no generated declarations needed at runtime).
    let ts_src: String = [stdlib_ts.as_str(), user_ts.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");

    let parse_res = parse::parse(parse_req);
    let ir        = resolve::resolve(parse_res);

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
        return CompileRes { defs: vec![], errors };
    }

    // Generate TypeScript interface declarations and type-compatibility assertions.
    let generated_dts    = ts_gen::gen_mapper_interfaces(&ir);
    let tc_assertions    = ts_gen::gen_typecheck_assertions(&ir, &user_ts);

    // Append assertions to user_ts so TypeScript verifies each mapper implementation
    // is assignable to its declared I/O signature, even without explicit annotations.
    let user_ts_for_check = if tc_assertions.is_empty() {
        user_ts.clone()
    } else {
        format!("{user_ts}\n{tc_assertions}")
    };

    // Type-check user TypeScript against the generated declarations.
    // Only user TS is checked — stdlib is trusted internal code.
    if !user_ts.is_empty() {
        match ground_ts::typecheck::typecheck(&generated_dts, &user_ts_for_check) {
            Ok(diags) => {
                let ts_errors: Vec<CompileError> = diags.iter()
                    .filter(|d| d.category == 1) // 1 = error
                    .map(|d| CompileError { message: d.message.clone(), loc: None })
                    .collect();
                if !ts_errors.is_empty() {
                    return CompileRes { defs: vec![], errors: ts_errors };
                }
            }
            Err(e) => {
                let msg = format!("TypeScript type-check engine error: {e}");
                return CompileRes { defs: vec![], errors: vec![CompileError { message: msg, loc: None }] };
            }
        }
    }

    let full_ts = if generated_dts.is_empty() {
        ts_src
    } else {
        format!("{}\n\n{}", generated_dts, ts_src)
    };

    let ctx = asm::lower(&ir, &full_ts);

    CompileRes { defs: ctx.defs, errors }
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
