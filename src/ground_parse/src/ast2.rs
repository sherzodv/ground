/// AST for RFC 0005 — every node is wrapped in `AstNode<T>` carrying byte-offset location.

// ---------------------------------------------------------------------------
// Location & node wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AstNodeLoc {
    pub unit:  u32,
    pub start: u32,
    pub end:   u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode<T> {
    pub loc:   AstNodeLoc,
    pub inner: T,
}

impl<T> AstNode<T> {
    pub fn new(unit: u32, start: u32, end: u32, inner: T) -> Self {
        AstNode { loc: AstNodeLoc { unit, start, end }, inner }
    }
}

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

/// One segment of a colon-separated reference.
/// `is_opt = true` when the segment was written `(ident)`.
#[derive(Debug, Clone, PartialEq)]
pub struct AstRefSeg {
    pub value:  String,
    pub is_opt: bool,
}

/// A colon-separated reference: `seg0 ":" seg1 ":" …`
#[derive(Debug, Clone, PartialEq)]
pub struct AstRef {
    pub segments: Vec<AstNode<AstRefSeg>>,
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstPrimitive { String, Integer, Reference }

// ---------------------------------------------------------------------------
// Type definitions — the universal type expression
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstTypeDefBody {
    /// Built-in scalar: `string` | `integer` | `reference`
    Primitive(AstPrimitive),
    /// Single reference to an existing type: `postgresql`, `type:region:type:zone`
    Ref(AstRef),
    /// Union of refs: `self | provider | cloud`
    Enum(Vec<AstNode<AstRef>>),
    /// Struct body: `{ link … type … }`
    Struct(Vec<AstNode<AstStructItem>>),
    /// List whose element type is described by the inner `AstTypeDef`: `[ … ]`
    List(Box<AstNode<AstTypeDef>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstTypeDef {
    pub name: Option<AstNode<String>>,
    pub body: AstNode<AstTypeDefBody>,
}

// ---------------------------------------------------------------------------
// Struct items & link definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstStructItem {
    TypeDef(AstNode<AstTypeDef>),
    LinkDef(AstNode<AstLinkDef>),
}

/// A link slot: `link name? = type-expr`
/// Anonymous links enumerate refs for composition; named links are typed fields.
#[derive(Debug, Clone, PartialEq)]
pub struct AstLinkDef {
    pub name: Option<AstNode<String>>,
    pub ty:   AstNode<AstTypeDef>,
}

// ---------------------------------------------------------------------------
// Instances & values
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstValue {
    Str(String),
    /// Integers are left as single-segment refs; the resolve pass interprets them.
    Ref(AstRef),
    List(Vec<AstNode<AstValue>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstField {
    Named { name: AstNode<String>, value: AstNode<AstValue> },
    /// Anonymous value (only valid inside `inst`, not `deploy`)
    Anon(AstNode<AstValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstInst {
    pub type_name: AstNode<String>,
    pub inst_name: AstNode<String>,
    pub fields:    Vec<AstNode<AstField>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstDeploy {
    pub what:   AstNode<AstRef>,
    pub target: AstNode<AstRef>,
    pub name:   AstNode<AstRef>,
    pub fields: Vec<AstNode<AstField>>,
}

// ---------------------------------------------------------------------------
// Top-level definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AstDef {
    Type(AstNode<AstTypeDef>),
    Link(AstNode<AstLinkDef>),
    Inst(AstNode<AstInst>),
    Deploy(AstNode<AstDeploy>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstUnit {
    pub unit: u32,
    pub defs: Vec<AstDef>,
}

// ---------------------------------------------------------------------------
// Parse request / response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AstParseError {
    pub message: String,
    pub loc:     AstNodeLoc,
}

#[derive(Debug)]
pub struct ParseReq {
    /// One entry per source unit (file); the unit index is its position in this vec.
    pub units: Vec<String>,
}

#[derive(Debug)]
pub struct ParseRes {
    pub units:  Vec<AstUnit>,
    pub errors: Vec<AstParseError>,
}
