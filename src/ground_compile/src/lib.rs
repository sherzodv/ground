pub mod ast;
pub mod ir;
pub mod parse;
pub mod resolve;
pub mod asm;

pub use asm::{AsmInst, AsmField, AsmValue, AsmVariant, AsmInstRef, asm_value_to_json};

// ---------------------------------------------------------------------------
// Embedded stdlib
// ---------------------------------------------------------------------------

const STD_GRD:               &str = include_str!("stdlib/std.grd");
const STD_AWS_PACK_GRD:      &str = include_str!("stdlib/aws/pack.grd");
const STD_AWS_TRANSFORM_GRD: &str = include_str!("stdlib/aws/transform.grd");
const STD_AWS_TRANSFORM_TS:  &str = include_str!("stdlib/aws/transform.ts");

/// Number of units prepended by the compiler as stdlib.
/// Callers can use this to offset unit indices in error locations.
pub const STDLIB_UNIT_COUNT: usize = 3;

fn make_stdlib_parse_units() -> Vec<ast::ParseUnit> {
    vec![
        ast::ParseUnit { name: "std".into(),      path: vec![],                                        src: STD_GRD.into() },
        ast::ParseUnit { name: "".into(),          path: vec!["std".into(), "aws".into()],             src: STD_AWS_PACK_GRD.into() },
        ast::ParseUnit { name: "transform".into(), path: vec!["std".into(), "aws".into()],             src: STD_AWS_TRANSFORM_GRD.into() },
    ]
}

// ---------------------------------------------------------------------------
// Public input types
// ---------------------------------------------------------------------------

pub struct CompileReq {
    pub units: Vec<Unit>,
}

pub struct Unit {
    pub name:   String,
    pub path:   Vec<String>,
    pub src:    String,
    /// Optional TypeScript source co-located with this unit.
    /// Hook functions defined in `src` are implemented here.
    pub ts_src: Option<String>,
}

// ---------------------------------------------------------------------------
// Public output types
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

/// Lookup table for all named instances in the program.
pub struct Symbol {
    pub instances: Vec<AsmInst>,
}

impl Symbol {
    pub fn get(&self, name: &str) -> Option<&AsmInst> {
        self.instances.iter().find(|i| i.name == name)
    }
}

/// A fully resolved plan context created from a `plan name` declaration.
pub struct Plan {
    pub name:      String,
    pub root:      AsmInst,
    pub fields:    Vec<AsmField>,
    pub reachable: Vec<AsmInst>,
}

pub struct CompileRes {
    pub symbol:  Symbol,
    pub plans:   Vec<Plan>,
    pub errors:  Vec<CompileError>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn compile(req: CompileReq) -> CompileRes {
    // Concatenate TypeScript: stdlib first, then user units.
    // All hook functions share one blob — names must be globally unique by convention.
    let mut ts_parts: Vec<&str> = vec![STD_AWS_TRANSFORM_TS];
    let user_ts: Vec<&str> = req.units.iter()
        .filter_map(|u| u.ts_src.as_deref())
        .collect();
    ts_parts.extend(user_ts);
    let ts_src = ts_parts.join("\n\n");

    // Prepend stdlib units before user units.
    let mut units: Vec<ast::ParseUnit> = make_stdlib_parse_units();
    units.extend(req.units.into_iter().map(|u| ast::ParseUnit {
        name: u.name, path: u.path, src: u.src,
    }));

    // Keep sources for error location resolution before moving units into parse.
    let srcs: Vec<String> = units.iter().map(|u| u.src.clone()).collect();

    let parse_res = parse::parse(ast::ParseReq { units });
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
        return CompileRes { symbol: Symbol { instances: vec![] }, plans: vec![], errors };
    }

    let ctx = asm::lower(&ir, &ts_src);

    let symbol = Symbol { instances: ctx.symbol.insts };

    let plans = ctx.plans.into_iter().map(|p| Plan {
        name:      p.name,
        root:      p.root,
        fields:    p.fields,
        reachable: p.reachable,
    }).collect();

    CompileRes { symbol, plans, errors }
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
