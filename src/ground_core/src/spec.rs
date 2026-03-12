#[derive(Debug, Clone)]
pub struct Spec {
    pub instances: Vec<Instance>,
    pub deploys:   Vec<DeployInstance>,
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub type_name: String,
    pub name:      String,
    pub fields:    Vec<ResolvedField>,
}

#[derive(Debug, Clone)]
pub struct ResolvedField {
    pub link_name: String,
    pub value:     ResolvedValue,
}

#[derive(Debug, Clone)]
pub enum ResolvedValue {
    Scalar(ScalarValue),
    Composite(Vec<ResolvedField>),
    List(Vec<ListEntry>),
}

#[derive(Debug, Clone)]
pub struct ListEntry {
    pub segments: Vec<ScalarValue>,
}

#[derive(Debug, Clone)]
pub enum ScalarValue {
    Int(i64),
    Bool(bool),
    Str(String),
    Ref(String),
    Enum(String),
    InstanceRef { type_name: String, name: String },
}

#[derive(Debug, Clone)]
pub struct DeployInstance {
    pub name:     String,
    pub provider: String,
    pub alias:    String,
    pub fields:   Vec<ResolvedField>,
}
