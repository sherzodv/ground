use ground_compile::ast::{ParseReq, ParseUnit};
use ground_compile::asm::*;
use ground_compile::parse::parse;
use ground_compile::resolve::resolve;

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

pub fn show_value(v: &AsmValue) -> String {
    match v {
        AsmValue::Str(s)              => format!("Str({:?})", s),
        AsmValue::Int(n)              => format!("Int({})", n),
        AsmValue::Ref(s)              => format!("Ref({:?})", s),
        AsmValue::Variant(gv)         => format!("Variant({}, {:?})", gv.type_name, gv.value),
        AsmValue::InstRef(ir)         => format!("InstRef({}, {})", ir.type_name, ir.name),
        AsmValue::Inst(gi)            => format!("Inst[{}]", show_inst_inline(gi)),
        AsmValue::Path(segs)          => segs.iter().map(show_value).collect::<Vec<_>>().join(":"),
        AsmValue::List(items)         => {
            let parts: Vec<_> = items.iter().map(show_value).collect();
            format!("List[{}]", parts.join(", "))
        }
    }
}

fn show_field(f: &AsmField) -> String {
    format!("{}={}", f.name, show_value(&f.value))
}

fn show_inst_inline(i: &AsmInst) -> String {
    let mut parts = vec![i.type_name.clone(), i.name.clone()];
    parts.extend(i.fields.iter().map(show_field));
    parts.join(", ")
}

pub fn show_inst(i: &AsmInst) -> String {
    format!("Inst[{}]", show_inst_inline(i))
}

pub fn show_deploy(d: &AsmDeploy) -> String {
    let target     = d.target.join(":");
    let dep_fields: Vec<_> = d.fields.iter().map(show_field).collect();
    let mut parts = vec![
        format!("Deploy[{}, {}]", target, d.name),
        format!("  inst: {}", show_inst(&d.inst)),
    ];
    if !dep_fields.is_empty() {
        parts.push(format!("  fields: {}", dep_fields.join(", ")));
    }
    parts.join("\n")
}

pub fn show_symbol(s: &AsmSymbol) -> String {
    if s.insts.is_empty() {
        return String::new();
    }
    let insts: Vec<_> = s.insts.iter().map(|i| format!("  {}", show_inst(i))).collect();
    format!("Symbol\n{}", insts.join("\n"))
}

// ---------------------------------------------------------------------------
// Entry point used by tests
// ---------------------------------------------------------------------------

/// Normalise expected strings: strip blank lines, trim each line.
pub fn norm(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse + resolve + lower `input`, format as compact string.
pub fn show(input: &str) -> String {
    let res = parse(ParseReq {
        units: vec![ParseUnit { name: "test".into(), path: vec![], src: input.to_string() }],
    });
    let ir  = resolve(res);
    let gen = lower(&ir);

    let mut lines: Vec<String> = Vec::new();
    let sym = show_symbol(&gen.symbol);
    if !sym.is_empty() {
        lines.push(sym);
    }
    lines.extend(gen.deploys.iter().map(show_deploy));
    norm(&lines.join("\n"))
}
