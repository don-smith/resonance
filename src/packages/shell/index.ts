import type { PackageDefinition, PackageRegistration } from '../../package-contract.ts';

const metadata = { id: 'shell', version: '1.0.0', hostVersion: '1', label: 'Shell', order: 0 } as const;

function register(): PackageRegistration {
  return {
    metadata,
    routes: [],
    assets: [
      { path: '/', file: 'src/packages/shell/index.html', contentType: 'text/html; charset=utf-8' },
      { path: '/assets/app.js', file: 'src/packages/shell/app.js', contentType: 'text/javascript; charset=utf-8' },
      { path: '/assets/styles.css', file: 'src/packages/shell/styles.css', contentType: 'text/css; charset=utf-8' },
      { path: '/assets/shell/theme-bootstrap.js', file: 'src/packages/shell/theme-bootstrap.js', contentType: 'text/javascript; charset=utf-8' },
      { path: '/assets/shell/shell.js', file: 'src/packages/shell/shell.js', contentType: 'text/javascript; charset=utf-8' },
      { path: '/assets/shell/shell.css', file: 'src/packages/shell/styles.css', contentType: 'text/css; charset=utf-8' },
      { path: '/assets/shell/ui.css', file: 'src/ui/ui.css', contentType: 'text/css; charset=utf-8' },
    ],
    navigation: [],
    browser: { id: 'shell', entry: '/assets/app.js', stylesheet: '/assets/shell/shell.css' },
  };
}

export const shellPackage: PackageDefinition = { metadata, register };
export default shellPackage;
