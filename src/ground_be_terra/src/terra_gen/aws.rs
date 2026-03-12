use std::collections::HashMap;

use serde_json::{json, Value};

use ground_core::low::*;

type Resources = HashMap<String, HashMap<String, Value>>;

pub fn generate(plan: &Plan) -> String {
    let mut res: Resources = HashMap::new();

    if let Some(p)   = &plan.provider         { gen_provider(&mut res, p); }
    if let Some(c)   = &plan.cluster          { gen_cluster(&mut res, c); }
    if let Some(v)   = &plan.vpc              { gen_vpc(&mut res, v); }
    if let Some(igw) = &plan.internet_gateway { gen_internet_gateway(&mut res, igw, plan); }
    if let Some(nat) = &plan.nat_gateway      { gen_nat_gateway(&mut res, nat); }

    for subnet   in &plan.subnets        { gen_subnet(&mut res, subnet, plan); }
    for rt       in &plan.route_tables   { gen_route_table(&mut res, rt, plan); }
    for identity in &plan.identities     { gen_identity(&mut res, identity); }
    for group    in &plan.network_groups { gen_network_group(&mut res, group, plan); }
    for log      in &plan.log_streams    { gen_log_stream(&mut res, log); }
    for rdb      in &plan.rdbs           { gen_rdb(&mut res, rdb, plan); }
    for workload in &plan.workloads      { gen_workload(&mut res, workload, plan); }
    for scaler   in &plan.scalers        { gen_scaler(&mut res, scaler, plan); }
    for rule     in &plan.ingress_rules  { gen_ingress_rule(&mut res, rule); }
    for rule     in &plan.db_access_rules { gen_db_access_rule(&mut res, rule, plan); }

    let region = plan.provider.as_ref().map(|p| p.region.as_str()).unwrap_or("");

    let mut required_providers = json!({
        "aws": { "source": "hashicorp/aws", "version": "~> 5.0" }
    });
    if !plan.rdbs.is_empty() {
        required_providers["random"] = json!({ "source": "hashicorp/random", "version": "~> 3.0" });
    }

    let mut out = json!({
        "terraform": { "required_providers": required_providers },
        "provider":  { "aws": { "region": region } },
        "resource":  res
    });

    if let Some(raw) = &plan.override_json {
        if let Ok(overlay) = serde_json::from_str::<Value>(raw) {
            deep_merge(&mut out, overlay);
        }
    }

    serde_json::to_string_pretty(&out).unwrap()
}

fn gen_ingress_rule(res: &mut Resources, rule: &IngressRule) {
    let src_s = sid(&rule.source_network);
    let tgt_s = sid(&rule.target_network);

    if rule.ports.is_empty() {
        add(res, "aws_vpc_security_group_ingress_rule", &format!("{src_s}_to_{tgt_s}"), json!({
            "security_group_id":            r(&format!("aws_security_group.{tgt_s}.id")),
            "referenced_security_group_id": r(&format!("aws_security_group.{src_s}.id")),
            "ip_protocol": "-1"
        }));
    } else {
        for port in &rule.ports {
            add(res, "aws_vpc_security_group_ingress_rule", &format!("{src_s}_to_{tgt_s}_{port}"), json!({
                "security_group_id":            r(&format!("aws_security_group.{tgt_s}.id")),
                "referenced_security_group_id": r(&format!("aws_security_group.{src_s}.id")),
                "ip_protocol": "tcp",
                "from_port":   port,
                "to_port":     port
            }));
        }
    }
}

fn gen_db_access_rule(res: &mut Resources, rule: &DbAccessRule, plan: &Plan) {
    let svc_s = sid(&rule.service_network);
    let rdb   = match plan.rdbs.iter().find(|r| r.name == rule.rdb) {
        Some(r) => r,
        None    => return,
    };
    let db_s = sid(&rdb.network);
    let port = rdb_port(&rdb.engine);

    add(res, "aws_vpc_security_group_ingress_rule", &format!("{svc_s}_to_{db_s}"), json!({
        "security_group_id":            r(&format!("aws_security_group.{db_s}.id")),
        "referenced_security_group_id": r(&format!("aws_security_group.{svc_s}.id")),
        "ip_protocol": "tcp",
        "from_port":   port,
        "to_port":     port
    }));
}

fn gen_rdb(res: &mut Resources, rdb: &ManagedRdb, plan: &Plan) {
    let s         = sid(&rdb.name);
    let db_s      = sid(&rdb.network);
    let sub_grp_s = sid(&rdb.subnet_group_name);

    let (engine, default_version) = match rdb.engine {
        RdbEngine::Postgres => ("postgres", "15"),
        RdbEngine::Mysql    => ("mysql",    "8.0"),
    };

    let version_str = match (rdb.version, &rdb.engine) {
        (Some(v), RdbEngine::Mysql) if v < 10 => format!("{v}.0"),
        (Some(v), _)                           => v.to_string(),
        (None, _)                              => default_version.to_string(),
    };

    let instance_class = rdb.instance_class.as_str();

    let db_name = rdb.name.replace('-', "_");

    // password
    add(res, "random_password", &s, json!({
        "length":  32,
        "special": false
    }));

    // subnet group — spans all private subnets
    let private_subnet_ids: Vec<Value> = plan.subnets.iter()
        .filter(|s| !s.public)
        .map(|s| Value::String(r(&format!("aws_subnet.{}.id", sid(&s.name)))))
        .collect();

    add(res, "aws_db_subnet_group", &sub_grp_s, json!({
        "name":       &rdb.subnet_group_name,
        "subnet_ids": private_subnet_ids
    }));

    // instance
    add(res, "aws_db_instance", &s, json!({
        "identifier":             &rdb.name,
        "engine":                 engine,
        "engine_version":         version_str,
        "instance_class":         instance_class,
        "allocated_storage":      rdb.storage,
        "db_name":                db_name,
        "username":               "ground",
        "password":               r(&format!("random_password.{s}.result")),
        "db_subnet_group_name":   r(&format!("aws_db_subnet_group.{sub_grp_s}.name")),
        "vpc_security_group_ids": [r(&format!("aws_security_group.{db_s}.id"))],
        "multi_az":               rdb.multi_az,
        "skip_final_snapshot":    true
    }));
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                deep_merge(base_map.entry(k).or_insert(Value::Null), v);
            }
        }
        (base, overlay) => *base = overlay,
    }
}

// -- Helpers -----------------------------------------------------------------

fn add(res: &mut Resources, rtype: &str, name: &str, body: Value) {
    res.entry(rtype.to_string()).or_default().insert(name.to_string(), body);
}

fn sid(name: &str) -> String {
    name.replace(['-', '/'], "_")
}

fn r(s: &str) -> String {
    format!("${{{s}}}")
}

fn rdb_port(engine: &RdbEngine) -> u16 {
    match engine {
        RdbEngine::Postgres => 5432,
        RdbEngine::Mysql    => 3306,
    }
}

fn vpc_id(plan: &Plan) -> String {
    match &plan.vpc {
        Some(v) => r(&format!("aws_vpc.{}.id", sid(&v.name))),
        None    => String::new(),
    }
}

fn cluster_name(plan: &Plan) -> String {
    match &plan.cluster {
        Some(c) => r(&format!("aws_ecs_cluster.{}.name", sid(&c.name))),
        None    => String::new(),
    }
}

fn cluster_id(plan: &Plan) -> String {
    match &plan.cluster {
        Some(c) => r(&format!("aws_ecs_cluster.{}.id", sid(&c.name))),
        None    => String::new(),
    }
}

fn private_subnet_ids(plan: &Plan) -> Value {
    let ids: Vec<Value> = plan.subnets.iter()
        .filter(|s| !s.public)
        .map(|s| Value::String(r(&format!("aws_subnet.{}.id", sid(&s.name)))))
        .collect();
    Value::Array(ids)
}

fn igw_id(plan: &Plan) -> String {
    match &plan.internet_gateway {
        Some(igw) => r(&format!("aws_internet_gateway.{}.id", sid(&igw.name))),
        None      => String::new(),
    }
}

fn nat_gw_id(plan: &Plan) -> String {
    match &plan.nat_gateway {
        Some(nat) => r(&format!("aws_nat_gateway.{}.id", sid(&nat.name))),
        None      => String::new(),
    }
}

fn iam_assume_ecs() -> String {
    serde_json::to_string(&json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Action": "sts:AssumeRole",
            "Effect": "Allow",
            "Principal": { "Service": "ecs-tasks.amazonaws.com" }
        }]
    })).unwrap()
}

fn db_env_vars(plan: &Plan, rdb_names: &[String]) -> Vec<Value> {
    let mut vars = Vec::new();
    for rdb_name in rdb_names {
        if let Some(rdb) = plan.rdbs.iter().find(|r| r.name == *rdb_name) {
            let s      = sid(rdb_name);
            let prefix = rdb_name.to_uppercase().replace('-', "_");
            let port   = rdb_port(&rdb.engine);
            let db_name = rdb_name.replace('-', "_");

            vars.push(json!({ "name": format!("{prefix}_HOST"),     "value": r(&format!("aws_db_instance.{s}.address")) }));
            vars.push(json!({ "name": format!("{prefix}_PORT"),     "value": port.to_string() }));
            vars.push(json!({ "name": format!("{prefix}_NAME"),     "value": db_name }));
            vars.push(json!({ "name": format!("{prefix}_USER"),     "value": "ground" }));
            vars.push(json!({ "name": format!("{prefix}_PASSWORD"), "value": r(&format!("random_password.{s}.result")) }));
        }
    }
    vars
}

// -- Generators --------------------------------------------------------------

fn gen_provider(res: &mut Resources, _provider: &Provider) {
    // provider block is emitted at the top level in generate(); nothing in resource
    let _ = res; // suppress unused warning
}

fn gen_cluster(res: &mut Resources, cluster: &Cluster) {
    let s = sid(&cluster.name);
    add(res, "aws_ecs_cluster", &s, json!({ "name": &cluster.name }));
}

fn gen_vpc(res: &mut Resources, vpc: &Vpc) {
    let s = sid(&vpc.name);
    add(res, "aws_vpc", &s, json!({
        "cidr_block":           &vpc.cidr,
        "enable_dns_hostnames": true,
        "enable_dns_support":   true,
        "tags": { "Name": &vpc.name }
    }));
}

fn gen_subnet(res: &mut Resources, subnet: &Subnet, plan: &Plan) {
    let s = sid(&subnet.name);
    add(res, "aws_subnet", &s, json!({
        "vpc_id":                        vpc_id(plan),
        "cidr_block":                    &subnet.cidr,
        "availability_zone":             &subnet.zone,
        "map_public_ip_on_launch":       subnet.public,
        "tags": { "Name": &subnet.name }
    }));
}

fn gen_internet_gateway(res: &mut Resources, igw: &InternetGateway, plan: &Plan) {
    let s = sid(&igw.name);
    add(res, "aws_internet_gateway", &s, json!({
        "vpc_id": vpc_id(plan),
        "tags":   { "Name": &igw.name }
    }));
}

fn gen_nat_gateway(res: &mut Resources, nat: &NatGateway) {
    let s      = sid(&nat.name);
    let eip_s  = format!("{s}_eip");
    let sub_s  = sid(&nat.public_subnet);

    add(res, "aws_eip", &eip_s, json!({ "domain": "vpc" }));

    add(res, "aws_nat_gateway", &s, json!({
        "allocation_id": r(&format!("aws_eip.{eip_s}.id")),
        "subnet_id":     r(&format!("aws_subnet.{sub_s}.id")),
        "tags":          { "Name": &nat.name }
    }));
}

fn gen_route_table(res: &mut Resources, rt: &RouteTable, plan: &Plan) {
    let s     = sid(&rt.name);
    let sub_s = sid(&rt.subnet);

    add(res, "aws_route_table", &s, json!({
        "vpc_id": vpc_id(plan),
        "tags":   { "Name": &rt.name }
    }));

    add(res, "aws_route_table_association", &s, json!({
        "subnet_id":      r(&format!("aws_subnet.{sub_s}.id")),
        "route_table_id": r(&format!("aws_route_table.{s}.id"))
    }));

    let route_s = format!("{s}_default");
    if rt.public {
        add(res, "aws_route", &route_s, json!({
            "route_table_id":         r(&format!("aws_route_table.{s}.id")),
            "destination_cidr_block": "0.0.0.0/0",
            "gateway_id":             igw_id(plan)
        }));
    } else {
        add(res, "aws_route", &route_s, json!({
            "route_table_id":         r(&format!("aws_route_table.{s}.id")),
            "destination_cidr_block": "0.0.0.0/0",
            "nat_gateway_id":         nat_gw_id(plan)
        }));
    }
}

fn gen_identity(res: &mut Resources, identity: &Identity) {
    let s = sid(&identity.name);

    // task role
    add(res, "aws_iam_role", &s, json!({
        "name":               &identity.name,
        "assume_role_policy": iam_assume_ecs()
    }));

    // exec role — ECS-specific implementation detail: every ECS task needs a
    // separate role for the ECS agent to pull images and write logs.
    // Not modelled in low::Plan; generated here as an AWS/ECS concern.
    let exec_name = identity.name.replace("-task", "-exec");
    let exec_s    = sid(&exec_name);

    add(res, "aws_iam_role", &exec_s, json!({
        "name":               &exec_name,
        "assume_role_policy": iam_assume_ecs()
    }));

    add(res, "aws_iam_role_policy_attachment", &exec_s, json!({
        "role":       r(&format!("aws_iam_role.{exec_s}.name")),
        "policy_arn": "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
    }));
}

fn gen_network_group(res: &mut Resources, group: &NetworkGroup, plan: &Plan) {
    let s = sid(&group.name);
    add(res, "aws_security_group", &s, json!({
        "name":   &group.name,
        "vpc_id": vpc_id(plan)
    }));

    add(res, "aws_vpc_security_group_egress_rule", &format!("{s}_all"), json!({
        "security_group_id": r(&format!("aws_security_group.{s}.id")),
        "cidr_ipv4":         "0.0.0.0/0",
        "ip_protocol":       "-1"
    }));
}

fn gen_log_stream(res: &mut Resources, log: &LogStream) {
    let s = sid(&log.name);
    add(res, "aws_cloudwatch_log_group", &s, json!({
        "name":              &log.name,
        "retention_in_days": log.retention_days
    }));
}

fn gen_workload(res: &mut Resources, wl: &Workload, plan: &Plan) {
    let s      = sid(&wl.name);
    let task_s = sid(&wl.identity);
    let exec_name = wl.identity.replace("-task", "-exec");
    let exec_s    = sid(&exec_name);

    let region = plan.provider.as_ref().map(|p| p.region.as_str()).unwrap_or("");

    let mut env_json: Vec<Value> = wl.env.iter()
        .map(|(k, v)| json!({ "name": k, "value": v }))
        .collect();

    env_json.extend(db_env_vars(plan, &wl.rdb_access));

    let mut container = json!({
        "name":  &wl.name,
        "image": &wl.image,
        "logConfiguration": {
            "logDriver": "awslogs",
            "options": {
                "awslogs-group":         &wl.log,
                "awslogs-region":        region,
                "awslogs-stream-prefix": "ecs"
            }
        }
    });

    if !env_json.is_empty() {
        container["environment"] = Value::Array(env_json);
    }

    let container_def = serde_json::to_string(&json!([container])).unwrap();

    add(res, "aws_ecs_task_definition", &s, json!({
        "family":                   &wl.name,
        "network_mode":             "awsvpc",
        "requires_compatibilities": ["FARGATE"],
        "cpu":                      wl.compute.cpu.to_string(),
        "memory":                   wl.compute.memory.to_string(),
        "execution_role_arn":       r(&format!("aws_iam_role.{exec_s}.arn")),
        "task_role_arn":            r(&format!("aws_iam_role.{task_s}.arn")),
        "container_definitions":    container_def
    }));

    add(res, "aws_ecs_service", &s, json!({
        "name":            &wl.name,
        "cluster":         cluster_id(plan),
        "task_definition": r(&format!("aws_ecs_task_definition.{s}.arn")),
        "desired_count":   1,
        "capacity_provider_strategy": [{ "capacity_provider": &wl.compute.aws, "weight": 1 }],
        "network_configuration": {
            "subnets":         private_subnet_ids(plan),
            "security_groups": [r(&format!("aws_security_group.{s}.id"))]
        }
    }));
}

fn gen_scaler(res: &mut Resources, scaler: &Scaler, plan: &Plan) {
    let s = sid(&scaler.workload);

    add(res, "aws_appautoscaling_target", &s, json!({
        "min_capacity":       scaler.min,
        "max_capacity":       scaler.max,
        "resource_id":        format!("service/{}/{}", cluster_name(plan), scaler.workload),
        "scalable_dimension": "ecs:service:DesiredCount",
        "service_namespace":  "ecs",
        "depends_on":         [r(&format!("aws_ecs_service.{s}"))]
    }));

    let metric_type = match scaler.metric {
        ScalingMetric::Cpu    => "ECSServiceAverageCPUUtilization",
        ScalingMetric::Memory => "ECSServiceAverageMemoryUtilization",
    };

    add(res, "aws_appautoscaling_policy", &format!("{s}_scale"), json!({
        "name":               format!("{}-scale", scaler.workload),
        "policy_type":        "TargetTrackingScaling",
        "resource_id":        r(&format!("aws_appautoscaling_target.{s}.resource_id")),
        "scalable_dimension": r(&format!("aws_appautoscaling_target.{s}.scalable_dimension")),
        "service_namespace":  r(&format!("aws_appautoscaling_target.{s}.service_namespace")),
        "target_tracking_scaling_policy_configuration": {
            "predefined_metric_specification": {
                "predefined_metric_type": metric_type
            },
            "target_value": scaler.target_pct
        }
    }));
}
