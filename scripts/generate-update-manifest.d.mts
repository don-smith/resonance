import type { ReleaseConfiguration } from "./release-configuration.mjs";

export interface UpdateManifest {
  version: string;
  notes: string;
  pub_date: string;
  platforms: Record<string, { signature: string; url: string }>;
}

export function generateUpdateManifest(input: {
  configuration: ReleaseConfiguration;
  artifactsDirectory: string;
  version: string;
  publishedAt?: string;
}): UpdateManifest;
