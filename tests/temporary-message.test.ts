import { afterEach, describe, expect, it, vi } from "vitest";

import { createTemporaryMessage } from "../apps/desktop/src/temporary-message.js";

describe("temporary messages", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("clears a confirmation after its duration", () => {
    vi.useFakeTimers();
    const update = vi.fn();
    const message = createTemporaryMessage(update, 5_000);

    message.show("Invite copied.");
    vi.advanceTimersByTime(5_000);

    expect(update.mock.calls).toEqual([["Invite copied."], [null]]);
  });

  it("keeps a replacement confirmation for its full duration", () => {
    vi.useFakeTimers();
    const update = vi.fn();
    const message = createTemporaryMessage(update, 5_000);

    message.show("First invite copied.");
    vi.advanceTimersByTime(4_000);
    message.show("Second invite copied.");
    vi.advanceTimersByTime(1_000);

    expect(update.mock.calls).toEqual([
      ["First invite copied."],
      ["Second invite copied."],
    ]);

    vi.advanceTimersByTime(4_000);

    expect(update.mock.calls).toEqual([
      ["First invite copied."],
      ["Second invite copied."],
      [null],
    ]);
  });
});
