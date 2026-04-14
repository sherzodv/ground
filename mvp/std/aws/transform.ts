// Ground-generated interfaces for transform.grd hooks.
// Each function is pure: typed input → typed output, no side effects.
// Stub bodies: TODO comments describe the computation needed.

// ── Shared types ──────────────────────────────────────────────────────────────
// Mirror of std.grd field shapes

type ServicePort = 'grpc' | 'http';
type BucketAccess = 'read' | 'write';
type DbEngine = 'postgres' | 'mysql';
type Size = 'small' | 'medium' | 'large' | 'xlarge';

interface StdObserve {
  tracing: boolean;
  datadog: boolean;
}

// Typed access items — each carries resolved resources needed to produce AWS rules
type AccessItem =
  | { kind: 'service'; target: StdService; port: ServicePort; target_sg: AwsSecurityGroup }
  | { kind: 'database'; target: StdDatabase; target_sg: AwsSecurityGroup }
  | { kind: 'bucket'; target: StdBucket; access: BucketAccess; s3: AwsS3Bucket }
  | { kind: 'secret'; target: StdSecret; sm: AwsSecretsManagerSecret };

interface StdService {
  name: string;
  port: ServicePort;
  access: AccessItem[];
  observe: StdObserve;
}

interface StdDatabase {
  name: string;
  engine: DbEngine;
  size: Size;
  storage: number;
}

interface StdSecret {
  name: string;
}

interface StdBucket {
  name: string;
  access: BucketAccess;
}

interface StdDomain {
  host: string;
}

interface StdEdge {
  sub: string;
  domain: StdDomain;
  backend: StdService;
}

interface StdSpace {
  host: string;
}

interface StdScaling {
  min: number;
  max: number;
}

// Mirror of pack.grd AWS vendor type shapes

interface AwsVpc {
  cidr_block: string;
  enable_dns_support: boolean;
  enable_dns_hostnames: boolean;
}

interface AwsInternetGateway {
  vpc: AwsVpc;
}

interface AwsSubnet {
  vpc: AwsVpc;
  cidr_block: string;
  availability_zone: string;
  map_public_ip_on_launch: boolean;
}

interface AwsEip {
  domain: 'vpc';
}

interface AwsNatGateway {
  subnet: AwsSubnet;
  allocation: AwsEip;
}

interface AwsRouteTable {
  vpc: AwsVpc;
  routes: Array<{
    cidr_block: string;
    gateway: AwsInternetGateway | AwsNatGateway;
  }>;
}

interface AwsRouteTableAssociation {
  subnet: AwsSubnet;
  route_table: AwsRouteTable;
}

interface AwsSecurityGroup {
  vpc: AwsVpc;
  name: string;
  description: string;
}

interface AwsVpcSecurityGroupIngressRule {
  security_group: AwsSecurityGroup;
  from_port: number;
  to_port: number;
  ip_protocol: string;
  cidr_ipv4?: string;
  referenced_security_group?: AwsSecurityGroup;
}

interface AwsVpcSecurityGroupEgressRule {
  security_group: AwsSecurityGroup;
  cidr_ipv4: string;
  ip_protocol: string;
}

interface AwsIamRole {
  name: string;
  assume_role_policy: string;
}

interface AwsIamRolePolicyAttachment {
  role: AwsIamRole;
  policy_arn: string;
}

interface AwsIamRolePolicy {
  name: string;
  role: AwsIamRole;
  policy: string;
}

interface AwsCloudwatchLogGroup {
  name: string;
  retention_in_days: number;
}

interface AwsEcsCluster {
  name: string;
}

interface AwsServiceDiscoveryPrivateDnsNamespace {
  name: string;
  vpc: AwsVpc;
}

interface AwsEcsTaskDefinition {
  family: string;
  network_mode: 'awsvpc' | 'bridge' | 'host' | 'none';
  requires_compatibilities: Array<'FARGATE' | 'EC2'>;
  cpu: number;
  memory: number;
  execution_role: AwsIamRole;
  task_role: AwsIamRole;
  log_group: AwsCloudwatchLogGroup;
}

interface AwsEcsService {
  name: string;
  cluster: AwsEcsCluster;
  task_definition: AwsEcsTaskDefinition;
  desired_count: number;
  launch_type: 'FARGATE' | 'EC2';
  subnets: AwsSubnet[];
  security_groups: AwsSecurityGroup[];
}

interface AwsDbSubnetGroup {
  name: string;
  subnets: AwsSubnet[];
}

interface AwsDbInstance {
  identifier: string;
  engine: DbEngine;
  engine_version: string;
  instance_class: string;
  allocated_storage: number;
  db_name: string;
  username: string;
  subnet_group: AwsDbSubnetGroup;
  security_groups: AwsSecurityGroup[];
  skip_final_snapshot: boolean;
}

interface AwsS3Bucket {
  bucket: string;
}

interface AwsSecretsManagerSecret {
  name: string;
  recovery_window_in_days: number;
}

interface AwsRoute53Zone {
  name: string;
}

interface AwsAcmCertificate {
  domain_name: string;
  subject_alternative_names: string[];
  zone: AwsRoute53Zone;
  validation_method: 'DNS' | 'EMAIL';
}

interface AwsRoute53Record {
  zone: AwsRoute53Zone;
  name: string;
  record_type: 'A' | 'CNAME' | 'MX' | 'TXT';
  ttl: number;
  records: string[];
}

interface AwsAcmCertificateValidation {
  certificate: AwsAcmCertificate;
  validation_records: AwsRoute53Record[];
}

interface AwsLb {
  name: string;
  load_balancer_type: 'application' | 'network';
  scheme: 'internal' | 'internet-facing';
  vpc: AwsVpc;
  security_groups: AwsSecurityGroup[];
}

interface RedirectAction {
  port: number;
  protocol: 'HTTP' | 'HTTPS';
  status_code: string;
}

interface FixedResponseAction {
  content_type: string;
  message_body: string;
  status_code: string;
}

interface AwsLbListener {
  load_balancer: AwsLb;
  port: number;
  protocol: 'HTTP' | 'HTTPS';
  ssl_policy?: string;
  certificate?: AwsAcmCertificateValidation;
  default_action: RedirectAction | FixedResponseAction;
}

interface AwsLbTargetGroup {
  name: string;
  port: number;
  protocol: 'HTTP' | 'HTTPS';
  target_type: 'ip' | 'instance' | 'lambda';
  vpc: AwsVpc;
  health_check_path: string;
  health_check_matcher: string;
}

interface AwsLbListenerRule {
  listener: AwsLbListener;
  target_group: AwsLbTargetGroup;
  host_header: string;
}

// ── Partial aws_deploy shape used as input (only fields each function needs) ──

interface AwsDeployInfra {
  stack_name: string;
  azs: string[];
  vpc: AwsVpc;
  public_subnets: AwsSubnet[];
  private_subnets: AwsSubnet[];
  internet_gw: AwsInternetGateway;
  nat_gw: AwsNatGateway;
  lb: AwsLb;
}

// ── Shared constants ──────────────────────────────────────────────────────────

const ECS_ASSUME_ROLE_POLICY = JSON.stringify({
  Version: '2012-10-17',
  Statement: [
    {
      Effect: 'Allow',
      Principal: { Service: 'ecs-tasks.amazonaws.com' },
      Action: 'sts:AssumeRole',
    },
  ],
});

const SIZE_CPU: Record<Size, number> = {
  small: 256,
  medium: 512,
  large: 1024,
  xlarge: 2048,
};

const SIZE_MEMORY: Record<Size, number> = {
  small: 512,
  medium: 1024,
  large: 2048,
  xlarge: 4096,
};

const DB_INSTANCE_CLASS: Record<Size, string> = {
  small: 'db.t3.micro',
  medium: 'db.t3.medium',
  large: 'db.r6g.large',
  xlarge: 'db.r6g.xlarge',
};

const DB_ENGINE_VERSION: Record<DbEngine, string> = {
  postgres: '15',
  mysql: '8.0',
};

const DB_PORT: Record<DbEngine, number> = {
  postgres: 5432,
  mysql: 3306,
};

// ─────────────────────────────────────────────────────────────────────────────
// 1. make_aws_deploy
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeAwsDeployI {
  stack_name: string;
  // AZs derived from deploy.region list (e.g. ["eu-central-1a", "eu-central-1b", "eu-central-1c"])
  azs: string[];
}

export interface MakeAwsDeployO {
  vpc: AwsVpc;
  public_subnets: AwsSubnet[];
  private_subnets: AwsSubnet[];
  internet_gw: AwsInternetGateway;
  eip: AwsEip;
  nat_gw: AwsNatGateway;
  public_rt: AwsRouteTable;
  private_rt: AwsRouteTable;
  public_rt_assoc: AwsRouteTableAssociation[];
  private_rt_assoc: AwsRouteTableAssociation[];
  lb: AwsLb;
}

export function make_aws_deploy(i: MakeAwsDeployI): MakeAwsDeployO {
  const vpc: AwsVpc = {
    cidr_block: '10.0.0.0/16',
    enable_dns_support: true,
    enable_dns_hostnames: true,
  };

  const internet_gw: AwsInternetGateway = { vpc };

  // Public subnets: 10.0.{i}/24 for i=0,1,2,...
  // Private subnets: 10.0.{i+10}/24 for i=0,1,2,...
  const public_subnets: AwsSubnet[] = i.azs.map((az, idx) => ({
    vpc,
    cidr_block: `10.0.${idx}.0/24`,
    availability_zone: az,
    map_public_ip_on_launch: true,
  }));

  const private_subnets: AwsSubnet[] = i.azs.map((az, idx) => ({
    vpc,
    cidr_block: `10.0.${idx + 10}.0/24`,
    availability_zone: az,
    map_public_ip_on_launch: false,
  }));

  const eip: AwsEip = { domain: 'vpc' };

  // NAT gateway placed in first public subnet
  const nat_gw: AwsNatGateway = {
    subnet: public_subnets[0],
    allocation: eip,
  };

  const public_rt: AwsRouteTable = {
    vpc,
    routes: [{ cidr_block: '0.0.0.0/0', gateway: internet_gw }],
  };

  const private_rt: AwsRouteTable = {
    vpc,
    routes: [{ cidr_block: '0.0.0.0/0', gateway: nat_gw }],
  };

  const public_rt_assoc: AwsRouteTableAssociation[] = public_subnets.map((subnet) => ({
    subnet,
    route_table: public_rt,
  }));

  const private_rt_assoc: AwsRouteTableAssociation[] = private_subnets.map((subnet) => ({
    subnet,
    route_table: private_rt,
  }));

  const lb_sg: AwsSecurityGroup = {
    vpc,
    name: `ground-${i.stack_name}-lb-sg`,
    description: `ALB security group for ${i.stack_name}`,
  };

  const lb: AwsLb = {
    name: `${i.stack_name}-lb`,
    load_balancer_type: 'application',
    scheme: 'internet-facing',
    vpc,
    security_groups: [lb_sg],
  };

  return {
    vpc,
    public_subnets,
    private_subnets,
    internet_gw,
    eip,
    nat_gw,
    public_rt,
    private_rt,
    public_rt_assoc,
    private_rt_assoc,
    lb,
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. make_lb_listener
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeLbListenerI {
  // from aws_deploy — only the lb and the validated certificate are needed
  lb: AwsLb;
  // cert produced by make_domain for this domain
  validation: AwsAcmCertificateValidation;
}

export interface MakeLbListenerO {
  https_listener: AwsLbListener;
  http_listener: AwsLbListener;
}

export function make_lb_listener(i: MakeLbListenerI): MakeLbListenerO {
  const https_listener: AwsLbListener = {
    load_balancer: i.lb,
    port: 443,
    protocol: 'HTTPS',
    ssl_policy: 'ELBSecurityPolicy-TLS13-1-2-2021-06',
    certificate: i.validation,
    default_action: {
      content_type: 'text/plain',
      message_body: 'not found',
      status_code: '404',
    } as FixedResponseAction,
  };

  const http_listener: AwsLbListener = {
    load_balancer: i.lb,
    port: 80,
    protocol: 'HTTP',
    default_action: {
      port: 443,
      protocol: 'HTTPS',
      status_code: 'HTTP_301',
    } as RedirectAction,
  };

  return { https_listener, http_listener };
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. make_service
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeServiceI {
  svc: StdService;
  sp: StdSpace;
  d: {
    stack_name: string;
    vpc: AwsVpc;
    private_subnets: AwsSubnet[];
  };
  size: Size;
  scaling: StdScaling;
  cluster: AwsEcsCluster;
}

export interface MakeServiceO {
  sg: AwsSecurityGroup;
  egress: AwsVpcSecurityGroupEgressRule;
  ecs: AwsEcsService;
  task_def: AwsEcsTaskDefinition;
  exec_role: AwsIamRole;
  exec_policy: AwsIamRolePolicyAttachment;
  task_role: AwsIamRole;
  log: AwsCloudwatchLogGroup;
  db_ingresses: AwsVpcSecurityGroupIngressRule[];
  svc_ingresses: AwsVpcSecurityGroupIngressRule[];
  iam_policies: AwsIamRolePolicy[];
}

export function make_service(i: MakeServiceI): MakeServiceO {
  const { svc, d, size, scaling, cluster } = i;

  const sg: AwsSecurityGroup = {
    vpc: d.vpc,
    name: `ground-${d.stack_name}-${svc.name}-sg`,
    description: `Security group for service ${svc.name}`,
  };

  const egress: AwsVpcSecurityGroupEgressRule = {
    security_group: sg,
    cidr_ipv4: '0.0.0.0/0',
    ip_protocol: '-1',
  };

  const exec_role: AwsIamRole = {
    name: `${svc.name}-exec-role`,
    assume_role_policy: ECS_ASSUME_ROLE_POLICY,
  };

  const exec_policy: AwsIamRolePolicyAttachment = {
    role: exec_role,
    policy_arn: 'arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy',
  };

  const task_role: AwsIamRole = {
    name: `${svc.name}-task-role`,
    assume_role_policy: ECS_ASSUME_ROLE_POLICY,
  };

  const log: AwsCloudwatchLogGroup = {
    name: `/ground/${d.stack_name}/${svc.name}`,
    retention_in_days: 30,
  };

  const task_def: AwsEcsTaskDefinition = {
    family: svc.name,
    network_mode: 'awsvpc',
    requires_compatibilities: ['FARGATE'],
    cpu: SIZE_CPU[size],
    memory: SIZE_MEMORY[size],
    execution_role: exec_role,
    task_role,
    log_group: log,
  };

  const ecs: AwsEcsService = {
    name: svc.name,
    cluster,
    task_definition: task_def,
    desired_count: scaling.min,
    launch_type: 'FARGATE',
    subnets: d.private_subnets,
    security_groups: [sg],
  };

  // Access dispatch — TypeScript pattern-matches the typed access list and produces
  // the appropriate AWS resources for each relationship kind
  const db_ingresses: AwsVpcSecurityGroupIngressRule[] = [];
  const svc_ingresses: AwsVpcSecurityGroupIngressRule[] = [];
  const iam_policies: AwsIamRolePolicy[] = [];

  for (const item of svc.access) {
    switch (item.kind) {
      case 'database': {
        const port = DB_PORT[item.target.engine];
        db_ingresses.push({
          security_group: item.target_sg,
          from_port: port,
          to_port: port,
          ip_protocol: 'tcp',
          referenced_security_group: sg,
        });
        break;
      }
      case 'service': {
        svc_ingresses.push({
          security_group: item.target_sg,
          from_port: 0,
          to_port: 65535,
          ip_protocol: 'tcp',
          referenced_security_group: sg,
        });
        break;
      }
      case 'secret': {
        const secretArn = `arn:aws:secretsmanager:*:*:secret:${item.sm.name}-*`;
        iam_policies.push({
          name: `${svc.name}-${item.target.name}-secret-policy`,
          role: task_role,
          policy: JSON.stringify({
            Version: '2012-10-17',
            Statement: [{ Effect: 'Allow', Action: ['secretsmanager:GetSecretValue'], Resource: secretArn }],
          }),
        });
        break;
      }
      case 'bucket': {
        const resource = `arn:aws:s3:::${item.s3.bucket}/*`;
        const actions = item.access === 'read' ? ['s3:GetObject'] : ['s3:PutObject', 's3:DeleteObject'];
        iam_policies.push({
          name: `${svc.name}-${item.target.name}-bucket-policy`,
          role: task_role,
          policy: JSON.stringify({
            Version: '2012-10-17',
            Statement: [{ Effect: 'Allow', Action: actions, Resource: resource }],
          }),
        });
        break;
      }
    }
  }

  return { sg, egress, ecs, task_def, exec_role, exec_policy, task_role, log, db_ingresses, svc_ingresses, iam_policies };
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. make_database
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeDatabaseI {
  db: StdDatabase;
  d: {
    stack_name: string;
    vpc: AwsVpc;
    private_subnets: AwsSubnet[];
  };
}

export interface MakeDatabaseO {
  sg: AwsSecurityGroup;
  rds: AwsDbInstance;
  subnet_group: AwsDbSubnetGroup;
}

export function make_database(i: MakeDatabaseI): MakeDatabaseO {
  const { db, d } = i;

  const sg: AwsSecurityGroup = {
    vpc: d.vpc,
    name: `ground-${d.stack_name}-${db.name}-sg`,
    description: `Security group for database ${db.name}`,
  };

  const subnet_group: AwsDbSubnetGroup = {
    name: `${d.stack_name}-${db.name}-subnet-group`,
    subnets: d.private_subnets,
  };

  const rds: AwsDbInstance = {
    identifier: db.name,
    engine: db.engine,
    engine_version: DB_ENGINE_VERSION[db.engine],
    instance_class: DB_INSTANCE_CLASS[db.size],
    allocated_storage: db.storage,
    db_name: db.name,
    username: 'ground',
    subnet_group,
    security_groups: [sg],
    skip_final_snapshot: true,
  };

  return { sg, rds, subnet_group };
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. make_secret
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeSecretI {
  sec: StdSecret;
  d: {
    stack_name: string;
  };
}

export interface MakeSecretO {
  sm: AwsSecretsManagerSecret;
}

export function make_secret(i: MakeSecretI): MakeSecretO {
  const sm: AwsSecretsManagerSecret = {
    name: `ground-${i.d.stack_name}/${i.sec.name}`,
    recovery_window_in_days: 7,
  };

  return { sm };
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. make_bucket
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeBucketI {
  bkt: StdBucket;
  d: {
    stack_name: string;
  };
}

export interface MakeBucketO {
  s3: AwsS3Bucket;
}

export function make_bucket(i: MakeBucketI): MakeBucketO {
  const s3: AwsS3Bucket = {
    bucket: `${i.d.stack_name}-${i.bkt.name}`,
  };

  return { s3 };
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. make_domain
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeDomainI {
  dom: StdDomain;
  d: {
    // route53 zone is resolved by matching dom.host to an existing zone
    zone: AwsRoute53Zone;
  };
}

export interface MakeDomainO {
  cert: AwsAcmCertificate;
  validation: AwsAcmCertificateValidation;
}

export function make_domain(i: MakeDomainI): MakeDomainO {
  const cert: AwsAcmCertificate = {
    domain_name: i.dom.host,
    subject_alternative_names: [`*.${i.dom.host}`],
    zone: i.d.zone,
    validation_method: 'DNS',
  };

  // Validation DNS records are derived from the certificate by ACM.
  // The actual CNAME name/value pairs are provider-generated at apply time;
  // we represent them as a placeholder record set here — Ground's reference
  // system will resolve these to the actual ACM-generated validation records.
  const validation_record: AwsRoute53Record = {
    zone: i.d.zone,
    name: `<acm-validation-record-name.${i.dom.host}>`, // resolved at apply time
    record_type: 'CNAME',
    ttl: 60,
    records: ['<acm-validation-record-value>'], // resolved at apply time
  };

  const validation: AwsAcmCertificateValidation = {
    certificate: cert,
    validation_records: [validation_record],
  };

  return { cert, validation };
}

// ─────────────────────────────────────────────────────────────────────────────
// 8. make_edge
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeEdgeI {
  edg: StdEdge;
  d: {
    vpc: AwsVpc;
    zone: AwsRoute53Zone;
    lb: AwsLb;
    // HTTPS listener produced by make_lb_listener for edg.domain
    https_listener: AwsLbListener;
  };
  // sg produced by make_service for edg.backend
  backend_sg: AwsSecurityGroup;
}

export interface MakeEdgeO {
  tg: AwsLbTargetGroup;
  rule: AwsLbListenerRule;
  record: AwsRoute53Record;
}

export function make_edge(i: MakeEdgeI): MakeEdgeO {
  const { edg, d } = i;

  const tg: AwsLbTargetGroup = {
    name: `${edg.backend.name}-tg`,
    port: 8080,
    protocol: 'HTTP',
    target_type: 'ip',
    vpc: d.vpc,
    health_check_path: '/health',
    health_check_matcher: '200',
  };

  const rule: AwsLbListenerRule = {
    listener: d.https_listener,
    target_group: tg,
    host_header: `${edg.sub}.${edg.domain.host}`,
  };

  // Route53 A record pointing to the ALB. The lb dns_name is a runtime value;
  // we use a placeholder — Ground's reference system resolves this at render time.
  const record: AwsRoute53Record = {
    zone: d.zone,
    name: `${edg.sub}.${edg.domain.host}`,
    record_type: 'A',
    ttl: 60,
    records: ['<lb-dns-name>'], // resolved at apply time from d.lb.dns_name
  };

  return { tg, rule, record };
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. make_space
// ─────────────────────────────────────────────────────────────────────────────

export interface MakeSpaceI {
  sp: StdSpace;
  d: {
    stack_name: string;
    vpc: AwsVpc;
  };
}

export interface MakeSpaceO {
  cluster: AwsEcsCluster;
  namespace: AwsServiceDiscoveryPrivateDnsNamespace;
}

export function make_space(i: MakeSpaceI): MakeSpaceO {
  const cluster: AwsEcsCluster = {
    name: `${i.d.stack_name}-${i.sp.host}`,
  };

  const namespace: AwsServiceDiscoveryPrivateDnsNamespace = {
    name: i.sp.host,
    vpc: i.d.vpc,
  };

  return { cluster, namespace };
}

