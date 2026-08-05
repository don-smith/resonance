import type { IncomingMessage, ServerResponse } from 'node:http';

export const MANIFEST_VERSION = 1;
export type PackageInput = Record<string, unknown>;
export type PackageConfig = PackageInput & { module?: string; enabled?: boolean };
export type RepositoryConfig = { version: typeof MANIFEST_VERSION; packages: Record<string, PackageConfig> };
export type PackageMetadata = { id: string; version: string; hostVersion: string; label: string; order: number };
export type HostContext = {
  repositoryRoot: string;
  appRoot: string;
  resolveRepositoryPath(relativePath: string): string | null;
  sendJson(response: ServerResponse, status: number, body: unknown): void;
};
export type RouteHandler = (request: IncomingMessage, response: ServerResponse, context: HostContext) => Promise<void>;
export type HttpMethod = 'GET' | 'POST';
export type RouteContribution = { method: HttpMethod; path: string; handler: RouteHandler };
export type AssetContribution = { path: string; file: string; contentType: string };
export type NavigationContribution = { id: string; label: string; order: number };
export type BrowserContribution = { id: string; entry: string; stylesheet: string };
export type PackageRegistration = {
  metadata: PackageMetadata;
  routes: RouteContribution[];
  assets: AssetContribution[];
  navigation: NavigationContribution[];
  browser: BrowserContribution;
  dispose?: () => void | Promise<void>;
};
export type PackageDefinition = {
  metadata: PackageMetadata;
  register(context: HostContext, input: PackageInput): PackageRegistration;
};
