import type { WorkspaceShellView } from "../workspace-view.js";

export type ShellFormAction = "create" | "join" | "retry";

export type ShellNativeCommands = {
  invoke<T>(command: string, arguments_?: Record<string, unknown>): Promise<T>;
};

export function parseShellFormAction(
  value: string | undefined,
): ShellFormAction {
  switch (value) {
    case "create":
    case "join":
    case "retry":
      return value;
    default:
      throw new Error(`Unknown shell form action: ${value ?? "missing"}`);
  }
}

export async function submitShellForm(
  native: ShellNativeCommands,
  action: ShellFormAction,
  values: Readonly<Record<string, string>>,
): Promise<WorkspaceShellView> {
  switch (action) {
    case "create":
      return native.invoke<WorkspaceShellView>("create_workspace", {
        request: {
          displayName: values.displayName ?? "",
          creatorDisplayName: values.creatorDisplayName ?? "",
          relayOverride: optionalValue(values.relayOverride),
        },
      });
    case "join":
      return native.invoke<WorkspaceShellView>("join_workspace", {
        request: {
          displayName: values.displayName ?? "",
          invite: values.invite ?? "",
        },
      });
    case "retry":
      return native.invoke<WorkspaceShellView>("retry_workspace_join", {
        request: { displayName: values.displayName ?? "" },
      });
  }
}

function optionalValue(value: string | undefined): string | null {
  const text = value?.trim() ?? "";
  return text || null;
}
