export interface ReleaseTarget {
  artifact: string;
}

export interface ReleaseConfiguration {
  manifestEndpoint: string;
  artifactBaseUrl: string;
  publicKey: string;
  targets: Record<string, ReleaseTarget>;
}

export const REQUIRED_TARGETS: readonly string[];
export function readReleaseConfiguration(path: string): ReleaseConfiguration;
export function validateReleaseConfiguration(
  configuration: ReleaseConfiguration,
): ReleaseConfiguration;
export function validateSigningSecret(value: string | undefined): void;
