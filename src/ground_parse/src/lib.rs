mod error;
mod helpers;

pub use error::ParseError;

use pest::iterators::{Pair, Pairs};
use pest_derive::Parser;

use ground_core::high::*;
use helpers::{fail, finish, Parsed};

#[derive(Parser)]
#[grammar = "src/ground.pest"]
struct GroundParser;

// -- Types ------------------------------------------------------------------

pub type Source<'a>  = (&'a str, &'a str);
pub type ParseReq<'a> = &'a [Source<'a>];
pub type ParseRes    = Result<Spec, Vec<ParseError>>;

// -- Error constructors -----------------------------------------------------

fn err(path: &str, pair: &Pair<Rule>, message: impl Into<String>) -> ParseError {
    let (line, col) = pair.as_span().start_pos().line_col();
    ParseError { path: path.to_string(), line, col, message: message.into() }
}

fn err_at(path: &str, line: usize, col: usize, message: impl Into<String>) -> ParseError {
    ParseError { path: path.to_string(), line, col, message: message.into() }
}

// -- Public -----------------------------------------------------------------

pub fn parse(req: ParseReq<'_>) -> ParseRes {
    let mut spec   = Spec { services: vec![], groups: vec![], regions: vec![], envs: vec![], stacks: vec![], deploys: vec![] };
    let mut errors = Vec::new();

    for (path, content) in req {
        match <GroundParser as pest::Parser<Rule>>::parse(Rule::file, content) {
            Ok(pairs) => {
                let (partial, errs) = parse_file(path, pairs);
                merge_spec(&mut spec, partial);
                errors.extend(errs);
            }
            Err(e) => {
                let (line, col) = match e.line_col {
                    pest::error::LineColLocation::Pos(lc)     => lc,
                    pest::error::LineColLocation::Span(lc, _) => lc,
                };
                errors.push(err_at(path, line, col, e.variant.message()));
            }
        }
    }

    if errors.is_empty() { Ok(spec) } else { Err(errors) }
}

fn merge_spec(dst: &mut Spec, src: Spec) {
    dst.services.extend(src.services);
    dst.groups.extend(src.groups);
    dst.regions.extend(src.regions);
    dst.envs.extend(src.envs);
    dst.stacks.extend(src.stacks);
    dst.deploys.extend(src.deploys);
}

// -- File -------------------------------------------------------------------

fn parse_file(path: &str, pairs: Pairs<Rule>) -> (Spec, Vec<ParseError>) {
    let mut spec   = Spec { services: vec![], groups: vec![], regions: vec![], envs: vec![], stacks: vec![], deploys: vec![] };
    let mut errors = Vec::new();

    let file = match pairs.into_iter().next() {
        Some(p) => p,
        None    => return (spec, vec![err_at(path, 1, 1, "empty source")]),
    };

    for pair in file.into_inner() {
        match pair.as_rule() {
            Rule::service_def => match parse_service(path, pair) {
                (Some(v), es) => { spec.services.push(v); errors.extend(es); }
                (None,    es) => errors.extend(es),
            },
            Rule::group_def => match parse_group(path, pair) {
                (Some(v), es) => { spec.groups.push(v); errors.extend(es); }
                (None,    es) => errors.extend(es),
            },
            Rule::region_def => match parse_region(path, pair) {
                (Some(v), es) => { spec.regions.push(v); errors.extend(es); }
                (None,    es) => errors.extend(es),
            },
            Rule::env_def => match parse_env(path, pair) {
                (Some(v), es) => { spec.envs.push(v); errors.extend(es); }
                (None,    es) => errors.extend(es),
            },
            Rule::stack_def => match parse_stack(path, pair) {
                (Some(v), es) => { spec.stacks.push(v); errors.extend(es); }
                (None,    es) => errors.extend(es),
            },
            Rule::deploy_def => match parse_deploy(path, pair) {
                (Some(v), es) => { spec.deploys.push(v); errors.extend(es); }
                (None,    es) => errors.extend(es),
            },
            Rule::EOI => {}
            r => errors.push(err_at(path, 1, 1, format!("unexpected rule: {r:?}"))),
        }
    }

    (spec, errors)
}

// -- Service ----------------------------------------------------------------

fn parse_service(path: &str, pair: Pair<Rule>) -> Parsed<Service> {
    let (svc_line, svc_col) = pair.as_span().start_pos().line_col();
    let mut inner = pair.into_inner();

    let ident = match inner.next() {
        Some(p) => p,
        None    => return fail(err_at(path, svc_line, svc_col, "expected service name")),
    };
    let name = ident.as_str().to_string();

    let mut image:   Option<String>  = None;
    let mut scaling: Option<Scaling> = None;
    let mut ports:   Vec<Port>       = Vec::new();
    let mut access:  Vec<AccessEntry>= Vec::new();
    let mut errors                   = Vec::new();

    for field in inner {
        let (fline, fcol) = field.as_span().start_pos().line_col();
        match field.as_rule() {
            Rule::service_field => {
                let f = match field.into_inner().next() {
                    Some(f) => f,
                    None    => { errors.push(err_at(path, fline, fcol, "expected field content")); continue; }
                };
                match f.as_rule() {
                    Rule::image_field => {
                        if image.is_some() {
                            errors.push(err(path, &f, format!("service '{name}': duplicate 'image' field")));
                        }
                        match parse_str_val(path, f) {
                            Ok(v)  => image = Some(v),
                            Err(e) => errors.push(e),
                        }
                    }
                    Rule::scaling_field => {
                        if scaling.is_some() {
                            errors.push(err(path, &f, format!("service '{name}': duplicate 'scaling' field")));
                        }
                        let (s, es) = parse_scaling(path, f);
                        errors.extend(es);
                        scaling = s;
                    }
                    Rule::ports_field => {
                        for entry in f.into_inner() {
                            let mut ei = entry.into_inner();
                            let pname  = ei.next().map(|p| p.as_str().to_string()).unwrap_or_default();
                            let number = ei.next()
                                .and_then(|p| p.as_str().parse::<u16>().ok())
                                .unwrap_or(0);
                            ports.push(Port { name: pname, number });
                        }
                    }
                    r => errors.push(err_at(path, fline, fcol, format!("unexpected field: {r:?}"))),
                }
            }
            Rule::access_block => {
                for entry in field.into_inner() {
                    let mut ei  = entry.into_inner();
                    let service = match ei.next() {
                        Some(p) => p.as_str().to_string(),
                        None    => continue,
                    };
                    let entry_ports = ei
                        .filter_map(|p| p.into_inner().next().map(|id| id.as_str().to_string()))
                        .collect();
                    access.push(AccessEntry { service, ports: entry_ports });
                }
            }
            r => errors.push(err_at(path, fline, fcol, format!("unexpected rule: {r:?}"))),
        }
    }

    if image.is_none() {
        errors.push(err_at(path, svc_line, svc_col,
            format!("service '{name}': missing required field 'image'")));
    }
    if let Some(ref s) = scaling {
        if s.min > s.max {
            errors.push(err_at(path, svc_line, svc_col,
                format!("service '{name}': scaling min ({}) > max ({})", s.min, s.max)));
        }
    }

    finish(Service { name, image: image.unwrap_or_default(), scaling, ports, access }, errors)
}

fn parse_scaling(path: &str, pair: Pair<Rule>) -> Parsed<Scaling> {
    let scaling_pair = match pair.into_inner().next() {
        Some(p) => p,
        None    => return fail(err_at(path, 1, 1, "expected scaling block")),
    };

    let mut s      = Scaling { min: 1, max: 1 };
    let mut errors = Vec::new();

    for entry in scaling_pair.into_inner() {
        let mut ei = entry.into_inner();
        let key_pair = match ei.next() {
            Some(p) => p,
            None    => { errors.push(err_at(path, 1, 1, "expected min/max key")); continue; }
        };
        let val_pair = match ei.next() {
            Some(p) => p,
            None    => { errors.push(err(path, &key_pair, "expected scaling value")); continue; }
        };
        match val_pair.as_str().parse::<u32>() {
            Ok(val) => match key_pair.as_str() {
                "min" => s.min = val,
                "max" => s.max = val,
                k     => errors.push(err(path, &key_pair, format!("expected min or max, got: {k}"))),
            },
            Err(_) => errors.push(err(path, &val_pair,
                format!("invalid integer: {}", val_pair.as_str()))),
        }
    }

    finish(s, errors)
}

fn parse_str_val(path: &str, pair: Pair<Rule>) -> Result<String, ParseError> {
    let (line, col) = pair.as_span().start_pos().line_col();
    pair.into_inner().next()
        .map(|p| p.as_str().to_string())
        .ok_or_else(|| err_at(path, line, col, "expected value"))
}

// -- Group ------------------------------------------------------------------

fn parse_group(path: &str, pair: Pair<Rule>) -> Parsed<Group> {
    let (line, col) = pair.as_span().start_pos().line_col();
    let mut inner   = pair.into_inner();

    let name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return fail(err_at(path, line, col, "expected group name")),
    };

    let services = inner.map(|p| p.as_str().to_string()).collect();

    finish(Group { name, services }, vec![])
}

// -- Region -----------------------------------------------------------------

fn parse_region(path: &str, pair: Pair<Rule>) -> Parsed<Region> {
    let (line, col) = pair.as_span().start_pos().line_col();
    let mut inner   = pair.into_inner();

    let name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return fail(err_at(path, line, col, "expected region name")),
    };

    let mut aws    = None;
    let mut zones  = Vec::new();
    let mut errors = Vec::new();

    for p in inner {
        match p.as_rule() {
            Rule::value    => aws = Some(p.as_str().to_string()),
            Rule::zone_def => match parse_zone(path, p) {
                (Some(z), es) => { zones.push(z); errors.extend(es); }
                (None,    es) => errors.extend(es),
            },
            _ => {}
        }
    }

    if aws.is_none() {
        errors.push(err_at(path, line, col, format!("region '{name}': missing 'aws' field")));
    }

    finish(Region { name, aws: aws.unwrap_or_default(), zones }, errors)
}

fn parse_zone(path: &str, pair: Pair<Rule>) -> Parsed<Zone> {
    let (line, col) = pair.as_span().start_pos().line_col();
    let mut inner   = pair.into_inner();

    let id = match inner.next() {
        Some(p) => match p.as_str().parse::<u32>() {
            Ok(v)  => v,
            Err(_) => return fail(err_at(path, line, col, "expected zone id")),
        },
        None => return fail(err_at(path, line, col, "expected zone id")),
    };

    let aws = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return fail(err_at(path, line, col, format!("zone {id}: missing 'aws' value"))),
    };

    finish(Zone { id, aws }, vec![])
}

// -- Env --------------------------------------------------------------------

fn parse_env(path: &str, pair: Pair<Rule>) -> Parsed<Env> {
    let (line, col) = pair.as_span().start_pos().line_col();
    let mut inner   = pair.into_inner();

    let name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return fail(err_at(path, line, col, "expected env name")),
    };

    let mut vars   = Vec::new();
    let mut errors = Vec::new();

    for entry in inner {
        let mut ei = entry.into_inner();
        let k = match ei.next() {
            Some(p) => p.as_str().to_string(),
            None    => { errors.push(err_at(path, line, col, "expected env key")); continue; }
        };
        let v = match ei.next() {
            Some(p) => p.as_str().to_string(),
            None    => { errors.push(err_at(path, line, col, format!("env '{name}': missing value for '{k}'"))); continue; }
        };
        vars.push((k, v));
    }

    finish(Env { name, vars }, errors)
}

// -- Stack ------------------------------------------------------------------

fn parse_stack(path: &str, pair: Pair<Rule>) -> Parsed<Stack> {
    let (line, col) = pair.as_span().start_pos().line_col();
    let mut inner   = pair.into_inner();

    let name = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return fail(err_at(path, line, col, "expected stack name")),
    };

    let mut env    = None;
    let mut region = None;
    let mut zones  = None;
    let mut group  = None;
    let mut errors = Vec::new();

    for field in inner {
        let f = match field.into_inner().next() {
            Some(p) => p,
            None    => continue,
        };
        match f.as_rule() {
            Rule::stack_env_field    => env    = f.into_inner().next().map(|p| p.as_str().to_string()),
            Rule::stack_region_field => region = f.into_inner().next().map(|p| p.as_str().to_string()),
            Rule::stack_group_field  => group  = f.into_inner().next().map(|p| p.as_str().to_string()),
            Rule::stack_zone_field   => {
                let ids = f.into_inner().next()
                    .map(|list| list.into_inner()
                        .filter_map(|p| p.as_str().parse::<u32>().ok())
                        .collect::<Vec<_>>())
                    .unwrap_or_default();
                zones = Some(ids);
            }
            r => errors.push(err_at(path, line, col, format!("unexpected stack field: {r:?}"))),
        }
    }

    for field in [("env", env.is_none()), ("region", region.is_none()), ("zone", zones.is_none()), ("group", group.is_none())] {
        if field.1 {
            errors.push(err_at(path, line, col, format!("stack '{name}': missing required field '{}'", field.0)));
        }
    }

    finish(Stack {
        name,
        env:    env.unwrap_or_default(),
        region: region.unwrap_or_default(),
        zones:  zones.unwrap_or_default(),
        group:  group.unwrap_or_default(),
    }, errors)
}

// -- Deploy -----------------------------------------------------------------

fn parse_deploy(path: &str, pair: Pair<Rule>) -> Parsed<Deploy> {
    let (line, col) = pair.as_span().start_pos().line_col();
    let mut inner   = pair.into_inner();

    let provider_str = match inner.next() {
        Some(p) => p.as_str().to_string(),
        None    => return fail(err_at(path, line, col, "expected provider name after 'to'")),
    };

    let provider = match provider_str.as_str() {
        "aws" => Provider::Aws,
        p     => return fail(err_at(path, line, col, format!("unknown provider '{p}'"))),
    };

    let mut stacks        = Vec::new();
    let mut override_json = None;

    for p in inner {
        match p.as_rule() {
            Rule::stack_list    => stacks.extend(p.into_inner().map(|p| p.as_str().to_string())),
            Rule::override_block => override_json = p.into_inner().next().map(|p| p.as_str().to_string()),
            _                   => {}
        }
    }

    if stacks.is_empty() {
        return fail(err_at(path, line, col, "deploy: stacks list must not be empty"));
    }

    finish(Deploy { provider, stacks, override_json }, vec![])
}
