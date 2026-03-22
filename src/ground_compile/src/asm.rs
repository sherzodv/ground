/// Generation tree — output of the lowering pass over `IrRes`.
///
/// All typed indices are replaced by resolved string values.
/// `AsmSymbol` holds every named instance in the program.
/// `AsmDeploy` is self-contained except for `InstRef` values,
/// which are resolved by name against `AsmRes::symbol`.
/// Generators walk `AsmDeploy` + `AsmSymbol` without needing a global arena or `IrRes`.

use crate::ir::*;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AsmRes {
    pub deploys: Vec<AsmDeploy>,
    pub symbol:  AsmSymbol,
}

/// Global symbol table — all named instances in the program.
#[derive(Debug, Clone)]
pub struct AsmSymbol {
    pub insts: Vec<AsmInst>,
}

/// A fully resolved, self-contained deployment context.
#[derive(Debug, Clone)]
pub struct AsmDeploy {
    pub target:  Vec<String>,   // scope path segments, e.g. ["aws"] or ["aws", "eu-central"]
    pub name:    String,        // deployment name, e.g. "prod"
    pub inst:    AsmInst,       // the instance being deployed
    pub fields:  Vec<AsmField>, // deploy-specific fields (separate from inst fields)
}

/// A fully resolved instance — no IDs, type name inlined.
#[derive(Debug, Clone)]
pub struct AsmInst {
    pub type_name: String,
    pub name:      String,
    pub type_hint: Option<String>, // explicit type annotation from source, if present
    pub fields:    Vec<AsmField>,
}

#[derive(Debug, Clone)]
pub struct AsmField {
    pub name:  String,
    pub value: AsmValue,
}

#[derive(Debug, Clone)]
pub enum AsmValue {
    Str(String),
    Int(i64),
    Ref(String),                 // reference primitive (opaque)
    Variant(AsmVariant),         // enum variant with type context
    InstRef(AsmInstRef),         // named instance — full data available in AsmRes::symbol
    Inst(Box<AsmInst>),          // anonymous inline instance (name == "_" in IrRes)
    Path(Vec<AsmValue>),         // multi-segment typed path, e.g. Variant:Variant
    List(Vec<AsmValue>),
}

#[derive(Debug, Clone)]
pub struct AsmVariant {
    pub type_name: String,       // enum type name, e.g. "zone"
    pub value:     String,       // variant string, e.g. "eu-west"
}

#[derive(Debug, Clone)]
pub struct AsmInstRef {
    pub type_name: String,
    pub name:      String,       // key for lookup in AsmRes::symbol
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn lower(ir: &IrRes) -> AsmRes {
    let deploys = ir.deploys.iter().map(|d| lower_deploy(d, ir)).collect();
    let symbol  = lower_symbol(ir);
    AsmRes { deploys, symbol }
}

// ---------------------------------------------------------------------------
// Symbol lowering
// ---------------------------------------------------------------------------

fn lower_symbol(ir: &IrRes) -> AsmSymbol {
    let insts = ir.insts.iter().enumerate()
        .filter(|(_, i)| i.name != "_")
        .map(|(idx, _)| lower_inst(InstId(idx as u32), ir))
        .collect();
    AsmSymbol { insts }
}

// ---------------------------------------------------------------------------
// Deploy lowering
// ---------------------------------------------------------------------------

fn lower_deploy(dep: &IrDeployDef, ir: &IrRes) -> AsmDeploy {
    let target = ref_to_strings(&dep.target, ir);
    let name   = ref_to_strings(&dep.name, ir).join(":");

    let what_id = match find_inst_in_ref(&dep.what, ir) {
        Some(id) => id,
        None     => {
            // Deploy `what` did not resolve to a known instance.
            let placeholder = AsmInst { type_name: String::new(), name: String::new(), type_hint: None, fields: vec![] };
            return AsmDeploy { target, name, inst: placeholder, fields: vec![] };
        }
    };

    let inst   = lower_inst(what_id, ir);
    let fields = dep.fields.iter().map(|f| lower_field(f, ir)).collect();

    AsmDeploy { target, name, inst, fields }
}

// ---------------------------------------------------------------------------
// Instance / field / value lowering
// ---------------------------------------------------------------------------

fn lower_inst(id: InstId, ir: &IrRes) -> AsmInst {
    let inst      = &ir.insts[id.0 as usize];
    let type_name = ir.types[inst.type_id.0 as usize].name.clone().unwrap_or_else(|| "_".into());
    let name      = inst.name.clone();
    let type_hint = inst.type_hint.clone();
    let fields    = inst.fields.iter().map(|f| lower_field(f, ir)).collect();
    AsmInst { type_name, name, type_hint, fields }
}

fn lower_field(f: &IrField, ir: &IrRes) -> AsmField {
    AsmField { name: f.name.clone(), value: lower_value(&f.value, ir) }
}

fn lower_value(v: &IrValue, ir: &IrRes) -> AsmValue {
    match v {
        IrValue::Str(s) => AsmValue::Str(s.clone()),
        IrValue::Int(n) => AsmValue::Int(*n),
        IrValue::Ref(s) => AsmValue::Ref(s.clone()),

        IrValue::Variant(tid, idx) => {
            let td        = &ir.types[tid.0 as usize];
            let type_name = td.name.clone().unwrap_or_else(|| "_".into());
            let value     = match &td.body {
                IrTypeBody::Enum(vs) => vs[*idx as usize].clone(),
                _                    => "?".into(),
            };
            AsmValue::Variant(AsmVariant { type_name, value })
        }

        IrValue::Inst(iid) => {
            let inst = &ir.insts[iid.0 as usize];
            if inst.name == "_" {
                // Anonymous inline instance — embed fully.
                AsmValue::Inst(Box::new(lower_inst(*iid, ir)))
            } else {
                // Named instance — emit a ref; full data lives in AsmSymbol.
                let type_name = ir.types[inst.type_id.0 as usize].name.clone().unwrap_or_else(|| "_".into());
                AsmValue::InstRef(AsmInstRef { type_name, name: inst.name.clone() })
            }
        }

        IrValue::Path(segs) => {
            AsmValue::Path(segs.iter().map(|s| lower_value(s, ir)).collect())
        }

        IrValue::List(items) => {
            AsmValue::List(items.iter().map(|s| lower_value(s, ir)).collect())
        }
    }
}

// ---------------------------------------------------------------------------
// Ref helpers
// ---------------------------------------------------------------------------

/// Flatten an IrRef to a list of name strings.
fn ref_to_strings(r: &IrRef, ir: &IrRes) -> Vec<String> {
    r.segments.iter().map(|seg| match &seg.value {
        IrRefSegValue::Pack(id)  => ir.scopes[id.0 as usize].name.clone().unwrap_or_else(|| "_".into()),
        IrRefSegValue::Type(id)  => ir.types[id.0 as usize].name.clone().unwrap_or_else(|| "_".into()),
        IrRefSegValue::Link(id)  => ir.links[id.0 as usize].name.clone().unwrap_or_else(|| "_".into()),
        IrRefSegValue::Inst(id)  => ir.insts[id.0 as usize].name.clone(),
        IrRefSegValue::Plain(s)  => s.clone(),
    }).collect()
}

/// Find the first InstId in an IrRef, falling back to name-lookup for Plain segments.
fn find_inst_in_ref(r: &IrRef, ir: &IrRes) -> Option<InstId> {
    for seg in &r.segments {
        match &seg.value {
            IrRefSegValue::Inst(id) => return Some(*id),
            IrRefSegValue::Plain(name) => {
                // Deploy `what` may be unresolved if the instance was declared after
                // the deploy in source; fall back to a linear name scan.
                if let Some(idx) = ir.insts.iter().position(|i| &i.name == name) {
                    return Some(InstId(idx as u32));
                }
            }
            _ => {}
        }
    }
    None
}
