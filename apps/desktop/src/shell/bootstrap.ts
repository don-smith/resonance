import type { ManifestRole } from "@resonance/contracts";

import { bundledPackageCatalog } from "../generated/bundled-package-catalog.js";
import {
  createPackageContext,
  disposePackageContexts,
} from "../package-context.js";
import { PackageHost, type BundledPackageManifest } from "../package-host.js";
import { createTemporaryMessage } from "../temporary-message.js";
import {
  isWorkspaceShellView,
  localMemberRole,
  workspaceViewChanged,
  type WorkspaceShellView,
} from "../workspace-view.js";
import {
  submitShellForm,
  type ShellFormAction,
  type ShellNativeCommands,
} from "./actions.js";
import {
  ownShellLifecycle,
  type ShellEventSource,
  type ShellWindow,
} from "./lifecycle.js";
import { DomShellRenderer, type ShellRenderer } from "./render.js";

export interface ShellPackageHost {
  manifestsForRole(
    role: ManifestRole | null,
  ): readonly BundledPackageManifest[];
  activate(packageId: string | null, role: ManifestRole | null): Promise<void>;
  dispose(): Promise<void>;
}

export type ShellNative = ShellNativeCommands & ShellEventSource;

export type ShellClipboard = {
  writeText(value: string): Promise<void>;
};

export async function bootstrapShell({
  native,
  window,
  renderer,
  packages,
  clipboard,
  dispose = () => packages.dispose(),
}: {
  native: ShellNative;
  window: ShellWindow;
  renderer: ShellRenderer;
  packages: ShellPackageHost;
  clipboard: ShellClipboard;
  dispose?: () => void | Promise<void>;
}): Promise<() => Promise<void>> {
  let currentView: WorkspaceShellView | null = null;
  let activePackageId: string | null = null;
  let actionMessage: string | null = null;
  const temporaryMessage = createTemporaryMessage((message) => {
    actionMessage = message;
    renderCurrent();
  });

  const showError = (error: unknown): void => {
    temporaryMessage.clear();
    actionMessage =
      typeof error === "string"
        ? error
        : isNativeError(error)
          ? error.message
          : "The request could not be completed.";
    renderCurrent();
  };

  const applyView = (candidate: unknown): boolean => {
    if (
      !isWorkspaceShellView(candidate) ||
      !workspaceViewChanged(currentView, candidate)
    ) {
      return false;
    }
    currentView = candidate;
    renderCurrent();
    return true;
  };

  const renderCurrent = (): void => {
    if (!currentView) return;
    const role = localMemberRole(currentView);
    const manifests = packages.manifestsForRole(role);
    if (!manifests.some(({ id }) => id === activePackageId)) {
      activePackageId =
        manifests.find(({ id }) => id === "resonance.workspace-files")?.id ??
        manifests[0]?.id ??
        null;
    }
    const blocked =
      currentView.state === "onboarding" ||
      currentView.state === "identity-error" ||
      currentView.state === "storage-error";
    const packageToActivate = blocked ? null : activePackageId;
    renderer.render(
      {
        view: currentView,
        message: actionMessage,
        manifests,
        activePackageId,
      },
      {
        selectPackage: (packageId) => {
          if (!manifests.some(({ id }) => id === packageId)) return;
          activePackageId = packageId;
          renderCurrent();
        },
        submit: (action, values) => {
          void submit(action, values);
        },
        copyInvite: () => {
          void copyInvite();
        },
      },
    );
    void packages.activate(packageToActivate, role).catch(showError);
  };

  const submit = async (
    action: ShellFormAction,
    values: Record<string, string>,
  ): Promise<void> => {
    temporaryMessage.clear();
    try {
      const result = await submitShellForm(native, action, values);
      if (!isWorkspaceShellView(result)) {
        throw new Error("The native shell returned an invalid workspace view.");
      }
      if (action === "retry") {
        actionMessage =
          result.state === "ready"
            ? "Membership is already active."
            : "Join retry sent. Waiting for the inviter.";
      }
      if (!applyView(result)) renderCurrent();
    } catch (error) {
      showError(error);
    }
  };

  const copyInvite = async (): Promise<void> => {
    try {
      const invite = await native.invoke<string>("create_workspace_invite");
      await clipboard.writeText(invite);
      temporaryMessage.show(
        "Invite copied. It grants access only while the inviter is online.",
      );
    } catch (error) {
      showError(error);
    }
  };

  const cleanup = await ownShellLifecycle<unknown>({
    events: native,
    window,
    eventName: "workspace:changed",
    onEvent: applyView,
    dispose,
  });

  try {
    applyView(await native.invoke<WorkspaceShellView>("workspace_view"));
  } catch (error) {
    renderer.showFatal(
      typeof error === "string" ? error : "Resonance could not start.",
    );
  }
  return cleanup;
}

function isNativeError(
  value: unknown,
): value is Readonly<{ code: string; message: string }> {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof value.code === "string" &&
    typeof value.message === "string"
  );
}

export async function startDesktopShell({
  document,
  window,
  native,
  clipboard,
}: {
  document: Document;
  window: ShellWindow;
  native: ShellNative;
  clipboard: ShellClipboard;
}): Promise<() => Promise<void>> {
  const shell = document.querySelector<HTMLDivElement>("#app");
  if (!shell) throw new Error("Resonance shell mount point is missing.");
  const packageMountRoot = document.createElement("section");
  packageMountRoot.className = "package-mounts";
  packageMountRoot.setAttribute("aria-label", "Package content");
  const packages = new PackageHost({
    root: packageMountRoot,
    catalog: bundledPackageCatalog,
    createContext: createPackageContext,
  });
  return bootstrapShell({
    native,
    window,
    renderer: new DomShellRenderer(shell, packageMountRoot),
    packages,
    clipboard,
    dispose: () => packages.dispose().then(disposePackageContexts),
  });
}
