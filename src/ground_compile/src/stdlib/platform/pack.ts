interface StdProject {
  _name: string;
}

interface PlatformStateI {
  project?: StdProject;
  region?: string[];
  encrypted?: boolean;
}

function make_state(input: PlatformStateI) {
  return make_aws_state(input as unknown as AwsStateI);
}
