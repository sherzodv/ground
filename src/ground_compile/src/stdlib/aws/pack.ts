function toAwsRegion(regionPrefix) {
  switch (regionPrefix) {
    case "us-east": return "us-east-1";
    case "us-west": return "us-west-2";
    case "eu-central": return "eu-central-1";
    case "eu-west": return "eu-west-1";
    case "ap-southeast": return "ap-southeast-1";
    case "me-central": return "me-central-1";
    default: return "us-east-1";
  }
}

function toAzLetter(zoneNumber) {
  const letters = "abcdefghijklmnopqrstuvwxyz";
  if (zoneNumber < 1 || zoneNumber > letters.length) return "a";
  return letters[zoneNumber - 1];
}

function parseZone(raw) {
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

function deploy(resolved, _input) {
  const region = Array.isArray(resolved.region) ? resolved.region : [];
  if (region.length === 0) return {};

  const first = String(region[0]);
  const regionPrefix = first.split(":", 1)[0];
  const alias = String(resolved._name || "");
  const prefix = String(resolved.prefix || "");
  const alias_u = alias.replaceAll("-", "_");
  const pfx_u = prefix.replaceAll("-", "_");
  const stem = `${pfx_u}${alias_u}`;
  const nameStem = `${prefix}${alias}`;

  return {
    aws_region: toAwsRegion(regionPrefix),
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
    zones: region.map((raw) => {
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
  };
}
