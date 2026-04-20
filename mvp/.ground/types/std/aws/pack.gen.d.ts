interface StdBucket {
  name: string;
  access: StdBucketAccess;
}

interface StdDatabase {
  engine: StdDatabaseEngine;
}

interface StdDomain {
  host: string;
}

interface StdSpace {
  host: string;
  services: (StdService)[];
}

interface StdEdge {
  domain: StdDomain;
  sub: string;
  backend: StdService;
}

interface StdObserve {
  tracing: boolean;
  datadog: boolean;
}

interface StdService {
  port: StdServicePort;
  access: ([StdService, "port"] | StdDatabase | [StdBucket, "access"] | StdSecret)[];
  observe: StdObserve;
}

interface StdSecret {}

interface StdStack {
  _: (StdService | StdDatabase | StdBucket | StdEdge | StdDomain | StdSpace | StdSecret)[];
}

type StdSize = "small" | "medium" | "large" | "xlarge";

interface StdScaling {
  min: number;
  max: number;
}

interface StdServiceConfig {
  service: StdService;
  size: StdSize;
  scaling: StdScaling;
}

interface StdDatabaseConfig {
  database: StdDatabase;
  size: StdSize;
  storage: number;
}

interface StdAwsAwsVpc {
  cidr_block: string;
  enable_dns_support: boolean;
  enable_dns_hostnames: boolean;
}

interface StdAwsAwsInternetGateway {
  vpc: StdAwsAwsVpc;
}

interface StdAwsAwsSubnet {
  vpc: StdAwsAwsVpc;
  cidr_block: string;
  availability_zone: string;
  map_public_ip_on_launch: boolean;
}

interface StdAwsAwsEip {
  domain: string;
}

interface StdAwsAwsNatGateway {
  subnet: StdAwsAwsSubnet;
  allocation: StdAwsAwsEip;
}

interface StdAwsAwsSecurityGroup {
  vpc: StdAwsAwsVpc;
  name: string;
  description: string;
}

interface StdAwsAwsLb {
  name: string;
  load_balancer_type: StdAwsAwsLbLoadBalancerType;
  scheme: StdAwsAwsLbScheme;
  vpc: StdAwsAwsVpc;
  security_groups: (StdAwsAwsSecurityGroup)[];
}

interface StdAwsAwsDeployRoot {
  ecs_key: string;
  ecs_name: string;
  vpc_key: string;
  vpc_name: string;
  gw_key: string;
  gw_name: string;
  nat_eip_key: string;
  nat_key: string;
  nat_name: string;
}

interface StdAwsAwsDeployZone {
  n: string;
  az: string;
  public_cidr: string;
  private_cidr: string;
  pub_key: string;
  pub_name: string;
  priv_key: string;
  priv_name: string;
  rpub_key: string;
  rpub_name: string;
  rprv_key: string;
  rprv_name: string;
  rpub_default_key: string;
  rprv_default_key: string;
}

type StdBucketAccess = "read" | "write";

type StdDatabaseEngine = "postgres" | "mysql";

type StdServicePort = "grpc" | "http";

type StdAwsAwsLbLoadBalancerType = "application" | "network";

type StdAwsAwsLbScheme = "internal" | "internet-facing";

interface DeployResolved {
  prefix: string;
  stack: StdStack;
  region: (string)[];
  services: (StdServiceConfig)[];
  databases: (StdDatabaseConfig)[];
}

interface DeployInput {}

interface DeployOutput {
  aws_region: string;
  root: StdAwsAwsDeployRoot;
  zones: (StdAwsAwsDeployZone)[];
  vpc: StdAwsAwsVpc;
  public_subnets: (StdAwsAwsSubnet)[];
  private_subnets: (StdAwsAwsSubnet)[];
  internet_gw: StdAwsAwsInternetGateway;
  nat_gw: StdAwsAwsNatGateway;
  lb: StdAwsAwsLb;
}

declare function deploy(resolved: DeployResolved, input: DeployInput): DeployOutput;