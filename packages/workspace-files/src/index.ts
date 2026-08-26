import type { PackageContentModule } from "@resonance/package-sdk";

import "@toast-ui/editor/dist/toastui-editor.css";
import "@toast-ui/editor/dist/theme/toastui-editor-dark.css";
import "./styles.css";
import { WorkspaceFilesPackage } from "./workspace-files-package.js";

export const mount: PackageContentModule<["workspace-files:v1"]>["mount"] = (
  root,
  context,
) => {
  const files = context.capabilities.workspaceFilesV1;
  if (!files) {
    throw new Error("The workspace-files:v1 capability is unavailable.");
  }
  return new WorkspaceFilesPackage(root, files);
};
