import { describe, expect, it, vi } from "vitest";

import type { ManifestRole } from "@resonance/contracts";

import {
  bootstrapShell,
  type ShellNative,
  type ShellPackageHost,
} from "../apps/desktop/src/shell/bootstrap.js";
import { parseShellFormAction } from "../apps/desktop/src/shell/actions.js";
import type {
  ShellRenderHandlers,
  ShellRenderer,
  ShellRenderModel,
} from "../apps/desktop/src/shell/render.js";
import type { WorkspaceShellView } from "../apps/desktop/src/workspace-view.js";

function view(
  revision: number,
  state: WorkspaceShellView["state"],
): WorkspaceShellView {
  const active = state !== "onboarding";
  return {
    revision,
    state,
    message: null,
    workspace: active
      ? { id: "workspace-id", displayName: "Team", lifecycle: "ready" }
      : null,
    localPublicIdentity: active ? "local-id" : "local-id",
    members: active
      ? [
          {
            publicIdentity: "local-id",
            displayName: "Ada",
            role: "viewer",
          },
        ]
      : [],
    peers: [],
  };
}

class FakeRenderer implements ShellRenderer {
  public readonly models: ShellRenderModel[] = [];
  public handlers: ShellRenderHandlers | null = null;
  public fatal: string | null = null;

  public render(model: ShellRenderModel, handlers: ShellRenderHandlers): void {
    this.models.push(structuredClone(model));
    this.handlers = handlers;
  }

  public showFatal(message: string): void {
    this.fatal = message;
  }
}

class FakePackages implements ShellPackageHost {
  public readonly activations: Array<{
    packageId: string | null;
    role: ManifestRole | null;
  }> = [];
  public readonly dispose = vi.fn(async () => undefined);
  private readonly manifests = [
    {
      id: "resonance.reference",
      name: "Reference",
      nav: { label: "Reference", icon: "book" },
      events: { emits: [], consumes: [] },
      minRole: "viewer" as const,
    },
    {
      id: "resonance.workspace-files",
      name: "Workspace files",
      nav: { label: "Files", icon: "files" },
      events: { emits: [], consumes: [] },
      minRole: "viewer" as const,
    },
  ];

  public manifestsForRole(role: ManifestRole | null) {
    return role === null ? [] : this.manifests;
  }

  public async activate(
    packageId: string | null,
    role: ManifestRole | null,
  ): Promise<void> {
    this.activations.push({ packageId, role });
  }
}

class FakeWindow {
  public unload: (() => void) | null = null;

  public addEventListener(_event: "beforeunload", handler: () => void): void {
    this.unload = handler;
  }

  public removeEventListener(): void {
    this.unload = null;
  }
}

describe("workspace shell", () => {
  it("coordinates onboarding, commands, navigation, events, and cleanup", async () => {
    const renderer = new FakeRenderer();
    const packages = new FakePackages();
    const window = new FakeWindow();
    let workspaceListener: ((payload: unknown) => void) | null = null;
    const unlisten = vi.fn();
    const invoke = vi.fn(
      async (command: string, _arguments_?: Record<string, unknown>) => {
        if (command === "workspace_view") return view(1, "onboarding");
        if (command === "create_workspace") return view(2, "ready");
        throw new Error(`unexpected command ${command}`);
      },
    );
    const native: ShellNative = {
      invoke: <T>(command: string, arguments_?: Record<string, unknown>) =>
        invoke(command, arguments_) as Promise<T>,
      listen: async (_event, handler) => {
        workspaceListener = handler as (payload: unknown) => void;
        return unlisten;
      },
    };

    const cleanup = await bootstrapShell({
      native,
      window,
      renderer,
      packages,
      clipboard: { writeText: vi.fn(async () => undefined) },
    });

    expect(renderer.models.at(-1)?.view.state).toBe("onboarding");
    expect(renderer.models.at(-1)?.manifests).toEqual([]);
    expect(packages.activations.at(-1)).toEqual({
      packageId: null,
      role: null,
    });

    renderer.handlers?.submit("create", {
      displayName: "Team",
      creatorDisplayName: "Ada",
    });
    await vi.waitFor(() => {
      expect(renderer.models.at(-1)?.view.revision).toBe(2);
    });
    expect(invoke).toHaveBeenCalledWith("create_workspace", {
      request: {
        displayName: "Team",
        creatorDisplayName: "Ada",
        relayOverride: null,
      },
    });
    expect(renderer.models.at(-1)?.activePackageId).toBe(
      "resonance.workspace-files",
    );
    expect(packages.activations.at(-1)).toEqual({
      packageId: "resonance.workspace-files",
      role: "viewer",
    });

    renderer.handlers?.selectPackage("resonance.reference");
    expect(packages.activations.at(-1)?.packageId).toBe("resonance.reference");
    (workspaceListener as ((payload: unknown) => void) | null)?.(
      view(3, "ready"),
    );
    expect(renderer.models.at(-1)?.view.revision).toBe(3);
    expect(JSON.stringify(renderer.models.at(-1))).not.toMatch(
      /open_markdown_file|replace_markdown_file|files-panel/,
    );

    await cleanup();
    expect(unlisten).toHaveBeenCalledOnce();
    expect(packages.dispose).toHaveBeenCalledOnce();
    expect(window.unload).toBeNull();
  });

  it("rejects unknown form actions instead of routing them to retry", () => {
    expect(() => parseShellFormAction("misspelled")).toThrow(
      "Unknown shell form action",
    );
  });
});
