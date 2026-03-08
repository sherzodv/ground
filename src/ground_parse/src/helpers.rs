use crate::ParseError;

/// One optional value alongside all errors discovered while parsing it.
pub(crate) type Parsed<T> = (Option<T>, Vec<ParseError>);

/// Infallible result — value with no errors.
#[allow(dead_code)]
pub(crate) fn ok<T>(v: T) -> Parsed<T> {
    (Some(v), vec![])
}

/// Structural failure — cannot continue parsing this item.
pub(crate) fn fail<T>(e: ParseError) -> Parsed<T> {
    (None, vec![e])
}

/// Seal a function: succeed if no errors were collected, fail otherwise.
pub(crate) fn finish<T>(value: T, errors: Vec<ParseError>) -> Parsed<T> {
    if errors.is_empty() { (Some(value), vec![]) } else { (None, errors) }
}

