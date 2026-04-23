/// Golden tests for the ASM lowering pass (`ground_compile::asm`).
///
/// ASM is plan-driven: without `plan` declarations it produces no output.
/// These tests compare the full normalized ASM output.
#[path = "helpers/golden_asm_helpers.rs"]
mod golden_asm_helpers;
use golden_asm_helpers::{norm, show, show_multi, show_with_ts};
use ground_compile::{compile, render_ctx_for_plan, CompileReq, Unit};

fn error_messages(errs: &[ground_compile::CompileError]) -> Vec<String> {
    errs.iter().map(|e| e.message.clone()).collect()
}

// ---------------------------------------------------------------------------
// Basic
// ---------------------------------------------------------------------------

#[test]
fn basic_001() {
    assert_eq!(
        show(
            r##"
            service = { image = reference }
            api = service { image: nginx }
        "##
        ),
        "",
    );
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

#[test]
fn plan_001() {
    assert_eq!(
        show(
            r##"
            service = { image = reference }
            plan api = service { image: nginx }
        "##
        ),
        norm(
            r##"
            Def[api = service { image: Ref(nginx) }]
        "##
        ),
    );
}

#[test]
fn plan_002() {
    assert_eq!(
        show(
            r##"
            scaling = { min = integer  max = integer }
            svc = { scaling = scaling }
            plan my-svc = svc { scaling: def:scaling { min: 2  max: 10 } }
        "##
        ),
        norm(
            r##"
            Def[my-svc = svc { scaling: Def[_ = scaling hint: scaling { min: Int(2), max: Int(10) }] }]
        "##
        ),
    );
}

#[test]
fn plan_003() {
    assert_eq!(
        show(
            r##"
            service = { image = reference }
            plan api = service { image: nginx }
        "##
        ),
        norm(
            r##"
            Def[api = service { image: Ref(nginx) }]
        "##
        ),
    );
}

#[test]
fn optional_001() {
    assert_eq!(
        show(
            r##"
            svc = { note = (string) }
            plan api = svc {}
        "##
        ),
        norm(
            r##"
            Def[api = svc]
        "##
        ),
    );
}

#[test]
fn tuple_001() {
    assert_eq!(
        show(
            r##"
            pair = string -> integer
            svc = { boo = pair }
            plan api = svc { boo: "boo" -> 1 }
        "##
        ),
        norm(
            r##"
            Def[api = svc { boo: Tuple[Str("boo") -> Int(1)] }]
        "##
        ),
    );
}

#[test]
fn tuple_list_001() {
    assert_eq!(
        show(
            r##"
            pair = string -> integer
            svc = { pairs = [ pair ] }
            plan api = svc { pairs: [ "boo" -> 1  "foo" -> 2 ] }
        "##
        ),
        norm(
            r##"
            Def[api = svc { pairs: List[Tuple[Str("boo") -> Int(1)], Tuple[Str("foo") -> Int(2)]] }]
        "##
        ),
    );
}

#[test]
fn primitive_list_string_literals_001() {
    assert_eq!(
        show(
            r##"
            sg = { cidr_blocks = [ ipv4net ] }
            plan app = sg { cidr_blocks: [ "10.0.0.0/16"  "10.1.0.0/16" ] }
        "##
        ),
        norm(
            r##"
            Def[app = sg { cidr_blocks: List[Str("10.0.0.0/16"), Str("10.1.0.0/16")] }]
        "##
        ),
    );
}

#[test]
fn nested_enum_cross_pack_composition_001() {
    assert_eq!(
        show_multi(vec![
            (
                "tf",
                vec!["std", "aws"],
                r##"
                eip = {
                    def domain = vpc | standard
                    domain = def:domain
                }
            "##,
            ),
            (
                "test",
                vec![],
                r##"
                use std:aws

                plan nat = aws:tf:eip {
                    domain: vpc
                }
            "##,
            ),
        ]),
        norm(
            r##"
            Def[nat = eip { domain: Variant(domain, "vpc") }]
        "##
        ),
    );
}

#[test]
fn nested_def_in_input_block_001() {
    assert_eq!(
        show_with_ts(
            r##"
            def eip {
                def domain = vpc | standard
                domain = def:domain
            } = make_eip {
                value = string
            }

            plan nat = eip {
                domain: vpc
            }
        "##,
            r##"
            function make_eip(i) {
                return { value: i.domain };
            }
        "##,
        ),
        norm(
            r##"
            Def[nat = eip { domain: Variant(domain, "vpc"), value: Str("vpc") }]
        "##
        ),
    );
}

#[test]
fn nested_def_in_input_block_enum_variant_shadowed_by_def_001() {
    assert_eq!(
        show_with_ts(
            r##"
            def vpc

            def eip {
                def domain = vpc | standard
                domain = def:domain
            } = make_eip {
                value = string
            }

            plan nat = eip {
                domain: vpc
            }
        "##,
            r##"
            function make_eip(i) {
                return { value: i.domain };
            }
        "##,
        ),
        norm(
            r##"
            Def[nat = eip { domain: Variant(domain, "vpc"), value: Str("vpc") }]
        "##
        ),
    );
}

// ---------------------------------------------------------------------------
// Mapper
// ---------------------------------------------------------------------------

#[test]
fn mapper_001() {
    let grd = r#"
        def tag = make_tag { name = string  value = string  enabled = boolean }
        plan ground-managed = tag {}
    "#;
    let ts = r#"
        function make_tag(_i) {
            return { name: "ground-managed", value: "true", enabled: true };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[ground-managed = tag { name: Str("ground-managed"), value: Str("true"), enabled: Bool(true) }]
        "##
        ),
    );
}

#[test]
fn mapper_output_json_null_001() {
    let grd = r#"
        def route = make_route {
            gateway_id = reference
        }

        plan main = route {}
    "#;
    let ts = r#"
        function make_route(_i) {
            return { gateway_id: null };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[main = route { gateway_id: Null }]
        "##
        ),
    );
}

#[test]
fn mapper_002() {
    let grd = r#"
        endpoint = { host = string  port = integer }
        def node { name = string } = make_node { ep = endpoint }
        plan api = node { name: "api" }
    "#;
    let ts = r#"
        function make_node(i) {
            return { ep: { host: i.name + ".internal", port: 8080 } };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[api = node { name: Str("api"), ep: Def[_ =  { host: Str("api.internal"), port: Int(8080) }] }]
        "##
        ),
    );
}

#[test]
fn mapper_002a_ipv4net_input_is_structured() {
    let grd = r#"
        network = { range = ipv4net }
        def deploy { net = network } = make_deploy { prefix = integer  first = integer }
        plan prod = deploy { net: def:network { range: "10.42.0.0/16" } }
    "#;
    let ts = r#"
        function make_deploy(i) {
            return {
                prefix: i.net.range.prefix,
                first: i.net.range.addr.a,
            };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[prod = deploy { net: Def[_ = network hint: network { range: Str("10.42.0.0/16") }], prefix: Int(16), first: Int(10) }]
        "##
        ),
    );
}

#[test]
fn mapper_003() {
    let grd = r#"
        def tags { prefix = string  count = integer } = make_tags { items = string }
        plan my-tags = tags { prefix: "svc"  count: 3 }
    "#;
    let ts = r#"
        function make_tags(i) {
            const items = [];
            for (let k = 0; k < i.count; k++) items.push(i.prefix + "-" + k);
            return { items };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[my-tags = tags { prefix: Str("svc"), count: Int(3), items: List[Str("svc-0"), Str("svc-1"), Str("svc-2")] }]
        "##
        ),
    );
}

#[test]
fn mapper_004() {
    let grd = r#"
        def rectangle { width = integer  height = integer } = mk_rect { area = integer }
        plan r1 {} = rectangle { width: 1  height: 2 }
    "#;
    let ts = r#"
        function mk_rect(i) {
            return { area: i.width * i.height };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[r1 = rectangle { width: Int(1), height: Int(2), area: Int(2) }]
        "##
        ),
    );
}

#[test]
fn mapper_005() {
    let grd = r#"
        def rectangle { width = integer  height = integer } = mk_rect { area = integer }
        plan r1 {} = rectangle { width: 1  height: 2 }
    "#;
    let ts = r#"
        function mk_rect(i) {
            return { area: i.width * i.height };
        }
        function rectangle(resolved, _input) {
            return { area: resolved.area + 10 };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[r1 = rectangle { width: Int(1), height: Int(2), area: Int(12) }]
        "##
        ),
    );
}

#[test]
fn mapper_inherits_explicit_base_mapper_001() {
    let grd = r#"
        pack test

        def thing {
          name = string
          tags = ([ string -> string ])
        }

        def platform {
          items = [ thing ]
        } = make_platform

        item = thing {
          name: "main"
          tags: [ "Name" -> "main" ]
        }

        plan infra = platform {
          items: [ item ]
        }
    "#;
    let ts = r#"
        function make_platform(input) {
            return {
                items: (input.items || []).map((it) => ({
                    ...it,
                    tags: {
                        ...(it.tags || {}),
                        "ground-managed": "true",
                    },
                })),
            };
        }
    "#;

    let res = compile(CompileReq {
        units: vec![Unit {
            name: "test".into(),
            path: vec![],
            src: grd.into(),
            ts_src: Some(ts.into()),
        }],
    });
    assert!(res.errors.is_empty(), "{:#?}", error_messages(&res.errors));

    let ctx = render_ctx_for_plan(&res, "infra").expect("render ctx");
    let item = &ctx["defs"][0]["as_obj"]["items"][0];
    assert_eq!(
        item["as_obj"]["tags"]["ground-managed"].as_str(),
        Some("true")
    );
}

#[test]
fn render_ctx_prefers_local_named_instance_over_base_def_001() {
    let std_src = r#"
        pack std:aws:tf

        def subnet {
          cidr_block = string
        }

        def platform {
          subnets = [ subnet ]
        } = make_platform
    "#;
    let std_ts = r#"
        function make_platform(input) {
            return input;
        }
    "#;
    let test_src = r#"
        pack test

        use std:aws

        subnet = aws:tf:subnet {
          cidr_block: "10.0.1.0/24"
        }

        plan infra = aws:tf:platform {
          subnets: [ subnet ]
        }
    "#;

    let res = compile(CompileReq {
        units: vec![
            Unit {
                name: "aws".into(),
                path: vec!["std".into()],
                src: std_src.into(),
                ts_src: Some(std_ts.into()),
            },
            Unit {
                name: "test".into(),
                path: vec![],
                src: test_src.into(),
                ts_src: None,
            },
        ],
    });
    assert!(res.errors.is_empty(), "{:#?}", error_messages(&res.errors));

    let ctx = render_ctx_for_plan(&res, "infra").expect("render ctx");
    let subnet = &ctx["defs"][0]["as_obj"]["subnets"][0];
    assert_eq!(subnet["name"].as_str(), Some("subnet"));
    assert_eq!(subnet["as_obj"]["cidr_block"].as_str(), Some("10.0.1.0/24"));
}

#[test]
fn mapper_006() {
    let grd = r#"
        deploy = { region = [ string ] }
        plan prd = deploy { region: [ eu-central:1  eu-central:2 ] }
    "#;
    let ts = r#"
        function deploy(resolved, _input) {
            return {
                aws_region: "eu-central-1",
                zones: resolved.region.map((raw, idx) => ({
                    n: String(idx + 1),
                    az: "eu-central-1" + String.fromCharCode(97 + idx),
                })),
            };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[prd = deploy { region: List[Str("eu-central:1"), Str("eu-central:2")], aws_region: Str("eu-central-1"), zones: List[Def[_ =  { n: Str("1"), az: Str("eu-central-1a") }], Def[_ =  { n: Str("2"), az: Str("eu-central-1b") }]] }]
        "##
        ),
    );
}

#[test]
fn mapper_007() {
    let grd = r#"
        secret
        service = { access = [ secret ] }
        stack = { = [ def:service | def:secret ] }
        def deploy { stack = stack } = map_deploy { names = string }
        svc = service { access: [ secret ] }
        all = stack { svc  secret }
        plan prd = deploy { stack: all }
    "#;
    let ts = r#"
        function map_deploy(i) {
            return {
                names: i.stack._.map((item) => JSON.stringify(item)),
            };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[prd = deploy { stack: DefRef(stack, all), names: List[Str("\"svc\""), Str("\"secret\"")] }]
        "##
        ),
    );
}

#[test]
fn mapper_008() {
    let grd = r#"
        access = read | write
        bucket = { name = string  access = access }
        database = { engine = string }
        service = { port = http | grpc  access = [ database | bucket:(access) | secret ] }
        def secret
        stack = { = [ def:service | def:database | def:bucket | def:secret ] }
        size = small | medium
        scaling = { min = integer  max = integer }
        service_config = { service = service  size = size  scaling = scaling }
        database_config = { database = database  size = size  storage = integer }
        def deploy { stack = stack  services = [ service_config ]  databases = [ database_config ] } = map_deploy {
            stack_names = string
            service_name = string
            service_access_0 = string
            database_name = string
        }

        main = database { engine: "pg" }
        media-bucket = bucket { name: "media-content"  access: write }
        media-secret secret
        api = service { port: http  access: [ main  media-bucket:write  media-secret ] }
        all = stack { api  main  media-bucket  media-secret }
        plan prd = deploy {
            stack: all
            services: [ { service: api  size: medium  scaling: { min: 1  max: 2 } } ]
            databases: [ { database: main  size: medium  storage: 20 } ]
        }
    "#;
    let ts = r#"
        function map_deploy(i) {
            return {
                stack_names: JSON.stringify(i.stack._),
                service_name: JSON.stringify(i.services[0].service.__ground_name),
                service_access_0: JSON.stringify(i.services[0].service.access[0]),
                database_name: JSON.stringify(i.databases[0].database.__ground_name),
            };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[prd = deploy { stack: DefRef(stack, all), services: List[Def[_ = service_config { service: DefRef(service, api), size: Variant(size, "medium"), scaling: Def[_ = scaling { min: Int(1), max: Int(2) }] }]], databases: List[Def[_ = database_config { database: DefRef(database, main), size: Variant(size, "medium"), storage: Int(20) }]], stack_names: Str("[{\"__ground_name\":\"api\",\"port\":\"http\",\"access\":[{\"__ground_name\":\"main\",\"engine\":\"pg\"},[{\"__ground_name\":\"media-bucket\",\"name\":\"media-content\",\"access\":\"write\"},\"write\"],{\"__ground_name\":\"media-secret\"}]},{\"__ground_name\":\"main\",\"engine\":\"pg\"},{\"__ground_name\":\"media-bucket\",\"name\":\"media-content\",\"access\":\"write\"},{\"__ground_name\":\"media-secret\"}]"), service_name: Str("\"api\""), service_access_0: Str("{\"__ground_name\":\"main\",\"engine\":\"pg\"}"), database_name: Str("\"main\"") }]
        "##
        ),
    );
}

#[test]
fn mapper_009() {
    let grd = r#"
        service = { port = http | grpc }
        stack = { = [ def:service ] }
        size = small | medium
        scaling = { min = integer  max = integer }
        service_config = { service = service  size = size  scaling = scaling }
        def deploy { region = [ string ]  stack = stack  services = [ service_config ] } = {
            seen_resolved = string
            seen_stack = string
            seen_input = string
        }

        api = service { port: http }
        all = stack { api }
        plan prd = deploy {
            region: [ eu-central:1 ]
            stack: all
            services: [ { service: api  size: medium  scaling: { min: 1  max: 2 } } ]
        }
    "#;
    let ts = r#"
        function deploy(resolved, input) {
            return {
                seen_resolved: JSON.stringify(resolved.services || null),
                seen_stack: JSON.stringify((resolved.stack && resolved.stack._) || null),
                seen_input: JSON.stringify(input.services || null),
            };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[prd = deploy { region: List[Str("eu-central:1")], stack: DefRef(stack, all), services: List[Def[_ = service_config { service: DefRef(service, api), size: Variant(size, "medium"), scaling: Def[_ = scaling { min: Int(1), max: Int(2) }] }]], seen_resolved: Str("[{\"__ground_name\":\"_\",\"service\":{\"__ground_name\":\"api\",\"__ground_type\":\"service\"},\"size\":\"medium\",\"scaling\":{\"__ground_name\":\"_\",\"min\":1,\"max\":2}}]"), seen_stack: Str("null"), seen_input: Str("null") }]
        "##
        ),
    );
}

#[test]
fn mapper_010() {
    let grd = r#"
        service = { port = http | grpc }
        database = { engine = string }
        size = small | medium | large
        scaling = { min = integer  max = integer }
        service_defaults = { size = size  scaling = scaling }
        database_defaults = { size = size  storage = integer }
        service_config = { service = service  size = size  scaling = scaling }
        database_config = { database = database  size = size  storage = integer }
        compute_pool = {
            services = [ service ]
            databases = [ database ]
            service_defaults = service_defaults
            database_defaults = database_defaults
        }
        def deploy {
            pool = compute_pool
            service_overrides = [ service_config ]
            database_overrides = [ database_config ]
        } = map_deploy {
            api_size = string
            api_min = integer
            db_size = string
            db_storage = integer
        }

        api = service { port: http }
        main = database { engine: "pg" }
        app = compute_pool {
            services: [ api ]
            databases: [ main ]
            service_defaults: { size: small  scaling: { min: 1  max: 1 } }
            database_defaults: { size: medium  storage: 20 }
        }
        plan prd = deploy {
            pool: app
            service_overrides: [ { service: api  size: large  scaling: { min: 2  max: 4 } } ]
        }
    "#;
    let ts = r#"
        function mergeService(pool, override) {
            return {
                size: (override && override.size) || (pool.service_defaults && pool.service_defaults.size) || null,
                scaling: {
                    min: (override && override.scaling && override.scaling.min)
                      || (pool.service_defaults && pool.service_defaults.scaling && pool.service_defaults.scaling.min)
                      || null,
                },
            };
        }
        function mergeDatabase(pool, override) {
            return {
                size: (override && override.size) || (pool.database_defaults && pool.database_defaults.size) || null,
                storage: (override && override.storage)
                  || (pool.database_defaults && pool.database_defaults.storage)
                  || null,
            };
        }
        function map_deploy(i) {
            const svc = mergeService(i.pool, i.service_overrides[0]);
            const db = mergeDatabase(i.pool, (i.database_overrides || [])[0] || null);
            return {
                api_size: svc.size,
                api_min: svc.scaling.min,
                db_size: db.size,
                db_storage: db.storage,
            };
        }
    "#;
    assert_eq!(
        show_with_ts(grd, ts),
        norm(
            r##"
            Def[prd = deploy { pool: DefRef(compute_pool, app), service_overrides: List[Def[_ = service_config { service: DefRef(service, api), size: Variant(size, "large"), scaling: Def[_ = scaling { min: Int(2), max: Int(4) }] }]], api_size: Str("large"), api_min: Int(2), db_size: Str("medium"), db_storage: Int(20) }]
        "##
        ),
    );
}

#[test]
fn mapper_011() {
    assert_eq!(
        show_multi(vec![
            ("std", vec![], r#"service = { image = reference }"#),
            (
                "app",
                vec![],
                r#"
                    use std:service
                    plan api = service { image: nginx }
                "#
            ),
        ]),
        norm(
            r##"
            Def[api = service { image: Ref(nginx) }]
        "##
        ),
    );
}

#[test]
fn mapper_012() {
    assert_eq!(
        show_multi(vec![
            (
                "std",
                vec![],
                r#"
                    service = { image = reference }
                    database = { engine = string }
                "#
            ),
            (
                "app",
                vec![],
                r#"
                    use std:def:*
                    plan api = service { image: nginx }
                "#
            ),
        ]),
        norm(
            r##"
            Def[api = service { image: Ref(nginx) }]
        "##
        ),
    );
}
