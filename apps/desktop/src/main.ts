import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import "./styles.css";
import { startDesktopShell } from "./shell/bootstrap.js";

void startDesktopShell({
  document,
  window,
  clipboard: navigator.clipboard,
  native: {
    invoke: <T>(command: string, arguments_?: Record<string, unknown>) =>
      invoke<T>(command, arguments_),
    listen: <T>(event: string, handler: (payload: T) => void) =>
      listen<T>(event, (payload) => handler(payload.payload)),
  },
});
