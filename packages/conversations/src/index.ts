import type { PackageContentModule } from "@resonance/package-sdk";

import "./styles.css";
import { ConversationsPackage } from "./package.js";

export const mount: PackageContentModule<["conversations:v1"]>["mount"] = (
  root,
  context,
) => {
  const conversations = context.capabilities.conversationsV1;
  if (!conversations)
    throw new Error("The conversations:v1 capability is unavailable.");
  return new ConversationsPackage(root, conversations);
};
