#[derive(Debug, Clone)]
pub struct Plan {
    pub stack_name:       String,
    pub region_name:      String,
    pub env_name:         String,
    pub group_name:       String,
    pub provider_name:    String,
    pub override_json:    Option<String>,
    pub workloads:        Vec<Workload>,
    pub identities:       Vec<Identity>,
    pub network_groups:   Vec<NetworkGroup>,
    pub log_streams:      Vec<LogStream>,
    pub scalers:          Vec<Scaler>,
    pub ingress_rules:    Vec<IngressRule>,
    pub provider:         Option<Provider>,
    pub cluster:          Option<Cluster>,
    pub vpc:              Option<Vpc>,
    pub subnets:          Vec<Subnet>,
    pub internet_gateway: Option<InternetGateway>,
    pub nat_gateway:      Option<NatGateway>,
    pub route_tables:     Vec<RouteTable>,
}

#[derive(Debug, Clone)]
pub struct Workload {
    pub name:     String,
    pub image:    String,
    pub identity: String,           // ref → Identity
    pub network:  String,           // ref → NetworkGroup
    pub log:      String,           // ref → LogStream
    pub env:      Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub name: String,
    pub kind: IdentityKind,
}

#[derive(Debug, Clone)]
pub enum IdentityKind {
    TaskRole,                       // the identity the workload runtime assumes
}

#[derive(Debug, Clone)]
pub struct NetworkGroup {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct LogStream {
    pub name:           String,
    pub retention_days: u32,
}

#[derive(Debug, Clone)]
pub struct IngressRule {
    pub source_network: String,     // ref → NetworkGroup (the service requesting access)
    pub target_network: String,     // ref → NetworkGroup (the service being accessed)
    pub ports:          Vec<u16>,   // resolved port numbers; empty = all traffic
}

#[derive(Debug, Clone)]
pub struct Scaler {
    pub workload:   String,         // ref → Workload
    pub min:        u32,
    pub max:        u32,
    pub metric:     ScalingMetric,
    pub target_pct: f64,
}

#[derive(Debug, Clone)]
pub enum ScalingMetric {
    Cpu,
    Memory,
}

#[derive(Debug, Clone)]
pub struct Provider {
    pub region: String,             // resolved provider region, e.g. "us-east-1"
}

#[derive(Debug, Clone)]
pub struct Cluster {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Vpc {
    pub name: String,
    pub cidr: String,
}

#[derive(Debug, Clone)]
pub struct Subnet {
    pub name:   String,
    pub cidr:   String,
    pub zone:   String,             // resolved zone identifier — Ground's abstraction
    pub public: bool,
}

#[derive(Debug, Clone)]
pub struct InternetGateway {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct NatGateway {
    pub name:          String,
    pub public_subnet: String,      // ref → Subnet name
}

#[derive(Debug, Clone)]
pub struct RouteTable {
    pub name:   String,
    pub subnet: String,             // ref → Subnet name
    pub public: bool,
}
