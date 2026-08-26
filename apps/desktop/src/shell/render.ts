import type { BundledPackageManifest } from "../package-host.js";
import { peerStatus, type WorkspaceShellView } from "../workspace-view.js";
import { parseShellFormAction, type ShellFormAction } from "./actions.js";

export type ShellRenderModel = Readonly<{
  view: WorkspaceShellView;
  message: string | null;
  manifests: readonly BundledPackageManifest[];
  activePackageId: string | null;
}>;

export type ShellRenderHandlers = Readonly<{
  selectPackage(packageId: string): void;
  submit(action: ShellFormAction, values: Record<string, string>): void;
  copyInvite(): void;
}>;

export interface ShellRenderer {
  render(model: ShellRenderModel, handlers: ShellRenderHandlers): void;
  showFatal(message: string): void;
}

export class DomShellRenderer implements ShellRenderer {
  public constructor(
    private readonly shell: HTMLElement,
    private readonly packageMountRoot: HTMLElement,
  ) {}

  public render(model: ShellRenderModel, handlers: ShellRenderHandlers): void {
    const { view } = model;
    const onboarding = view.state === "onboarding";
    const blocked =
      view.state === "identity-error" || view.state === "storage-error";
    this.shell.innerHTML = `
      <main class="shell" aria-labelledby="app-title">
        <aside class="navigation" aria-label="Runtime navigation">
          <p class="brand">Resonance</p>
          <nav aria-label="Packages"><div class="package-navigation"></div></nav>
        </aside>
        <section class="workspace shell-owned" id="workspace">
          <div class="shell-copy">
            <p class="eyebrow">${onboarding ? "Get started" : "Workspace"}</p>
            <h1 class="shell-title" id="app-title"></h1>
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
              <details class="people shell-people"><summary>Members and peers</summary>
                <h2>Members</h2><ul class="members"></ul>
                <h2>Peers</h2><ul class="peers"></ul>
              </details>
            </section>
          </div>
        </section>
      </main>`;

    this.requiredElement<HTMLDivElement>(".package-mount-anchor").replaceWith(
      this.packageMountRoot,
    );
    this.packageMountRoot.hidden =
      onboarding || blocked || model.activePackageId === null;
    this.renderPackageNavigation(model, handlers);
    this.requiredElement<HTMLHeadingElement>("#app-title").textContent =
      onboarding
        ? "Create a workspace or join one with an invite."
        : (view.workspace?.displayName ?? "Workspace unavailable");
    this.requiredElement<HTMLParagraphElement>(".message").textContent =
      model.message ?? view.message ?? "";
    this.renderPeople(view);

    for (const form of this.shell.querySelectorAll<HTMLFormElement>(
      "form[data-action]",
    )) {
      form.addEventListener("submit", (event) => {
        event.preventDefault();
        const action = parseShellFormAction(form.dataset.action);
        const values = Object.fromEntries(
          [...new FormData(form).entries()]
            .filter(
              (entry): entry is [string, string] =>
                typeof entry[1] === "string",
            )
            .map(([name, value]) => [name, value]),
        );
        handlers.submit(action, values);
      });
    }
    this.shell
      .querySelector<HTMLButtonElement>('[data-action="invite"]')
      ?.addEventListener("click", handlers.copyInvite);
  }

  public showFatal(message: string): void {
    this.shell.textContent = message;
  }

  private renderPackageNavigation(
    model: ShellRenderModel,
    handlers: ShellRenderHandlers,
  ): void {
    const navigation = this.requiredElement<HTMLDivElement>(
      ".package-navigation",
    );
    for (const manifest of model.manifests) {
      const button = this.shell.ownerDocument.createElement("button");
      button.type = "button";
      button.textContent = manifest.nav.label;
      button.setAttribute(
        "aria-current",
        model.activePackageId === manifest.id ? "page" : "false",
      );
      button.addEventListener("click", () =>
        handlers.selectPackage(manifest.id),
      );
      navigation.append(button);
    }
  }

  private renderPeople(view: WorkspaceShellView): void {
    const members = this.requiredElement<HTMLUListElement>(".members");
    for (const member of view.members) {
      const item = this.shell.ownerDocument.createElement("li");
      item.textContent = `${member.displayName} · ${member.role}`;
      members.append(item);
    }
    const peers = this.requiredElement<HTMLUListElement>(".peers");
    for (const peer of view.peers) {
      const item = this.shell.ownerDocument.createElement("li");
      item.textContent = `${peer.displayName} · ${peerStatus(peer)}`;
      peers.append(item);
    }
  }

  private requiredElement<T extends Element>(selector: string): T {
    const element = this.shell.querySelector<T>(selector);
    if (!element) throw new Error(`Missing shell element: ${selector}`);
    return element;
  }
}

function field(label: string, name: string, type = "text"): string {
  return `<label>${label}<input name="${name}" type="${type}" required /></label>`;
}
