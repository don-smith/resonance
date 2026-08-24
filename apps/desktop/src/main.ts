import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import "./styles.css";
import { bundledPackageCatalog } from "./generated/bundled-package-catalog.js";
import {
  createPackageContext,
  disposePackageContexts,
} from "./package-context.js";
import { PackageHost } from "./package-host.js";
import { createTemporaryMessage } from "./temporary-message.js";
import {
  isWorkspaceShellView,
  peerStatus,
  type WorkspaceShellView,
  workspaceViewChanged,
} from "./workspace-view.js";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Resonance shell mount point is missing.");
const shell = app;
const packageMountRoot = document.createElement("section");
packageMountRoot.className = "package-mounts";
packageMountRoot.setAttribute("aria-label", "Package content");
const packageHost = new PackageHost({
  root: packageMountRoot,
  catalog: bundledPackageCatalog,
  createContext: createPackageContext,
});
let activePackageId =
  packageHost.manifests.find(({ id }) => id === "resonance.workspace-files")
    ?.id ??
  packageHost.manifests[0]?.id ??
  null;
let actionMessage: string | null = null;
let currentView: WorkspaceShellView | null = null;
const temporaryMessage = createTemporaryMessage((message) => {
  actionMessage = message;
  if (currentView) render(currentView);
});

function field(label: string, name: string, type = "text"): string {
  return `<label>${label}<input name="${name}" type="${type}" required /></label>`;
}

function render(view: WorkspaceShellView): void {
  currentView = view;
  const onboarding = view.state === "onboarding";
  const blocked =
    view.state === "identity-error" || view.state === "storage-error";
  shell.innerHTML = `
    <main class="shell" aria-labelledby="app-title">
      <aside class="navigation" aria-label="Runtime navigation">
        <p class="brand">Resonance</p>
        <nav aria-label="Packages"><div class="package-navigation"></div></nav>
      </aside>
      <section class="workspace" id="workspace">
        <p class="eyebrow">${onboarding ? "Get started" : "Workspace"}</p>
        <h1 id="app-title"></h1>
        <p class="message" role="status"></p>
        <section class="onboarding" ${onboarding ? "" : "hidden"}>
          <h2>Create a workspace</h2>
          <form data-action="create">
            ${field("Workspace name", "displayName")}
            ${field("Your name", "creatorDisplayName")}
            <label>Relay override, optional<input name="relayOverride" type="url" /></label>
            <button type="submit">Create workspace</button>
          </form>
          <h2>Join with an invite</h2>
          <form data-action="join">
            ${field("Your name", "displayName")}
            ${field("Invite", "invite")}
            <button type="submit">Join workspace</button>
          </form>
        </section>
        <section class="active-workspace" ${onboarding || blocked ? "hidden" : ""}>
          <div class="workspace-actions">
            <button type="button" data-action="invite">Copy invite</button>
            <form class="retry-join" data-action="retry" ${view.state === "joining" ? "" : "hidden"}>
              ${field("Your name", "displayName")}
              <button type="submit">Retry join</button>
            </form>
          </div>
          <div class="package-mount-anchor"></div>
          <details class="people"><summary>Members and peers</summary>
            <h2>Members</h2><ul class="members"></ul>
            <h2>Peers</h2><ul class="peers"></ul>
          </details>
        </section>
      </section>
    </main>`;

  requiredElement<HTMLDivElement>(".package-mount-anchor").replaceWith(
    packageMountRoot,
  );
  packageMountRoot.hidden = onboarding || blocked || activePackageId === null;
  void packageHost.activate(packageMountRoot.hidden ? null : activePackageId);
  renderPackageNavigation();

  requiredElement<HTMLHeadingElement>("#app-title").textContent = onboarding
    ? "Create a workspace or join one with an invite."
    : (view.workspace?.displayName ?? "Workspace unavailable");
  requiredElement<HTMLParagraphElement>(".message").textContent =
    actionMessage ?? view.message ?? "";
  renderPeople(view);

  for (const form of document.querySelectorAll<HTMLFormElement>(
    "form[data-action]",
  )) {
    form.addEventListener("submit", submitForm);
  }
  document
    .querySelector<HTMLButtonElement>('[data-action="invite"]')
    ?.addEventListener("click", copyInvite);
}

function renderPackageNavigation(): void {
  const navigation = requiredElement<HTMLDivElement>(".package-navigation");
  for (const manifest of packageHost.manifests) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = manifest.nav.label;
    button.setAttribute(
      "aria-current",
      activePackageId === manifest.id ? "page" : "false",
    );
    button.addEventListener("click", () => selectPackage(manifest.id));
    navigation.append(button);
  }
}

function selectPackage(packageId: string): void {
  if (activePackageId === packageId) return;
  activePackageId = packageId;
  if (currentView) render(currentView);
}

function renderPeople(view: WorkspaceShellView): void {
  const members = requiredElement<HTMLUListElement>(".members");
  for (const member of view.members) {
    const item = document.createElement("li");
    item.textContent = `${member.displayName} · ${member.role}`;
    members.append(item);
  }
  const peers = requiredElement<HTMLUListElement>(".peers");
  for (const peer of view.peers) {
    const item = document.createElement("li");
    item.textContent = `${peer.displayName} · ${peerStatus(peer)}`;
    peers.append(item);
  }
}

async function submitForm(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const values = new FormData(form);
  const action = form.dataset.action;
  temporaryMessage.clear();
  try {
    const result = await invoke<WorkspaceShellView>(
      action === "create"
        ? "create_workspace"
        : action === "join"
          ? "join_workspace"
          : "retry_workspace_join",
      {
        request:
          action === "create"
            ? {
                displayName: String(values.get("displayName") ?? ""),
                creatorDisplayName: String(
                  values.get("creatorDisplayName") ?? "",
                ),
                relayOverride: optionalValue(values.get("relayOverride")),
              }
            : action === "join"
              ? {
                  displayName: String(values.get("displayName") ?? ""),
                  invite: String(values.get("invite") ?? ""),
                }
              : { displayName: String(values.get("displayName") ?? "") },
      },
    );
    if (isWorkspaceShellView(result)) {
      if (action === "retry") {
        actionMessage =
          result.state === "ready"
            ? "Membership is already active."
            : "Join retry sent. Waiting for the inviter.";
      }
      render(result);
    }
  } catch (error) {
    showActionError(error);
  }
}

async function copyInvite(): Promise<void> {
  try {
    const invite = await invoke<string>("create_workspace_invite");
    await navigator.clipboard.writeText(invite);
    temporaryMessage.show(
      "Invite copied. It grants access only while the inviter is online.",
    );
  } catch (error) {
    showActionError(error);
  }
}

function optionalValue(value: FormDataEntryValue | null): string | null {
  const text = typeof value === "string" ? value.trim() : "";
  return text || null;
}

function showActionError(error: unknown): void {
  temporaryMessage.clear();
  actionMessage =
    typeof error === "string" ? error : "The request could not be completed.";
  requiredElement<HTMLParagraphElement>(".message").textContent = actionMessage;
}

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Missing shell element: ${selector}`);
  return element;
}

window.addEventListener(
  "beforeunload",
  () => {
    void packageHost.dispose().then(disposePackageContexts);
  },
  { once: true },
);

void Promise.all([
  invoke<WorkspaceShellView>("workspace_view"),
  listen<unknown>("workspace:changed", (event) => {
    if (
      isWorkspaceShellView(event.payload) &&
      workspaceViewChanged(currentView, event.payload)
    ) {
      render(event.payload);
    }
  }),
])
  .then(([view]) => {
    if (isWorkspaceShellView(view)) render(view);
  })
  .catch((error: unknown) => {
    shell.textContent =
      typeof error === "string" ? error : "Resonance could not start.";
  });
