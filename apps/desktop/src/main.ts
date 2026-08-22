import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import "./styles.css";
import {
  isWorkspaceShellView,
  peerStatus,
  type WorkspaceShellView,
} from "./workspace-view.js";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) {
  throw new Error("Resonance shell mount point is missing.");
}
const shell = app;

function field(label: string, name: string, type = "text"): string {
  return `<label>${label}<input name="${name}" type="${type}" required /></label>`;
}

function render(view: WorkspaceShellView): void {
  const onboarding = view.state === "onboarding";
  const blocked =
    view.state === "identity-error" || view.state === "storage-error";
  shell.innerHTML = `
    <main class="shell" aria-labelledby="app-title">
      <aside class="navigation" aria-label="Runtime navigation">
        <p class="brand">Resonance</p>
        <nav><a aria-current="page" href="#workspace">Workspace</a></nav>
      </aside>
      <section class="workspace" id="workspace">
        <p class="eyebrow">${onboarding ? "Get started" : "Workspace"}</p>
        <h1 id="app-title"></h1>
        <p class="message" role="status"></p>
        <div class="identity"></div>
        <section class="onboarding" ${onboarding ? "" : "hidden"}>
          <h2>Create a workspace</h2>
          <form data-action="create">
            ${field("Workspace name", "displayName")}
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
            <button type="button" data-action="retry" ${view.state === "joining" ? "" : "hidden"}>Retry join</button>
          </div>
          <h2>Members</h2>
          <ul class="members"></ul>
          <h2>Peers</h2>
          <ul class="peers"></ul>
        </section>
      </section>
    </main>
  `;

  const title = requiredElement<HTMLHeadingElement>("#app-title");
  title.textContent = onboarding
    ? "Create a workspace or join one with an invite."
    : (view.workspace?.displayName ?? "Workspace unavailable");
  requiredElement<HTMLParagraphElement>(".message").textContent =
    view.message ?? "";
  requiredElement<HTMLDivElement>(".identity").textContent =
    view.localPublicIdentity
      ? `This installation: ${view.localPublicIdentity}`
      : "";

  const members = requiredElement<HTMLUListElement>(".members");
  for (const member of view.members) {
    const item = document.createElement("li");
    item.textContent = `${member.displayName} · ${member.role}`;
    members.append(item);
  }
  const peers = requiredElement<HTMLUListElement>(".peers");
  for (const peer of view.peers) {
    const item = document.createElement("li");
    item.textContent = `${peer.publicIdentity.slice(0, 12)} · ${peerStatus(peer)}`;
    peers.append(item);
  }

  for (const form of document.querySelectorAll<HTMLFormElement>(
    "form[data-action]",
  )) {
    form.addEventListener("submit", submitForm);
  }
  document
    .querySelector<HTMLButtonElement>('[data-action="invite"]')
    ?.addEventListener("click", copyInvite);
  document
    .querySelector<HTMLButtonElement>('[data-action="retry"]')
    ?.addEventListener("click", retryJoin);
}

async function submitForm(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const values = new FormData(form);
  try {
    const command =
      form.dataset.action === "create" ? "create_workspace" : "join_workspace";
    const result = await invoke<WorkspaceShellView>(command, {
      request:
        command === "create_workspace"
          ? {
              displayName: String(values.get("displayName") ?? ""),
              relayOverride: optionalValue(values.get("relayOverride")),
            }
          : {
              displayName: String(values.get("displayName") ?? ""),
              invite: String(values.get("invite") ?? ""),
            },
    });
    if (isWorkspaceShellView(result)) {
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
    showActionMessage(
      "Invite copied. It grants access only while the inviter is online.",
    );
  } catch (error) {
    showActionError(error);
  }
}

async function retryJoin(): Promise<void> {
  const displayName = window.prompt("Your name")?.trim();
  if (!displayName) {
    return;
  }
  try {
    const result = await invoke<WorkspaceShellView>("retry_workspace_join", {
      request: { displayName },
    });
    if (isWorkspaceShellView(result)) {
      render(result);
    }
  } catch (error) {
    showActionError(error);
  }
}

function optionalValue(value: FormDataEntryValue | null): string | null {
  const text = typeof value === "string" ? value.trim() : "";
  return text || null;
}

function showActionMessage(message: string): void {
  requiredElement<HTMLParagraphElement>(".message").textContent = message;
}

function showActionError(error: unknown): void {
  showActionMessage(
    typeof error === "string" ? error : "The request could not be completed.",
  );
}

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) {
    throw new Error(`Missing shell element: ${selector}`);
  }
  return element;
}

void Promise.all([
  invoke<WorkspaceShellView>("workspace_view"),
  listen<unknown>("workspace:changed", (event) => {
    if (isWorkspaceShellView(event.payload)) {
      render(event.payload);
    }
  }),
])
  .then(([view]) => {
    if (isWorkspaceShellView(view)) {
      render(view);
    }
  })
  .catch((error: unknown) => {
    shell.textContent =
      typeof error === "string" ? error : "Resonance could not start.";
  });
