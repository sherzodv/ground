interface MkDemoServiceInput {
  name: string;
  port: number;
  enabled: boolean;
}

interface MkDemoServiceOutput {
  endpoint: unknown;
  tags: string[];
}

declare function mk_demo_service(i: MkDemoServiceInput): MkDemoServiceOutput;