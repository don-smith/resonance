import type { ReleaseConfiguration } from "./release-configuration.mjs";

export function collectSignedUpdateArtifacts(input: {
  configuration: ReleaseConfiguration;
  target: string;
  sourceDirectory: string;
  outputDirectory: string;
}): { artifact: string; signature: string };
