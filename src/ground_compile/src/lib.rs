mod resolve;

use ground_parse::parse_to_items;

pub use ground_core::{
    Spec, Instance, DeployInstance, ResolvedField, ResolvedValue, ScalarValue, ListEntry,
    ParseError,
};

pub fn compile(sources: &[(&str, &str)]) -> Result<Spec, Vec<ground_core::ParseError>> {
    let mut all_items = Vec::new();
    let mut errors = Vec::new();

    for (path, content) in sources {
        match parse_to_items(path, content) {
            Ok(items)   => all_items.extend(items),
            Err(mut es) => errors.append(&mut es),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    resolve::resolve_file(all_items)
}
