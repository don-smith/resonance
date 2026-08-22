export type TemporaryMessage = {
  clear(): void;
  show(message: string): void;
};

export function createTemporaryMessage(
  update: (message: string | null) => void,
  durationMs = 5_000,
): TemporaryMessage {
  let timeout: ReturnType<typeof setTimeout> | undefined;

  function clear(): void {
    if (timeout !== undefined) {
      clearTimeout(timeout);
      timeout = undefined;
    }
    update(null);
  }

  return {
    clear,
    show(message) {
      if (timeout !== undefined) {
        clearTimeout(timeout);
      }
      update(message);
      timeout = setTimeout(() => {
        timeout = undefined;
        update(null);
      }, durationMs);
    },
  };
}
