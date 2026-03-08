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

    // Services
    let env_vars: Vec<(String, String)> = env.vars.clone();
    for svc_name in &group.services {
        match spec.services.iter().find(|s| s.name == *svc_name) {
            Some(svc) => compile_service(&mut plan, svc, &spec.services, &env_vars, &mut errors),
            None      => errors.push(format!("group '{}': unknown service '{svc_name}'", group.name)),
        }
    }

    if errors.is_empty() { Ok(plan) } else { Err(errors) }
}

fn compile_service(plan: &mut Plan, svc: &high::Service, all_services: &[high::Service], env_vars: &[(String, String)], errors: &mut Vec<String>) {
    let task_id  = format!("{}-task", svc.name);
    let log_name = format!("/ground/{}", svc.name);

    plan.identities.push(Identity { name: task_id.clone(), kind: IdentityKind::TaskRole });

    plan.network_groups.push(NetworkGroup { name: svc.name.clone() });

    plan.log_streams.push(LogStream { name: log_name.clone(), retention_days: 7 });

    plan.workloads.push(Workload {
        name:     svc.name.clone(),
        image:    svc.image.clone(),
        identity: task_id,
        network:  svc.name.clone(),
        log:      log_name,
        env:      env_vars.to_vec(),
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

    for entry in &svc.access {
        let target = match all_services.iter().find(|s| s.name == entry.service) {
            Some(t) => t,
            None    => {
                errors.push(format!("service '{}': access references unknown service '{}'", svc.name, entry.service));
                continue;
            }
        };

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
    }
}
