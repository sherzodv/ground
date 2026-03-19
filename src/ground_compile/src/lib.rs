pub mod ast;
pub mod ir;
pub mod parse;
pub mod resolve;
pub mod asm;

pub use asm::{AsmInst, AsmField, AsmValue, AsmVariant, AsmInstRef};

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
    pub target:  Vec<String>,
    pub name:    String,
    pub inst:    AsmInst,
    pub fields:  Vec<AsmField>,
    pub members: Vec<AsmInstRef>,  // ordered refs to instances referenced from inst's fields
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
        ast::ParseUnit { name: "".into(), path: vec![], src: STDLIB.to_string() },
    ];
    units.extend(req.units.into_iter().map(|u| ast::ParseUnit {
        name: u.name, path: u.path, src: u.src,
    }));

    let parse_res = parse::parse(ast::ParseReq { units });
    let ir        = resolve::resolve(parse_res);

    let errors: Vec<CompileError> = ir.errors.iter()
        .map(|e| CompileError { message: e.message.clone() })
        .collect();

    let ctx = asm::lower(&ir);

    let symbol = Symbol { instances: ctx.symbol.insts };

    let deploys = ctx.deploys.into_iter().map(|d| {
        let members = collect_inst_refs(&d.inst);
        Deploy {
            target:  d.target,
            name:    d.name,
            inst:    d.inst,
            fields:  d.fields,
            members,
        }
    }).collect();

    CompileRes { symbol, deploys, errors }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect unique `AsmInstRef`s referenced from an instance's fields (shallow — top-level fields only).
fn collect_inst_refs(inst: &AsmInst) -> Vec<AsmInstRef> {
    let mut refs: Vec<AsmInstRef> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for f in &inst.fields {
        collect_refs_in_value(&f.value, &mut refs, &mut seen);
    }
    refs
}

fn collect_refs_in_value(
    v: &AsmValue,
    refs: &mut Vec<AsmInstRef>,
    seen: &mut std::collections::HashSet<String>,
) {
    match v {
        AsmValue::InstRef(r) => {
            if seen.insert(r.name.clone()) {
                refs.push(AsmInstRef { type_name: r.type_name.clone(), name: r.name.clone() });
            }
        }
        AsmValue::Path(segs) => {
            for s in segs { collect_refs_in_value(s, refs, seen); }
        }
        AsmValue::List(items) => {
            for i in items { collect_refs_in_value(i, refs, seen); }
        }
        AsmValue::Inst(i) => {
            for f in &i.fields { collect_refs_in_value(&f.value, refs, seen); }
        }
        _ => {}
    }
}
