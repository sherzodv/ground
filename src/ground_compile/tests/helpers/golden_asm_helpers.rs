use ground_compile::asm::*;
use ground_compile::ast::{ParseReq, ParseUnit};
use ground_compile::ir::IrRes;
use ground_compile::parse::parse;
use ground_compile::resolve::resolve;

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

pub fn show_value(v: &AsmValue) -> String {
    match v {
        AsmValue::Str(s) => format!("Str({:?})", s),
        AsmValue::Int(n) => format!("Int({})", n),
        AsmValue::Bool(b) => format!("Bool({})", b),
        AsmValue::Ref(s) => format!("Ref({})", s),
        AsmValue::Variant(gv) => match &gv.payload {
            None => format!("Variant({}, {:?})", gv.type_name, gv.value),
            Some(p) => format!(
                "Variant({}, {:?}, {})",
                gv.type_name,
                gv.value,
                show_value(p)
            ),
        },
        AsmValue::DefRef(ir) => format!("DefRef({}, {})", ir.type_name, ir.name),
        AsmValue::Def(gi) => format!("Def[{}]", show_def_inline(gi)),
        AsmValue::Path(segs) => segs.iter().map(show_value).collect::<Vec<_>>().join(":"),
        AsmValue::List(items) => {
            let parts: Vec<_> = items.iter().map(show_value).collect();
            format!("List[{}]", parts.join(", "))
        }
    }
}

fn show_field(f: &AsmField) -> String {
    format!("{}: {}", f.name, show_value(&f.value))
}

fn show_def_inline(d: &AsmDef) -> String {
    let mut head = format!("{} = {}", d.name, d.type_name);
    if let Some(hint) = &d.type_hint {
        head.push_str(&format!(" hint: {}", hint));
    }
    if d.fields.is_empty() {
        head
    } else {
        let fields = d
            .fields
            .iter()
            .map(show_field)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{head} {{ {fields} }}")
    }
}

pub fn show_def(d: &AsmDef) -> String {
    format!("Def[{}]", show_def_inline(d))
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

/// Parse + resolve + lower `input` with TypeScript source for mapper execution.
pub fn show(grd_src: &str) -> String {
    show_with_ts(grd_src, "")
}

pub fn show_multi(units: Vec<(&str, Vec<&str>, &str)>) -> String {
    let res = parse(ParseReq {
        units: units
            .into_iter()
            .map(|(name, path, src)| ParseUnit {
                name: name.into(),
                path: path.into_iter().map(|s| s.to_string()).collect(),
                declared_pack: None,
                src: src.to_string(),
                ts_src: None,
            })
            .collect(),
    });
    let ir = resolve(res);
    show_asm(lower(&ir, ""), ir)
}

/// Parse + resolve + lower `input` with TypeScript source for mapper execution.
pub fn show_with_ts(grd_src: &str, ts_src: &str) -> String {
    let res = parse(ParseReq {
        units: vec![ParseUnit {
            name: "test".into(),
            path: vec![],
            declared_pack: None,
            src: grd_src.to_string(),
            ts_src: Some(ts_src.to_string()),
        }],
    });
    let ir = resolve(res);
    show_asm(lower(&ir, ts_src), ir)
}

fn show_asm(asm: AsmRes, ir: IrRes) -> String {
    let _ = ir;
    norm(&asm.defs.iter().map(show_def).collect::<Vec<_>>().join("\n"))
}
