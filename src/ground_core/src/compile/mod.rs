use crate::high;
use crate::low::*;

pub fn compile(spec: &high::Spec) -> Result<Vec<(String, Plan)>, Vec<String>> {
    let mut results = Vec::new();
    let mut errors  = Vec::new();

    for deploy in &spec.deploys {
        let provider_name = match deploy.provider { high::Provider::Aws => "aws".to_string() };
        for stack_name in &deploy.stacks {
            match compile_stack(spec, stack_name, provider_name.clone(), deploy.override_json.clone()) {
                Ok(plan) => results.push((stack_name.clone(), plan)),
                Err(es)  => errors.extend(es),
            }
        }
    }

    if errors.is_empty() { Ok(results) } else { Err(errors) }
}

fn compile_stack(spec: &high::Spec, stack_name: &str, provider_name: String, override_json: Option<String>) -> Result<Plan, Vec<String>> {
    let mut errors = Vec::new();

    let stack = spec.stacks.iter().find(|s| s.name == stack_name);
    let stack = match stack {
        Some(s) => s,
        None    => return Err(vec![format!("deploy references unknown stack '{stack_name}'")]),
    };

    let region = spec.regions.iter().find(|r| r.name == stack.region);
    if region.is_none() {
        errors.push(format!("stack '{stack_name}': unknown region '{}'", stack.region));
    }

    let env = spec.envs.iter().find(|e| e.name == stack.env);
    if env.is_none() {
        errors.push(format!("stack '{stack_name}': unknown env '{}'", stack.env));
    }

    let group = spec.groups.iter().find(|g| g.name == stack.group);
    if group.is_none() {
        errors.push(format!("stack '{stack_name}': unknown group '{}'", stack.group));
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let region = region.unwrap();
    let env    = env.unwrap();
    let group  = group.unwrap();

    let zones: Vec<&high::Zone> = stack.zones.iter()
        .filter_map(|id| region.zones.iter().find(|z| z.id == *id)
            .or_else(|| { errors.push(format!("stack '{stack_name}': unknown zone {id} in region '{}'", region.name)); None }))
        .collect();

    if !errors.is_empty() {
        return Err(errors);
    }

    let multi_az = zones.len() >= 2;

    let mut plan = Plan {
        stack_name:       stack.name.clone(),
        region_name:      region.name.clone(),
        env_name:         env.name.clone(),
        group_name:       group.name.clone(),
        provider_name,
        override_json,
        workloads:        vec![],
        identities:       vec![],
        network_groups:   vec![],
        log_streams:      vec![],
        scalers:          vec![],
        ingress_rules:    vec![],
        rdbs:             vec![],
        db_access_rules:  vec![],
        provider:         None,
        cluster:          None,
        vpc:              None,
        subnets:          vec![],
        internet_gateway: None,
        nat_gateway:      None,
        route_tables:     vec![],
    };

    // Provider
    plan.provider = Some(Provider { region: region.aws.clone() });

    // Cluster
    plan.cluster = Some(Cluster { name: format!("ground-{}", stack.name) });

    // VPC
    plan.vpc = Some(Vpc { name: format!("ground-{}", stack.name), cidr: "10.0.0.0/16".into() });

    // Subnets, IGW, NAT, route tables
    for (idx, zone) in zones.iter().enumerate() {
        let pub_cidr  = format!("10.0.{}.0/24", idx * 2);
        let priv_cidr = format!("10.0.{}.0/24", idx * 2 + 1);
        let pub_name  = format!("{}-pub-{}",  stack.name, zone.id);
        let priv_name = format!("{}-priv-{}", stack.name, zone.id);

        plan.subnets.push(Subnet { name: pub_name.clone(),  cidr: pub_cidr,  zone: zone.aws.clone(), public: true  });
        plan.subnets.push(Subnet { name: priv_name.clone(), cidr: priv_cidr, zone: zone.aws.clone(), public: false });

        plan.route_tables.push(RouteTable { name: format!("rt-{pub_name}"),  subnet: pub_name,  public: true  });
        plan.route_tables.push(RouteTable { name: format!("rt-{priv_name}"), subnet: priv_name, public: false });
    }

    let igw_name = format!("ground-{}", stack.name);
    plan.internet_gateway = Some(InternetGateway { name: igw_name });

    let first_pub = plan.subnets.iter().find(|s| s.public).map(|s| s.name.clone()).unwrap();
    plan.nat_gateway = Some(NatGateway { name: format!("ground-{}", stack.name), public_subnet: first_pub });

    // Group members: services and rdbs
    let env_vars: Vec<(String, String)> = env.vars.clone();
    for member_name in &group.members {
        if let Some(svc) = spec.services.iter().find(|s| s.name == *member_name) {
            compile_service(&mut plan, svc, &spec.services, &spec.rdbs, &spec.computes, &env_vars, &mut errors);
        } else if let Some(rdb) = spec.rdbs.iter().find(|r| r.name == *member_name) {
            compile_rdb(&mut plan, rdb, &spec.computes, multi_az, stack_name, &mut errors);
        } else {
            errors.push(format!("group '{}': unknown member '{member_name}'", group.name));
        }
    }

    if errors.is_empty() { Ok(plan) } else { Err(errors) }
}

fn resolve_compute(name: Option<&str>, all_computes: &[high::Compute], context: &str, errors: &mut Vec<String>) -> ComputeSpec {
    let default = ComputeSpec { cpu: 256, memory: 512, aws: "FARGATE".into() };
    let name = match name { Some(n) => n, None => return default };
    match all_computes.iter().find(|c| c.name == name) {
        Some(c) => ComputeSpec {
            cpu:    c.cpu.unwrap_or(256),
            memory: c.memory.unwrap_or(512),
            aws:    c.aws.clone().unwrap_or_else(|| "FARGATE".into()),
        },
        None => {
            errors.push(format!("{context}: unknown compute '{name}'"));
            default
        }
    }
}

fn rdb_instance_class(rdb: &high::Rdb, all_computes: &[high::Compute], context: &str, errors: &mut Vec<String>) -> String {
    if let Some(compute_name) = &rdb.compute {
        match all_computes.iter().find(|c| c.name == *compute_name) {
            Some(c) => if let Some(aws) = &c.aws {
                return aws.clone();
            },
            None => errors.push(format!("{context}: unknown compute '{compute_name}'")),
        }
    }
    // fall back to size
    match rdb.size {
        Some(high::RdbSize::Medium) => "db.t3.medium".into(),
        Some(high::RdbSize::Large)  => "db.r6g.large".into(),
        Some(high::RdbSize::Xlarge) => "db.r6g.xlarge".into(),
        _                           => "db.t3.micro".into(),
    }
}

fn compile_rdb(plan: &mut Plan, rdb: &high::Rdb, all_computes: &[high::Compute], multi_az: bool, stack_name: &str, errors: &mut Vec<String>) {
    let network_name      = format!("{}-db", rdb.name);
    let subnet_group_name = format!("ground-{}-{}", stack_name, rdb.name);
    let context           = format!("database '{}'", rdb.name);

    plan.network_groups.push(NetworkGroup { name: network_name.clone() });

    plan.rdbs.push(ManagedRdb {
        name:              rdb.name.clone(),
        engine:            match rdb.engine {
            high::RdbEngine::Postgres => RdbEngine::Postgres,
            high::RdbEngine::Mysql    => RdbEngine::Mysql,
        },
        version:           rdb.version,
        instance_class:    rdb_instance_class(rdb, all_computes, &context, errors),
        storage:           rdb.storage.unwrap_or(20),
        multi_az,
        subnet_group_name,
        network:           network_name,
    });
}

fn compile_service(plan: &mut Plan, svc: &high::Service, all_services: &[high::Service], all_rdbs: &[high::Rdb], all_computes: &[high::Compute], env_vars: &[(String, String)], errors: &mut Vec<String>) {
    let task_id  = format!("{}-task", svc.name);
    let log_name = format!("/ground/{}", svc.name);
    let context  = format!("service '{}'", svc.name);

    plan.identities.push(Identity { name: task_id.clone(), kind: IdentityKind::TaskRole });

    plan.network_groups.push(NetworkGroup { name: svc.name.clone() });

    plan.log_streams.push(LogStream { name: log_name.clone(), retention_days: 7 });

    // Collect rdb access while processing access entries
    let mut rdb_access: Vec<String> = Vec::new();

    for entry in &svc.access {
        if let Some(target) = all_services.iter().find(|s| s.name == entry.target) {
            // service → service access
            let ports = if entry.ports.is_empty() {
                target.ports.iter().map(|p| p.number).collect()
            } else {
                let mut resolved = Vec::new();
                for pname in &entry.ports {
                    match target.ports.iter().find(|p| p.name == *pname) {
                        Some(p) => resolved.push(p.number),
                        None    => errors.push(format!(
                            "service '{}': access to '{}' references unknown port '{pname}'",
                            svc.name, target.name
                        )),
                    }
                }
                resolved
            };

            plan.ingress_rules.push(IngressRule {
                source_network: svc.name.clone(),
                target_network: target.name.clone(),
                ports,
            });
        } else if all_rdbs.iter().any(|r| r.name == entry.target) {
            // service → rdb access
            rdb_access.push(entry.target.clone());
            plan.db_access_rules.push(DbAccessRule {
                service_network: svc.name.clone(),
                rdb:             entry.target.clone(),
            });
        } else {
            errors.push(format!("service '{}': access references unknown target '{}'", svc.name, entry.target));
        }
    }

    let compute = resolve_compute(svc.compute.as_deref(), all_computes, &context, errors);

    plan.workloads.push(Workload {
        name:     svc.name.clone(),
        image:    svc.image.clone(),
        identity: task_id,
        network:  svc.name.clone(),
        log:      log_name,
        env:      env_vars.to_vec(),
        rdb_access,
        compute,
    });

    if let Some(scaling) = &svc.scaling {
        plan.scalers.push(Scaler {
            workload:   svc.name.clone(),
            min:        scaling.min,
            max:        scaling.max,
            metric:     ScalingMetric::Cpu,
            target_pct: 70.0,
        });
    }
}
