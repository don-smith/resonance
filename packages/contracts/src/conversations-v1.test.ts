import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import {
  conversationsErrorMessages,
  conversationsV1Operations,
  validateConversationsEnvelope,
  validateConversationsInvalidation,
  validateConversationsRequest,
  validateConversationsResponse,
  type ConversationsEnvelope,
} from "./conversations-v1.ts";

async function fixture<T>(kind: "valid" | "invalid"): Promise<T> {
  return JSON.parse(
    await readFile(
      resolve(
        `packages/contracts/fixtures/conversations-v1/${kind}/corpus.json`,
      ),
      "utf8",
    ),
  ) as T;
}

describe("conversations v1 semantic contract", () => {
  it("accepts every operation, response, finite state, safe error, and invalidation fixture", async () => {
    const corpus = await fixture<ConversationsEnvelope[]>("valid");
    for (const value of corpus) {
      expect(
        validateConversationsEnvelope(value),
        JSON.stringify(value),
      ).toMatchObject({ kind: "valid" });
    }
    const requests = new Set(
      corpus
        .filter((value) => value.kind === "request")
        .map((value) => value.value.operation),
    );
    expect(requests).toEqual(new Set(conversationsV1Operations));
    const errors = corpus
      .filter((value) => value.kind === "error")
      .map((value) => value.value.code)
      .sort();
    expect(errors).toEqual(Object.keys(conversationsErrorMessages).sort());
  });

  it("rejects unknown, oversized, secret-bearing, raw-record, address, authority, and thread fixtures", async () => {
    const corpus =
      await fixture<Array<{ name: string; value: unknown }>>("invalid");
    for (const { name, value } of corpus) {
      expect(validateConversationsEnvelope(value), name).toMatchObject({
        kind: "invalid",
      });
    }
    expect(corpus.map(({ name }) => name)).toEqual(
      expect.arrayContaining([
        "secret-bearing key",
        "recipient key",
        "raw record",
        "signature",
        "address",
        "membership snapshot",
        "authority head",
        "SQLite value",
        "workspace token",
        "transport handle",
        "undeclared thread",
        "unsafe response authority",
        "unsafe error details",
        "unsafe invalidation record",
      ]),
    );
  });

  it("enforces UTF-8 byte, page, collection, and cross-field bounds", () => {
    expect(
      validateConversationsRequest({
        operation: "create-channel",
        name: "é".repeat(41),
      }),
    ).toMatchObject({ kind: "invalid" });
    expect(
      validateConversationsRequest({
        operation: "post-message",
        channelId: "channel",
        markdown: "é".repeat(8_193),
      }),
    ).toMatchObject({ kind: "invalid" });
    expect(
      validateConversationsResponse({
        operation: "snapshot",
        snapshot: {
          workspaceId: "workspace",
          synchronization: "current",
          channels: Array.from({ length: 257 }, (_, index) => ({
            channelId: `channel-${index}`,
            name: "channel",
            archived: false,
            unreadCount: 0,
            canManage: false,
          })),
        },
      }),
    ).toMatchObject({ kind: "invalid" });
  });

  it("validates request, response, and invalidation values independently", () => {
    expect(
      validateConversationsRequest({ operation: "snapshot" }),
    ).toMatchObject({ kind: "valid" });
    expect(
      validateConversationsResponse({
        operation: "synchronization-state",
        synchronization: "offline",
      }),
    ).toMatchObject({ kind: "valid" });
    expect(
      validateConversationsInvalidation({
        workspaceId: "workspace",
        channelId: "general",
      }),
    ).toMatchObject({ kind: "valid" });
  });
});
