import type { PackageDefinition, RepositoryConfig } from '../package-contract.ts';
import { docsPackage } from './docs.ts';
import { homePackage } from './home.ts';
import { shellPackage } from './shell.ts';

export function createDefaultPackages(_config: RepositoryConfig): PackageDefinition[] {
  return [shellPackage, homePackage, docsPackage];
}
