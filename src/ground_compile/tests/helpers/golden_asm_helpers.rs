use ground_compile::ast::{ParseReq, ParseUnit};
use ground_compile::asm::*;
use ground_compile::ir::{IrRes, ScopeId, ScopeKind};
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
        AsmValue::Variant(gv)         => match &gv.payload {
            None    => format!("Variant({}, {:?})", gv.type_name, gv.value),
            Some(p) => format!("Variant({}, {:?}, {})", gv.type_name, gv.value, show_value(p)),
        },
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
    if let Some(hint) = &i.type_hint {
        parts.push(format!("hint={}", hint));
    }
    parts.extend(i.fields.iter().map(show_field));
    parts.join(", ")
}

pub fn show_inst(i: &AsmInst) -> String {
    format!("Inst[{}]", show_inst_inline(i))
}

pub fn show_plan(p: &AsmPlan) -> String {
    let plan_fields: Vec<_> = p.fields.iter().map(show_field).collect();
    let mut parts = vec![
        format!("Plan[{}]", p.name),
        format!("  root: {}", show_inst(&p.root)),
    ];
    if !plan_fields.is_empty() {
        parts.push(format!("  fields: {}", plan_fields.join(", ")));
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
// Scope tree
// ---------------------------------------------------------------------------

fn show_scope_asm(scope_id: ScopeId, ir: &IrRes, scoped_insts: &[(ScopeId, &AsmInst)]) -> String {
    let scope = &ir.scopes[scope_id.0 as usize];
    let name  = format!("pack:{}", scope.name.as_deref().unwrap_or("_"));

    let mut parts: Vec<String> = Vec::new();

    // Named instances belonging to this scope
    for (sid, inst) in scoped_insts {
        if *sid == scope_id {
            parts.push(show_inst(inst));
        }
    }

    // Child Pack scopes only (Type scopes are empty at ASM level — type info erased)
    for (i, s) in ir.scopes.iter().enumerate() {
        if s.parent == Some(scope_id) && s.kind == ScopeKind::Pack {
            parts.push(show_scope_asm(ScopeId(i as u32), ir, scoped_insts));
        }
    }

    if parts.is_empty() {
        format!("Scope[{}]", name)
    } else {
        format!("Scope[{},\n{},\n]", name, parts.join(",\n"))
    }
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

/// Parse + resolve + lower multiple units, format as compact string.
pub fn show_multi(units: Vec<(&str, Vec<&str>, &str)>) -> String {
    let req = ParseReq {
        units: units.into_iter().map(|(name, path, src)| ParseUnit {
            name: name.into(),
            path: path.into_iter().map(|s| s.to_string()).collect(),
            src:  src.to_string(),
        }).collect(),
    };
    let res = parse(req);
    let ir  = resolve(res);
    show_asm(lower(&ir, ""), ir)
}

/// Parse + resolve + lower `input`, format as compact string.
pub fn show(input: &str) -> String {
    let res = parse(ParseReq {
        units: vec![ParseUnit { name: "test".into(), path: vec![], src: input.to_string() }],
    });
    let ir  = resolve(res);
    show_asm(lower(&ir, ""), ir)
}

/// Like `show` but also supplies TypeScript source for hook execution.
pub fn show_with_ts(grd_src: &str, ts_src: &str) -> String {
    let res = parse(ParseReq {
        units: vec![ParseUnit { name: "test".into(), path: vec![], src: grd_src.to_string() }],
    });
    let ir  = resolve(res);
    show_asm(lower(&ir, ts_src), ir)
}

fn show_asm(asm: AsmRes, ir: IrRes) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Pair each named IR inst's scope with its lowered AsmInst (order preserved by lower)
    let ir_named_scopes: Vec<ScopeId> = ir.insts.iter()
        .filter(|i| i.name != "_")
        .map(|i| i.scope)
        .collect();
    let scoped_insts: Vec<(ScopeId, &AsmInst)> = ir_named_scopes.iter().copied()
        .zip(asm.symbol.insts.iter())
        .collect();

    // Scope tree: direct Pack children of root
    for (i, s) in ir.scopes.iter().enumerate().skip(1) {
        if s.parent == Some(ScopeId(0)) && s.kind == ScopeKind::Pack {
            lines.push(show_scope_asm(ScopeId(i as u32), &ir, &scoped_insts));
        }
    }

    // Plans
    lines.extend(asm.plans.iter().map(show_plan));

    norm(&lines.join("\n"))
}
