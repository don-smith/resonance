export type ShellEventSource = {
  listen<T>(event: string, handler: (payload: T) => void): Promise<() => void>;
};

export type ShellWindow = {
  addEventListener(
    event: "beforeunload",
    handler: () => void,
    options?: { once?: boolean },
  ): void;
  removeEventListener(event: "beforeunload", handler: () => void): void;
};

export async function ownShellLifecycle<T>({
  events,
  window,
  eventName,
  onEvent,
  dispose,
}: {
  events: ShellEventSource;
  window: ShellWindow;
  eventName: string;
  onEvent(payload: T): void;
  dispose(): void | Promise<void>;
}): Promise<() => Promise<void>> {
  const unlisten = await events.listen<T>(eventName, onEvent);
  let cleanupPromise: Promise<void> | null = null;
  const cleanup = (): Promise<void> => {
    cleanupPromise ??= Promise.resolve()
      .then(unlisten)
      .then(dispose)
      .finally(() => window.removeEventListener("beforeunload", unload));
    return cleanupPromise;
  };
  const unload = (): void => {
    void cleanup();
  };
  window.addEventListener("beforeunload", unload, { once: true });
  return cleanup;
}
