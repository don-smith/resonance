import Ajv2020, { type ErrorObject } from "ajv/dist/2020.js";

import schema from "../../schema/conversations.v1.json" with { type: "json" };
import type { ConversationsError } from "./errors.ts";
import type {
  ConversationsEnvelope,
  ConversationsInvalidation,
  ConversationsRequest,
  ConversationsResponse,
} from "./types.ts";

export type ConversationsDiagnostic = Readonly<{
  path: string;
  message: string;
}>;
export type ConversationsValidationResult<T> =
  | Readonly<{ kind: "valid"; value: T }>
  | Readonly<{ kind: "invalid"; diagnostics: ConversationsDiagnostic[] }>;

const validator = new Ajv2020({ allErrors: true, strict: true }).compile(
  schema,
);
const utf8 = new TextEncoder();

function diagnostics(errors: ErrorObject[] | null | undefined) {
  return (errors ?? [])
    .map((error) => ({
      path: error.instancePath || "/",
      message: error.message ?? "is invalid",
    }))
    .sort((left, right) =>
      `${left.path}:${left.message}`.localeCompare(
        `${right.path}:${right.message}`,
      ),
    );
}

function semanticDiagnostics(
  envelope: ConversationsEnvelope,
): ConversationsDiagnostic[] {
  const output: ConversationsDiagnostic[] = [];
  const addUtf8Limit = (value: string, path: string, limit: number) => {
    if (utf8.encode(value).byteLength > limit) {
      output.push({ path, message: `exceeds ${limit} UTF-8 bytes` });
    }
  };
  const addChannelNameFormat = (value: string, path: string) => {
    if (!value.startsWith("#") || value.length === 1) {
      output.push({
        path,
        message: "must include a public channel name after '#'.",
      });
    }
  };
  if (envelope.kind === "request") {
    if (
      envelope.value.operation === "create-channel" ||
      envelope.value.operation === "rename-channel"
    ) {
      addUtf8Limit(envelope.value.name, "/value/name", 80);
      addChannelNameFormat(envelope.value.name, "/value/name");
    }
    if (envelope.value.operation === "post-message") {
      addUtf8Limit(envelope.value.markdown, "/value/markdown", 16_384);
    }
  } else if (envelope.kind === "response") {
    const channels =
      "snapshot" in envelope.value
        ? envelope.value.snapshot.channels
        : "channel" in envelope.value
          ? [envelope.value.channel]
          : [];
    const channelIds = new Set<string>();
    for (const [index, channel] of channels.entries()) {
      if (channelIds.has(channel.channelId)) {
        output.push({
          path: `/value/snapshot/channels/${index}/channelId`,
          message: "must be unique",
        });
      }
      channelIds.add(channel.channelId);
      const channelNamePath = `/value/${"snapshot" in envelope.value ? `snapshot/channels/${index}` : "channel"}/name`;
      addUtf8Limit(channel.name, channelNamePath, 80);
      addChannelNameFormat(channel.name, channelNamePath);
    }
    const messages =
      "page" in envelope.value
        ? envelope.value.page.messages
        : "message" in envelope.value
          ? [envelope.value.message]
          : [];
    const messageIds = new Set<string>();
    for (const [index, message] of messages.entries()) {
      if (messageIds.has(message.messageId)) {
        output.push({
          path: `/value/page/messages/${index}/messageId`,
          message: "must be unique",
        });
      }
      messageIds.add(message.messageId);
      if (
        "page" in envelope.value &&
        message.channelId !== envelope.value.page.channelId
      ) {
        output.push({
          path: `/value/page/messages/${index}/channelId`,
          message: "must match the page channelId",
        });
      }
      addUtf8Limit(
        message.markdown,
        `/value/${"page" in envelope.value ? `page/messages/${index}` : "message"}/markdown`,
        16_384,
      );
    }
  }
  return output;
}

export function validateConversationsEnvelope(
  candidate: unknown,
): ConversationsValidationResult<ConversationsEnvelope> {
  if (!validator(candidate)) {
    return { kind: "invalid", diagnostics: diagnostics(validator.errors) };
  }
  const value = candidate as ConversationsEnvelope;
  const semantic = semanticDiagnostics(value);
  return semantic.length === 0
    ? { kind: "valid", value }
    : { kind: "invalid", diagnostics: semantic };
}

function validateValue<T>(
  kind: ConversationsEnvelope["kind"],
  candidate: unknown,
): ConversationsValidationResult<T> {
  const result = validateConversationsEnvelope({ kind, value: candidate });
  return result.kind === "valid"
    ? { kind: "valid", value: result.value.value as T }
    : result;
}

export const validateConversationsRequest = (candidate: unknown) =>
  validateValue<ConversationsRequest>("request", candidate);
export const validateConversationsResponse = (candidate: unknown) =>
  validateValue<ConversationsResponse>("response", candidate);
export const validateConversationsError = (candidate: unknown) =>
  validateValue<ConversationsError>("error", candidate);
export const validateConversationsInvalidation = (candidate: unknown) =>
  validateValue<ConversationsInvalidation>("invalidation", candidate);
