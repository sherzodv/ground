#[derive(Debug, Clone)]
pub struct Spec {
    pub services: Vec<Service>,
    pub groups:   Vec<Group>,
    pub regions:  Vec<Region>,
    pub envs:     Vec<Env>,
    pub stacks:   Vec<Stack>,
    pub deploys:  Vec<Deploy>,
}

#[derive(Debug, Clone)]
pub struct Service {
    pub name:    String,
    pub image:   String,
    pub scaling: Option<Scaling>,
    pub ports:   Vec<Port>,
    pub access:  Vec<AccessEntry>,
}

#[derive(Debug, Clone)]
pub struct Port {
    pub name:   String,
    pub number: u16,
}

#[derive(Debug, Clone)]
pub struct AccessEntry {
    pub service: String,
    pub ports:   Vec<String>,   // port names; empty = all declared ports
}

#[derive(Debug, Clone)]
pub struct Scaling {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub name:     String,
    pub services: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Region {
    pub name:  String,
    pub aws:   String,
    pub zones: Vec<Zone>,
}

#[derive(Debug, Clone)]
pub struct Zone {
    pub id:  u32,
    pub aws: String,
}

#[derive(Debug, Clone)]
pub struct Env {
    pub name: String,
    pub vars: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct Stack {
    pub name:   String,
    pub env:    String,
    pub region: String,
    pub zones:  Vec<u32>,
    pub group:  String,
}

#[derive(Debug, Clone)]
pub struct Deploy {
    pub provider:      Provider,
    pub stacks:        Vec<String>,
    pub override_json: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Provider { Aws }
