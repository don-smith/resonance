import Ajv2020 from "ajv/dist/2020.js";

import { roles, type ManifestRole } from "@resonance/contracts";

import workspaceShellViewSchema from "../schema/workspace-shell-view.v1.json" with { type: "json" };

export type WorkspaceShellView = Readonly<{
  revision: number;
  state:
    | "onboarding"
    | "initializing"
    | "ready"
    | "joining"
    | "identity-error"
    | "storage-error";
  message: string | null;
  health: Readonly<{
    identity: "healthy" | "unavailable" | "offline";
    storage: "healthy" | "unavailable" | "offline";
    network: "healthy" | "unavailable" | "offline";
  }>;
  workspace: Readonly<{
    id: string;
    displayName: string;
    lifecycle: "initializing" | "ready" | "joining";
  }> | null;
  localPublicIdentity: string | null;
  members: ReadonlyArray<
    Readonly<{
      publicIdentity: string;
      displayName: string;
      role: string;
    }>
  >;
  peers: ReadonlyArray<
    Readonly<{
      publicIdentity: string;
      displayName: string;
      online: boolean;
      connection: "direct" | "relayed" | "unknown";
    }>
  >;
}>;

const validate = new Ajv2020({ allErrors: true, strict: true }).compile(
  workspaceShellViewSchema,
);

export function isWorkspaceShellView(
  value: unknown,
): value is WorkspaceShellView {
  return validate(value);
}

export function workspaceViewChanged(
  current: WorkspaceShellView | null,
  incoming: WorkspaceShellView,
): boolean {
  return current === null || incoming.revision > current.revision;
}

export function localMemberRole(view: WorkspaceShellView): ManifestRole | null {
  const identity = view.localPublicIdentity;
  if (!identity) return null;
  const role = view.members.find(
    ({ publicIdentity }) => publicIdentity === identity,
  )?.role;
  return role && roles.includes(role as ManifestRole)
    ? (role as ManifestRole)
    : null;
}

export function peerStatus(peer: WorkspaceShellView["peers"][number]): string {
  if (!peer.online) return "Offline";
  return peer.connection === "unknown"
    ? "Online"
    : `Online, ${peer.connection}`;
}
