use std::fmt;

#[derive(Debug)]
pub struct ParseError {
    pub path:    String,
    pub line:    usize,
    pub col:     usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}:{}: {}", self.path, self.line, self.col, self.message)
    }
}

impl std::error::Error for ParseError {}
