import { readFile } from 'node:fs/promises';
import type { Telemetry } from '../../package-contract.ts';
import { createDeepAgent, type BackendProtocolV2 } from 'deepagents';
import { ChatOpenAI } from '@langchain/openai';
import { tool } from 'langchain';
import { z } from 'zod/v4';
import type { BacklogDecision, BacklogMutation, BacklogStore } from './backlog-store.ts';
import type { BacklogAgentRuntime, BacklogAgentRuntimeFactory, BacklogAgentRuntimeFactoryOptions, BacklogAgentTurn, BacklogAgentUpdate } from './agent-session.ts';

const skillPath = '/skills/manage-backlog/SKILL.md';
const denied = (requestedPath: string) => `Permission denied: ${requestedPath} is not available to the Backlog agent.`;

export function createPackagedSkillBackend(skill: string): BackendProtocolV2 {
  const now = new Date(0).toISOString();
  const directory = (requestedPath: string) => requestedPath.endsWith('/') ? requestedPath : `${requestedPath}/`;
  return {
    ls(requestedPath) {
      switch (directory(requestedPath)) {
        case '/skills/': return { files: [{ path: '/skills/manage-backlog/', is_dir: true }] };
        case '/skills/manage-backlog/': return { files: [{ path: skillPath, is_dir: false }] };
        default: return { error: denied(requestedPath) };
      }
    },
    read(requestedPath) { return requestedPath === skillPath ? { content: skill, mimeType: 'text/markdown' } : { error: denied(requestedPath) }; },
    readRaw(requestedPath) { return requestedPath === skillPath ? { data: { content: skill, mimeType: 'text/markdown', created_at: now, modified_at: now } } : { error: denied(requestedPath) }; },
    grep(_pattern, requestedPath = '/') { return { error: denied(requestedPath || '/') }; },
    glob(_pattern, requestedPath = '/') { return { error: denied(requestedPath) }; },
    write(requestedPath) { return { error: denied(requestedPath) }; },
    edit(requestedPath) { return { error: denied(requestedPath) }; },
  };
}

function mutationResult(options: BacklogAgentRuntimeFactoryOptions, result: BacklogMutation) {
  options.onMutation(result);
  return JSON.stringify({ changed: true, affectedPaths: result.affectedPaths });
}
function decisionResult(decision: BacklogDecision) {
  return JSON.stringify({ path: decision.path, title: decision.title, status: decision.status, priority: decision.priority, markdown: decision.markdown });
}
function createBacklogTools(options: BacklogAgentRuntimeFactoryOptions) {
  const canonicalPath = z.string().min(1).max(512).describe('A canonical decision path returned by list_decisions, such as backlog/plans/example.md.');
  return [
    tool(async () => JSON.stringify({ decisions: await options.store.listDecisions() }), {
      name: 'list_decisions', description: 'List authorized Backlog decisions and canonical paths. Never infer a path from a title.', schema: z.object({}),
    }),
    tool(async ({ path: requestedPath }) => decisionResult(await options.store.readDecision(requestedPath)), {
      name: 'read_plan', description: 'Read one authorized decision and linked Markdown by canonical decision path.', schema: z.object({ path: canonicalPath }),
    }),
    tool(async (input) => mutationResult(options, await options.store.createDecision(input)), {
      name: 'create_decision', description: 'Create one validated decision and linked Markdown plan. The store derives the plan path as plans/<kebab-case-title>.md; do not provide or invent a path.',
      schema: z.object({ title: z.string().trim().min(1).max(200), status: z.enum(['recently-done', 'in-progress', 'is-ready', 'in-planning']), priority: z.enum(['P0', 'P1', 'P2', 'P3']), markdown: z.string().max(256 * 1024) }),
    }),
    tool(async ({ path: requestedPath, markdown }) => mutationResult(options, await options.store.editPlan(requestedPath, markdown)), {
      name: 'edit_plan', description: 'Replace Markdown for one existing authorized decision path.', schema: z.object({ path: canonicalPath, markdown: z.string().max(256 * 1024) }),
    }),
    tool(async ({ path: requestedPath, status }) => mutationResult(options, await options.store.setStatus(requestedPath, status)), {
      name: 'set_status', description: 'Set status for one existing authorized decision path.', schema: z.object({ path: canonicalPath, status: z.enum(['recently-done', 'in-progress', 'is-ready', 'in-planning']) }),
    }),
    tool(async ({ path: requestedPath, priority }) => mutationResult(options, await options.store.setPriority(requestedPath, priority)), {
      name: 'set_priority', description: 'Set priority for one existing authorized decision path.', schema: z.object({ path: canonicalPath, priority: z.enum(['P0', 'P1', 'P2', 'P3']) }),
    }),
    tool(async ({ path: requestedPath }) => {
      const decision = await options.store.readDecision(requestedPath);
      return JSON.stringify(await options.requestDeletion(decision));
    }, {
      name: 'request_delete', description: 'Request browser-confirmed deletion. This never deletes files.', schema: z.object({ path: canonicalPath }),
    }),
  ];
}

function selectedContext(selected: BacklogDecision): string {
  return `<active-decision path="${selected.path}">\n<title>${selected.title}</title>\n<status>${selected.status}</status>\n<priority>${selected.priority}</priority>\n<plan-markdown>\n${selected.markdown}\n</plan-markdown>\n</active-decision>`;
}
function textOf(chunk: unknown): string {
  if (!chunk || typeof chunk !== 'object') return '';
  const content = (chunk as { content?: unknown }).content;
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content.flatMap((part) => typeof part === 'string' ? [part] : part && typeof part === 'object' && typeof (part as { text?: unknown }).text === 'string' ? [(part as { text: string }).text] : []).join('');
}

export class DeepAgentsRuntime implements BacklogAgentRuntime {
  constructor(private readonly agent: any, private readonly telemetry: Telemetry) {}
  async *stream(turn: BacklogAgentTurn): AsyncIterable<BacklogAgentUpdate> {
    const streamSpan = this.telemetry.span('backlog.model.stream');
    this.telemetry.info('Backlog model stream started', { threadId: turn.threadId });
    let chunks = 0;
    try {
    const messages = turn.messages.map((message, index) => ({
      role: message.role,
      content: message.role === 'user' && index === turn.messages.length - 1 ? `${selectedContext(turn.selected)}\n\n<user-request>\n${message.content}\n</user-request>` : message.content,
    }));
    const stream = await this.agent.stream({ messages }, { configurable: { thread_id: turn.threadId }, streamMode: 'messages' });
      for await (const value of stream) {
        const [chunk, metadata] = value as [unknown, { langgraph_node?: unknown }];
        if (metadata?.langgraph_node !== 'model' && metadata?.langgraph_node !== 'model_request') continue;
        const text = textOf(chunk);
        if (text) { chunks += 1; yield { kind: 'assistant', text }; }
      }
      streamSpan.end({ status: 'ok', chunks });
      this.telemetry.info('Backlog model stream completed', { chunks });
    } catch (error) {
      streamSpan.fail(error, { chunks });
      this.telemetry.error('Backlog model stream failed', { error, chunks });
      throw error;
    }
  }
  async dispose() {}
}

export function createDeepAgentsRuntimeFactory({ provider, model }: { provider: 'openai' | 'openrouter'; model: string }): BacklogAgentRuntimeFactory {
  return async (options) => {
    const skill = await readFile(new URL('./skills/manage-backlog/SKILL.md', import.meta.url), 'utf8');
    const runtimeTelemetry = options.telemetry.child({ provider, model });
    const agent = createDeepAgent({
      model: new ChatOpenAI({
        model,
        apiKey: options.apiKey,
        temperature: 0,
        configuration: provider === 'openrouter' ? { baseURL: 'https://openrouter.ai/api/v1' } : {},
      }),
      tools: createBacklogTools(options),
      backend: createPackagedSkillBackend(skill),
      skills: ['/skills/'],
      checkpointer: false,
      systemPrompt: 'You are Resonance Backlog Agent. Read /skills/manage-backlog/SKILL.md before acting. Only package-owned domain tools may access or change Backlog data; generic filesystem tools have no repository authority.',
    });
    return new DeepAgentsRuntime(agent, runtimeTelemetry);
  };
}
