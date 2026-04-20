type ServicePortName = "grpc" | "http";
type BucketAccessName = "read" | "write";
type DatabaseEngine = "postgres" | "mysql";
type DeploySize = "small" | "medium" | "large" | "xlarge";

interface StdObserve {
  tracing?: boolean;
  datadog?: boolean;
}

interface StdService {
  _name: string;
  port?: ServicePortName;
  access?: unknown[];
  observe?: StdObserve;
}

interface StdDatabase {
  _name: string;
  engine?: DatabaseEngine;
}

interface StdBucket {
  _name: string;
  name?: string;
  access?: BucketAccessName;
}

interface StdSecret {
  _name: string;
}

interface StdServiceDefaults {
  size?: DeploySize;
  scaling?: StdScaling;
}

interface StdDatabaseDefaults {
  size?: DeploySize;
  storage?: number;
}

interface StdComputePool {
  _name: string;
  services?: StdService[];
  databases?: StdDatabase[];
  service_defaults?: StdServiceDefaults;
  database_defaults?: StdDatabaseDefaults;
}

interface StdScaling {
  min?: number;
  max?: number;
}

interface StdServiceConfig {
  service?: StdService;
  size?: DeploySize;
  scaling?: StdScaling;
}

interface StdDatabaseConfig {
  database?: StdDatabase;
  size?: DeploySize;
  storage?: number;
}

interface AwsDeployInput {
  prefix?: string;
  region?: string[];
  pool?: StdComputePool;
  service_overrides?: StdServiceConfig[];
  database_overrides?: StdDatabaseConfig[];
}

interface ResolvedServiceConfig {
  service: StdService;
  size?: DeploySize;
  scaling?: StdScaling;
}

interface ResolvedDatabaseConfig {
  database: StdDatabase;
  size?: DeploySize;
  storage?: number;
}

interface DeployRoot {
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

interface DeployZoneBase {
  n: string;
  az: string;
  public_cidr: string;
  private_cidr: string;
}

interface DeployZone extends DeployZoneBase {
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

interface DeployService {
  name: string;
  port: number;
  port_name: ServicePortName;
  cpu: number;
  memory: number;
  desired_count: number;
  log_group_key: string;
  log_group_name: string;
  task_definition_key: string;
  task_definition_family: string;
  service_key: string;
  service_name: string;
  security_group_key: string;
  security_group_name: string;
  task_role_key: string;
  task_role_name: string;
}

interface DeployDatabase {
  key: string;
  name: string;
  identifier: string;
  db_name: string;
  engine: DatabaseEngine;
  engine_version: string;
  instance_class: string;
  allocated_storage: number;
  port: number;
  security_group_key: string;
  security_group_name: string;
  subnet_group_key: string;
  subnet_group_name: string;
}

interface DeployBucket {
  key: string;
  name: string;
  bucket: string;
}

interface DeploySecret {
  key: string;
  name: string;
}

interface ServiceServiceAccess {
  name: string;
  source_security_group_key: string;
  target_security_group_key: string;
  from_port: number;
  to_port: number;
}

interface ServiceDatabaseAccess {
  name: string;
  source_security_group_key: string;
  target_security_group_key: string;
  from_port: number;
  to_port: number;
}

interface ServiceBucketAccess {
  name: string;
  role_name: string;
  policy_name: string;
  policy_json: string;
}

interface ServiceSecretAccess {
  name: string;
  role_name: string;
  policy_name: string;
  policy_json: string;
}

interface AwsDeployOutput {
  provider_region: string;
  root: DeployRoot;
  zones: DeployZone[];
  services: DeployService[];
  databases: DeployDatabase[];
  buckets: DeployBucket[];
  secrets: DeploySecret[];
  service_service_accesses: ServiceServiceAccess[];
  service_database_accesses: ServiceDatabaseAccess[];
  service_bucket_accesses: ServiceBucketAccess[];
  service_secret_accesses: ServiceSecretAccess[];
}

type AccessTarget =
  | { kind: "service"; service_name: string; port_name?: ServicePortName }
  | { kind: "database"; database_name: string }
  | { kind: "bucket"; bucket_name: string; bucket?: string; access_name?: BucketAccessName }
  | { kind: "secret"; secret_name: string };

function asObject(v: unknown): Record<string, unknown> | null {
  if (!v || typeof v !== "object" || Array.isArray(v)) return null;
  return v as Record<string, unknown>;
}

function asString(v: unknown): string | undefined {
  return typeof v === "string" ? v : undefined;
}

function sanitizeKey(raw: string): string {
  return String(raw).replace(/[^A-Za-z0-9_]/g, "_");
}

function toAwsRegion(regionPrefix: string): string {
  switch (regionPrefix) {
    case "us-east":
      return "us-east-1";
    case "us-west":
      return "us-west-2";
    case "eu-central":
      return "eu-central-1";
    case "eu-west":
      return "eu-west-1";
    case "ap-southeast":
      return "ap-southeast-1";
    case "me-central":
      return "me-central-1";
    default:
      return "us-east-1";
  }
}

function toAzLetter(zoneNumber: number): string {
  const letters = "abcdefghijklmnopqrstuvwxyz";
  if (zoneNumber < 1 || zoneNumber > letters.length) return "a";
  return letters[zoneNumber - 1];
}

function parseZone(raw: string): DeployZoneBase {
  const parts = String(raw).split(":", 2);
  const regionPrefix = parts[0];
  const zoneRaw = parts[1] || "1";
  const parsed = Number.parseInt(zoneRaw, 10);
  const n = Number.isFinite(parsed) && parsed > 0 ? parsed : 1;
  const awsRegion = toAwsRegion(regionPrefix);
  const pubIdx = (n - 1) * 2;
  const privIdx = pubIdx + 1;

  return {
    n: String(n),
    az: `${awsRegion}${toAzLetter(n)}`,
    public_cidr: `10.0.${pubIdx}.0/24`,
    private_cidr: `10.0.${privIdx}.0/24`,
  };
}

function servicePortNumber(port: ServicePortName | undefined): number {
  switch (port) {
    case "grpc":
      return 50051;
    case "http":
    default:
      return 80;
  }
}

function databasePort(engine: DatabaseEngine | undefined): number {
  switch (engine) {
    case "mysql":
      return 3306;
    case "postgres":
    default:
      return 5432;
  }
}

function databaseVersion(engine: DatabaseEngine | undefined): string {
  switch (engine) {
    case "mysql":
      return "8.0";
    case "postgres":
    default:
      return "15";
  }
}

function databaseClass(size: DeploySize | undefined): string {
  switch (size) {
    case "medium":
      return "db.t3.small";
    case "large":
      return "db.t3.medium";
    case "xlarge":
      return "db.t3.large";
    case "small":
    default:
      return "db.t3.micro";
  }
}

function serviceCpu(size: DeploySize | undefined): number {
  switch (size) {
    case "medium":
      return 512;
    case "large":
      return 1024;
    case "xlarge":
      return 2048;
    case "small":
    default:
      return 256;
  }
}

function serviceMemory(size: DeploySize | undefined): number {
  switch (size) {
    case "medium":
      return 1024;
    case "large":
      return 2048;
    case "xlarge":
      return 4096;
    case "small":
    default:
      return 512;
  }
}

function bucketPolicyJson(bucketName: string, accessName: BucketAccessName | undefined): string {
  const actions = accessName === "write"
    ? ["s3:GetObject", "s3:PutObject", "s3:DeleteObject", "s3:ListBucket"]
    : ["s3:GetObject", "s3:ListBucket"];
  return JSON.stringify({
    Version: "2012-10-17",
    Statement: [{
      Effect: "Allow",
      Action: actions,
      Resource: [
        `arn:aws:s3:::${bucketName}`,
        `arn:aws:s3:::${bucketName}/*`,
      ],
    }],
  });
}

function secretPolicyJson(secretName: string): string {
  return JSON.stringify({
    Version: "2012-10-17",
    Statement: [{
      Effect: "Allow",
      Action: ["secretsmanager:DescribeSecret", "secretsmanager:GetSecretValue"],
      Resource: `arn:aws:secretsmanager:*:*:secret:${secretName}*`,
    }],
  });
}

function isServiceDef(v: unknown): v is StdService {
  const obj = asObject(v);
  return !!obj && typeof obj._name === "string" && ("port" in obj || "observe" in obj || "access" in obj);
}

function isDatabaseDef(v: unknown): v is StdDatabase {
  const obj = asObject(v);
  return !!obj && typeof obj._name === "string" && "engine" in obj;
}

function isBucketDef(v: unknown): v is StdBucket {
  const obj = asObject(v);
  return !!obj && typeof obj._name === "string" && "name" in obj && "access" in obj;
}

function isSecretDef(v: unknown): v is StdSecret {
  const obj = asObject(v);
  return !!obj && typeof obj._name === "string" && Object.keys(obj).length === 1;
}

function parseAccessItem(raw: unknown): AccessTarget | null {
  if (Array.isArray(raw) && raw.length === 2) {
    const [payload, qualifier] = raw;
    if ((qualifier === "http" || qualifier === "grpc") && isServiceDef(payload)) {
      return {
        kind: "service",
        service_name: payload._name,
        port_name: qualifier,
      };
    }
    if ((qualifier === "read" || qualifier === "write") && isBucketDef(payload)) {
      return {
        kind: "bucket",
        bucket_name: payload._name,
        bucket: payload.name,
        access_name: qualifier,
      };
    }
  }

  const obj = asObject(raw);
  if (!obj) return null;

  const keys = Object.keys(obj);
  if (keys.length === 1 && (keys[0] === "http" || keys[0] === "grpc")) {
    const payload = obj[keys[0]];
    if (isServiceDef(payload)) {
      return {
        kind: "service",
        service_name: payload._name,
        port_name: keys[0],
      };
    }
  }

  if (keys.length === 1 && (keys[0] === "read" || keys[0] === "write")) {
    const payload = obj[keys[0]];
    if (isBucketDef(payload)) {
      return {
        kind: "bucket",
        bucket_name: payload._name,
        bucket: payload.name,
        access_name: keys[0],
      };
    }
  }

  if (isServiceDef(obj)) {
    return {
      kind: "service",
      service_name: obj._name,
      port_name: obj.port,
    };
  }

  if (isDatabaseDef(obj)) {
    return {
      kind: "database",
      database_name: obj._name,
    };
  }

  if (isBucketDef(obj)) {
      return {
        kind: "bucket",
        bucket_name: obj._name,
        bucket: obj.name,
        access_name: obj.access,
      };
  }

  if (isSecretDef(obj)) {
    return {
      kind: "secret",
      secret_name: obj._name,
    };
  }

  return null;
}

function mergeScaling(base: StdScaling | undefined, override: StdScaling | undefined): StdScaling | undefined {
  if (!base && !override) return undefined;
  return {
    min: override?.min ?? base?.min,
    max: override?.max ?? base?.max,
  };
}

function mergeServiceConfigs(
  pool: StdComputePool | undefined,
  overrides: StdServiceConfig[],
): ResolvedServiceConfig[] {
  const services = Array.isArray(pool?.services) ? pool?.services.filter(isServiceDef) : [];
  const defaults = pool?.service_defaults;
  const overridesByName = new Map<string, StdServiceConfig>();
  for (const override of overrides) {
    if (!isServiceDef(override.service)) continue;
    overridesByName.set(override.service._name, override);
  }
  return services.map((service) => {
    const override = overridesByName.get(service._name);
    return {
      service,
      size: override?.size ?? defaults?.size,
      scaling: mergeScaling(defaults?.scaling, override?.scaling),
    };
  });
}

function mergeDatabaseConfigs(
  pool: StdComputePool | undefined,
  overrides: StdDatabaseConfig[],
): ResolvedDatabaseConfig[] {
  const databases = Array.isArray(pool?.databases) ? pool?.databases.filter(isDatabaseDef) : [];
  const defaults = pool?.database_defaults;
  const overridesByName = new Map<string, StdDatabaseConfig>();
  for (const override of overrides) {
    if (!isDatabaseDef(override.database)) continue;
    overridesByName.set(override.database._name, override);
  }
  return databases.map((database) => {
    const override = overridesByName.get(database._name);
    return {
      database,
      size: override?.size ?? defaults?.size,
      storage: override?.storage ?? defaults?.storage,
    };
  });
}

function make_aws_deploy(input: AwsDeployInput): AwsDeployOutput | {} {
  const region = Array.isArray(input.region) ? input.region : [];
  if (region.length === 0) return {};

  const first = String(region[0]);
  const regionPrefix = first.split(":", 1)[0];
  const alias = input.pool?._name || "deploy";
  const prefix = String(input.prefix || "");
  const aliasKey = sanitizeKey(alias);
  const prefixKey = sanitizeKey(prefix);
  const stem = `${prefixKey}${aliasKey}`;
  const nameStem = `${prefix}${alias}`;

  const serviceConfigs = mergeServiceConfigs(
    input.pool,
    Array.isArray(input.service_overrides) ? input.service_overrides : [],
  );
  const databaseConfigs = mergeDatabaseConfigs(
    input.pool,
    Array.isArray(input.database_overrides) ? input.database_overrides : [],
  );

  const services: DeployService[] = serviceConfigs
    .map((cfg) => {
      const svc = cfg.service;
      const svcKey = sanitizeKey(svc._name);
      return {
        name: svc._name,
        port: servicePortNumber(svc.port),
        port_name: svc.port || "http",
        cpu: serviceCpu(cfg.size),
        memory: serviceMemory(cfg.size),
        desired_count: cfg.scaling?.min ?? 1,
        log_group_key: `${stem}_${svcKey}_logs`,
        log_group_name: `/ground/${nameStem}/${svc._name}`,
        task_definition_key: `${stem}_${svcKey}_task_def`,
        task_definition_family: `${nameStem}-${svc._name}`,
        service_key: `${stem}_${svcKey}_svc`,
        service_name: `${nameStem}-${svc._name}`,
        security_group_key: `${stem}_${svcKey}_sg`,
        security_group_name: `${nameStem}-${svc._name}-sg`,
        task_role_key: `${stem}_${svcKey}_task_role`,
        task_role_name: `${nameStem}-${svc._name}-task`,
      };
    });

  const servicesByName = new Map(services.map((svc) => [svc.name, svc] as const));

  const databases: DeployDatabase[] = databaseConfigs
    .map((cfg) => {
      const db = cfg.database;
      const dbKey = sanitizeKey(db._name);
      return {
        key: `${stem}_${dbKey}_db`,
        name: db._name,
        identifier: `${nameStem}-${db._name}`,
        db_name: dbKey.toLowerCase(),
        engine: db.engine || "postgres",
        engine_version: databaseVersion(db.engine),
        instance_class: databaseClass(cfg.size),
        allocated_storage: typeof cfg.storage === "number" ? cfg.storage : 20,
        port: databasePort(db.engine),
        security_group_key: `${stem}_${dbKey}_db_sg`,
        security_group_name: `${nameStem}-${db._name}-db-sg`,
        subnet_group_key: `${stem}_${dbKey}_db_subnets`,
        subnet_group_name: `${nameStem}-${db._name}-db-subnets`,
      };
    });

  const databasesByName = new Map(databases.map((db) => [db.name, db] as const));

  const bucketsByName = new Map<string, DeployBucket>();
  const secretsByDefName = new Map<string, DeploySecret>();

  const serviceServiceAccesses: ServiceServiceAccess[] = [];
  const serviceDatabaseAccesses: ServiceDatabaseAccess[] = [];
  const serviceBucketAccesses: ServiceBucketAccess[] = [];
  const serviceSecretAccesses: ServiceSecretAccess[] = [];

  for (const cfg of serviceConfigs) {
    const sourceService = servicesByName.get(cfg.service._name);
    if (!sourceService) continue;
    const accessItems = Array.isArray(cfg.service.access) ? cfg.service.access : [];

    for (const rawAccess of accessItems) {
      const access = parseAccessItem(rawAccess);
      if (!access) continue;

      if (access.kind === "service") {
        const targetService = servicesByName.get(access.service_name);
        if (!targetService) continue;
        const portName = access.port_name || targetService.port_name;
        const port = servicePortNumber(portName);
        serviceServiceAccesses.push({
          name: `${sourceService.name}-${targetService.name}-${portName}`,
          source_security_group_key: sourceService.security_group_key,
          target_security_group_key: targetService.security_group_key,
          from_port: port,
          to_port: port,
        });
        continue;
      }

      if (access.kind === "database") {
        const targetDatabase = databasesByName.get(access.database_name);
        if (!targetDatabase) continue;
        serviceDatabaseAccesses.push({
          name: `${sourceService.name}-${targetDatabase.name}-db`,
          source_security_group_key: sourceService.security_group_key,
          target_security_group_key: targetDatabase.security_group_key,
          from_port: targetDatabase.port,
          to_port: targetDatabase.port,
        });
        continue;
      }

      if (access.kind === "bucket") {
        let bucket = bucketsByName.get(access.bucket_name);
        if (!bucket) {
          const bucketKey = sanitizeKey(access.bucket_name);
          bucket = {
            key: `${stem}_${bucketKey}_bucket`,
            name: access.bucket_name,
            bucket: access.bucket || `${nameStem}-${access.bucket_name}`,
          };
          bucketsByName.set(access.bucket_name, bucket);
        }
        const accessName = access.access_name || "read";
        serviceBucketAccesses.push({
          name: `${sourceService.name}-${bucket.name}-${accessName}`,
          role_name: sourceService.task_role_name,
          policy_name: `${nameStem}-${sourceService.name}-${bucket.name}-${accessName}`,
          policy_json: bucketPolicyJson(bucket.bucket, accessName),
        });
        continue;
      }

      let secret = secretsByDefName.get(access.secret_name);
      if (!secret) {
        const secretKey = sanitizeKey(access.secret_name);
        secret = {
          key: `${stem}_${secretKey}_secret`,
          name: `${nameStem}-${access.secret_name}`,
        };
        secretsByDefName.set(access.secret_name, secret);
      }
      serviceSecretAccesses.push({
        name: `${sourceService.name}-${access.secret_name}-secret`,
        role_name: sourceService.task_role_name,
        policy_name: `${nameStem}-${sourceService.name}-${access.secret_name}-secret`,
        policy_json: secretPolicyJson(secret.name),
      });
    }
  }

  return {
    provider_region: toAwsRegion(regionPrefix),
    root: {
      ecs_key: `${stem}_ecs`,
      ecs_name: `${nameStem}-ecs`,
      vpc_key: `${stem}_vpc`,
      vpc_name: `${nameStem}-vpc`,
      gw_key: `${stem}_gw`,
      gw_name: `${nameStem}-gw`,
      nat_eip_key: `${stem}_nat_eip`,
      nat_key: `${stem}_nat`,
      nat_name: `${nameStem}-nat`,
    },
    zones: region.map((raw): DeployZone => {
      const zone = parseZone(raw);
      return {
        ...zone,
        pub_key: `${stem}_npub_${zone.n}`,
        pub_name: `${nameStem}-npub-${zone.n}`,
        priv_key: `${stem}_nprv_${zone.n}`,
        priv_name: `${nameStem}-nprv-${zone.n}`,
        rpub_key: `${stem}_rpub_${zone.n}`,
        rpub_name: `${nameStem}-rpub-${zone.n}`,
        rprv_key: `${stem}_rprv_${zone.n}`,
        rprv_name: `${nameStem}-rprv-${zone.n}`,
        rpub_default_key: `${stem}_rpub_${zone.n}_default`,
        rprv_default_key: `${stem}_rprv_${zone.n}_default`,
      };
    }),
    services,
    databases,
    buckets: Array.from(bucketsByName.values()),
    secrets: Array.from(secretsByDefName.values()),
    service_service_accesses: serviceServiceAccesses,
    service_database_accesses: serviceDatabaseAccesses,
    service_bucket_accesses: serviceBucketAccesses,
    service_secret_accesses: serviceSecretAccesses,
  };
}

export {};
