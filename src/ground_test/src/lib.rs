#[cfg(test)]
mod parse {
    use ground_parse::parse;

    fn ok(input: &str) {
        parse(&[("test", input)]).unwrap_or_else(|es| panic!("{}", es[0]));
    }

    fn err(input: &str) -> Vec<ground_parse::ParseError> {
        parse(&[("test", input)]).expect_err("expected parse errors but got Ok")
    }

    #[test]
    fn test_empty_service_missing_image() {
        let es = err("service svc-api {}");
        assert!(es.iter().any(|e| e.message.contains("missing required field 'image'")));
    }

    #[test]
    fn test_service_with_image_and_scaling() {
        ok("service svc-api { image: svc-api:prod scaling: { min: 2, max: 10 } }");
    }

    #[test]
    fn test_multiple_services() {
        ok(r#"
            service svc-core { image: svc-core:prod }
            service svc-pay  { image: svc-pay:prod  }
        "#);
    }

    #[test]
    fn test_multiple_files() {
        let spec = parse(&[
            ("svc-core.grd", "service svc-core { image: svc-core:prod }"),
            ("svc-pay.grd",  "service svc-pay  { image: svc-pay:prod  }"),
        ]).unwrap();
        assert_eq!(spec.services.len(), 2);
    }

    #[test]
    fn test_duplicate_image_field() {
        let es = err("service svc-api { image: a:1 image: b:2 }");
        assert!(es.iter().any(|e| e.message.contains("duplicate 'image'")));
    }

    #[test]
    fn test_scaling_min_greater_than_max() {
        let es = err("service svc-api { image: svc-api:prod scaling: { min: 10, max: 2 } }");
        assert!(es.iter().any(|e| e.message.contains("min") && e.message.contains("max")));
    }

    #[test]
    fn test_multiple_errors_collected() {
        let es = err("service a {} service b {}");
        assert_eq!(es.iter().filter(|e| e.message.contains("missing required field 'image'")).count(), 2);
    }

    #[test]
    fn test_service_ports() {
        let spec = parse(&[("test", "service svc { image: svc:prod ports: { http: 8080, grpc: 9090 } }")]).unwrap();
        assert_eq!(spec.services[0].ports.len(), 2);
        assert_eq!(spec.services[0].ports[0].name,   "http");
        assert_eq!(spec.services[0].ports[0].number, 8080);
        assert_eq!(spec.services[0].ports[1].name,   "grpc");
        assert_eq!(spec.services[0].ports[1].number, 9090);
    }

    #[test]
    fn test_service_access() {
        let spec = parse(&[("test", r#"
            service svc-api {
              image: svc-api:prod
              access { svc-internal: http, grpc }
            }
            service svc-internal { image: svc-internal:prod ports: { http: 8080, grpc: 9090 } }
        "#)]).unwrap();
        let access = &spec.services[0].access;
        assert_eq!(access.len(), 1);
        assert_eq!(access[0].target, "svc-internal");
        assert_eq!(access[0].ports,   ["http", "grpc"]);
    }

    #[test]
    fn test_access_compile_ingress() {
        let spec = parse(&[("test", r#"
            service svc-api      { image: svc-api:prod      access { svc-internal: http } }
            service svc-internal { image: svc-internal:prod ports: { http: 8080 } }
            group g { svc-api svc-internal }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let plan = ground_core::compile::compile(&spec).unwrap().remove(0).1;
        assert_eq!(plan.ingress_rules.len(), 1);
        assert_eq!(plan.ingress_rules[0].source_network, "svc-api");
        assert_eq!(plan.ingress_rules[0].target_network, "svc-internal");
        assert_eq!(plan.ingress_rules[0].ports, [8080]);
    }

    #[test]
    fn test_access_unknown_service() {
        let spec = parse(&[("test", r#"
            service svc-api { image: svc-api:prod access { svc-missing: http } }
            group g { svc-api }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let es = ground_core::compile::compile(&spec).unwrap_err();
        assert!(es.iter().any(|e| e.contains("unknown target")));
    }

    #[test]
    fn test_access_unknown_port() {
        let spec = parse(&[("test", r#"
            service svc-api      { image: svc-api:prod      access { svc-internal: missing } }
            service svc-internal { image: svc-internal:prod ports: { http: 8080 } }
            group g { svc-api svc-internal }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let es = ground_core::compile::compile(&spec).unwrap_err();
        assert!(es.iter().any(|e| e.contains("unknown port")));
    }

    #[test]
    fn test_group() {
        let spec = parse(&[("test", "service svc-api { image: svc-api:prod } group backend { svc-api }")]).unwrap();
        assert_eq!(spec.groups.len(), 1);
        assert_eq!(spec.groups[0].name, "backend");
        assert_eq!(spec.groups[0].members, ["svc-api"]);
    }

    #[test]
    fn test_region_zone() {
        let spec = parse(&[("test", r#"
            region us-east {
              aws: us-east-1
              zone 1 { aws: us-east-1a }
              zone 2 { aws: us-east-1b }
            }
        "#)]).unwrap();
        assert_eq!(spec.regions.len(), 1);
        assert_eq!(spec.regions[0].aws, "us-east-1");
        assert_eq!(spec.regions[0].zones.len(), 2);
    }

    #[test]
    fn test_env() {
        let spec = parse(&[("test", "env prod { LOG_LEVEL: info }")]).unwrap();
        assert_eq!(spec.envs.len(), 1);
        assert_eq!(spec.envs[0].vars, [("LOG_LEVEL".into(), "info".into())]);
    }

    #[test]
    fn test_stack_missing_fields() {
        let es = err("stack prod { region: us-east }");
        assert!(es.iter().any(|e| e.message.contains("missing required field")));
    }

    #[test]
    fn test_deploy_unknown_provider() {
        let es = err("deploy to gcp { stacks: [prod] }");
        assert!(es.iter().any(|e| e.message.contains("unknown provider")));
    }

    #[test]
    fn test_multiple_deploys() {
        let spec = parse(&[("test", r#"
            deploy to aws { stacks: [prod] }
            deploy to aws { stacks: [staging] }
        "#)]).unwrap();
        assert_eq!(spec.deploys.len(), 2);
    }

    #[test]
    fn test_database_minimal() {
        let spec = parse(&[("test", "database db-main { engine: postgres }")]).unwrap();
        assert_eq!(spec.rdbs.len(), 1);
        assert_eq!(spec.rdbs[0].name, "db-main");
        assert!(matches!(spec.rdbs[0].engine, ground_core::high::RdbEngine::Postgres));
        assert!(spec.rdbs[0].version.is_none());
        assert!(spec.rdbs[0].size.is_none());
        assert!(spec.rdbs[0].storage.is_none());
    }

    #[test]
    fn test_database_all_fields() {
        let spec = parse(&[("test", r#"
            database db-main {
                engine:  postgres
                version: 15
                size:    large
                storage: 100
            }
        "#)]).unwrap();
        let db = &spec.rdbs[0];
        assert_eq!(db.version, Some(15));
        assert!(matches!(db.size, Some(ground_core::high::RdbSize::Large)));
        assert_eq!(db.storage, Some(100));
    }

    #[test]
    fn test_database_missing_engine() {
        let es = err("database db-main {}");
        assert!(es.iter().any(|e| e.message.contains("missing required field 'engine'")));
    }

    #[test]
    fn test_service_access_database() {
        let spec = parse(&[("test", r#"
            database db-main { engine: postgres }
            service svc-api { image: svc-api:prod access { db-main } }
        "#)]).unwrap();
        assert_eq!(spec.services[0].access[0].target, "db-main");
    }

    #[test]
    fn test_access_database_compile() {
        let spec = parse(&[("test", r#"
            database db-main { engine: postgres }
            service svc-api { image: svc-api:prod access { db-main } }
            group g { svc-api db-main }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let plan = ground_core::compile::compile(&spec).unwrap().remove(0).1;
        assert_eq!(plan.db_access_rules.len(), 1);
        assert_eq!(plan.db_access_rules[0].service_network, "svc-api");
        assert_eq!(plan.db_access_rules[0].rdb, "db-main");
        assert_eq!(plan.workloads[0].rdb_access, ["db-main"]);
    }

    #[test]
    fn test_compute_parse() {
        let spec = parse(&[("test", r#"
            compute spot {
              cpu:    512
              memory: 1024
              aws:    fargate:spot
            }
        "#)]).unwrap();
        assert_eq!(spec.computes.len(), 1);
        let c = &spec.computes[0];
        assert_eq!(c.name,   "spot");
        assert_eq!(c.cpu,    Some(512));
        assert_eq!(c.memory, Some(1024));
        assert_eq!(c.aws,    Some("fargate:spot".into()));
    }

    #[test]
    fn test_compute_service_ref() {
        let spec = parse(&[("test", r#"
            compute spot { cpu: 512 memory: 1024 aws: fargate:spot }
            service svc-api { image: svc-api:prod compute: spot }
        "#)]).unwrap();
        assert_eq!(spec.services[0].compute, Some("spot".into()));
    }

    #[test]
    fn test_compute_rdb_ref() {
        let spec = parse(&[("test", r#"
            compute db-large { aws: db.r6g.large }
            database db-main { engine: postgres compute: db-large }
        "#)]).unwrap();
        assert_eq!(spec.rdbs[0].compute, Some("db-large".into()));
    }

    #[test]
    fn test_compute_resolves_in_service() {
        let spec = parse(&[("test", r#"
            compute spot { cpu: 512 memory: 1024 aws: fargate:spot }
            service svc-api { image: svc-api:prod compute: spot }
            group g { svc-api }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let plan = ground_core::compile::compile(&spec).unwrap().remove(0).1;
        let wl = &plan.workloads[0];
        assert_eq!(wl.compute.cpu,    512);
        assert_eq!(wl.compute.memory, 1024);
        assert_eq!(wl.compute.aws,    "fargate:spot");
    }

    #[test]
    fn test_compute_rdb_instance_class() {
        let spec = parse(&[("test", r#"
            compute db-large { aws: db.r6g.large }
            database db-main { engine: postgres compute: db-large }
            group g { db-main }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let plan = ground_core::compile::compile(&spec).unwrap().remove(0).1;
        assert_eq!(plan.rdbs[0].instance_class, "db.r6g.large");
    }

    #[test]
    fn test_compute_default_when_absent() {
        let spec = parse(&[("test", r#"
            service svc-api { image: svc-api:prod }
            group g { svc-api }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let plan = ground_core::compile::compile(&spec).unwrap().remove(0).1;
        let wl = &plan.workloads[0];
        assert_eq!(wl.compute.cpu,    256);
        assert_eq!(wl.compute.memory, 512);
        assert_eq!(wl.compute.aws,    "FARGATE");
    }

    #[test]
    fn test_compute_unknown_ref_error() {
        let spec = parse(&[("test", r#"
            service svc-api { image: svc-api:prod compute: missing }
            group g { svc-api }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let es = ground_core::compile::compile(&spec).unwrap_err();
        assert!(es.iter().any(|e| e.contains("unknown compute")));
    }

    #[test]
    fn test_access_unknown_database() {
        let spec = parse(&[("test", r#"
            service svc-api { image: svc-api:prod access { db-missing } }
            group g { svc-api }
            region r { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env e {}
            stack s { env: e region: r zone: [1] group: g }
            deploy to aws { stacks: [s] }
        "#)]).unwrap();
        let es = ground_core::compile::compile(&spec).unwrap_err();
        assert!(es.iter().any(|e| e.contains("unknown target")));
    }
}


/// File-based golden tests.
///
/// Each `.md` file in `fixtures/` contains a ` ```ground ` block with the
/// input and a ` ```json ` block with the expected Terraform JSON output.
///
/// To regenerate expected output after a generator change:
///   UPDATE_FIXTURES=1 cargo test -- files
#[cfg(test)]
mod files {
    use std::{fs, path::Path};

    use ground_be_terra::terra_gen::aws;
    use ground_parse::parse;
    use serde_json::Value;

    fn extract_block<'a>(content: &'a str, lang: &str) -> Option<&'a str> {
        let open = format!("```{lang}\n");
        let (_, after) = content.split_once(open.as_str())?;
        let end = after.find("\n```")?;
        Some(&after[..end])
    }

    fn update_json_block(content: &str, actual_str: &str) -> String {
        let open  = "```json\n";
        let close = "\n```";
        if let Some((before, after_open)) = content.split_once(open) {
            let after_close = after_open.find(close)
                .map(|i| &after_open[i + close.len()..])
                .unwrap_or("");
            format!("{before}{open}{actual_str}{close}{after_close}")
        } else {
            format!("{}\n{open}{actual_str}{close}\n", content.trim_end())
        }
    }

    fn run_fixture(path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;

        // Strip ## Explain section — human docs, not part of the test
        let testable = match content.split_once("\n## Explain") {
            Some((before, _)) => before,
            None              => &content,
        };

        let input = extract_block(testable, "ground")
            .ok_or_else(|| format!("{}: missing ```ground block", path.display()))?;

        let spec = parse(&[(path.to_str().unwrap(), input)])
            .map_err(|es| format!("{}: {}", path.display(), es[0]))?;

        let plan = ground_core::compile::compile(&spec)
            .map_err(|es| format!("{}: {}", path.display(), es[0]))?
            .remove(0).1;

        let actual_str = aws::generate(&plan);

        if std::env::var("UPDATE_FIXTURES").is_ok() {
            // update_json_block operates on full content so ## Explain is preserved
            let updated = update_json_block(&content, &actual_str);
            fs::write(path, updated).map_err(|e| format!("{}: {e}", path.display()))?;
            return Ok(());
        }

        let expected_str = extract_block(testable, "json")
            .ok_or_else(|| format!(
                "{}: missing ```json block; run with UPDATE_FIXTURES=1 to generate",
                path.display()
            ))?;

        if expected_str.trim().is_empty() {
            return Err(format!(
                "{}: ```json block is empty; run with UPDATE_FIXTURES=1 to generate",
                path.display()
            ));
        }

        let actual: Value   = serde_json::from_str(&actual_str).unwrap();
        let expected: Value = serde_json::from_str(expected_str)
            .map_err(|e| format!("{}: invalid expected JSON: {e}", path.display()))?;

        if actual != expected {
            return Err(format!(
                "{}: output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
                path.display(),
                serde_json::to_string_pretty(&expected).unwrap(),
                serde_json::to_string_pretty(&actual).unwrap(),
            ));
        }

        Ok(())
    }

    #[test]
    fn fixture_files() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let entries  = fs::read_dir(&fixtures)
            .unwrap_or_else(|_| panic!("fixtures dir not found: {}", fixtures.display()));

        let mut failures = Vec::new();
        let mut count    = 0;

        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().map_or(false, |e| e == "md") {
                count += 1;
                if let Err(e) = run_fixture(&path) {
                    failures.push(e);
                }
            }
        }

        assert!(count > 0, "no .md files found in {}", fixtures.display());
        if !failures.is_empty() {
            panic!("{} fixture(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
        }
    }
}

#[cfg(test)]
mod codegen {
    use ground_be_terra::terra_gen::aws;
    use ground_core::high::*;
    use ground_parse::parse;
    use serde_json::Value;

    fn gen(input: &str) -> Value {
        let mut spec = parse(&[("test", input)]).unwrap_or_else(|es| panic!("{}", es[0]));
        spec.groups.push(Group { name: "backend".into(), members: spec.services.iter().map(|s| s.name.clone()).collect() });
        spec.regions.push(Region { name: "us-east".into(), aws: "us-east-1".into(), zones: vec![Zone { id: 1, aws: "us-east-1a".into() }] });
        spec.envs.push(Env { name: "prod".into(), vars: vec![] });
        spec.stacks.push(Stack { name: "prod".into(), env: "prod".into(), region: "us-east".into(), zones: vec![1], group: "backend".into() });
        spec.deploys.push(Deploy { provider: Provider::Aws, stacks: vec!["prod".into()], override_json: None });
        let plan = ground_core::compile::compile(&spec)
            .unwrap_or_else(|es| panic!("{}", es[0]))
            .remove(0).1;
        serde_json::from_str(&aws::generate(&plan)).unwrap()
    }

    fn res<'a>(v: &'a Value, rtype: &str, name: &str) -> &'a Value {
        &v["resource"][rtype][name]
    }

    #[test]
    fn test_minimal_service() {
        let v = gen("service svc-api { image: svc-api:prod }");
        assert!(!res(&v, "aws_ecs_task_definition",  "svc_api").is_null());
        assert!(!res(&v, "aws_ecs_service",          "svc_api").is_null());
        assert!(!res(&v, "aws_iam_role",             "svc_api_task").is_null());
        assert!(!res(&v, "aws_iam_role",             "svc_api_exec").is_null());
        assert!(!res(&v, "aws_security_group",       "svc_api").is_null());
        assert!(!res(&v, "aws_cloudwatch_log_group", "_ground_svc_api").is_null());
    }

    #[test]
    fn test_network_stack_generated() {
        let v = gen("service svc-api { image: svc-api:prod }");
        assert!(!v["resource"]["aws_vpc"].is_null());
        assert!(!v["resource"]["aws_ecs_cluster"].is_null());
        assert!(!v["resource"]["aws_internet_gateway"].is_null());
        assert!(!v["resource"]["aws_nat_gateway"].is_null());
        assert!(!v["resource"]["aws_subnet"].is_null());
    }

    #[test]
    fn test_no_var_references() {
        let v = gen("service svc-api { image: svc-api:prod }");
        let s = serde_json::to_string(&v).unwrap();
        assert!(!s.contains("var."), "generated JSON contains var.* references: {s}");
    }

    #[test]
    fn test_provider_block() {
        let v = gen("service svc-api { image: svc-api:prod }");
        assert_eq!(v["provider"]["aws"]["region"], "us-east-1");
    }

    #[test]
    fn test_scaling() {
        let v = gen("service svc-api { image: svc-api:prod scaling: { min: 2, max: 10 } }");
        let target = res(&v, "aws_appautoscaling_target", "svc_api");
        assert_eq!(target["min_capacity"], 2);
        assert_eq!(target["max_capacity"], 10);
        assert!(!res(&v, "aws_appautoscaling_policy", "svc_api_scale").is_null());
    }

    #[test]
    fn test_env_injected() {
        let input = r#"
            service svc-api { image: svc-api:prod }
            group   backend { svc-api }
            region  us-east { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env     prod    { LOG_LEVEL: info }
            stack   prod    { env: prod region: us-east zone: [1] group: backend }
            deploy to aws   { stacks: [prod] }
        "#;
        let spec = ground_parse::parse(&[("test", input)]).unwrap();
        let plan = ground_core::compile::compile(&spec)
            .unwrap_or_else(|es| panic!("{}", es[0]))
            .remove(0).1;
        let v: Value = serde_json::from_str(&aws::generate(&plan)).unwrap();
        let task_def = &v["resource"]["aws_ecs_task_definition"]["svc_api"];
        let container_defs = task_def["container_definitions"].as_str().unwrap();
        assert!(container_defs.contains("LOG_LEVEL"), "env var not injected: {container_defs}");
    }

    #[test]
    fn test_database_resources_generated() {
        let input = r#"
            database db-main { engine: postgres version: 15 size: medium storage: 50 }
            service svc-api { image: svc-api:prod access { db-main } }
            group   backend { svc-api db-main }
            region  us-east { aws: us-east-1 zone 1 { aws: us-east-1a } zone 2 { aws: us-east-1b } }
            env     prod    {}
            stack   prod    { env: prod region: us-east zone: [1, 2] group: backend }
            deploy to aws   { stacks: [prod] }
        "#;
        let spec = ground_parse::parse(&[("test", input)]).unwrap();
        let plan = ground_core::compile::compile(&spec)
            .unwrap_or_else(|es| panic!("{}", es[0]))
            .remove(0).1;
        let v: Value = serde_json::from_str(&aws::generate(&plan)).unwrap();

        // DB instance with correct class and storage
        let db = &v["resource"]["aws_db_instance"]["db_main"];
        assert_eq!(db["instance_class"], "db.t3.medium");
        assert_eq!(db["allocated_storage"], 50);
        assert_eq!(db["multi_az"], true);   // 2 zones → multi-AZ
        assert_eq!(db["engine_version"], "15");

        // random password
        assert!(!v["resource"]["random_password"]["db_main"].is_null());

        // security group rule: svc-api → db-main
        assert!(!v["resource"]["aws_vpc_security_group_ingress_rule"]["svc_api_to_db_main_db"].is_null());

        // DB env vars injected into task definition
        let td = &v["resource"]["aws_ecs_task_definition"]["svc_api"];
        let cdefs = td["container_definitions"].as_str().unwrap();
        assert!(cdefs.contains("DB_MAIN_HOST"));
        assert!(cdefs.contains("DB_MAIN_PASSWORD"));

        // random provider added
        assert!(v["terraform"]["required_providers"]["random"].is_object());
    }

    #[test]
    fn test_compute_in_task_definition() {
        let input = r#"
            compute spot { cpu: 512 memory: 1024 aws: fargate:spot }
            service svc-api { image: svc-api:prod compute: spot }
            group   backend { svc-api }
            region  us-east { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env     prod    {}
            stack   prod    { env: prod region: us-east zone: [1] group: backend }
            deploy to aws   { stacks: [prod] }
        "#;
        let spec = ground_parse::parse(&[("test", input)]).unwrap();
        let plan = ground_core::compile::compile(&spec)
            .unwrap_or_else(|es| panic!("{}", es[0]))
            .remove(0).1;
        let v: Value = serde_json::from_str(&aws::generate(&plan)).unwrap();

        let td = &v["resource"]["aws_ecs_task_definition"]["svc_api"];
        assert_eq!(td["cpu"],    "512");
        assert_eq!(td["memory"], "1024");

        let svc = &v["resource"]["aws_ecs_service"]["svc_api"];
        assert_eq!(svc["capacity_provider_strategy"][0]["capacity_provider"], "fargate:spot");
    }

    #[test]
    fn test_compile_unknown_group_error() {
        let input = r#"
            service svc-api { image: svc-api:prod }
            region  us-east { aws: us-east-1 zone 1 { aws: us-east-1a } }
            env     prod    { LOG_LEVEL: info }
            stack   prod    { env: prod region: us-east zone: [1] group: missing }
            deploy to aws   { stacks: [prod] }
        "#;
        let spec = ground_parse::parse(&[("test", input)]).unwrap();
        let err  = ground_core::compile::compile(&spec).unwrap_err();
        assert!(err.iter().any(|e| e.contains("unknown group")));
    }
}
