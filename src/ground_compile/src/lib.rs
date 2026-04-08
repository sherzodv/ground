pub mod ast;
pub mod ir;
pub mod parse;
pub mod resolve;
pub mod asm;

pub use asm::{AsmInst, AsmField, AsmValue, AsmVariant, AsmInstRef, AsmExpansion, AsmOutput, AsmLinkOutput, AsmOverrides};

const STDLIB: &str = include_str!("stdlib.grd");

// ---------------------------------------------------------------------------
// Public input types
// ---------------------------------------------------------------------------

pub struct CompileReq {
    pub units: Vec<Unit>,
}

pub struct Unit {
    pub name: String,
    pub path: Vec<String>,
    pub src:  String,
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

/// A fully resolved, self-contained deployment context.
pub struct Deploy {
    pub target:    Vec<String>,
    pub name:      String,
    pub inst:      AsmInst,
    pub fields:    Vec<AsmField>,
    pub expansion: Option<AsmExpansion>,
    pub overrides: AsmOverrides,
}

pub struct CompileRes {
    pub symbol:  Symbol,
    pub deploys: Vec<Deploy>,
    pub errors:  Vec<CompileError>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn compile(req: CompileReq) -> CompileRes {
    let mut units = vec![
        ast::ParseUnit { name: "std".into(), path: vec![], src: STDLIB.to_string() },
    ];
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

    let ctx = asm::lower(&ir);

    let symbol = Symbol { instances: ctx.symbol.insts };

    let deploys = ctx.deploys.into_iter().map(|d| Deploy {
        target:    d.target,
        name:      d.name,
        inst:      d.inst,
        fields:    d.fields,
        expansion: d.expansion,
        overrides: d.overrides,
    }).collect();

    CompileRes { symbol, deploys, errors }
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
