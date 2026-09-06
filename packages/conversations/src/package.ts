import {
  isConversationsError,
  type ConversationChannel,
  type ConversationMessage,
  type ConversationsSnapshot,
  type ConversationsV1,
  type PackageInstance,
} from "@resonance/package-sdk";

import { synchronizationLabel, unreadLabel } from "./view.js";

const GENERAL_CHANNEL_NAME = "#general";

function channelNameForAuthoring(name: string): string {
  return name.startsWith("#") ? name : `#${name}`;
}

export class ConversationsPackage implements PackageInstance {
  readonly #root: HTMLElement;
  readonly #conversations: ConversationsV1;
  readonly #submit = (event: Event) => void this.#handleSubmit(event);
  readonly #click = (event: Event) => void this.#handleClick(event);
  readonly #input = (event: Event) => this.#rememberDraft(event);
  #unsubscribe: (() => void) | null;
  #snapshot: ConversationsSnapshot | null = null;
  #messages: readonly ConversationMessage[] = [];
  #nextCursor: string | null = null;
  #selectedChannelId: string | null = null;
  #renderedChannelId: string | null = null;
  #message: string | null = null;
  #drafts = new Map<string, string>();
  #focusedDraft: string | null = null;
  #version = 0;
  #active = false;
  #disposed = false;

  public constructor(root: HTMLElement, conversations: ConversationsV1) {
    this.#root = root;
    this.#conversations = conversations;
    this.#root.addEventListener("submit", this.#submit);
    this.#root.addEventListener("click", this.#click);
    this.#root.addEventListener("input", this.#input);
    this.#unsubscribe = conversations.subscribe((invalidation) => {
      if (
        !this.#disposed &&
        invalidation.workspaceId === this.#snapshot?.workspaceId
      )
        void this.#refresh();
    });
  }

  public async activate(): Promise<void> {
    if (this.#disposed) return;
    this.#active = true;
    await this.#refresh();
  }

  public deactivate(): void {
    this.#active = false;
  }

  public dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#active = false;
    this.#version += 1;
    this.#unsubscribe?.();
    this.#unsubscribe = null;
    this.#root.removeEventListener("submit", this.#submit);
    this.#root.removeEventListener("click", this.#click);
    this.#root.removeEventListener("input", this.#input);
    this.#root.replaceChildren();
  }

  async #refresh(forceRender = false): Promise<void> {
    const version = ++this.#version;
    try {
      const snapshot = await this.#conversations.snapshot();
      if (version !== this.#version || this.#disposed) return;
      const previousSnapshot = this.#snapshot;
      const previousMessages = this.#messages;
      const previousCursor = this.#nextCursor;
      const hadMessage = this.#message !== null;
      this.#snapshot = snapshot;
      this.#message = null;
      const current = this.#selectedChannelId;
      const selected =
        snapshot.channels.find(
          ({ channelId, archived }) => channelId === current && !archived,
        ) ??
        snapshot.channels.find(
          ({ name, archived }) => name === GENERAL_CHANNEL_NAME && !archived,
        ) ??
        snapshot.channels.find(({ archived }) => !archived) ??
        snapshot.channels[0];
      this.#selectedChannelId = selected?.channelId ?? null;
      const page = selected
        ? await this.#conversations.messages(selected.channelId, null, 100)
        : null;
      this.#messages = page?.messages ?? [];
      this.#nextCursor = page?.nextCursor ?? null;
      if (version !== this.#version || this.#disposed) return;
      const viewChanged =
        hadMessage ||
        this.#renderedChannelId !== this.#selectedChannelId ||
        previousCursor !== this.#nextCursor ||
        JSON.stringify(previousSnapshot) !== JSON.stringify(this.#snapshot) ||
        JSON.stringify(previousMessages) !== JSON.stringify(this.#messages);
      if (this.#active && (forceRender || viewChanged)) this.#render();
    } catch (error) {
      if (version !== this.#version || this.#disposed) return;
      this.#message = isConversationsError(error)
        ? error.message
        : "Conversations could not be refreshed.";
      if (this.#active) this.#render();
    }
  }

  #render(): void {
    const document = this.#root.ownerDocument;
    this.#root.replaceChildren();
    const heading = document.createElement("header");
    heading.className = "conversations-heading";
    const title = document.createElement("h2");
    title.textContent = "Conversations";
    const status = document.createElement("span");
    status.className = "conversations-sync";
    status.textContent = this.#snapshot
      ? synchronizationLabel(this.#snapshot.synchronization)
      : "Waiting to sync";
    heading.append(title, status);
    this.#root.append(heading);
    if (this.#message) {
      const message = document.createElement("p");
      message.className = "conversations-notice";
      message.setAttribute("role", "status");
      message.textContent = this.#message;
      this.#root.append(message);
    }
    const layout = document.createElement("div");
    layout.className = "conversations-layout";
    const sidebar = document.createElement("aside");
    sidebar.append(this.#channelList(document), this.#newChannelForm(document));
    const conversation = document.createElement("section");
    const selected = this.#snapshot?.channels.find(
      ({ channelId }) => channelId === this.#selectedChannelId,
    );
    if (selected) this.#renderChannel(document, conversation, selected);
    else {
      const empty = document.createElement("p");
      empty.textContent = "No public channels are available yet.";
      conversation.append(empty);
    }
    layout.append(sidebar, conversation);
    this.#root.append(layout);
    this.#renderedChannelId = selected?.channelId ?? null;
    this.#restoreFocusedDraft();
  }

  #channelList(document: Document): HTMLElement {
    const list = document.createElement("ul");
    list.className = "conversations-channels";
    for (const channel of this.#snapshot?.channels ?? []) {
      const item = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.dataset.action = "select-channel";
      button.dataset.channelId = channel.channelId;
      button.disabled = channel.archived;
      button.textContent = `${channel.name}${channel.archived ? " (archived)" : ""}`;
      const unread = document.createElement("span");
      unread.textContent = unreadLabel(channel.unreadCount);
      item.append(button, unread);
      list.append(item);
    }
    return list;
  }

  #newChannelForm(document: Document): HTMLFormElement {
    const form = document.createElement("form");
    form.dataset.action = "create-channel";
    const input = document.createElement("input");
    input.name = "name";
    input.required = true;
    input.maxLength = 80;
    input.placeholder = "#new-public-channel";
    this.#restoreDraft("create-channel", input);
    const button = document.createElement("button");
    button.type = "submit";
    button.textContent = "Create channel";
    form.append(input, button);
    return form;
  }

  #renderChannel(
    document: Document,
    section: HTMLElement,
    channel: ConversationChannel,
  ): void {
    const title = document.createElement("h3");
    title.textContent = channel.name;
    section.append(title);
    if (channel.canManage && !channel.archived) {
      const controls = document.createElement("div");
      controls.className = "conversations-controls";
      const rename = document.createElement("form");
      rename.dataset.action = "rename-channel";
      const input = document.createElement("input");
      input.name = "name";
      input.value = channel.name;
      input.required = true;
      input.maxLength = 80;
      this.#restoreDraft("rename-channel", input);
      const submit = document.createElement("button");
      submit.type = "submit";
      submit.textContent = "Rename";
      rename.append(input, submit);
      const archive = document.createElement("button");
      archive.type = "button";
      archive.dataset.action = "archive-channel";
      archive.textContent = "Archive";
      controls.append(rename, archive);
      section.append(controls);
    }
    const messages = document.createElement("ol");
    messages.className = "conversations-messages";
    for (const message of this.#messages) {
      const item = document.createElement("li");
      const attribution = document.createElement("strong");
      attribution.textContent = message.author.displayName;
      const markdown = document.createElement("p");
      markdown.className = "conversations-markdown";
      markdown.textContent = message.markdown;
      item.append(attribution, markdown);
      messages.append(item);
    }
    if (this.#messages.length === 0) {
      const empty = document.createElement("p");
      empty.textContent =
        channel.name === GENERAL_CHANNEL_NAME
          ? "#general is ready for the first message."
          : "This channel has no messages yet.";
      section.append(empty);
    } else {
      section.append(messages);
      if (this.#nextCursor) {
        const more = document.createElement("button");
        more.type = "button";
        more.dataset.action = "load-more";
        more.textContent = "Load more";
        section.append(more);
      }
      if (channel.unreadCount > 0) {
        const read = document.createElement("button");
        read.type = "button";
        read.dataset.action = "mark-read";
        read.dataset.messageId = this.#messages.at(-1)?.messageId;
        read.textContent = "Mark read";
        section.append(read);
      }
    }
    if (!channel.archived) {
      const post = document.createElement("form");
      post.dataset.action = "post-message";
      const markdown = document.createElement("textarea");
      markdown.name = "markdown";
      markdown.maxLength = 16_384;
      markdown.required = true;
      markdown.placeholder = "Write Markdown";
      this.#restoreDraft("post-message", markdown);
      const send = document.createElement("button");
      send.type = "submit";
      send.textContent = "Post message";
      post.append(markdown, send);
      section.append(post);
    }
  }

  #rememberDraft(event: Event): void {
    const target = event.target;
    if (
      !(
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement
      )
    )
      return;
    const action = target.form?.dataset.action;
    if (!action || !target.name) return;
    const key = `${action}:${target.name}`;
    this.#drafts.set(key, target.value);
    this.#focusedDraft = key;
  }

  #restoreDraft(
    action: string,
    control: HTMLInputElement | HTMLTextAreaElement,
  ): void {
    const value = this.#drafts.get(`${action}:${control.name}`);
    if (value !== undefined) control.value = value;
  }

  #restoreFocusedDraft(): void {
    if (!this.#focusedDraft) return;
    const [action, name] = this.#focusedDraft.split(":", 2);
    const root = this.#root as unknown as {
      querySelector?: (
        selector: string,
      ) => HTMLInputElement | HTMLTextAreaElement | null;
    };
    root
      .querySelector?.(`form[data-action="${action}"] [name="${name}"]`)
      ?.focus();
  }

  async #handleSubmit(event: Event): Promise<void> {
    const form = event.target as HTMLFormElement;
    const action = form.dataset.action;
    if (!action) return;
    event.preventDefault();
    const values = new FormData(form);
    await this.#mutate(action, async () => {
      if (action === "create-channel") {
        const channel = await this.#conversations.createChannel(
          channelNameForAuthoring(String(values.get("name") ?? "")),
        );
        this.#selectedChannelId = channel.channelId;
      } else if (action === "rename-channel" && this.#selectedChannelId) {
        await this.#conversations.renameChannel(
          this.#selectedChannelId,
          channelNameForAuthoring(String(values.get("name") ?? "")),
        );
      } else if (action === "post-message" && this.#selectedChannelId) {
        await this.#conversations.postMessage(
          this.#selectedChannelId,
          String(values.get("markdown") ?? ""),
        );
      }
    });
  }

  async #handleClick(event: Event): Promise<void> {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const button = target.closest<HTMLButtonElement>("button[data-action]");
    if (!button || !this.#root.contains(button)) return;
    const action = button.dataset.action;
    if (action === "select-channel") {
      this.#selectedChannelId = button.dataset.channelId ?? null;
      await this.#refresh(true);
    } else if (
      action === "load-more" &&
      this.#selectedChannelId &&
      this.#nextCursor
    ) {
      await this.#loadMore();
    } else if (action === "archive-channel" && this.#selectedChannelId) {
      await this.#mutate("archive-channel", () =>
        this.#conversations.archiveChannel(this.#selectedChannelId!),
      );
    } else if (
      action === "mark-read" &&
      this.#selectedChannelId &&
      button.dataset.messageId
    ) {
      await this.#mutate("mark-read", () =>
        this.#conversations.markRead(
          this.#selectedChannelId!,
          button.dataset.messageId!,
        ),
      );
    }
  }

  async #loadMore(): Promise<void> {
    const version = this.#version;
    try {
      const page = await this.#conversations.messages(
        this.#selectedChannelId!,
        this.#nextCursor,
        100,
      );
      if (version !== this.#version || this.#disposed) return;
      this.#messages = [...this.#messages, ...page.messages];
      this.#nextCursor = page.nextCursor;
      this.#message = null;
      this.#render();
    } catch (error) {
      if (version !== this.#version || this.#disposed) return;
      this.#message = isConversationsError(error)
        ? error.message
        : "More messages could not be loaded.";
      this.#render();
    }
  }

  async #mutate(
    action: string,
    operation: () => Promise<unknown>,
  ): Promise<void> {
    try {
      await operation();
      const clearedDrafts = this.#clearDrafts(action);
      this.#message = null;
      await this.#refresh();
      if (clearedDrafts && this.#active) this.#render();
    } catch (error) {
      this.#message = isConversationsError(error)
        ? error.message
        : "The conversation action failed.";
      this.#render();
    }
  }

  #clearDrafts(action: string): boolean {
    const prefix = `${action}:`;
    let cleared = false;
    for (const key of this.#drafts.keys()) {
      if (key.startsWith(prefix)) {
        this.#drafts.delete(key);
        cleared = true;
      }
    }
    if (this.#focusedDraft?.startsWith(prefix)) {
      this.#focusedDraft = null;
      cleared = true;
    }
    return cleared;
  }
}
