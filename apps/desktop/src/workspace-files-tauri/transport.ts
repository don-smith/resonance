import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type WorkspaceFilesTransport = {
  invoke(command: string, arguments_: { request: unknown }): Promise<unknown>;
  listen(
    eventName: string,
    handler: (event: { payload: unknown }) => void,
  ): Promise<() => void>;
};

export const tauriWorkspaceFilesTransport: WorkspaceFilesTransport = {
  invoke: (command, arguments_) => invoke(command, arguments_),
  listen: (eventName, handler) => listen(eventName, handler),
};
