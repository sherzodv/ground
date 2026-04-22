interface StdProject {
  _name: string;
}

interface TfStateStoreI {
  project?: StdProject;
  state?: "local" | "remote";
  region?: string[];
  encrypted?: boolean;
}

function make_state_store(input: TfStateStoreI) {
  return make_aws_state(input as unknown as AwsStateI);
}
