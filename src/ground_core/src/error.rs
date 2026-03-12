#[derive(Debug, Clone)]
pub struct ParseError {
    pub path:    String,
    pub line:    usize,
    pub col:     usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}: {}", self.path, self.line, self.col, self.message)
    }
}

impl std::error::Error for ParseError {}
