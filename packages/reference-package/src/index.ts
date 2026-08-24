import type {
  PackageContentModule,
  PackageInstance,
} from "../../sdk/src/index.js";

import "./styles.css";

export const mount: PackageContentModule["mount"] = (
  root,
  context,
): PackageInstance => {
  let disposed = false;
  let visits = 0;
  const heading = root.ownerDocument.createElement("h2");
  heading.textContent = context.package.name;
  const description = root.ownerDocument.createElement("p");
  description.textContent =
    "This content was loaded through the bundled package catalog.";
  const status = root.ownerDocument.createElement("p");
  status.setAttribute("role", "status");
  const button = root.ownerDocument.createElement("button");
  button.type = "button";
  button.textContent = "Record a visit";
  const recordVisit = () => {
    visits += 1;
    status.textContent = `Recorded visits: ${visits}`;
  };
  button.addEventListener("click", recordVisit);
  status.textContent = "Recorded visits: 0";
  root.append(heading, description, button, status);

  return {
    activate() {
      if (disposed) return;
      root.setAttribute("data-package-active", "true");
    },
    deactivate() {
      root.removeAttribute("data-package-active");
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      button.removeEventListener("click", recordVisit);
      root.replaceChildren();
    },
  };
};
