import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { parseDocument } from 'yaml';
import { z } from 'zod';
import { createMarkdownRenderer } from '../../markdown.ts';
import type { HostContext, PackageDefinition, PackageInput, PackageRegistration } from '../../package-contract.ts';

const metadata = { id: 'backlog', version: '1.0.0', hostVersion: '1', label: 'Backlog', order: 30 } as const;
const TODO_PATH = 'backlog/todo.yaml';
const statusOrder = ['recently-done', 'in-progress', 'is-ready', 'in-planning'] as const;
const priorityOrder = ['P0', 'P1', 'P2', 'P3'] as const;
const decisionSchema = z.object({
  title: z.string().trim().min(1),
  plan: z.string().min(1).refine((value) => !value.startsWith('/') && !value.includes('\\'), 'Plan must be a forward-slash relative path.'),
  status: z.enum(statusOrder),
  priority: z.enum(priorityOrder),
}).strict();
const backlogSchema = z.object({ version: z.literal(1), decisions: z.array(decisionSchema) }).strict();

type ParsedItem = z.infer<typeof decisionSchema>;
type BacklogItem = ParsedItem & { path: string };
type OrderedItem = BacklogItem & { sourceIndex: number };

class BacklogSourceError extends Error {}

export function backlogInput(input: PackageInput) {
  if (Object.keys(input).length > 0) throw new Error('Backlog does not accept package inputs.');
  return {};
}

export function parseBacklogItems(source: string): ParsedItem[] {
  const document = parseDocument(source, { uniqueKeys: true });
  if (document.errors.length > 0) throw new BacklogSourceError(`Backlog source is invalid: ${document.errors[0].message}`);
  const parsed = backlogSchema.safeParse(document.toJS());
  if (!parsed.success) throw new BacklogSourceError(`Backlog source is invalid: ${parsed.error.issues[0].message}`);
  return parsed.data.decisions;
}

function planPath(context: HostContext, item: ParsedItem): string | null {
  return context.resolveRepositoryPath(path.posix.join('backlog', item.plan));
}

function compareItems(left: OrderedItem, right: OrderedItem): number {
  return statusOrder.indexOf(left.status) - statusOrder.indexOf(right.status)
    || priorityOrder.indexOf(left.priority) - priorityOrder.indexOf(right.priority)
    || left.sourceIndex - right.sourceIndex;
}

async function readItems(context: HostContext): Promise<BacklogItem[]> {
  const todoPath = context.resolveRepositoryPath(TODO_PATH);
  if (!todoPath) throw new Error('Backlog source is unavailable.');
  const parsed = parseBacklogItems(await readFile(path.join(context.repositoryRoot, todoPath), 'utf8'));
  return parsed.flatMap((item, sourceIndex) => {
    const itemPath = planPath(context, item);
    return itemPath ? [{ ...item, path: itemPath, sourceIndex }] : [];
  }).sort(compareItems).map(({ sourceIndex: _sourceIndex, ...item }) => item);
}

function sendRouteFailure(response: { json(status: number, body: unknown): void }, error: unknown, fallback: string) {
  if (error instanceof BacklogSourceError) response.json(422, { error: error.message });
  else response.json(404, { error: fallback });
}

function register(_context: HostContext, input: PackageInput): PackageRegistration {
  backlogInput(input);
  const renderer = createMarkdownRenderer();
  return {
    metadata,
    routes: [
      {
        method: 'GET',
        path: '/api/backlog/items',
        handler: async (_request, response, context) => {
          try { response.json(200, { items: await readItems(context) }); }
          catch (error) { sendRouteFailure(response, error, 'Backlog source not found'); }
        },
      },
      {
        method: 'GET',
        path: '/api/backlog/plan',
        handler: async (request, response, context) => {
          const requested = new URL(request.url, 'http://127.0.0.1').searchParams.get('path') || '';
          try {
            const item = (await readItems(context)).find((candidate) => candidate.path === requested);
            if (!item) { response.json(404, { error: 'Backlog item not found' }); return; }
            const content = await readFile(path.join(context.repositoryRoot, item.path), 'utf8');
            response.json(200, { path: item.path, title: item.title, html: renderer.render(content) });
          } catch (error) { sendRouteFailure(response, error, 'Backlog item not found'); }
        },
      },
    ],
    assets: [
      { path: '/assets/backlog/backlog.js', file: 'src/packages/backlog/backlog.js', contentType: 'text/javascript; charset=utf-8' },
      { path: '/assets/backlog/backlog.css', file: 'src/packages/backlog/backlog.css', contentType: 'text/css; charset=utf-8' },
    ],
    navigation: [{ id: metadata.id, label: metadata.label, order: metadata.order }],
    browser: { id: metadata.id, entry: '/assets/backlog/backlog.js', stylesheet: '/assets/backlog/backlog.css' },
  };
}

const backlogPackage: PackageDefinition = { metadata, register };
export default backlogPackage;
