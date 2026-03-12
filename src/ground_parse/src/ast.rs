// All pub — types are used by ground_compile

#[derive(Debug)]
pub struct AstFile {
    pub items: Vec<AstItem>,
}

#[derive(Debug)]
pub enum AstItem {
    TypeDecl(AstTypeDecl),
    LinkDecl(AstLinkDecl),
    Instance(AstInstance),
    Deploy(AstDeploy),
}

#[derive(Debug)]
pub struct AstTypeDecl {
    pub name: String,
    pub body: AstTypeBody,
    pub line: usize,
    pub col:  usize,
}

#[derive(Debug)]
pub enum AstTypeBody {
    Primitive(String),
    Enum(Vec<String>),
    Composite(Vec<AstCompositeMember>),
}

#[derive(Debug)]
pub enum AstCompositeMember {
    Bare    { link_name: String, line: usize, col: usize },
    Inline  { link_name: String, type_expr: String, line: usize, col: usize },
    Default { link_name: String, default: AstDefaultVal, line: usize, col: usize },
}

#[derive(Debug)]
pub enum AstDefaultVal {
    Block(Vec<(String, String)>),  // field_name → raw value
    Single(String),
}

#[derive(Debug)]
pub struct AstLinkDecl {
    pub name:      String,
    pub type_expr: String,  // raw string to parse in resolve
    pub line:      usize,
    pub col:       usize,
}

#[derive(Debug)]
pub struct AstInstance {
    pub type_name: String,
    pub name:      String,
    pub fields:    Vec<AstField>,
    pub line:      usize,
    pub col:       usize,
}

#[derive(Debug)]
pub struct AstDeploy {
    pub name:     String,
    pub provider: String,
    pub alias:    String,
    pub fields:   Vec<AstField>,
    pub line:     usize,
    pub col:      usize,
}

#[derive(Debug)]
pub struct AstField {
    pub link_name: String,
    pub value:     AstFieldValue,
    pub line:      usize,
    pub col:       usize,
}

#[derive(Debug)]
pub enum AstFieldValue {
    Single(String),
    List(Vec<String>),
    Block(Vec<AstField>),
}
