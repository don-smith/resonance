import type { PackageContentModule } from "@resonance/package-sdk";

import "./styles.css";

// The host calls mount once and retains this instance across navigation.
export const mount: PackageContentModule["mount"] = (root, context) => {
  const heading = root.ownerDocument.createElement("h2");
  heading.textContent = context.package.name;
  root.append(heading);
  let disposed = false;

  return {
    activate() {
      if (!disposed) root.setAttribute("data-package-active", "true");
    },
    deactivate() {
      root.removeAttribute("data-package-active");
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      // Remove package-owned listeners, subscriptions, timers, editors, and
      // object URLs here before clearing the package region.
      root.replaceChildren();
    },
  };
};
